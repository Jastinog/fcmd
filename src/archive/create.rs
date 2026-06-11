//! Creation of archives (zip / tar / tar.gz / tar.bz2 / tar.xz).

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::{ArchiveFormat, ProgressFn, never_cancel};

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

