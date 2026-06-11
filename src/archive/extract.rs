//! Listing and extraction of archives (zip / tar / tar.gz / tar.bz2 / tar.xz).

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use super::{
    ArchiveEntry, ArchiveFormat, ConflictDecision, ConflictFn, ExtractOutcome, ProgressFn,
    copy_with_cancel, never_cancel,
};

/// List all entries in an archive. Returns a flat list sorted by path.
pub fn list_archive(path: &Path) -> io::Result<(ArchiveFormat, Vec<ArchiveEntry>)> {
    let format = ArchiveFormat::from_path(path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown archive format"))?;

    let mut entries = match format {
        ArchiveFormat::Zip => list_zip(path)?,
        ArchiveFormat::Tar => list_tar(path, TarCompression::None)?,
        ArchiveFormat::TarGz => list_tar(path, TarCompression::Gz)?,
        ArchiveFormat::TarBz2 => list_tar(path, TarCompression::Bz2)?,
        ArchiveFormat::TarXz => list_tar(path, TarCompression::Xz)?,
    };

    // Synthesize missing directory entries
    let existing_dirs: HashSet<String> = entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path.clone())
        .collect();

    let mut dirs_to_add = HashSet::new();
    for entry in &entries {
        let mut current = entry.path.as_str();
        // Strip trailing slash for directory entries
        if current.ends_with('/') {
            current = &current[..current.len() - 1];
        }
        while let Some(pos) = current.rfind('/') {
            let dir_path = format!("{}/", &current[..pos]);
            if existing_dirs.contains(&dir_path) || dirs_to_add.contains(&dir_path) {
                break;
            }
            dirs_to_add.insert(dir_path);
            current = &current[..pos];
        }
    }

    for dir_path in dirs_to_add {
        entries.push(ArchiveEntry {
            path: dir_path,
            size: 0,
            is_dir: true,
            modified: None,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((format, entries))
}

/// Extract a single entry (file or directory) from an archive.
///
/// Convenience wrapper: no progress, overwrites existing files, never cancels.
/// Use [`extract_stream`] to drive progress/conflict/cancel.
#[allow(dead_code)]
pub fn extract_entry(
    archive_path: &Path,
    entry_path: &str,
    dest: &Path,
) -> io::Result<Vec<PathBuf>> {
    let cancel = never_cancel();
    let outcome = extract_stream(
        archive_path,
        Some(entry_path),
        dest,
        0,
        &mut |_, _, _| {},
        &mut |_, _, _, _| ConflictDecision::Overwrite,
        &cancel,
    )?;
    Ok(outcome.extracted)
}

/// Extract all entries from an archive.
///
/// Convenience wrapper: no progress, overwrites existing files, never cancels.
#[allow(dead_code)]
pub fn extract_all(archive_path: &Path, dest: &Path) -> io::Result<Vec<PathBuf>> {
    let cancel = never_cancel();
    let outcome = extract_stream(
        archive_path,
        None,
        dest,
        0,
        &mut |_, _, _| {},
        &mut |_, _, _, _| ConflictDecision::Overwrite,
        &cancel,
    )?;
    Ok(outcome.extracted)
}

/// The data inputs to an extraction (everything that isn't a callback or the
/// cancel flag), bundled so the worker functions take a handful of args.
struct ExtractRequest<'a> {
    archive_path: &'a Path,
    /// Selects which entries to extract: `None` extracts everything, `Some(path)`
    /// extracts a single file (exact match) or a directory subtree (trailing `/`).
    filter: Option<&'a str>,
    dest: &'a Path,
    /// Best-effort hint for the progress denominator (0 if unknown).
    total: usize,
}

/// Extract entries, streaming progress and routing per-file overwrite conflicts.
///
/// See [`ExtractRequest`] for the data inputs. Cancellation is honoured between
/// entries and within a single large file (`cancel`).
pub fn extract_stream(
    archive_path: &Path,
    filter: Option<&str>,
    dest: &Path,
    total: usize,
    on_progress: &mut ProgressFn,
    resolve: &mut ConflictFn,
    cancel: &AtomicBool,
) -> io::Result<ExtractOutcome> {
    let format = ArchiveFormat::from_path(archive_path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown archive format"))?;

    let req = ExtractRequest {
        archive_path,
        filter,
        dest,
        total,
    };
    match format {
        ArchiveFormat::Zip => extract_zip_stream(&req, on_progress, resolve, cancel),
        _ => extract_tar_stream(&req, tar_compression(format), on_progress, resolve, cancel),
    }
}

/// Whether an entry named `name` is selected by `filter`.
fn entry_matches(filter: Option<&str>, name: &str) -> bool {
    match filter {
        None => true,
        Some(f) if f.ends_with('/') => name.starts_with(f),
        Some(f) => name == f,
    }
}

// ── Unix permission / symlink helpers ────────────────────────────────
//
// Gated behind `#[cfg(unix)]`; on other platforms mode bits and symlinks are
// simply not reproduced (the previous behaviour).

/// Whether a unix mode word describes a symlink (`S_IFLNK`).
#[cfg(unix)]
fn is_symlink_mode(mode: u32) -> bool {
    mode & 0o170000 == 0o120000
}

/// Apply unix permission bits to an already-written path.
#[cfg(unix)]
fn apply_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // Mask to the permission bits; the type bits (S_IFREG etc.) are not settable.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777))
}

/// (Re)create a symlink at `link` pointing at `target`, replacing whatever is
/// already there so the underlying `symlink(2)` does not fail with `EEXIST`.
#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(target, link)
}

// ── Symlink-slip defence ─────────────────────────────────────────────
//
// Sanitizing the entry *name* keeps `out_path` lexically under `dest`, but an
// attacker can still escape by planting a symlink and then writing *through* it
// (entry A: symlink `foo` → `/etc`; entry B: regular file `foo/evil`). A
// pre-existing symlink in `dest` is the same hazard. We therefore never let a
// write descend through a symlinked path component.

/// Create every directory component of `dir` under `dest`, refusing to descend
/// through any component that already exists as a symlink (or as a non-dir).
/// Returns `Ok(false)` — caller skips the entry — if descent would leave `dest`.
/// `dir` must already be `dest`-joined and lexically sanitized.
fn ensure_dir_within(dest: &Path, dir: &Path) -> io::Result<bool> {
    let Ok(rel) = dir.strip_prefix(dest) else {
        return Ok(false);
    };
    let mut cur = dest.to_path_buf();
    for comp in rel.components() {
        cur.push(comp);
        match std::fs::symlink_metadata(&cur) {
            Ok(meta) => {
                let ft = meta.file_type();
                if ft.is_symlink() || !ft.is_dir() {
                    // Descending here would follow a symlink (or clobber a file)
                    // and could redirect the write outside `dest`.
                    return Ok(false);
                }
            }
            Err(_) => std::fs::create_dir(&cur)?,
        }
    }
    Ok(true)
}

/// Remove `path` if it is itself a symlink, so a following `File::create` writes
/// a fresh regular file instead of being redirected through the link.
fn remove_if_symlink(path: &Path) {
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        let _ = std::fs::remove_file(path);
    }
}

// ── Zip ──────────────────────────────────────────────────────────────

fn list_zip(path: &Path) -> io::Result<Vec<ArchiveEntry>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut entries = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let name = entry.name().to_string();
        let is_dir = entry.is_dir();
        let size = entry.size();
        let modified = zip_datetime_to_systemtime(entry.last_modified());
        entries.push(ArchiveEntry {
            path: name,
            size,
            is_dir,
            modified,
        });
    }
    Ok(entries)
}

/// Convert a zip `DateTime` (stored in local time, but treated as UTC here —
/// zip carries no zone) to a `SystemTime`. Used for display and conflict
/// prompts, so an exact zone is unnecessary; correctness across leap years is.
fn zip_datetime_to_systemtime(dt: Option<zip::DateTime>) -> Option<SystemTime> {
    use chrono::{TimeZone, Utc};
    let dt = dt?;
    let naive = Utc
        .with_ymd_and_hms(
            dt.year() as i32,
            dt.month() as u32,
            dt.day() as u32,
            dt.hour() as u32,
            dt.minute() as u32,
            dt.second() as u32,
        )
        .single()?;
    Some(SystemTime::from(naive))
}

fn extract_zip_stream(
    req: &ExtractRequest,
    on_progress: &mut ProgressFn,
    resolve: &mut ConflictFn,
    cancel: &AtomicBool,
) -> io::Result<ExtractOutcome> {
    let ExtractRequest {
        archive_path,
        filter,
        dest,
        total,
    } = *req;
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut outcome = ExtractOutcome::default();
    let mut done = 0usize;

    for i in 0..archive.len() {
        // Honour cancellation between entries; an in-flight file is not interrupted.
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }

        let mut entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let name = entry.name().to_string();
        if !entry_matches(filter, &name) {
            continue;
        }

        // Sanitize before joining: `name` is the raw archive path and may contain
        // `..` components. Path::starts_with is lexical and would NOT catch `..`,
        // so we must strip traversal components first (mirrors the tar path).
        let safe_name = sanitize_archive_path(&name);
        let out_path = dest.join(&safe_name);
        if !out_path.starts_with(dest) {
            continue;
        }

        on_progress(done, total, &name);

        if entry.is_dir() {
            if !ensure_dir_within(dest, &out_path)? {
                continue;
            }
        } else {
            if let Some(parent) = out_path.parent()
                && !ensure_dir_within(dest, parent)?
            {
                continue;
            }
            if out_path.exists() {
                let modified = zip_datetime_to_systemtime(entry.last_modified());
                match resolve(&name, &out_path, entry.size(), modified) {
                    ConflictDecision::Overwrite => {}
                    ConflictDecision::Skip => {
                        outcome.skipped += 1;
                        done += 1;
                        continue;
                    }
                    ConflictDecision::Abort => {
                        outcome.cancelled = true;
                        break;
                    }
                }
            }

            #[cfg(unix)]
            {
                let mode = entry.unix_mode();
                if mode.map(is_symlink_mode).unwrap_or(false) {
                    // zip stores the link target as the entry's file content.
                    let mut target = String::new();
                    entry.read_to_string(&mut target)?;
                    create_symlink(Path::new(&target), &out_path)?;
                } else {
                    // Never write through an existing symlink at this path.
                    remove_if_symlink(&out_path);
                    let mut out_file = File::create(&out_path)?;
                    if !copy_with_cancel(&mut entry, &mut out_file, cancel)? {
                        drop(out_file);
                        let _ = std::fs::remove_file(&out_path);
                        outcome.cancelled = true;
                        break;
                    }
                    if let Some(m) = mode {
                        apply_mode(&out_path, m)?;
                    }
                }
            }
            #[cfg(not(unix))]
            {
                remove_if_symlink(&out_path);
                let mut out_file = File::create(&out_path)?;
                if !copy_with_cancel(&mut entry, &mut out_file, cancel)? {
                    drop(out_file);
                    let _ = std::fs::remove_file(&out_path);
                    outcome.cancelled = true;
                    break;
                }
            }
        }
        outcome.extracted.push(out_path);
        done += 1;
    }
    Ok(outcome)
}

// ── Tar ──────────────────────────────────────────────────────────────

enum TarCompression {
    None,
    Gz,
    Bz2,
    Xz,
}

fn tar_compression(format: ArchiveFormat) -> TarCompression {
    match format {
        ArchiveFormat::TarGz => TarCompression::Gz,
        ArchiveFormat::TarBz2 => TarCompression::Bz2,
        ArchiveFormat::TarXz => TarCompression::Xz,
        _ => TarCompression::None,
    }
}

/// Strip leading `/`, `./` and `..` components from archive entry paths to prevent path traversal.
fn sanitize_archive_path(name: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for component in name.split('/') {
        match component {
            "" | "." => {}
            ".." => { parts.pop(); }
            c => parts.push(c),
        }
    }
    let mut result = parts.join("/");
    if name.ends_with('/') && !result.is_empty() {
        result.push('/');
    }
    result
}

fn open_tar_reader(path: &Path, compression: TarCompression) -> io::Result<Box<dyn Read>> {
    let file = File::open(path)?;
    let buf = BufReader::new(file);
    match compression {
        TarCompression::None => Ok(Box::new(buf)),
        TarCompression::Gz => {
            let decoder = flate2::read::GzDecoder::new(buf);
            Ok(Box::new(decoder))
        }
        TarCompression::Bz2 => {
            let decoder = bzip2::read::BzDecoder::new(buf);
            Ok(Box::new(decoder))
        }
        TarCompression::Xz => {
            let decoder = xz2::read::XzDecoder::new(buf);
            Ok(Box::new(decoder))
        }
    }
}

fn list_tar(path: &Path, compression: TarCompression) -> io::Result<Vec<ArchiveEntry>> {
    let reader = open_tar_reader(path, compression)?;
    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();

    for entry_result in archive.entries()? {
        let entry = entry_result?;
        let path_str = entry.path()?.to_string_lossy().into_owned();
        let is_dir = entry.header().entry_type().is_dir();
        let size = entry.size();
        let modified = entry.header().mtime().ok().map(|secs| {
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)
        });
        entries.push(ArchiveEntry {
            path: path_str,
            size,
            is_dir,
            modified,
        });
    }
    Ok(entries)
}

fn extract_tar_stream(
    req: &ExtractRequest,
    compression: TarCompression,
    on_progress: &mut ProgressFn,
    resolve: &mut ConflictFn,
    cancel: &AtomicBool,
) -> io::Result<ExtractOutcome> {
    let ExtractRequest {
        archive_path,
        filter,
        dest,
        total,
    } = *req;
    let reader = open_tar_reader(archive_path, compression)?;
    let mut archive = tar::Archive::new(reader);
    let mut outcome = ExtractOutcome::default();
    let mut done = 0usize;

    for entry_result in archive.entries()? {
        // Honour cancellation between entries; an in-flight file is not interrupted.
        if cancel.load(Ordering::Relaxed) {
            outcome.cancelled = true;
            break;
        }

        let mut entry = entry_result?;
        let name = entry.path()?.to_string_lossy().into_owned();
        if !entry_matches(filter, &name) {
            continue;
        }

        let safe_name = sanitize_archive_path(&name);
        let out_path = dest.join(&safe_name);
        if !out_path.starts_with(dest) {
            continue;
        }

        on_progress(done, total, &name);

        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            if !ensure_dir_within(dest, &out_path)? {
                continue;
            }
        } else {
            if let Some(parent) = out_path.parent()
                && !ensure_dir_within(dest, parent)?
            {
                continue;
            }
            if out_path.exists() {
                let size = entry.size();
                let modified = entry
                    .header()
                    .mtime()
                    .ok()
                    .map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
                match resolve(&name, &out_path, size, modified) {
                    ConflictDecision::Overwrite => {}
                    ConflictDecision::Skip => {
                        outcome.skipped += 1;
                        done += 1;
                        continue;
                    }
                    ConflictDecision::Abort => {
                        outcome.cancelled = true;
                        break;
                    }
                }
            }

            if entry_type.is_symlink() {
                // A symlink/hardlink in a tar has no payload; the target lives in
                // the header. Without this branch it would land as an empty file.
                #[cfg(unix)]
                if let Some(target) = entry.link_name()? {
                    create_symlink(&target, &out_path)?;
                }
            } else if entry_type.is_hard_link() {
                #[cfg(unix)]
                if let Some(target) = entry.link_name()? {
                    // Hardlink targets are paths inside the archive; resolve them
                    // against `dest`, sanitized the same way as entry names.
                    let safe = sanitize_archive_path(&target.to_string_lossy());
                    let link_target = dest.join(safe);
                    let _ = std::fs::remove_file(&out_path);
                    std::fs::hard_link(&link_target, &out_path)?;
                }
            } else {
                // Never write through an existing symlink at this path.
                remove_if_symlink(&out_path);
                let mut out_file = File::create(&out_path)?;
                if !copy_with_cancel(&mut entry, &mut out_file, cancel)? {
                    drop(out_file);
                    let _ = std::fs::remove_file(&out_path);
                    outcome.cancelled = true;
                    break;
                }
                #[cfg(unix)]
                if let Ok(mode) = entry.header().mode() {
                    apply_mode(&out_path, mode)?;
                }
            }
        }
        outcome.extracted.push(out_path);
        done += 1;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_datetime_maps_to_expected_systemtime() {
        // 2021-06-15 12:00:00 UTC == unix timestamp 1623758400.
        let dt = zip::DateTime::from_date_and_time(2021, 6, 15, 12, 0, 0).unwrap();
        let st = zip_datetime_to_systemtime(Some(dt)).unwrap();
        let secs = st.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        assert_eq!(secs, 1_623_758_400);
    }

    #[test]
    fn sanitize_strips_traversal() {
        assert_eq!(sanitize_archive_path("../../etc/passwd"), "etc/passwd");
        assert_eq!(sanitize_archive_path("/abs/path"), "abs/path");
        assert_eq!(sanitize_archive_path("a/../b"), "b");
        assert_eq!(sanitize_archive_path("./foo/./bar"), "foo/bar");
        assert_eq!(sanitize_archive_path("dir/"), "dir/");
    }
}
