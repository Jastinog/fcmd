//! Creation of archives (zip / tar / tar.gz / tar.bz2 / tar.xz).

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::{ArchiveFormat, ProgressFn, copy_with_cancel, never_cancel};

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

/// What kind of filesystem object a [`CreateEntry`] represents.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Dir,
    File,
    Symlink,
}

/// One flattened item to add to an archive, in pre-order (a directory appears
/// before its contents) so the writer always sees parents first.
struct CreateEntry {
    abs: PathBuf,
    /// Path inside the archive, relative to `base_dir`, with `/` separators.
    rel: String,
    kind: EntryKind,
    /// Source unix mode bits (permissions), preserved into the archive.
    mode: u32,
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
    // `symlink_metadata` does not follow links, so symlinks are stored as links
    // rather than being followed and packed as their target's content.
    let meta = std::fs::symlink_metadata(abs)?;
    let file_type = meta.file_type();

    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    };
    #[cfg(not(unix))]
    let mode = if file_type.is_dir() { 0o755 } else { 0o644 };

    if file_type.is_symlink() {
        out.push(CreateEntry {
            abs: abs.to_path_buf(),
            rel,
            kind: EntryKind::Symlink,
            mode,
        });
    } else if file_type.is_dir() {
        out.push(CreateEntry {
            abs: abs.to_path_buf(),
            rel: rel.clone(),
            kind: EntryKind::Dir,
            mode,
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
            kind: EntryKind::File,
            mode,
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
    Zst(zstd::stream::write::Encoder<'static, File>),
}

impl io::Write for CompWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(w) => w.write(buf),
            Self::Gz(w) => w.write(buf),
            Self::Bz2(w) => w.write(buf),
            Self::Xz(w) => w.write(buf),
            Self::Zst(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(w) => w.flush(),
            Self::Gz(w) => w.flush(),
            Self::Bz2(w) => w.flush(),
            Self::Xz(w) => w.flush(),
            Self::Zst(w) => w.flush(),
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
            Self::Zst(w) => {
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
    fn add_dir(&mut self, rel: &str, abs: &Path, mode: u32) -> io::Result<()> {
        match self {
            ArchiveSink::Zip { writer, options } => writer
                .add_directory(format!("{rel}/"), options.unix_permissions(mode))
                .map_err(io::Error::other),
            ArchiveSink::Tar(b) => b.append_dir(rel, abs),
        }
    }

    /// Add a regular file. Returns `Ok(false)` if cancelled mid-file (zip only;
    /// see below) so the caller can stop. The archive stays valid either way.
    fn add_file(&mut self, rel: &str, abs: &Path, mode: u32, cancel: &AtomicBool) -> io::Result<bool> {
        match self {
            ArchiveSink::Zip { writer, options } => {
                writer
                    .start_file(rel, options.unix_permissions(mode))
                    .map_err(io::Error::other)?;
                let mut f = File::open(abs)?;
                // zip finalizes the started entry with the bytes written so far on
                // `finish()`, so a cancelled file becomes a valid truncated entry.
                copy_with_cancel(&mut f, writer, cancel)
            }
            ArchiveSink::Tar(b) => {
                // `append_file` derives the header (including mode) from the open
                // file and copies it in one opaque call, so tar creation can only
                // be cancelled between files (handled by the loop).
                let mut f = File::open(abs)?;
                b.append_file(rel, &mut f)?;
                Ok(true)
            }
        }
    }

    /// Store `abs` (a symlink) as a link entry rather than following it.
    fn add_symlink(&mut self, rel: &str, abs: &Path, mode: u32) -> io::Result<()> {
        let target = std::fs::read_link(abs)?;
        match self {
            ArchiveSink::Zip { writer, options } => writer
                .add_symlink(
                    rel,
                    target.to_string_lossy().as_ref(),
                    options.unix_permissions(mode),
                )
                .map_err(io::Error::other),
            ArchiveSink::Tar(b) => {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_mode(mode & 0o7777);
                b.append_link(&mut header, rel, &target)
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
                ArchiveFormat::TarZst => {
                    CompWriter::Zst(zstd::stream::write::Encoder::new(file, 0)?)
                }
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
        match entry.kind {
            EntryKind::Dir => sink.add_dir(&entry.rel, &entry.abs, entry.mode)?,
            EntryKind::File => {
                // Mid-file cancel: stop without counting the partial entry.
                if !sink.add_file(&entry.rel, &entry.abs, entry.mode, cancel)? {
                    cancelled = true;
                    break;
                }
            }
            EntryKind::Symlink => sink.add_symlink(&entry.rel, &entry.abs, entry.mode)?,
        }
        written += 1;
    }
    sink.finish()?;
    Ok((written, cancelled))
}

