//! Session-scoped trash tracking and restore.
//!
//! `dd` moves files to the system trash via the `trash` crate. The crate can
//! enumerate and restore the trash on Linux/BSD (Freedesktop) and Windows, but
//! **not** on macOS, where it only exposes `delete`. To offer a uniform
//! "restore what I just trashed" experience on every platform, we capture a
//! restorable handle at deletion time:
//!
//! - **Freedesktop / Windows**: after deleting, look up the matching
//!   [`trash::TrashItem`] and keep it; restore goes through `os_limited`.
//! - **macOS**: snapshot `~/.Trash` around the deletion to learn where the OS
//!   put the file, then move it back ourselves on restore.
//!
//! Handles are only meaningful for the current session and only while the
//! trashed copy still exists (it may be emptied or restored by another tool).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// A single item moved to the system trash this session, with enough
/// information to put it back where it came from.
#[derive(Debug, Clone)]
pub struct TrashedItem {
    /// Stable, session-unique identifier used by the UI to target a restore.
    pub id: u64,
    /// The location the item lived at before it was trashed.
    pub original_path: PathBuf,
    #[cfg(target_os = "macos")]
    location: PathBuf,
    #[cfg(trash_os_limited)]
    item: trash::TrashItem,
}

#[cfg(test)]
impl TrashedItem {
    /// Build a fake handle for UI/navigation tests. Restoring it fails (no real
    /// trashed copy backs it), which is irrelevant to those tests.
    pub fn new_for_test(original_path: PathBuf) -> Self {
        TrashedItem {
            id: next_id(),
            original_path,
            #[cfg(target_os = "macos")]
            location: PathBuf::from("/nonexistent/fcmd-test-trash"),
            #[cfg(trash_os_limited)]
            item: trash::TrashItem {
                id: std::ffi::OsString::new(),
                name: std::ffi::OsString::new(),
                original_parent: PathBuf::new(),
                time_deleted: 0,
            },
        }
    }
}

impl TrashedItem {
    /// The item's file name, for display.
    pub fn name(&self) -> String {
        self.original_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.original_path.to_string_lossy().into_owned())
    }
}

/// Move `path` to the system trash, returning a restorable handle when the
/// platform supports it (`Ok(None)` means the file was trashed but cannot be
/// tracked for restore).
pub fn trash(path: &Path) -> std::io::Result<Option<TrashedItem>> {
    trash_impl(path)
}

/// Restore a previously trashed item to its original location. Fails if the
/// original path is now occupied or the trashed copy has gone away.
pub fn restore(item: &TrashedItem) -> std::io::Result<()> {
    if item.original_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists", item.original_path.display()),
        ));
    }
    restore_impl(item)
}

// --- macOS: snapshot ~/.Trash, move the file back ourselves ---

#[cfg(target_os = "macos")]
fn trash_impl(path: &Path) -> std::io::Result<Option<TrashedItem>> {
    let original_path = path.to_path_buf();
    let trash_dir = dirs::home_dir().map(|h| h.join(".Trash"));
    let before = trash_dir.as_deref().map(snapshot).unwrap_or_default();
    trash::delete(path).map_err(std::io::Error::other)?;
    let location = trash_dir
        .as_deref()
        .and_then(|dir| added_entry(dir, &before, path.file_name().unwrap_or_default()));
    Ok(location.map(|location| TrashedItem {
        id: next_id(),
        original_path,
        location,
    }))
}

#[cfg(target_os = "macos")]
fn restore_impl(item: &TrashedItem) -> std::io::Result<()> {
    if !item.location.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "item is no longer in the trash",
        ));
    }
    if let Some(parent) = item.original_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    move_back(&item.location, &item.original_path)
}

/// Set of entry names currently in `dir` (empty if it cannot be read).
#[cfg(target_os = "macos")]
fn snapshot(dir: &Path) -> std::collections::HashSet<std::ffi::OsString> {
    let mut set = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            set.insert(entry.file_name());
        }
    }
    set
}

/// The entry that appeared in `dir` for the file we just trashed (`orig_name`).
///
/// Candidates are entries not present in `before`. The OS keeps the original
/// name, or appends a suffix on a name clash (e.g. `report 2.txt`), so we rank
/// by how well the name matches before falling back to recency — this avoids
/// mistaking a sidecar the OS may create alongside (e.g. `.DS_Store`) for the
/// trashed file.
#[cfg(target_os = "macos")]
fn added_entry(
    dir: &Path,
    before: &std::collections::HashSet<std::ffi::OsString>,
    orig_name: &std::ffi::OsStr,
) -> Option<PathBuf> {
    let stem = Path::new(orig_name)
        .file_stem()
        .unwrap_or(orig_name)
        .to_string_lossy()
        .into_owned();
    let rd = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(u8, std::time::SystemTime, PathBuf)> = None;
    for entry in rd.flatten() {
        let name = entry.file_name();
        if before.contains(&name) {
            continue;
        }
        // 2 = exact name, 1 = same stem (suffixed on clash), 0 = unrelated.
        let rank = if name == orig_name {
            2
        } else if name.to_string_lossy().starts_with(&stem) {
            1
        } else {
            0
        };
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if best
            .as_ref()
            .is_none_or(|(r, t, _)| (rank, mtime) >= (*r, *t))
        {
            best = Some((rank, mtime, entry.path()));
        }
    }
    best.map(|(_, _, p)| p)
}

/// Move `from` to `to`, falling back to a recursive copy across devices.
#[cfg(target_os = "macos")]
fn move_back(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_recursive(from, to)?;
            let meta = std::fs::symlink_metadata(from)?;
            if meta.is_dir() {
                std::fs::remove_dir_all(from)
            } else {
                std::fs::remove_file(from)
            }
        }
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "macos")]
fn copy_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.is_symlink() {
        let target = std::fs::read_link(from)?;
        std::os::unix::fs::symlink(target, to)
    } else if meta.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)?.flatten() {
            copy_recursive(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

// --- Freedesktop / Windows: handles via the `trash` crate's os_limited API ---

#[cfg(trash_os_limited)]
fn trash_impl(path: &Path) -> std::io::Result<Option<TrashedItem>> {
    let original_path = path.to_path_buf();
    trash::delete(path).map_err(std::io::Error::other)?;
    // Find the entry we just created: same original path, most recent deletion.
    let item = trash::os_limited::list()
        .map_err(std::io::Error::other)?
        .into_iter()
        .filter(|it| it.original_path() == original_path)
        .max_by_key(|it| it.time_deleted);
    Ok(item.map(|item| TrashedItem {
        id: next_id(),
        original_path,
        item,
    }))
}

#[cfg(trash_os_limited)]
fn restore_impl(item: &TrashedItem) -> std::io::Result<()> {
    trash::os_limited::restore_all([item.item.clone()]).map_err(std::io::Error::other)
}

// --- Other platforms (e.g. iOS/Android): trash works, restore does not ---

#[cfg(not(any(target_os = "macos", trash_os_limited)))]
fn trash_impl(path: &Path) -> std::io::Result<Option<TrashedItem>> {
    trash::delete(path).map_err(std::io::Error::other)?;
    Ok(None)
}

#[cfg(not(any(target_os = "macos", trash_os_limited)))]
fn restore_impl(_item: &TrashedItem) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "restoring from trash is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full round-trip against the real OS trash. Ignored by default because it
    /// moves a file to the system Trash; run with `--ignored` to validate the
    /// platform-specific tracking/restore path.
    #[test]
    #[ignore = "moves a real file to the system trash"]
    fn trash_and_restore_roundtrip() {
        let dir = dirs::home_dir().expect("home dir");
        let path = dir.join(".fcmd_trash_roundtrip_test.txt");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, b"restore me").unwrap();

        let item = trash(&path).unwrap().expect("item should be tracked");
        assert!(!path.exists(), "file should be gone after trashing");

        restore(&item).unwrap();
        assert!(path.exists(), "file should be back after restore");
        assert_eq!(std::fs::read(&path).unwrap(), b"restore me");

        std::fs::remove_file(&path).unwrap();
    }
}
