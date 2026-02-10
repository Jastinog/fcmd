use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RegisterOp {
    Yank,
    Cut,
}

pub struct Register {
    pub paths: Vec<PathBuf>,
    pub op: RegisterOp,
}

pub enum OpRecord {
    Copied { _src: PathBuf, dst: PathBuf },
    Moved { src: PathBuf, dst: PathBuf },
    Deleted { original: PathBuf, trash: PathBuf },
    Created { path: PathBuf },
    Renamed { from: PathBuf, to: PathBuf },
}

pub struct UndoStack {
    entries: Vec<Vec<OpRecord>>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn push(&mut self, records: Vec<OpRecord>) {
        if !records.is_empty() {
            self.entries.push(records);
        }
    }

    pub fn pop(&mut self) -> Option<Vec<OpRecord>> {
        self.entries.pop()
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
fn path_size(p: &Path) -> u64 {
    if p.is_dir() {
        fs::read_dir(p)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| path_size(&e.path()))
            .sum()
    } else {
        fs::metadata(p).map(|m| m.len()).unwrap_or(0)
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
    if fs::rename(src, &dst).is_ok() {
        // Same filesystem rename — instant, credit all bytes
        ctx.bytes_done += src_size;
        ctx.report();
    } else {
        // Cross-filesystem: copy then remove
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
                    if src.parent().map_or(false, |p| p == dst_dir) {
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

// --- Operations ---

pub fn delete_path(path: &Path) -> std::io::Result<OpRecord> {
    let trash = trash_dir()?;
    let name = filename(path)?;
    let trash_path = unique_trash_name(&trash, &name);
    if fs::rename(path, &trash_path).is_err() {
        if path.is_dir() {
            copy_dir(path, &trash_path)?;
            fs::remove_dir_all(path)?;
        } else {
            fs::copy(path, &trash_path)?;
            fs::remove_file(path)?;
        }
    }
    Ok(OpRecord::Deleted {
        original: path.into(),
        trash: trash_path,
    })
}

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
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir")
    })?;
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
            OpRecord::Deleted { original, trash } => {
                fs::rename(trash, original)?;
            }
            OpRecord::Created { path } => remove_path(path)?,
            OpRecord::Renamed { from, to } => {
                fs::rename(to, from)?;
            }
        }
    }
    Ok(format!("Undone {count} operation(s)"))
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
    for i in 1u32.. {
        let new_name = match &ext {
            Some(e) => format!("{stem}_{i}{e}"),
            None => format!("{stem}_{i}"),
        };
        let p = dir.join(&new_name);
        if !p.exists() {
            return p;
        }
    }
    unreachable!()
}

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

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn trash_dir() -> std::io::Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME not set"))?;
    let dir = PathBuf::from(home).join(".local/share/fc/trash");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn unique_trash_name(trash: &Path, name: &str) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    trash.join(format!("{ts}_{name}"))
}
