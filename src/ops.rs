use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RegisterOp {
    Yank,
    #[allow(dead_code)]
    Cut,
}

pub struct Register {
    pub paths: Vec<PathBuf>,
    pub op: RegisterOp,
}

pub enum OpRecord {
    Copied { _src: PathBuf, dst: PathBuf },
    Moved { src: PathBuf, dst: PathBuf },
    Created { path: PathBuf },
    Renamed { from: PathBuf, to: PathBuf },
}

const MAX_UNDO: usize = 50;

pub struct UndoStack {
    entries: VecDeque<Vec<OpRecord>>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub fn push(&mut self, records: Vec<OpRecord>) {
        if !records.is_empty() {
            self.entries.push_back(records);
            if self.entries.len() > MAX_UNDO {
                self.entries.pop_front();
            }
        }
    }

    pub fn pop(&mut self) -> Option<Vec<OpRecord>> {
        self.entries.pop_back()
    }
}

// --- Progress ---

pub enum ProgressMsg {
    Progress {
        bytes_done: u64,
        bytes_total: u64,
        item_index: usize,
        item_total: usize,
    },
    Finished {
        records: Vec<OpRecord>,
        error: Option<String>,
        bytes_total: u64,
    },
}

/// Total size of a path (recursive for directories).
/// Uses symlink_metadata to avoid following symlink cycles.
pub fn path_size(p: &Path) -> u64 {
    let meta = match fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if meta.is_symlink() {
        meta.len()
    } else if meta.is_dir() {
        fs::read_dir(p)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| path_size(&e.path()))
            .sum()
    } else {
        meta.len()
    }
}

struct ProgressCtx {
    tx: mpsc::Sender<ProgressMsg>,
    bytes_done: u64,
    bytes_total: u64,
    item_index: usize,
    item_total: usize,
}

impl ProgressCtx {
    fn report(&self) {
        let _ = self.tx.send(ProgressMsg::Progress {
            bytes_done: self.bytes_done,
            bytes_total: self.bytes_total,
            item_index: self.item_index,
            item_total: self.item_total,
        });
    }
}

fn copy_dir_progress(src: &Path, dst: &Path, ctx: &mut ProgressCtx) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_progress(&entry.path(), &target, ctx)?;
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            fs::copy(entry.path(), &target)?;
            ctx.bytes_done += size;
            ctx.report();
        }
    }
    Ok(())
}

fn copy_path_progress(
    src: &Path,
    dst_dir: &Path,
    ctx: &mut ProgressCtx,
) -> std::io::Result<OpRecord> {
    let name = filename(src)?;
    let dst = resolve_conflict(dst_dir, &name);
    if src.is_dir() {
        ctx.report();
        copy_dir_progress(src, &dst, ctx)?;
    } else {
        let size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
        fs::copy(src, &dst)?;
        ctx.bytes_done += size;
        ctx.report();
    }
    Ok(OpRecord::Copied {
        _src: src.into(),
        dst,
    })
}

fn move_path_progress(
    src: &Path,
    dst_dir: &Path,
    ctx: &mut ProgressCtx,
) -> std::io::Result<OpRecord> {
    let name = filename(src)?;
    let dst = resolve_conflict(dst_dir, &name);
    let src_size = path_size(src);
    match fs::rename(src, &dst) {
        Ok(()) => {
            // Same filesystem rename — instant, credit all bytes
            ctx.bytes_done += src_size;
            ctx.report();
        }
        Err(ref e) if is_cross_device(e) => {
            // Cross-device: copy then remove
            if src.is_dir() {
                ctx.report();
                copy_dir_progress(src, &dst, ctx)?;
                fs::remove_dir_all(src)?;
            } else {
                fs::copy(src, &dst)?;
                ctx.bytes_done += src_size;
                ctx.report();
                fs::remove_file(src)?;
            }
        }
        Err(e) => return Err(e),
    }
    Ok(OpRecord::Moved {
        src: src.into(),
        dst,
    })
}

pub fn paste_in_background(
    paths: Vec<PathBuf>,
    dst_dir: PathBuf,
    op: RegisterOp,
    tx: mpsc::Sender<ProgressMsg>,
) {
    std::thread::spawn(move || {
        // Pre-compute total bytes
        let bytes_total: u64 = paths.iter().map(|p| path_size(p)).sum();
        let item_total = paths.len();

        let mut ctx = ProgressCtx {
            tx: tx.clone(),
            bytes_done: 0,
            bytes_total,
            item_index: 0,
            item_total,
        };

        // Send initial 0% progress
        ctx.report();

        let mut records = Vec::new();
        for (i, src) in paths.iter().enumerate() {
            ctx.item_index = i;

            let result = match op {
                RegisterOp::Yank => copy_path_progress(src, &dst_dir, &mut ctx),
                RegisterOp::Cut => {
                    if src.parent().is_some_and(|p| p == dst_dir) {
                        continue;
                    }
                    move_path_progress(src, &dst_dir, &mut ctx)
                }
            };
            match result {
                Ok(rec) => records.push(rec),
                Err(e) => {
                    let _ = tx.send(ProgressMsg::Finished {
                        records,
                        error: Some(format!("{e}")),
                        bytes_total,
                    });
                    return;
                }
            }
        }
        let _ = tx.send(ProgressMsg::Finished {
            records,
            error: None,
            bytes_total,
        });
    });
}

// --- Directory size calculation ---

pub enum DuMsg {
    Progress {
        done: usize,
        total: usize,
        current: String,
    },
    Finished {
        sizes: Vec<(PathBuf, u64)>,
    },
}

pub fn du_in_background(dirs: Vec<PathBuf>, tx: mpsc::Sender<DuMsg>) {
    std::thread::spawn(move || {
        let total = dirs.len();
        let mut sizes = Vec::new();
        for (i, dir) in dirs.iter().enumerate() {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = tx.send(DuMsg::Progress {
                done: i,
                total,
                current: name,
            });
            let size = path_size(dir);
            sizes.push((dir.clone(), size));
        }
        let _ = tx.send(DuMsg::Finished { sizes });
    });
}

/// Recursive directory stats: (total_size, file_count, dir_count).
pub fn dir_stats(p: &Path) -> (u64, usize, usize) {
    let mut size = 0u64;
    let mut files = 0usize;
    let mut dirs = 0usize;
    dir_stats_inner(p, &mut size, &mut files, &mut dirs);
    (size, files, dirs)
}

fn dir_stats_inner(p: &Path, size: &mut u64, files: &mut usize, dirs: &mut usize) {
    let rd = match fs::read_dir(p) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            *files += 1;
            *size += fs::symlink_metadata(entry.path()).map(|m| m.len()).unwrap_or(0);
        } else if ft.is_dir() {
            *dirs += 1;
            dir_stats_inner(&entry.path(), size, files, dirs);
        } else {
            *files += 1;
            *size += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
}

// --- Operations ---

pub fn mkdir(dir: &Path, name: &str) -> std::io::Result<OpRecord> {
    let path = dir.join(name);
    fs::create_dir_all(&path)?;
    Ok(OpRecord::Created { path })
}

pub fn touch(dir: &Path, name: &str) -> std::io::Result<OpRecord> {
    let path = dir.join(name);
    if path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{name} already exists"),
        ));
    }
    fs::File::create(&path)?;
    Ok(OpRecord::Created { path })
}

pub fn rename_path(path: &Path, new_name: &str) -> std::io::Result<OpRecord> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir"))?;
    let new = parent.join(new_name);
    if new.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{new_name} already exists"),
        ));
    }
    fs::rename(path, &new)?;
    Ok(OpRecord::Renamed {
        from: path.into(),
        to: new,
    })
}

// --- Undo ---

pub fn undo(records: &[OpRecord]) -> std::io::Result<String> {
    let count = records.len();
    for rec in records.iter().rev() {
        match rec {
            OpRecord::Copied { dst, .. } => remove_path(dst)?,
            OpRecord::Moved { src, dst } => {
                fs::rename(dst, src)?;
            }
            OpRecord::Created { path } => remove_path(path)?,
            OpRecord::Renamed { from, to } => {
                fs::rename(to, from)?;
            }
        }
    }
    Ok(format!("Undone {count} operation(s)"))
}

// --- chmod / chown ---

#[cfg(unix)]
pub fn chmod(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
pub fn chmod(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "chmod is not supported on this platform",
    ))
}

#[cfg(unix)]
pub fn chown(path: &Path, uid: Option<u32>, gid: Option<u32>) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let uid = uid.map(|u| u as libc::uid_t).unwrap_or(u32::MAX);
    let gid = gid.map(|g| g as libc::gid_t).unwrap_or(u32::MAX);
    let ret = unsafe { libc::chown(c_path.as_ptr(), uid, gid) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
pub fn chown(_path: &Path, _uid: Option<u32>, _gid: Option<u32>) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "chown is not supported on this platform",
    ))
}

// --- Helpers ---

fn filename(p: &Path) -> std::io::Result<String> {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no filename"))
}

fn resolve_conflict(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.into());
    let ext = Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()));
    for i in 1u32..=99999 {
        let new_name = match &ext {
            Some(e) => format!("{stem}_{i}{e}"),
            None => format!("{stem}_{i}"),
        };
        let p = dir.join(&new_name);
        if !p.exists() {
            return p;
        }
    }
    // Fallback with timestamp
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let fallback = match &ext {
        Some(e) => format!("{stem}_{ts}{e}"),
        None => format!("{stem}_{ts}"),
    };
    dir.join(fallback)
}

#[cfg(test)]
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn is_cross_device(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::CrossesDevices
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use std::sync::atomic::{AtomicU32, Ordering};
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn tmp_dir() -> PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fc_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // --- UndoStack ---

    #[test]
    fn undo_stack_push_pop() {
        let mut stack = UndoStack::new();
        assert!(stack.pop().is_none());

        stack.push(vec![OpRecord::Created {
            path: PathBuf::from("/tmp/test"),
        }]);
        assert!(stack.pop().is_some());
        assert!(stack.pop().is_none());
    }

    #[test]
    fn undo_stack_ignores_empty() {
        let mut stack = UndoStack::new();
        stack.push(vec![]);
        assert!(stack.pop().is_none());
    }

    #[test]
    fn undo_stack_max_limit() {
        let mut stack = UndoStack::new();
        for i in 0..MAX_UNDO + 20 {
            stack.push(vec![OpRecord::Created {
                path: PathBuf::from(format!("/tmp/{i}")),
            }]);
        }
        assert_eq!(stack.entries.len(), MAX_UNDO);
    }

    // --- resolve_conflict ---

    #[test]
    fn resolve_conflict_no_conflict() {
        let dir = tmp_dir();
        let result = resolve_conflict(&dir, "newfile.txt");
        assert_eq!(result, dir.join("newfile.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_conflict_with_existing() {
        let dir = tmp_dir();
        fs::write(dir.join("file.txt"), "").unwrap();
        let result = resolve_conflict(&dir, "file.txt");
        assert_eq!(result, dir.join("file_1.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_conflict_multiple() {
        let dir = tmp_dir();
        fs::write(dir.join("file.txt"), "").unwrap();
        fs::write(dir.join("file_1.txt"), "").unwrap();
        fs::write(dir.join("file_2.txt"), "").unwrap();
        let result = resolve_conflict(&dir, "file.txt");
        assert_eq!(result, dir.join("file_3.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_conflict_no_extension() {
        let dir = tmp_dir();
        fs::write(dir.join("Makefile"), "").unwrap();
        let result = resolve_conflict(&dir, "Makefile");
        assert_eq!(result, dir.join("Makefile_1"));
        let _ = fs::remove_dir_all(&dir);
    }

    // --- mkdir, touch, rename ---

    #[test]
    fn mkdir_creates_directory() {
        let dir = tmp_dir();
        let rec = mkdir(&dir, "subdir").unwrap();
        assert!(dir.join("subdir").is_dir());
        match rec {
            OpRecord::Created { path } => assert_eq!(path, dir.join("subdir")),
            _ => panic!("expected Created"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn touch_creates_file() {
        let dir = tmp_dir();
        let rec = touch(&dir, "newfile").unwrap();
        assert!(dir.join("newfile").is_file());
        match rec {
            OpRecord::Created { path } => assert_eq!(path, dir.join("newfile")),
            _ => panic!("expected Created"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn touch_rejects_existing() {
        let dir = tmp_dir();
        fs::write(dir.join("exists"), "").unwrap();
        let err = touch(&dir, "exists");
        assert!(err.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_works() {
        let dir = tmp_dir();
        fs::write(dir.join("old"), "content").unwrap();
        let rec = rename_path(&dir.join("old"), "new").unwrap();
        assert!(!dir.join("old").exists());
        assert!(dir.join("new").is_file());
        match rec {
            OpRecord::Renamed { from, to } => {
                assert_eq!(from, dir.join("old"));
                assert_eq!(to, dir.join("new"));
            }
            _ => panic!("expected Renamed"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_rejects_existing_target() {
        let dir = tmp_dir();
        fs::write(dir.join("a"), "").unwrap();
        fs::write(dir.join("b"), "").unwrap();
        let err = rename_path(&dir.join("a"), "b");
        assert!(err.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    // --- undo ---

    #[test]
    fn undo_created_file() {
        let dir = tmp_dir();
        let path = dir.join("file");
        fs::write(&path, "").unwrap();
        let records = vec![OpRecord::Created { path: path.clone() }];
        undo(&records).unwrap();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_renamed() {
        let dir = tmp_dir();
        let from = dir.join("old");
        let to = dir.join("new");
        fs::write(&to, "content").unwrap();
        let records = vec![OpRecord::Renamed {
            from: from.clone(),
            to: to.clone(),
        }];
        undo(&records).unwrap();
        assert!(!to.exists());
        assert!(from.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    // --- copy_dir ---

    #[test]
    fn copy_dir_recursive() {
        let dir = tmp_dir();
        let src = dir.join("src");
        let dst = dir.join("dst");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), "hello").unwrap();
        fs::write(src.join("sub/b.txt"), "world").unwrap();

        copy_dir(&src, &dst).unwrap();

        assert!(dst.join("a.txt").is_file());
        assert!(dst.join("sub/b.txt").is_file());
        assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "hello");
        assert_eq!(fs::read_to_string(dst.join("sub/b.txt")).unwrap(), "world");
        let _ = fs::remove_dir_all(&dir);
    }

    // --- path_size ---

    #[test]
    fn path_size_file() {
        let dir = tmp_dir();
        let file = dir.join("test.dat");
        fs::write(&file, "12345").unwrap();
        assert_eq!(path_size(&file), 5);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_size_dir() {
        let dir = tmp_dir();
        fs::write(dir.join("a"), "123").unwrap();
        fs::write(dir.join("b"), "45678").unwrap();
        assert_eq!(path_size(&dir), 8);
        let _ = fs::remove_dir_all(&dir);
    }
}
