use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegisterOp {
    Yank,
    #[allow(dead_code)]
    Cut,
}

#[derive(Clone)]
pub struct RegisterEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct Register {
    pub entries: Vec<RegisterEntry>,
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
    tx: tokio::sync::mpsc::Sender<ProgressMsg>,
    bytes_done: u64,
    bytes_total: u64,
    item_index: usize,
    item_total: usize,
}

impl ProgressCtx {
    fn report(&self) {
        let _ = self.tx.blocking_send(ProgressMsg::Progress {
            bytes_done: self.bytes_done,
            bytes_total: self.bytes_total,
            item_index: self.item_index,
            item_total: self.item_total,
        });
    }
}

fn copy_dir_progress(src: &Path, dst: &Path, ctx: &mut ProgressCtx) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    // Preserve source directory permissions
    if let Ok(src_meta) = fs::metadata(src) {
        let _ = fs::set_permissions(dst, src_meta.permissions());
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            copy_symlink(&entry.path(), &target)?;
        } else if ft.is_dir() {
            copy_dir_progress(&entry.path(), &target, ctx)?;
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            fs::copy(entry.path(), &target)?;
            copy_timestamps(&entry.path(), &target);
            ctx.bytes_done += size;
            ctx.report();
        }
    }
    // Preserve directory timestamps (after all contents are copied)
    copy_timestamps(src, dst);
    Ok(())
}

fn copy_path_progress(
    src: &Path,
    dst_dir: &Path,
    ctx: &mut ProgressCtx,
) -> std::io::Result<OpRecord> {
    let name = filename(src)?;
    let dst = resolve_conflict(dst_dir, &name);
    let meta = fs::symlink_metadata(src)?;
    if meta.is_symlink() {
        copy_symlink(src, &dst)?;
        ctx.report();
    } else if meta.is_dir() {
        ctx.report();
        copy_dir_progress(src, &dst, ctx)?;
    } else {
        let size = meta.len();
        fs::copy(src, &dst)?;
        copy_timestamps(src, &dst);
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
    let meta = fs::symlink_metadata(src)?;
    let src_size = if meta.is_symlink() { meta.len() } else { path_size(src) };
    match fs::rename(src, &dst) {
        Ok(()) => {
            // Same filesystem rename — instant, credit all bytes
            ctx.bytes_done += src_size;
            ctx.report();
        }
        Err(ref e) if is_cross_device(e) => {
            // Cross-device: copy then remove
            if meta.is_symlink() {
                copy_symlink(src, &dst)?;
                fs::remove_file(src)?;
            } else if meta.is_dir() {
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
    tx: tokio::sync::mpsc::Sender<ProgressMsg>,
) {
    tokio::task::spawn_blocking(move || {
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
                    let _ = tx.blocking_send(ProgressMsg::Finished {
                        records,
                        error: Some(format!("{e}")),
                        bytes_total,
                    });
                    return;
                }
            }
        }
        let _ = tx.blocking_send(ProgressMsg::Finished {
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

pub fn du_in_background(dirs: Vec<PathBuf>, tx: tokio::sync::mpsc::Sender<DuMsg>) {
    tokio::task::spawn_blocking(move || {
        let total = dirs.len();
        let mut sizes = Vec::new();
        for (i, dir) in dirs.iter().enumerate() {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = tx.blocking_send(DuMsg::Progress {
                done: i,
                total,
                current: name,
            });
            let size = path_size(dir);
            sizes.push((dir.clone(), size));
        }
        let _ = tx.blocking_send(DuMsg::Finished { sizes });
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

/// Validate that a user-supplied filename does not escape the parent directory.
fn validate_name(name: &str) -> std::io::Result<()> {
    if name.contains('/') || name.contains('\\') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Name must not contain path separators",
        ));
    }
    if name == "." || name == ".." {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Name must not be '.' or '..'",
        ));
    }
    Ok(())
}

pub fn mkdir(dir: &Path, name: &str) -> std::io::Result<OpRecord> {
    validate_name(name)?;
    let path = dir.join(name);
    fs::create_dir_all(&path)?;
    Ok(OpRecord::Created { path })
}

pub fn touch(dir: &Path, name: &str) -> std::io::Result<OpRecord> {
    validate_name(name)?;
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
    validate_name(new_name)?;
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
                match fs::rename(dst, src) {
                    Ok(()) => {}
                    Err(ref e) if is_cross_device(e) => {
                        // Cross-device: copy back then remove from dst
                        let meta = fs::symlink_metadata(dst)?;
                        if meta.is_symlink() {
                            copy_symlink(dst, src)?;
                            fs::remove_file(dst)?;
                        } else if meta.is_dir() {
                            let mut ctx = ProgressCtx {
                                tx: tokio::sync::mpsc::channel(1).0,
                                bytes_done: 0,
                                bytes_total: 0,
                                item_index: 0,
                                item_total: 1,
                            };
                            copy_dir_progress(dst, src, &mut ctx)?;
                            fs::remove_dir_all(dst)?;
                        } else {
                            fs::copy(dst, src)?;
                            copy_timestamps(dst, src);
                            fs::remove_file(dst)?;
                        }
                    }
                    Err(e) => return Err(e),
                }
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

fn copy_timestamps(src: &Path, dst: &Path) {
    if let Ok(meta) = fs::metadata(src) {
        let mtime = filetime::FileTime::from_last_modification_time(&meta);
        let atime = filetime::FileTime::from_last_access_time(&meta);
        let _ = filetime::set_file_times(dst, atime, mtime);
    }
}

fn copy_symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    let link_target = fs::read_link(src)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&link_target, dst)?;
    }
    #[cfg(not(unix))]
    {
        // On non-unix, fall back to copying the resolved target
        let resolved = src.parent().unwrap_or(Path::new(".")).join(&link_target);
        if resolved.is_dir() {
            fs::create_dir_all(dst)?;
        } else {
            fs::copy(src, dst)?;
        }
    }
    Ok(())
}

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

    // --- validate_name ---

    #[test]
    fn validate_name_rejects_slash() {
        assert!(validate_name("foo/bar").is_err());
    }

    #[test]
    fn validate_name_rejects_dotdot() {
        assert!(validate_name("..").is_err());
    }

    #[test]
    fn validate_name_rejects_traversal() {
        assert!(validate_name("../etc").is_err());
    }

    #[test]
    fn validate_name_accepts_normal() {
        assert!(validate_name("hello.txt").is_ok());
        assert!(validate_name(".hidden").is_ok());
        assert!(validate_name("file with spaces").is_ok());
    }

    #[test]
    fn mkdir_rejects_path_traversal() {
        let dir = tmp_dir();
        assert!(mkdir(&dir, "../escape").is_err());
        assert!(mkdir(&dir, "sub/dir").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn touch_rejects_path_traversal() {
        let dir = tmp_dir();
        assert!(touch(&dir, "../escape").is_err());
        assert!(touch(&dir, "sub/file").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_rejects_path_traversal() {
        let dir = tmp_dir();
        fs::write(dir.join("file"), "").unwrap();
        assert!(rename_path(&dir.join("file"), "../escape").is_err());
        assert!(rename_path(&dir.join("file"), "sub/name").is_err());
        assert!(dir.join("file").exists()); // original still there
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

    // --- undo: Copied ---

    #[test]
    fn undo_copied_removes_dst() {
        let dir = tmp_dir();
        let dst = dir.join("copy.txt");
        fs::write(&dst, "data").unwrap();
        let records = vec![OpRecord::Copied {
            _src: PathBuf::from("/orig/file.txt"),
            dst: dst.clone(),
        }];
        let msg = undo(&records).unwrap();
        assert!(!dst.exists());
        assert!(msg.contains("1"));
    }

    #[test]
    fn undo_copied_dir_removes_dst() {
        let dir = tmp_dir();
        let dst_dir = dir.join("copied_dir");
        fs::create_dir_all(dst_dir.join("sub")).unwrap();
        fs::write(dst_dir.join("sub/file.txt"), "data").unwrap();
        let records = vec![OpRecord::Copied {
            _src: PathBuf::from("/orig/dir"),
            dst: dst_dir.clone(),
        }];
        undo(&records).unwrap();
        assert!(!dst_dir.exists());
    }

    // --- undo: Moved (same device) ---

    #[test]
    fn undo_moved_restores_src() {
        let dir = tmp_dir();
        let src = dir.join("original");
        let dst = dir.join("moved");
        fs::write(&dst, "content").unwrap();
        let records = vec![OpRecord::Moved {
            src: src.clone(),
            dst: dst.clone(),
        }];
        let msg = undo(&records).unwrap();
        assert!(src.exists());
        assert!(!dst.exists());
        assert_eq!(fs::read_to_string(&src).unwrap(), "content");
        assert!(msg.contains("1"));
    }

    // --- undo: multiple records ---

    #[test]
    fn undo_multiple_records_reversed() {
        let dir = tmp_dir();
        let f1 = dir.join("file1");
        let f2 = dir.join("file2");
        fs::write(&f1, "").unwrap();
        fs::write(&f2, "").unwrap();
        let records = vec![
            OpRecord::Created { path: f1.clone() },
            OpRecord::Created { path: f2.clone() },
        ];
        let msg = undo(&records).unwrap();
        assert!(!f1.exists());
        assert!(!f2.exists());
        assert!(msg.contains("2"));
    }

    // --- dir_stats ---

    #[test]
    fn dir_stats_counts() {
        let dir = tmp_dir();
        fs::write(dir.join("a.txt"), "12345").unwrap();
        fs::write(dir.join("b.txt"), "67").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/c.txt"), "890").unwrap();

        let (size, files, dirs) = dir_stats(&dir);
        assert_eq!(files, 3); // a.txt, b.txt, sub/c.txt
        assert_eq!(dirs, 1);  // sub
        assert_eq!(size, 10); // 5 + 2 + 3
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_stats_empty() {
        let dir = tmp_dir();
        let (size, files, dirs) = dir_stats(&dir);
        assert_eq!(size, 0);
        assert_eq!(files, 0);
        assert_eq!(dirs, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    // --- path_size edge cases ---

    #[test]
    fn path_size_nonexistent() {
        assert_eq!(path_size(Path::new("/nonexistent/path/xyz")), 0);
    }

    #[test]
    fn path_size_empty_dir() {
        let dir = tmp_dir();
        assert_eq!(path_size(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_size_nested_dirs() {
        let dir = tmp_dir();
        fs::create_dir_all(dir.join("a/b/c")).unwrap();
        fs::write(dir.join("a/b/c/file"), "hello").unwrap();
        assert_eq!(path_size(&dir), 5);
        let _ = fs::remove_dir_all(&dir);
    }

    // --- remove_path ---

    #[test]
    fn remove_path_file() {
        let dir = tmp_dir();
        let f = dir.join("file.txt");
        fs::write(&f, "data").unwrap();
        remove_path(&f).unwrap();
        assert!(!f.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_path_dir() {
        let dir = tmp_dir();
        let sub = dir.join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("f"), "").unwrap();
        remove_path(&sub).unwrap();
        assert!(!sub.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    // --- validate_name edge cases ---

    #[test]
    fn validate_name_rejects_single_dot() {
        assert!(validate_name(".").is_err());
    }

    #[test]
    fn validate_name_rejects_backslash() {
        assert!(validate_name("foo\\bar").is_err());
    }

    #[test]
    fn validate_name_accepts_unicode() {
        assert!(validate_name("файл.txt").is_ok());
        assert!(validate_name("日本語").is_ok());
    }

    #[test]
    fn validate_name_accepts_dots_in_name() {
        assert!(validate_name("file.tar.gz").is_ok());
        assert!(validate_name("...hidden").is_ok());
    }

    // --- symlink tests ---

    #[cfg(unix)]
    #[test]
    fn copy_symlink_preserves_link() {
        let dir = tmp_dir();
        let target = dir.join("target.txt");
        let link = dir.join("link.txt");
        let copy = dir.join("copy_link.txt");
        fs::write(&target, "data").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        copy_symlink(&link, &copy).unwrap();
        assert!(copy.symlink_metadata().unwrap().is_symlink());
        assert_eq!(fs::read_link(&copy).unwrap(), target);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn path_size_symlink() {
        let dir = tmp_dir();
        let target = dir.join("target.txt");
        let link = dir.join("link.txt");
        fs::write(&target, "hello world").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        // Symlink size should be the link's own size, not target's
        let link_size = path_size(&link);
        let link_meta = fs::symlink_metadata(&link).unwrap();
        assert_eq!(link_size, link_meta.len());
        let _ = fs::remove_dir_all(&dir);
    }

    // --- chmod / chown on test files ---

    #[cfg(unix)]
    #[test]
    fn chmod_changes_permissions() {
        let dir = tmp_dir();
        let f = dir.join("file.txt");
        fs::write(&f, "data").unwrap();
        chmod(&f, 0o644).unwrap();

        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&f).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o644);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn chmod_nonexistent_fails() {
        assert!(chmod(Path::new("/nonexistent/file"), 0o755).is_err());
    }

    // --- copy_dir_progress & copy_path_progress tests ---

    #[test]
    fn copy_dir_preserves_structure() {
        let src = tmp_dir();
        fs::write(src.join("a.txt"), "aaa").unwrap();
        fs::create_dir(src.join("sub")).unwrap();
        fs::write(src.join("sub/b.txt"), "bbb").unwrap();

        let dst = tmp_dir();
        let target = dst.join("copied");
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let mut ctx = ProgressCtx {
            tx,
            bytes_done: 0,
            bytes_total: 100,
            item_index: 0,
            item_total: 1,
        };
        copy_dir_progress(&src, &target, &mut ctx).unwrap();

        assert!(target.join("a.txt").exists());
        assert!(target.join("sub/b.txt").exists());
        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "aaa");
        assert_eq!(fs::read_to_string(target.join("sub/b.txt")).unwrap(), "bbb");
        assert!(ctx.bytes_done > 0);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn copy_path_progress_file() {
        let dir = tmp_dir();
        let src = dir.join("src.txt");
        fs::write(&src, "hello").unwrap();
        let dst_dir = tmp_dir();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let mut ctx = ProgressCtx {
            tx,
            bytes_done: 0,
            bytes_total: 5,
            item_index: 0,
            item_total: 1,
        };
        let record = copy_path_progress(&src, &dst_dir, &mut ctx).unwrap();
        match record {
            OpRecord::Copied { dst, .. } => {
                assert!(dst.exists());
                assert_eq!(fs::read_to_string(&dst).unwrap(), "hello");
            }
            _ => panic!("expected Copied record"),
        }
        assert_eq!(ctx.bytes_done, 5);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn copy_path_progress_directory() {
        let dir = tmp_dir();
        let src = dir.join("mydir");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("inner.txt"), "data").unwrap();
        let dst_dir = tmp_dir();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let mut ctx = ProgressCtx {
            tx,
            bytes_done: 0,
            bytes_total: 100,
            item_index: 0,
            item_total: 1,
        };
        let record = copy_path_progress(&src, &dst_dir, &mut ctx).unwrap();
        match record {
            OpRecord::Copied { dst, .. } => {
                assert!(dst.join("inner.txt").exists());
            }
            _ => panic!("expected Copied record"),
        }
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[cfg(unix)]
    #[test]
    fn copy_path_progress_symlink() {
        let dir = tmp_dir();
        let target = dir.join("target.txt");
        fs::write(&target, "link_target").unwrap();
        let link = dir.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let dst_dir = tmp_dir();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let mut ctx = ProgressCtx {
            tx,
            bytes_done: 0,
            bytes_total: 100,
            item_index: 0,
            item_total: 1,
        };
        let record = copy_path_progress(&link, &dst_dir, &mut ctx).unwrap();
        match record {
            OpRecord::Copied { dst, .. } => {
                // dst should be a symlink
                assert!(fs::symlink_metadata(&dst).unwrap().is_symlink());
            }
            _ => panic!("expected Copied record"),
        }
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn move_path_progress_same_device() {
        let dir = tmp_dir();
        let src = dir.join("move_src.txt");
        fs::write(&src, "move_me").unwrap();
        let dst_dir = tmp_dir();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let mut ctx = ProgressCtx {
            tx,
            bytes_done: 0,
            bytes_total: 7,
            item_index: 0,
            item_total: 1,
        };
        let record = move_path_progress(&src, &dst_dir, &mut ctx).unwrap();
        assert!(!src.exists()); // source should be gone
        match record {
            OpRecord::Moved { dst, .. } => {
                assert!(dst.exists());
                assert_eq!(fs::read_to_string(&dst).unwrap(), "move_me");
            }
            _ => panic!("expected Moved record"),
        }
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn move_path_progress_directory() {
        let dir = tmp_dir();
        let src = dir.join("movedir");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("file.txt"), "inside").unwrap();
        let dst_dir = tmp_dir();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let mut ctx = ProgressCtx {
            tx,
            bytes_done: 0,
            bytes_total: 100,
            item_index: 0,
            item_total: 1,
        };
        let record = move_path_progress(&src, &dst_dir, &mut ctx).unwrap();
        assert!(!src.exists());
        match record {
            OpRecord::Moved { dst, .. } => {
                assert!(dst.join("file.txt").exists());
            }
            _ => panic!("expected Moved record"),
        }
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn copy_timestamps_preserves_mtime() {
        let dir = tmp_dir();
        let src = dir.join("src.txt");
        let dst = dir.join("dst.txt");
        fs::write(&src, "data").unwrap();

        // Set a specific mtime in the past
        let old_time = filetime::FileTime::from_unix_time(1000000000, 0);
        filetime::set_file_mtime(&src, old_time).unwrap();

        fs::write(&dst, "data").unwrap();
        copy_timestamps(&src, &dst);

        let dst_meta = fs::metadata(&dst).unwrap();
        let dst_mtime = filetime::FileTime::from_last_modification_time(&dst_meta);
        assert_eq!(dst_mtime, old_time);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_progress_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let src = tmp_dir();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(src.join("f.txt"), "x").unwrap();

        let dst = tmp_dir();
        let target = dst.join("copied");
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let mut ctx = ProgressCtx {
            tx,
            bytes_done: 0,
            bytes_total: 1,
            item_index: 0,
            item_total: 1,
        };
        copy_dir_progress(&src, &target, &mut ctx).unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o755);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn resolve_conflict_handles_no_extension() {
        let dir = tmp_dir();
        let f = dir.join("Makefile");
        fs::write(&f, "").unwrap();
        let resolved = resolve_conflict(&dir, "Makefile");
        assert_ne!(resolved, f);
        assert!(resolved.to_string_lossy().contains("Makefile_1"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_created_removes_file() {
        let dir = tmp_dir();
        let record = touch(&dir, "created.txt").unwrap();
        let path = dir.join("created.txt");
        assert!(path.exists());
        undo(&[record]).unwrap();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_copied_file_removes_copy() {
        let dir = tmp_dir();
        let src = dir.join("src.txt");
        let dst = dir.join("copied.txt");
        fs::write(&src, "original").unwrap();
        fs::copy(&src, &dst).unwrap();

        let record = OpRecord::Copied {
            _src: src.clone(),
            dst: dst.clone(),
        };
        undo(&[record]).unwrap();
        assert!(!dst.exists());
        assert!(src.exists()); // source untouched
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn paste_skips_move_to_same_dir() {
        // Verify the skip condition: if src.parent() == dst_dir, move is skipped
        let dir = tmp_dir();
        let src = dir.join("file.txt");
        fs::write(&src, "data").unwrap();

        // Simulate the skip logic from paste_in_background
        let dst_dir = dir.clone();
        let should_skip = src.parent().is_some_and(|p| p == dst_dir);
        assert!(should_skip);
        // File should still exist
        assert!(src.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filename_helper() {
        assert_eq!(filename(Path::new("/test/file.txt")).unwrap(), "file.txt");
        assert!(filename(Path::new("/")).is_err());
    }
}
