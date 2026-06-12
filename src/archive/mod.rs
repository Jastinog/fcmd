//! Archive support: format detection plus listing, extraction and creation.
//!
//! Shared types ([`ArchiveFormat`], [`ArchiveEntry`], conflict/progress callbacks)
//! live here; the heavy lifting is split between [`extract`] and [`create`].

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

mod create;
mod extract;

pub use create::*;
pub use extract::*;

/// Copy `reader` into `writer` in chunks, checking `cancel` between chunks so a
/// single huge entry can be interrupted (plain `io::copy` only lets callers
/// cancel between entries). Returns `Ok(true)` on completion, `Ok(false)` if
/// cancelled mid-copy.
pub(crate) fn copy_with_cancel<R: Read + ?Sized, W: Write + ?Sized>(
    reader: &mut R,
    writer: &mut W,
    cancel: &AtomicBool,
) -> io::Result<bool> {
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            return Ok(true);
        }
        writer.write_all(&buf[..n])?;
    }
}

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
    TarZst,
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
        } else if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
            Some(ArchiveFormat::TarZst)
        } else if name.ends_with(".tar") {
            Some(ArchiveFormat::Tar)
        } else if name.ends_with(".zip") {
            Some(ArchiveFormat::Zip)
        } else {
            None
        }
    }

    /// Detect format from the file's leading magic bytes. Fallback for archives
    /// with a missing or misleading extension. Compressed streams are reported as
    /// their `Tar*` form, matching this module's convention that `.gz`/`.bz2`/
    /// `.xz`/`.zst` wrap a tar.
    pub fn sniff(path: &Path) -> Option<Self> {
        use std::io::Read;
        let mut file = std::fs::File::open(path).ok()?;
        // 264 bytes covers the tar `ustar` magic at offset 257.
        let mut buf = [0u8; 264];
        let n = file.read(&mut buf).ok()?;
        let head = &buf[..n];
        if head.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            Some(ArchiveFormat::Zip)
        } else if head.starts_with(&[0x1F, 0x8B]) {
            Some(ArchiveFormat::TarGz)
        } else if head.starts_with(&[0x42, 0x5A, 0x68]) {
            Some(ArchiveFormat::TarBz2)
        } else if head.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
            Some(ArchiveFormat::TarXz)
        } else if head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            Some(ArchiveFormat::TarZst)
        } else if n >= 262 && &buf[257..262] == b"ustar" {
            Some(ArchiveFormat::Tar)
        } else {
            None
        }
    }

    /// Detect by extension first, falling back to magic-byte sniffing.
    pub fn detect(path: &Path) -> Option<Self> {
        Self::from_path(path).or_else(|| Self::sniff(path))
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::TarGz => "tar.gz",
            ArchiveFormat::TarBz2 => "tar.bz2",
            ArchiveFormat::TarXz => "tar.xz",
            ArchiveFormat::TarZst => "tar.zst",
        }
    }
}

pub fn is_archive(path: &Path) -> bool {
    ArchiveFormat::detect(path).is_some()
}

/// The archive's base name with its format extension removed, for use as an
/// "extract into a subfolder" target: `data.tar.gz` → `data`, `pkg.zip` → `pkg`.
/// Compound extensions are matched before their single-suffix forms. Falls back
/// to the full file name when no known extension matches or stripping leaves it
/// empty (e.g. a dotfile like `.zip`).
pub fn archive_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    // Longest/compound-first so `.tar.gz` wins over `.gz`/`.tar`.
    const EXTS: &[&str] = &[
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tgz", ".tbz2", ".txz", ".tzst", ".tar",
        ".zip",
    ];
    for ext in EXTS {
        // Extensions are ASCII, so byte lengths match between `lower` and `name`
        // and the cut lands on a char boundary.
        if let Some(stem) = lower.strip_suffix(ext)
            && !stem.is_empty()
        {
            return name[..stem.len()].to_string();
        }
    }
    name
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
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn copy_with_cancel_stops_midstream() {
        use std::sync::atomic::Ordering;

        // A reader that requests cancellation right after handing out its first
        // chunk, so the next loop iteration must bail.
        struct FlipReader<'a> {
            remaining: usize,
            cancel: &'a AtomicBool,
        }
        impl Read for FlipReader<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.remaining == 0 {
                    return Ok(0);
                }
                let n = buf.len().min(self.remaining);
                self.remaining -= n;
                self.cancel.store(true, Ordering::Relaxed);
                Ok(n)
            }
        }

        let cancel = AtomicBool::new(false);
        let mut reader = FlipReader {
            remaining: 10 * 1024 * 1024,
            cancel: &cancel,
        };
        let mut sink: Vec<u8> = Vec::new();
        let completed = copy_with_cancel(&mut reader, &mut sink, &cancel).unwrap();
        assert!(!completed, "copy should report it was cancelled");
        assert_eq!(
            sink.len(),
            128 * 1024,
            "exactly one chunk copied before cancel"
        );
    }

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
    fn archive_stem_strips_known_extensions() {
        let stem = |p: &str| archive_stem(Path::new(p));
        assert_eq!(stem("data.tar.gz"), "data");
        assert_eq!(stem("/tmp/foo.tar.bz2"), "foo");
        assert_eq!(stem("pkg.zip"), "pkg");
        assert_eq!(stem("x.tgz"), "x");
        assert_eq!(stem("a.tar.zst"), "a");
        assert_eq!(stem("archive.tar"), "archive");
        assert_eq!(stem("MyArchive.ZIP"), "MyArchive"); // case-insensitive match
        assert_eq!(stem("report.2024.tar.xz"), "report.2024"); // only the format ext goes
        assert_eq!(stem("noext"), "noext"); // unknown → unchanged
        assert_eq!(stem(".zip"), ".zip"); // dotfile, empty stem → fall back
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
    fn sniff_detects_misnamed_zip() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("real.zip");
        make_hello_zip(&zip, b"hi");
        let renamed = dir.path().join("mystery.bin");
        std::fs::rename(&zip, &renamed).unwrap();

        // Extension yields nothing; magic-byte sniff recovers the format.
        assert_eq!(ArchiveFormat::from_path(&renamed), None);
        assert_eq!(ArchiveFormat::detect(&renamed), Some(ArchiveFormat::Zip));
        assert!(is_archive(&renamed));

        let (fmt, entries) = list_archive(&renamed).unwrap();
        assert_eq!(fmt, ArchiveFormat::Zip);
        assert!(entries.iter().any(|e| e.path == "hello.txt"));

        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract_all(&renamed, &out).unwrap();
        assert!(out.join("hello.txt").exists());
    }

    #[test]
    fn roundtrip_tar_zst() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "zstd content").unwrap();

        let arc = dir.path().join("out.tar.zst");
        create_archive(&[src.join("a.txt")], &src, &arc).unwrap();

        let (fmt, entries) = list_archive(&arc).unwrap();
        assert_eq!(fmt, ArchiveFormat::TarZst);
        assert!(entries.iter().any(|e| e.path == "a.txt"));

        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract_all(&arc, &out).unwrap();
        assert_eq!(
            std::fs::read_to_string(out.join("a.txt")).unwrap(),
            "zstd content"
        );
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
        create_archive(&[src.join("a.txt"), src.join("sub")], &src, &tar_path).unwrap();

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
        assert!(
            !escaped.exists(),
            "file escaped extraction dir via Zip Slip"
        );
        assert!(dest.join("escape.txt").exists());
        for p in &extracted {
            assert!(p.starts_with(&dest), "extracted path {p:?} escaped dest");
        }
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

    #[cfg(unix)]
    #[test]
    fn roundtrip_preserves_exec_bit_and_symlink_tar() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("run.sh"), b"#!/bin/sh\necho hi\n").unwrap();
        std::fs::set_permissions(src.join("run.sh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        std::os::unix::fs::symlink("run.sh", src.join("link")).unwrap();

        let tar = dir.path().join("a.tar");
        create_archive(&[src.join("run.sh"), src.join("link")], &src, &tar).unwrap();

        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract_all(&tar, &out).unwrap();

        let mode = std::fs::metadata(out.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "executable bit lost on tar round-trip");

        let link_meta = std::fs::symlink_metadata(out.join("link")).unwrap();
        assert!(
            link_meta.file_type().is_symlink(),
            "tar symlink did not survive"
        );
        assert_eq!(
            std::fs::read_link(out.join("link")).unwrap(),
            Path::new("run.sh")
        );
    }

    #[cfg(unix)]
    #[test]
    fn roundtrip_preserves_exec_bit_and_symlink_zip() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("run.sh"), b"#!/bin/sh\necho hi\n").unwrap();
        std::fs::set_permissions(src.join("run.sh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        std::os::unix::fs::symlink("run.sh", src.join("link")).unwrap();

        let zip = dir.path().join("a.zip");
        create_archive(&[src.join("run.sh"), src.join("link")], &src, &zip).unwrap();

        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract_all(&zip, &out).unwrap();

        let mode = std::fs::metadata(out.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "executable bit lost on zip round-trip");

        let link_meta = std::fs::symlink_metadata(out.join("link")).unwrap();
        assert!(
            link_meta.file_type().is_symlink(),
            "zip symlink did not survive"
        );
        assert_eq!(
            std::fs::read_link(out.join("link")).unwrap(),
            Path::new("run.sh")
        );
    }

    /// Build a raw 512-byte USTAR symlink header. The safe `tar::Builder` rejects
    /// `..` in entry names, so a malicious archive must be hand-assembled.
    #[cfg(unix)]
    fn raw_tar_symlink_block(name: &str, target: &str) -> Vec<u8> {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..108].copy_from_slice(b"0000777\0"); // mode
        h[124..136].copy_from_slice(b"00000000000\0"); // size = 0
        h[156] = b'2'; // typeflag: symlink
        h[157..157 + target.len()].copy_from_slice(target.as_bytes());
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        // Checksum: sum of all bytes treating the checksum field as spaces.
        for b in &mut h[148..156] {
            *b = b' ';
        }
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        let chk = format!("{sum:06o}\0 ");
        h[148..156].copy_from_slice(chk.as_bytes());
        h.to_vec()
    }

    #[cfg(unix)]
    #[test]
    fn tar_symlink_entry_name_cannot_escape_dest() {
        let dir = tempfile::tempdir().unwrap();
        let tar = dir.path().join("evil.tar");
        {
            let mut bytes = raw_tar_symlink_block("../escape_link", "/etc/passwd");
            bytes.extend(std::iter::repeat_n(0u8, 1024)); // end-of-archive marker
            std::fs::write(&tar, &bytes).unwrap();
        }

        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();
        extract_all(&tar, &dest).unwrap();

        // The `..` is stripped: nothing lands above dest.
        assert!(!dir.path().join("escape_link").exists());
        let landed = dest.join("escape_link");
        assert!(
            std::fs::symlink_metadata(&landed)
                .unwrap()
                .file_type()
                .is_symlink(),
            "symlink should be recreated inside dest"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_slip_through_planted_symlink_is_blocked() {
        // entry A plants a symlink pointing outside dest; entry B tries to write
        // through it. The write must be refused, not escape into `outside`.
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();

        let zip = dir.path().join("evil.zip");
        {
            let file = File::create(&zip).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opt = zip::write::SimpleFileOptions::default();
            w.add_symlink("foo", outside.to_string_lossy().as_ref(), opt)
                .unwrap();
            w.start_file("foo/evil.txt", opt).unwrap();
            w.write_all(b"pwned").unwrap();
            w.finish().unwrap();
        }

        let dest = dir.path().join("dest");
        std::fs::create_dir(&dest).unwrap();
        extract_all(&zip, &dest).unwrap();

        assert!(
            !outside.join("evil.txt").exists(),
            "symlink slip escaped dest via a planted symlink"
        );
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
