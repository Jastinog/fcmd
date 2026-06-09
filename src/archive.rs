use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct ArchiveEntry {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    #[allow(dead_code)]
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
}

impl ArchiveFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Some(ArchiveFormat::TarGz)
        } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
            Some(ArchiveFormat::TarBz2)
        } else if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            Some(ArchiveFormat::TarXz)
        } else if name.ends_with(".tar") {
            Some(ArchiveFormat::Tar)
        } else if name.ends_with(".zip") {
            Some(ArchiveFormat::Zip)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::TarGz => "tar.gz",
            ArchiveFormat::TarBz2 => "tar.bz2",
            ArchiveFormat::TarXz => "tar.xz",
        }
    }
}

pub fn is_archive(path: &Path) -> bool {
    ArchiveFormat::from_path(path).is_some()
}

// ── Streaming hooks (progress / conflict / cancel) ───────────────────
//
// These let the app layer stream archive create/extract through the task
// manager the way `ops::paste` does, without `archive.rs` knowing about tokio:
// the caller passes plain closures and an `AtomicBool` cancel flag.

/// Per-entry overwrite decision for extract conflicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictDecision {
    Overwrite,
    Skip,
    /// Stop the whole operation, keeping what was already extracted.
    Abort,
}

/// Reported before each entry is processed: `(done, total, current_name)`.
pub type ProgressFn<'a> = dyn FnMut(usize, usize, &str) + 'a;

/// Asked when an extracted file would overwrite an existing one. Receives the
/// archive entry name, the destination path, and the source entry's size and
/// mtime so the caller can build a conflict prompt.
pub type ConflictFn<'a> = dyn FnMut(&str, &Path, u64, Option<SystemTime>) -> ConflictDecision + 'a;

/// Result of a streaming extract.
#[derive(Debug, Default)]
pub struct ExtractOutcome {
    pub extracted: Vec<PathBuf>,
    pub skipped: usize,
    /// True if cancelled (via the flag) or aborted at a conflict prompt.
    pub cancelled: bool,
}

/// A cancel flag that is never set — for the synchronous wrappers / tests.
#[allow(dead_code)]
fn never_cancel() -> AtomicBool {
    AtomicBool::new(false)
}

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

/// Extract entries, streaming progress and routing per-file overwrite conflicts.
///
/// `filter` selects which entries to extract: `None` extracts everything, `Some(path)`
/// extracts a single file (exact match) or a directory subtree (trailing `/`). `total`
/// is a best-effort hint for the progress denominator (0 if unknown). Cancellation is
/// honoured between entries; a single in-flight file is not interrupted.
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

    match format {
        ArchiveFormat::Zip => {
            extract_zip_stream(archive_path, filter, dest, total, on_progress, resolve, cancel)
        }
        _ => extract_tar_stream(
            archive_path,
            filter,
            dest,
            total,
            tar_compression(format),
            on_progress,
            resolve,
            cancel,
        ),
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

/// Approximate a zip `DateTime` as a `SystemTime`. Good enough for display
/// and conflict prompts; not used for any correctness-sensitive comparison.
fn zip_datetime_to_systemtime(dt: Option<zip::DateTime>) -> Option<SystemTime> {
    dt.map(|dt| {
        let (year, month, day, hour, min, sec) = (
            dt.year() as i64,
            dt.month() as u64,
            dt.day() as u64,
            dt.hour() as u64,
            dt.minute() as u64,
            dt.second() as u64,
        );
        let days = (year - 1970) * 365 + (year - 1969) / 4
            + match month {
                1 => 0,
                2 => 31,
                _ => {
                    let m = month - 1;
                    let leap = if year % 4 == 0 { 1 } else { 0 };
                    (m * 30 + m.div_ceil(2) - 2 + leap) as i64
                }
            }
            + day as i64
            - 1;
        let secs = days as u64 * 86400 + hour * 3600 + min * 60 + sec;
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)
    })
}

#[allow(clippy::too_many_arguments)]
fn extract_zip_stream(
    archive_path: &Path,
    filter: Option<&str>,
    dest: &Path,
    total: usize,
    on_progress: &mut ProgressFn,
    resolve: &mut ConflictFn,
    cancel: &AtomicBool,
) -> io::Result<ExtractOutcome> {
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
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
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
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
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

#[allow(clippy::too_many_arguments)]
fn extract_tar_stream(
    archive_path: &Path,
    filter: Option<&str>,
    dest: &Path,
    total: usize,
    compression: TarCompression,
    on_progress: &mut ProgressFn,
    resolve: &mut ConflictFn,
    cancel: &AtomicBool,
) -> io::Result<ExtractOutcome> {
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

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
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
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
        outcome.extracted.push(out_path);
        done += 1;
    }
    Ok(outcome)
}

/// Create an archive from a list of file/directory paths.
///
/// Convenience wrapper: no progress, never cancels. Use [`create_stream`] to
/// drive progress and cancellation through the task manager.
#[allow(dead_code)]
pub fn create_archive(paths: &[PathBuf], base_dir: &Path, output: &Path) -> io::Result<()> {
    let cancel = never_cancel();
    create_stream(paths, base_dir, output, &mut |_, _, _| {}, &cancel)?;
    Ok(())
}

/// One flattened item to add to an archive, in pre-order (a directory appears
/// before its contents) so the writer always sees parents first.
struct CreateEntry {
    abs: PathBuf,
    /// Path inside the archive, relative to `base_dir`, with `/` separators.
    rel: String,
    is_dir: bool,
}

/// Walk `paths` (relative to `base_dir`) into a flat, pre-ordered entry list so
/// `create_stream` can report per-file progress and cancel between files —
/// `tar::append_dir_all` would otherwise archive a whole directory in one opaque
/// call with no callback or cancellation point inside.
fn collect_create_entries(
    paths: &[PathBuf],
    base_dir: &Path,
    out: &mut Vec<CreateEntry>,
) -> io::Result<()> {
    for path in paths {
        let rel = path
            .strip_prefix(base_dir)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .to_string_lossy()
            .into_owned();
        push_create_entry(path, rel, out)?;
    }
    Ok(())
}

fn push_create_entry(abs: &Path, rel: String, out: &mut Vec<CreateEntry>) -> io::Result<()> {
    if abs.is_dir() {
        out.push(CreateEntry {
            abs: abs.to_path_buf(),
            rel: rel.clone(),
            is_dir: true,
        });
        // Sort children for deterministic archive ordering across platforms.
        let mut children: Vec<_> = std::fs::read_dir(abs)?.collect::<Result<_, _>>()?;
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_rel = format!("{rel}/{}", child.file_name().to_string_lossy());
            push_create_entry(&child.path(), child_rel, out)?;
        }
    } else {
        out.push(CreateEntry {
            abs: abs.to_path_buf(),
            rel,
            is_dir: false,
        });
    }
    Ok(())
}

/// A tar compression writer that finalizes its footer on `finish()` (a plain
/// `flush()` won't write the gzip/bzip2/xz trailer).
enum CompWriter {
    Plain(File),
    Gz(flate2::write::GzEncoder<File>),
    Bz2(bzip2::write::BzEncoder<File>),
    Xz(xz2::write::XzEncoder<File>),
}

impl io::Write for CompWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(w) => w.write(buf),
            Self::Gz(w) => w.write(buf),
            Self::Bz2(w) => w.write(buf),
            Self::Xz(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(w) => w.flush(),
            Self::Gz(w) => w.flush(),
            Self::Bz2(w) => w.flush(),
            Self::Xz(w) => w.flush(),
        }
    }
}

impl CompWriter {
    fn finish(self) -> io::Result<()> {
        match self {
            Self::Plain(_) => Ok(()),
            Self::Gz(w) => {
                w.finish()?;
                Ok(())
            }
            Self::Bz2(w) => {
                w.finish()?;
                Ok(())
            }
            Self::Xz(w) => {
                w.finish()?;
                Ok(())
            }
        }
    }
}

/// Unified write end: hides the zip-vs-tar differences behind add_dir/add_file.
enum ArchiveSink {
    Zip {
        writer: zip::ZipWriter<File>,
        options: zip::write::SimpleFileOptions,
    },
    Tar(tar::Builder<CompWriter>),
}

impl ArchiveSink {
    fn add_dir(&mut self, rel: &str, abs: &Path) -> io::Result<()> {
        match self {
            ArchiveSink::Zip { writer, options } => writer
                .add_directory(format!("{rel}/"), *options)
                .map_err(io::Error::other),
            ArchiveSink::Tar(b) => b.append_dir(rel, abs),
        }
    }

    fn add_file(&mut self, rel: &str, abs: &Path) -> io::Result<()> {
        match self {
            ArchiveSink::Zip { writer, options } => {
                writer.start_file(rel, *options).map_err(io::Error::other)?;
                let mut f = File::open(abs)?;
                io::copy(&mut f, writer)?;
                Ok(())
            }
            ArchiveSink::Tar(b) => {
                let mut f = File::open(abs)?;
                b.append_file(rel, &mut f)
            }
        }
    }

    fn finish(self) -> io::Result<()> {
        match self {
            ArchiveSink::Zip { writer, .. } => {
                writer.finish().map_err(io::Error::other)?;
                Ok(())
            }
            ArchiveSink::Tar(b) => {
                let writer = b.into_inner()?;
                writer.finish()
            }
        }
    }
}

/// Create an archive, streaming per-file progress and honouring cancellation
/// between files. Returns `(written, cancelled)`: `written` is the number of
/// entries added (the resulting archive is still valid even if cancelled), and
/// `cancelled` is true if the operation stopped early. On error the partial
/// output file is removed.
pub fn create_stream(
    paths: &[PathBuf],
    base_dir: &Path,
    output: &Path,
    on_progress: &mut ProgressFn,
    cancel: &AtomicBool,
) -> io::Result<(usize, bool)> {
    let result = create_stream_inner(paths, base_dir, output, on_progress, cancel);
    if result.is_err() {
        // Clean up partial file on failure.
        let _ = std::fs::remove_file(output);
    }
    result
}

fn create_stream_inner(
    paths: &[PathBuf],
    base_dir: &Path,
    output: &Path,
    on_progress: &mut ProgressFn,
    cancel: &AtomicBool,
) -> io::Result<(usize, bool)> {
    let format = ArchiveFormat::from_path(output)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown archive format"))?;

    let mut entries = Vec::new();
    collect_create_entries(paths, base_dir, &mut entries)?;
    let total = entries.len();

    let file = File::create(output)?;
    let mut sink = match format {
        ArchiveFormat::Zip => ArchiveSink::Zip {
            writer: zip::ZipWriter::new(file),
            options: zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        },
        _ => {
            let writer = match format {
                ArchiveFormat::TarGz => CompWriter::Gz(flate2::write::GzEncoder::new(
                    file,
                    flate2::Compression::default(),
                )),
                ArchiveFormat::TarBz2 => CompWriter::Bz2(bzip2::write::BzEncoder::new(
                    file,
                    bzip2::Compression::default(),
                )),
                ArchiveFormat::TarXz => CompWriter::Xz(xz2::write::XzEncoder::new(file, 6)),
                _ => CompWriter::Plain(file),
            };
            ArchiveSink::Tar(tar::Builder::new(writer))
        }
    };

    let mut cancelled = false;
    let mut written = 0usize;
    for (done, entry) in entries.iter().enumerate() {
        // Honour cancellation between files; an in-flight file is not interrupted.
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        on_progress(done, total, &entry.rel);
        if entry.is_dir {
            sink.add_dir(&entry.rel, &entry.abs)?;
        } else {
            sink.add_file(&entry.rel, &entry.abs)?;
        }
        written += 1;
    }
    sink.finish()?;
    Ok((written, cancelled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn format_detection() {
        assert_eq!(
            ArchiveFormat::from_path(Path::new("test.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("test.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("test.tgz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("test.tar.bz2")),
            Some(ArchiveFormat::TarBz2)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("test.tar.xz")),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            ArchiveFormat::from_path(Path::new("test.tar")),
            Some(ArchiveFormat::Tar)
        );
        assert_eq!(ArchiveFormat::from_path(Path::new("test.txt")), None);
    }

    #[test]
    fn is_archive_works() {
        assert!(is_archive(Path::new("foo.zip")));
        assert!(is_archive(Path::new("foo.tar.gz")));
        assert!(!is_archive(Path::new("foo.rs")));
    }

    #[test]
    fn list_and_extract_zip() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("test.zip");

        // Create a zip with two files
        {
            let file = File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("hello.txt", options).unwrap();
            writer.write_all(b"Hello World").unwrap();
            writer.start_file("sub/nested.txt", options).unwrap();
            writer.write_all(b"Nested").unwrap();
            writer.finish().unwrap();
        }

        // List
        let (fmt, entries) = list_archive(&zip_path).unwrap();
        assert_eq!(fmt, ArchiveFormat::Zip);
        assert!(entries.iter().any(|e| e.path == "hello.txt"));
        assert!(entries.iter().any(|e| e.path == "sub/nested.txt"));

        // Extract single
        let extract_dir = dir.path().join("out1");
        std::fs::create_dir(&extract_dir).unwrap();
        let extracted = extract_entry(&zip_path, "hello.txt", &extract_dir).unwrap();
        assert_eq!(extracted.len(), 1);
        assert!(extract_dir.join("hello.txt").exists());

        // Extract all
        let extract_all_dir = dir.path().join("out2");
        std::fs::create_dir(&extract_all_dir).unwrap();
        let extracted = extract_all(&zip_path, &extract_all_dir).unwrap();
        assert!(extracted.len() >= 2);
        assert!(extract_all_dir.join("hello.txt").exists());
        assert!(extract_all_dir.join("sub/nested.txt").exists());
    }

    #[test]
    fn list_and_extract_tar_gz() {
        let dir = tempfile::tempdir().unwrap();
        let tar_path = dir.path().join("test.tar.gz");

        // Create test files
        let src = dir.path().join("src_files");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "aaa").unwrap();
        std::fs::create_dir(src.join("sub")).unwrap();
        std::fs::write(src.join("sub/b.txt"), "bbb").unwrap();

        // Create archive
        create_archive(
            &[src.join("a.txt"), src.join("sub")],
            &src,
            &tar_path,
        )
        .unwrap();

        // List
        let (fmt, entries) = list_archive(&tar_path).unwrap();
        assert_eq!(fmt, ArchiveFormat::TarGz);
        assert!(entries.iter().any(|e| e.path == "a.txt"));

        // Extract all
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract_all(&tar_path, &out).unwrap();
        assert!(out.join("a.txt").exists());
    }

    #[test]
    fn create_zip_archive() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("file.txt"), "content").unwrap();

        let zip_path = dir.path().join("out.zip");
        create_archive(&[src.join("file.txt")], &src, &zip_path).unwrap();

        let (fmt, entries) = list_archive(&zip_path).unwrap();
        assert_eq!(fmt, ArchiveFormat::Zip);
        assert!(entries.iter().any(|e| e.path == "file.txt"));
    }

    #[test]
    fn synthesizes_missing_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("test.zip");

        // Create zip with only files (no explicit dir entries)
        {
            let file = File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("a/b/c.txt", options).unwrap();
            writer.write_all(b"deep").unwrap();
            writer.finish().unwrap();
        }

        let (_, entries) = list_archive(&zip_path).unwrap();
        // Should have synthesized "a/" and "a/b/"
        assert!(entries.iter().any(|e| e.path == "a/" && e.is_dir));
        assert!(entries.iter().any(|e| e.path == "a/b/" && e.is_dir));
    }

    #[test]
    fn zip_slip_traversal_is_neutralized() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("evil.zip");

        // Craft a zip whose entry name escapes via `..`.
        {
            let file = File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("../../escape.txt", options).unwrap();
            writer.write_all(b"pwned").unwrap();
            writer.finish().unwrap();
        }

        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();
        let extracted = extract_all(&zip_path, &dest).unwrap();

        // The traversal must be stripped: the file lands inside `dest`, never above it.
        let escaped = dir.path().join("escape.txt");
        assert!(!escaped.exists(), "file escaped extraction dir via Zip Slip");
        assert!(dest.join("escape.txt").exists());
        for p in &extracted {
            assert!(p.starts_with(&dest), "extracted path {p:?} escaped dest");
        }
    }

    #[test]
    fn sanitize_strips_traversal() {
        assert_eq!(sanitize_archive_path("../../etc/passwd"), "etc/passwd");
        assert_eq!(sanitize_archive_path("/abs/path"), "abs/path");
        assert_eq!(sanitize_archive_path("a/../b"), "b");
        assert_eq!(sanitize_archive_path("./foo/./bar"), "foo/bar");
        assert_eq!(sanitize_archive_path("dir/"), "dir/");
    }

    /// Build a one-file zip at `path` whose `hello.txt` holds `content`.
    fn make_hello_zip(path: &Path, content: &[u8]) {
        let file = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opt = zip::write::SimpleFileOptions::default();
        w.start_file("hello.txt", opt).unwrap();
        w.write_all(content).unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn extract_stream_skip_vs_overwrite_on_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        make_hello_zip(&zip_path, b"new");

        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("hello.txt"), b"old").unwrap();

        let cancel = AtomicBool::new(false);

        // Skip → existing content preserved, counted as skipped, not extracted.
        let outcome = extract_stream(
            &zip_path,
            None,
            &dest,
            1,
            &mut |_, _, _| {},
            &mut |_, _, _, _| ConflictDecision::Skip,
            &cancel,
        )
        .unwrap();
        assert_eq!(outcome.skipped, 1);
        assert!(outcome.extracted.is_empty());
        assert_eq!(std::fs::read(dest.join("hello.txt")).unwrap(), b"old");

        // Overwrite → file replaced.
        let outcome = extract_stream(
            &zip_path,
            None,
            &dest,
            1,
            &mut |_, _, _| {},
            &mut |_, _, _, _| ConflictDecision::Overwrite,
            &cancel,
        )
        .unwrap();
        assert_eq!(outcome.skipped, 0);
        assert_eq!(outcome.extracted.len(), 1);
        assert_eq!(std::fs::read(dest.join("hello.txt")).unwrap(), b"new");
    }

    #[test]
    fn extract_stream_resolver_only_called_on_existing() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        make_hello_zip(&zip_path, b"data");
        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();

        let cancel = AtomicBool::new(false);
        let mut conflicts = 0;
        let outcome = extract_stream(
            &zip_path,
            None,
            &dest,
            1,
            &mut |_, _, _| {},
            &mut |_, _, _, _| {
                conflicts += 1;
                ConflictDecision::Overwrite
            },
            &cancel,
        )
        .unwrap();
        // No pre-existing file → resolver never consulted.
        assert_eq!(conflicts, 0);
        assert_eq!(outcome.extracted.len(), 1);
    }

    #[test]
    fn extract_stream_abort_stops_early() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        make_hello_zip(&zip_path, b"new");
        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("hello.txt"), b"old").unwrap();

        let cancel = AtomicBool::new(false);
        let outcome = extract_stream(
            &zip_path,
            None,
            &dest,
            1,
            &mut |_, _, _| {},
            &mut |_, _, _, _| ConflictDecision::Abort,
            &cancel,
        )
        .unwrap();
        assert!(outcome.cancelled);
        assert_eq!(std::fs::read(dest.join("hello.txt")).unwrap(), b"old");
    }

    #[test]
    fn extract_stream_honours_cancel_flag() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        make_hello_zip(&zip_path, b"data");
        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();

        let cancel = AtomicBool::new(true); // pre-cancelled
        let outcome = extract_stream(
            &zip_path,
            None,
            &dest,
            1,
            &mut |_, _, _| {},
            &mut |_, _, _, _| ConflictDecision::Overwrite,
            &cancel,
        )
        .unwrap();
        assert!(outcome.cancelled);
        assert!(outcome.extracted.is_empty());
        assert!(!dest.join("hello.txt").exists());
    }

    #[test]
    fn extract_stream_reports_progress_with_total() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        make_hello_zip(&zip_path, b"data");
        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();

        let cancel = AtomicBool::new(false);
        let mut calls: Vec<(usize, usize, String)> = Vec::new();
        extract_stream(
            &zip_path,
            None,
            &dest,
            1,
            &mut |d, t, name| calls.push((d, t, name.to_string())),
            &mut |_, _, _, _| ConflictDecision::Overwrite,
            &cancel,
        )
        .unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (0, 1, "hello.txt".to_string()));
    }

    #[test]
    fn create_stream_reports_progress_and_count() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();
        std::fs::create_dir(src.join("sub")).unwrap();
        std::fs::write(src.join("sub/b.txt"), "b").unwrap();

        let out = dir.path().join("out.zip");
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        let (written, cancelled) = create_stream(
            &[src.join("a.txt"), src.join("sub")],
            &src,
            &out,
            &mut |_, _, _| calls += 1,
            &cancel,
        )
        .unwrap();
        assert!(!cancelled);
        // Entries in pre-order: a.txt, sub/, sub/b.txt = 3.
        assert_eq!(written, 3);
        assert_eq!(calls, 3);

        // The archive is valid and round-trips.
        let (_, entries) = list_archive(&out).unwrap();
        assert!(entries.iter().any(|e| e.path == "a.txt"));
        assert!(entries.iter().any(|e| e.path == "sub/b.txt"));
    }

    #[test]
    fn create_stream_honours_cancel_flag() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();

        let out = dir.path().join("out.zip");
        let cancel = AtomicBool::new(true); // pre-cancelled
        let (written, cancelled) =
            create_stream(&[src.join("a.txt")], &src, &out, &mut |_, _, _| {}, &cancel).unwrap();
        assert!(cancelled);
        assert_eq!(written, 0);
    }
}
