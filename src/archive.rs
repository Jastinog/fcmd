use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
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
pub fn extract_entry(
    archive_path: &Path,
    entry_path: &str,
    dest: &Path,
) -> io::Result<Vec<PathBuf>> {
    let format = ArchiveFormat::from_path(archive_path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown archive format"))?;

    match format {
        ArchiveFormat::Zip => extract_zip_entry(archive_path, entry_path, dest),
        _ => extract_tar_entry(archive_path, entry_path, dest, tar_compression(format)),
    }
}

/// Extract all entries from an archive.
pub fn extract_all(archive_path: &Path, dest: &Path) -> io::Result<Vec<PathBuf>> {
    let format = ArchiveFormat::from_path(archive_path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown archive format"))?;

    match format {
        ArchiveFormat::Zip => extract_zip_all(archive_path, dest),
        _ => extract_tar_all(archive_path, dest, tar_compression(format)),
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
        let modified = entry.last_modified().map(|dt| {
            let (year, month, day, hour, min, sec) = (
                dt.year() as i64,
                dt.month() as u64,
                dt.day() as u64,
                dt.hour() as u64,
                dt.minute() as u64,
                dt.second() as u64,
            );
            // Approximate conversion — good enough for display
            let days = (year - 1970) * 365 + (year - 1969) / 4
                + match month {
                    1 => 0,
                    2 => 31,
                    _ => {
                        let m = month - 1;
                        let leap = if year % 4 == 0 { 1 } else { 0 };
                        (m * 30 + (m + 1) / 2 - 2 + leap) as i64
                    }
                }
                + day as i64
                - 1;
            let secs = days as u64 * 86400 + hour * 3600 + min * 60 + sec;
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)
        });
        entries.push(ArchiveEntry {
            path: name,
            size,
            is_dir,
            modified,
        });
    }
    Ok(entries)
}

fn extract_zip_entry(
    archive_path: &Path,
    entry_path: &str,
    dest: &Path,
) -> io::Result<Vec<PathBuf>> {
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let is_dir_extract = entry_path.ends_with('/');
    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let name = entry.name().to_string();
        let should_extract = if is_dir_extract {
            name.starts_with(entry_path)
        } else {
            name == entry_path
        };

        if !should_extract {
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

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
        extracted.push(out_path);
    }
    Ok(extracted)
}

fn extract_zip_all(archive_path: &Path, dest: &Path) -> io::Result<Vec<PathBuf>> {
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut extracted = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let name = entry.name().to_string();
        // Sanitize raw archive path to strip `..`/`.`/leading `/` (Zip Slip guard).
        let safe_name = sanitize_archive_path(&name);
        let out_path = dest.join(&safe_name);
        if !out_path.starts_with(dest) {
            continue;
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
        extracted.push(out_path);
    }
    Ok(extracted)
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

fn extract_tar_entry(
    archive_path: &Path,
    entry_path: &str,
    dest: &Path,
    compression: TarCompression,
) -> io::Result<Vec<PathBuf>> {
    let reader = open_tar_reader(archive_path, compression)?;
    let mut archive = tar::Archive::new(reader);
    let is_dir_extract = entry_path.ends_with('/');
    let mut extracted = Vec::new();

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let name = entry.path()?.to_string_lossy().into_owned();

        let should_extract = if is_dir_extract {
            name.starts_with(entry_path)
        } else {
            name == entry_path
        };

        if !should_extract {
            continue;
        }

        let safe_name = sanitize_archive_path(&name);
        let out_path = dest.join(&safe_name);
        if !out_path.starts_with(dest) {
            continue;
        }

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
        extracted.push(out_path);
    }
    Ok(extracted)
}

fn extract_tar_all(
    archive_path: &Path,
    dest: &Path,
    compression: TarCompression,
) -> io::Result<Vec<PathBuf>> {
    let reader = open_tar_reader(archive_path, compression)?;
    let mut archive = tar::Archive::new(reader);
    let mut extracted = Vec::new();

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let name = entry.path()?.to_string_lossy().into_owned();
        let safe_name = sanitize_archive_path(&name);
        let out_path = dest.join(&safe_name);
        if !out_path.starts_with(dest) {
            continue;
        }

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
        extracted.push(out_path);
    }
    Ok(extracted)
}

/// Create an archive from a list of file/directory paths.
pub fn create_archive(
    paths: &[PathBuf],
    base_dir: &Path,
    output: &Path,
) -> io::Result<()> {
    let format = ArchiveFormat::from_path(output)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown archive format"))?;

    let result = match format {
        ArchiveFormat::Zip => create_zip(paths, base_dir, output),
        _ => create_tar(paths, base_dir, output, format),
    };
    if result.is_err() {
        // Clean up partial file on failure
        let _ = std::fs::remove_file(output);
    }
    result
}

fn create_zip(paths: &[PathBuf], base_dir: &Path, output: &Path) -> io::Result<()> {
    let file = File::create(output)?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for path in paths {
        let rel = path
            .strip_prefix(base_dir)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        if path.is_dir() {
            add_dir_to_zip(&mut archive, path, rel, options)?;
        } else {
            let name = rel.to_string_lossy().into_owned();
            archive
                .start_file(&name, options)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            let mut f = File::open(path)?;
            io::copy(&mut f, &mut archive)?;
        }
    }
    archive
        .finish()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(())
}

fn add_dir_to_zip(
    archive: &mut zip::ZipWriter<File>,
    dir: &Path,
    rel: &Path,
    options: zip::write::SimpleFileOptions,
) -> io::Result<()> {
    let dir_name = format!("{}/", rel.to_string_lossy());
    archive
        .add_directory(&dir_name, options)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let child = entry.path();
        let child_rel = rel.join(entry.file_name());
        if child.is_dir() {
            add_dir_to_zip(archive, &child, &child_rel, options)?;
        } else {
            let name = child_rel.to_string_lossy().into_owned();
            archive
                .start_file(&name, options)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            let mut f = File::open(&child)?;
            io::copy(&mut f, archive)?;
        }
    }
    Ok(())
}

fn create_tar(
    paths: &[PathBuf],
    base_dir: &Path,
    output: &Path,
    format: ArchiveFormat,
) -> io::Result<()> {
    let file = File::create(output)?;

    // We need to finalize compressors explicitly (flush() alone won't write
    // the compression footer).  Use an enum so we keep the concrete type.
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
                Self::Gz(w) => { w.finish()?; Ok(()) }
                Self::Bz2(w) => { w.finish()?; Ok(()) }
                Self::Xz(w) => { w.finish()?; Ok(()) }
            }
        }
    }

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

    let mut archive = tar::Builder::new(writer);

    for path in paths {
        let rel = path
            .strip_prefix(base_dir)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        if path.is_dir() {
            archive.append_dir_all(rel, path)?;
        } else {
            let mut f = File::open(path)?;
            archive.append_file(rel, &mut f)?;
        }
    }
    let writer = archive.into_inner()?;
    writer.finish()?;
    Ok(())
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
}
