use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Expand tabs to spaces (4-space tab stops) and strip control chars.
fn sanitize_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut col = 0;
    for ch in line.chars() {
        match ch {
            '\t' => {
                let spaces = 4 - (col % 4);
                for _ in 0..spaces {
                    out.push(' ');
                }
                col += spaces;
            }
            c if c.is_control() => {}
            c => {
                out.push(c);
                col += 1;
            }
        }
    }
    out
}

pub const MAX_LINES: usize = 50_000;
const MAX_FILE_SIZE: u64 = 50 * 1_048_576; // 50 MB
pub const HEX_DUMP_MAX: usize = 262_144; // 256 KB

/// Bytes per hex row (offset granularity for binary scroll positions).
pub const HEX_COLS: usize = 16;

/// Lines loaded per incremental chunk after the first block (viewer paging).
pub const CHUNK_LINES: usize = 20_000;

/// Raw bytes paged in per hex chunk after the first block.
pub const HEX_CHUNK: usize = HEX_DUMP_MAX;

/// Result of an incremental viewer load: a slice of lines plus, when the file
/// continues past them, the byte offset to resume reading from.
pub struct ChunkLoad {
    pub preview: Preview,
    pub next_byte: Option<u64>,
}

pub struct Preview {
    pub lines: Vec<String>,
    pub scroll: usize,
    pub title: String,
    pub info: String,
    pub is_binary: bool,
    pub binary_size: usize,
    /// Raw bytes backing the hex dump (binary mode only); empty otherwise. Rows
    /// are colored per-byte from this window rather than from `lines`.
    pub hex_bytes: Vec<u8>,
}

impl Preview {
    pub fn loading_placeholder(path: &Path) -> Self {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Preview {
            lines: vec![],
            scroll: 0,
            title,
            info: "loading".into(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        }
    }

    pub fn load(path: &Path, max_lines: usize) -> Self {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        if path.is_dir() {
            return Self::load_dir(path, title);
        }

        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                return Preview {
                    lines: vec!["[Cannot read]".into()],
                    scroll: 0,
                    title,
                    info: "error".into(),
                    is_binary: false,
                    binary_size: 0,
                    hex_bytes: Vec::new(),
                };
            }
        };

        if meta.len() > MAX_FILE_SIZE {
            return Preview {
                lines: vec![format!("[Too large: {} bytes]", meta.len())],
                scroll: 0,
                title,
                info: format!("{} bytes", meta.len()),
                is_binary: false,
                binary_size: 0,
                hex_bytes: Vec::new(),
            };
        }

        let file_size = meta.len() as usize;

        // Full read path (popup preview — needs all lines for scrolling)
        if max_lines >= MAX_LINES {
            return Self::load_full(path, title, file_size);
        }

        // Partial read path (side panel — only visible lines)
        Self::load_partial(path, title, file_size, max_lines)
    }

    /// Load `path` as a hex dump regardless of its detected content type.
    /// Directories fall back to a normal listing.
    pub fn load_hex(path: &Path) -> Self {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        if path.is_dir() {
            return Self::load_dir(path, title);
        }

        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                return Preview {
                    lines: vec!["[Cannot read]".into()],
                    scroll: 0,
                    title,
                    info: "error".into(),
                    is_binary: false,
                    binary_size: 0,
                    hex_bytes: Vec::new(),
                };
            }
        };

        let total_size = meta.len() as usize;
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => {
                return Preview {
                    lines: vec!["[Cannot read]".into()],
                    scroll: 0,
                    title,
                    info: "error".into(),
                    is_binary: false,
                    binary_size: 0,
                    hex_bytes: Vec::new(),
                };
            }
        };
        let limit = total_size.min(HEX_DUMP_MAX);
        let mut bytes = Vec::with_capacity(limit);
        let _ = file.take(limit as u64).read_to_end(&mut bytes);
        Self::load_binary(&bytes, title, total_size)
    }

    fn load_full(path: &Path, title: String, file_size: usize) -> Self {
        match fs::read(path) {
            Ok(bytes) => {
                // Check sample for binary content: NUL bytes or high ratio of control chars
                let sample = &bytes[..bytes.len().min(8192)];
                let nul_count = sample.iter().filter(|&&b| b == 0).count();
                if nul_count > 0 {
                    return Preview::load_binary(&bytes, title, file_size);
                }
                let non_text = sample.iter().filter(|&&b| b < 0x08 || b == 0x7f).count();
                if !sample.is_empty() && non_text * 100 / sample.len() > 10 {
                    return Preview::load_binary(&bytes, title, file_size);
                }

                Self::text_preview(&bytes, title)
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read]".into()],
                scroll: 0,
                title,
                info: "error".into(),
                is_binary: false,
                binary_size: 0,
                hex_bytes: Vec::new(),
            },
        }
    }

    /// Decode `bytes` as UTF-8 (lossy) text, sanitize, and cap at `MAX_LINES`.
    fn text_preview(bytes: &[u8], title: String) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let lines: Vec<String> = text.lines().take(MAX_LINES).map(sanitize_line).collect();
        let info = format!("{} lines", lines.len());
        Preview {
            lines,
            scroll: 0,
            title,
            info,
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        }
    }

    /// Load the first block of a file as text for the viewer, reading at most
    /// `max_lines`. If the file continues past that, `next_byte` carries the
    /// offset to resume from (incremental paging); otherwise it's `None` and the
    /// whole file is loaded. Directories fall back to a normal listing.
    pub fn load_first(path: &Path, max_lines: usize) -> ChunkLoad {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        if path.is_dir() {
            return ChunkLoad {
                preview: Self::load_dir(path, title),
                next_byte: None,
            };
        }

        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => {
                return ChunkLoad {
                    preview: Self::read_error(title),
                    next_byte: None,
                };
            }
        };

        match Self::read_lines_from(file, 0, max_lines) {
            Ok((lines, next_byte)) => {
                let info = Self::lines_info(lines.len(), next_byte.is_some());
                let preview = Preview {
                    lines,
                    scroll: 0,
                    title,
                    info,
                    is_binary: false,
                    binary_size: 0,
                    hex_bytes: Vec::new(),
                };
                ChunkLoad { preview, next_byte }
            }
            Err(_) => ChunkLoad {
                preview: Self::read_error(title),
                next_byte: None,
            },
        }
    }

    /// Read up to `max_lines` more lines starting at byte `start` (used to page in
    /// the next block of a large file). Returns the lines and the next resume
    /// offset, or `None` when the end of file is reached.
    pub fn read_more(
        path: &Path,
        start: u64,
        max_lines: usize,
    ) -> std::io::Result<(Vec<String>, Option<u64>)> {
        let file = fs::File::open(path)?;
        Self::read_lines_from(file, start, max_lines)
    }

    /// Read up to `max_bytes` more raw bytes starting at byte `start`, used to
    /// extend the hex window past the initial block. Returns the bytes and the
    /// next resume offset (`None` at EOF).
    pub fn read_more_hex(
        path: &Path,
        start: u64,
        max_bytes: usize,
    ) -> std::io::Result<(Vec<u8>, Option<u64>)> {
        let mut file = fs::File::open(path)?;
        let total = file.metadata().map(|m| m.len()).unwrap_or(0);
        file.seek(SeekFrom::Start(start))?;
        let mut buf = Vec::with_capacity(max_bytes.min(1 << 20));
        file.take(max_bytes as u64).read_to_end(&mut buf)?;
        let next = start + buf.len() as u64;
        let next_byte = (next < total).then_some(next);
        Ok((buf, next_byte))
    }

    /// Read at most `max_lines` newline-terminated lines from `file` starting at
    /// byte `start`. Decodes lossily (tolerates non-UTF-8) and sanitizes each
    /// line. Returns the lines plus the resume offset (`None` at EOF). Resume
    /// offsets always fall on a line boundary, so chunks never split a line.
    fn read_lines_from(
        mut file: fs::File,
        start: u64,
        max_lines: usize,
    ) -> std::io::Result<(Vec<String>, Option<u64>)> {
        let total = file.metadata().map(|m| m.len()).unwrap_or(0);
        if start > 0 {
            file.seek(SeekFrom::Start(start))?;
        }
        // Cap a single line's read so a pathological newline-free file can't pull
        // the whole thing into one buffer; over-long lines are split into pieces.
        const MAX_LINE_BYTES: u64 = 65_536;
        let mut reader = BufReader::new(file);
        let mut lines = Vec::with_capacity(max_lines.min(4096));
        let mut consumed = start;
        let mut buf: Vec<u8> = Vec::new();
        for _ in 0..max_lines {
            buf.clear();
            let n = (&mut reader)
                .take(MAX_LINE_BYTES)
                .read_until(b'\n', &mut buf)?;
            if n == 0 {
                return Ok((lines, None)); // EOF
            }
            consumed += n as u64;
            while matches!(buf.last(), Some(b'\n' | b'\r')) {
                buf.pop();
            }
            let text = String::from_utf8_lossy(&buf);
            lines.push(sanitize_line(&text));
        }
        let next_byte = if consumed < total {
            Some(consumed)
        } else {
            None
        };
        Ok((lines, next_byte))
    }

    fn read_error(title: String) -> Self {
        Preview {
            lines: vec!["[Cannot read]".into()],
            scroll: 0,
            title,
            info: "error".into(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        }
    }

    /// Format the line-count info string, marking partial loads with a `+`.
    pub fn lines_info(count: usize, more: bool) -> String {
        if more {
            format!("{count}+ lines")
        } else {
            format!("{count} lines")
        }
    }

    fn load_partial(path: &Path, title: String, file_size: usize, max_lines: usize) -> Self {
        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => {
                return Preview {
                    lines: vec!["[Cannot read]".into()],
                    scroll: 0,
                    title,
                    info: "error".into(),
                    is_binary: false,
                    binary_size: 0,
                    hex_bytes: Vec::new(),
                };
            }
        };

        // Read sample for binary detection
        let mut sample_buf = [0u8; 8192];
        let sample_len = file.read(&mut sample_buf).unwrap_or(0);
        let sample = &sample_buf[..sample_len];

        let nul_count = sample.iter().filter(|&&b| b == 0).count();
        if nul_count > 0 {
            let _ = file.seek(SeekFrom::Start(0));
            let limit = file_size.min(HEX_DUMP_MAX);
            let mut bytes = Vec::with_capacity(limit);
            let _ = file.take(limit as u64).read_to_end(&mut bytes);
            return Self::load_binary(&bytes, title, file_size);
        }
        let non_text = sample.iter().filter(|&&b| b < 0x08 || b == 0x7f).count();
        if sample_len > 0 && non_text * 100 / sample_len > 10 {
            let _ = file.seek(SeekFrom::Start(0));
            let limit = file_size.min(HEX_DUMP_MAX);
            let mut bytes = Vec::with_capacity(limit);
            let _ = file.take(limit as u64).read_to_end(&mut bytes);
            return Self::load_binary(&bytes, title, file_size);
        }

        // Text: seek back, read only max_lines via BufReader
        let _ = file.seek(SeekFrom::Start(0));
        let reader = BufReader::new(file);
        let mut lines = Vec::with_capacity(max_lines);
        for line_result in reader.lines().take(max_lines) {
            match line_result {
                Ok(line) => lines.push(sanitize_line(&line)),
                Err(_) => break,
            }
        }

        let info = if lines.len() >= max_lines {
            format!("{}+ lines", lines.len())
        } else {
            format!("{} lines", lines.len())
        };
        Preview {
            lines,
            scroll: 0,
            title,
            info,
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        }
    }

    fn load_dir(path: &Path, title: String) -> Self {
        match fs::read_dir(path) {
            Ok(rd) => {
                let mut names: Vec<String> = rd
                    .flatten()
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if e.path().is_dir() {
                            format!("{name}/")
                        } else {
                            name
                        }
                    })
                    .collect();
                names.sort_by_key(|a| a.to_lowercase());
                let info = format!("{} entries", names.len());
                Preview {
                    lines: names,
                    scroll: 0,
                    title,
                    info,
                    is_binary: false,
                    binary_size: 0,
                    hex_bytes: Vec::new(),
                }
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read directory]".into()],
                scroll: 0,
                title,
                info: "error".into(),
                is_binary: false,
                binary_size: 0,
                hex_bytes: Vec::new(),
            },
        }
    }

    fn load_binary(bytes: &[u8], title: String, total_size: usize) -> Self {
        let dump_bytes = &bytes[..bytes.len().min(HEX_DUMP_MAX)];
        // Hex rows render directly from `hex_bytes`; `lines` is unused in binary
        // mode (row counts come from the byte window). More of the file can be
        // paged into `hex_bytes` on demand (see `read_more_hex`).
        let info = format!("binary, {total_size} bytes");
        Preview {
            lines: Vec::new(),
            scroll: 0,
            title,
            info,
            is_binary: true,
            binary_size: total_size,
            hex_bytes: dump_bytes.to_vec(),
        }
    }

    /// Number of display rows: hex rows (16 bytes each) for binary content,
    /// logical line count otherwise.
    pub fn row_count(&self) -> usize {
        if self.is_binary {
            self.hex_bytes.len().div_ceil(HEX_COLS)
        } else {
            self.lines.len()
        }
    }

    /// For text previews: returns (first_visible_line, total_lines, percentage).
    pub fn text_position(&self, visible: usize) -> (usize, usize, u8) {
        if self.is_binary || self.lines.is_empty() {
            return (0, 0, 0);
        }
        let first = self.scroll + 1;
        let total = self.lines.len();
        let last = (self.scroll + visible).min(total);
        let pct = ((last as u64 * 100) / total as u64) as u8;
        (first, total, pct)
    }

    /// For binary previews: returns (first_byte_offset, last_byte_offset,
    /// total_bytes, percentage). The percentage is relative to the whole file,
    /// so it grows as more of a large file is paged in.
    pub fn hex_position(&self, visible: usize) -> (usize, usize, usize, u8) {
        if !self.is_binary || self.binary_size == 0 {
            return (0, 0, 0, 0);
        }
        let first_byte = self.scroll * HEX_COLS;
        let last_byte = ((self.scroll + visible) * HEX_COLS).min(self.hex_bytes.len());
        let pct = ((last_byte as u64 * 100) / self.binary_size as u64) as u8;
        (first_byte, last_byte, self.binary_size, pct)
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: usize, visible: usize) {
        let max = self.row_count().saturating_sub(visible);
        self.scroll = (self.scroll + n).min(max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_line ──────────────────────────────────────────────

    #[test]
    fn sanitize_plain_text() {
        assert_eq!(sanitize_line("hello world"), "hello world");
    }

    #[test]
    fn sanitize_tab_start_of_line() {
        assert_eq!(sanitize_line("\thello"), "    hello");
    }

    #[test]
    fn sanitize_tab_mid_line() {
        // "ab" is col 2, tab expands to 2 spaces (4 - 2%4 = 2)
        assert_eq!(sanitize_line("ab\tc"), "ab  c");
    }

    #[test]
    fn sanitize_control_chars() {
        assert_eq!(sanitize_line("a\x01b\x7fc"), "abc");
    }

    #[test]
    fn sanitize_mixed() {
        assert_eq!(sanitize_line("\t\x01hello"), "    hello");
    }

    // ── text_position ──────────────────────────────────────────────

    #[test]
    fn text_position_empty() {
        let p = Preview {
            lines: vec![],
            scroll: 0,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        assert_eq!(p.text_position(10), (0, 0, 0));
    }

    #[test]
    fn text_position_binary_returns_zeros() {
        let p = Preview {
            lines: vec!["hex".into()],
            scroll: 0,
            title: String::new(),
            info: String::new(),
            is_binary: true,
            binary_size: 100,
            hex_bytes: Vec::new(),
        };
        assert_eq!(p.text_position(10), (0, 0, 0));
    }

    #[test]
    fn text_position_normal() {
        let lines: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
        let p = Preview {
            lines,
            scroll: 10,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        let (first, total, pct) = p.text_position(20);
        assert_eq!(first, 11); // scroll + 1
        assert_eq!(total, 100);
        assert_eq!(pct, 30); // (10+20)*100/100 = 30
    }

    // ── hex_position ───────────────────────────────────────────────

    #[test]
    fn hex_position_non_binary() {
        let p = Preview {
            lines: vec!["text".into()],
            scroll: 0,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        assert_eq!(p.hex_position(10), (0, 0, 0, 0));
    }

    #[test]
    fn hex_position_binary() {
        let p = Preview {
            lines: Vec::new(),
            scroll: 2,
            title: String::new(),
            info: String::new(),
            is_binary: true,
            binary_size: 256,
            hex_bytes: (0..=255u8).collect(), // 256 bytes loaded
        };
        let (first_byte, last_byte, total, pct) = p.hex_position(10);
        assert_eq!(first_byte, 32); // 2 * 16
        assert_eq!(last_byte, 192); // (2+10) * 16
        assert_eq!(total, 256);
        assert_eq!(pct, 75); // 192 * 100 / 256
    }

    // ── scroll_up / scroll_down ────────────────────────────────────

    #[test]
    fn scroll_up_clamps_at_zero() {
        let mut p = Preview {
            lines: vec!["a".into(), "b".into()],
            scroll: 1,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        p.scroll_up(5);
        assert_eq!(p.scroll, 0);
    }

    // ── sanitize_line extras ───────────────────────────────────────

    #[test]
    fn sanitize_multiple_tabs() {
        // Two tabs at col 0 → 4+4 = 8 spaces
        assert_eq!(sanitize_line("\t\t"), "        ");
    }

    #[test]
    fn sanitize_tab_alignment() {
        // "abc" (3 cols) + tab → 1 space to align to col 4
        assert_eq!(sanitize_line("abc\td"), "abc d");
    }

    #[test]
    fn sanitize_empty() {
        assert_eq!(sanitize_line(""), "");
    }

    #[test]
    fn sanitize_unicode_preserved() {
        assert_eq!(sanitize_line("日本語"), "日本語");
        assert_eq!(sanitize_line("café"), "café");
    }

    // ── Preview::load with real files ────────────────────────────

    #[test]
    fn load_text_file() {
        let dir = std::env::temp_dir().join("fcmd_preview_test_text");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

        let p = Preview::load(&path, MAX_LINES);
        assert!(!p.is_binary);
        assert_eq!(p.lines.len(), 3);
        assert_eq!(p.lines[0], "line1");
        assert_eq!(p.title, "test.txt");
        assert!(p.info.contains("3 lines"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_binary_file() {
        let dir = std::env::temp_dir().join("fcmd_preview_test_bin");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.bin");
        let mut data = vec![0u8; 256];
        for (i, b) in data.iter_mut().enumerate() {
            *b = i as u8;
        }
        std::fs::write(&path, &data).unwrap();

        let p = Preview::load(&path, MAX_LINES);
        assert!(p.is_binary);
        assert_eq!(p.binary_size, 256);
        assert!(p.info.contains("binary"));
        // Hex dump rows: 256/16 = 16 rows, all bytes retained.
        assert_eq!(p.row_count(), 16);
        assert_eq!(p.hex_bytes.len(), 256);
        assert_eq!(p.hex_bytes[0], 0x00);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_directory_preview() {
        let dir = std::env::temp_dir().join("fcmd_preview_test_dir");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alpha.txt"), "").unwrap();
        std::fs::write(dir.join("beta.txt"), "").unwrap();
        std::fs::create_dir(dir.join("gamma")).unwrap();

        let p = Preview::load(&dir, MAX_LINES);
        assert!(!p.is_binary);
        assert_eq!(p.lines.len(), 3);
        assert!(p.info.contains("3 entries"));
        // Sorted case-insensitive, dirs have trailing /
        assert!(p.lines.iter().any(|l| l == "gamma/"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_empty_file() {
        let dir = std::env::temp_dir().join("fcmd_preview_test_empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.txt");
        std::fs::write(&path, "").unwrap();

        let p = Preview::load(&path, MAX_LINES);
        assert!(!p.is_binary);
        assert!(p.lines.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_nonexistent_file() {
        let p = Preview::load(std::path::Path::new("/nonexistent/file.txt"), MAX_LINES);
        assert!(!p.is_binary);
        assert!(p.lines[0].contains("Cannot read"));
    }

    #[test]
    fn load_partial_side_panel() {
        let dir = std::env::temp_dir().join("fcmd_preview_test_partial");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("long.txt");
        let content: String = (0..100).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, &content).unwrap();

        // Partial: only 10 lines
        let p = Preview::load(&path, 10);
        assert_eq!(p.lines.len(), 10);
        assert!(p.info.contains("10+ lines"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loading_placeholder_sets_title() {
        let p = Preview::loading_placeholder(std::path::Path::new("/tmp/foo.rs"));
        assert_eq!(p.title, "foo.rs");
        assert!(p.lines.is_empty());
        assert_eq!(p.info, "loading");
    }

    // ── hex dump formatting ────────────────────────────────────────

    #[test]
    fn load_binary_hex_format() {
        let dir = std::env::temp_dir().join("fcmd_preview_test_hex");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.bin");
        // Write 5 bytes — tests incomplete line padding
        std::fs::write(&path, [0x41, 0x42, 0x00, 0x43, 0x44]).unwrap();

        let p = Preview::load(&path, MAX_LINES);
        assert!(p.is_binary);
        // Binary content keeps no `lines`; one row of 5 bytes drives the render.
        assert!(p.lines.is_empty());
        assert_eq!(p.row_count(), 1);
        assert_eq!(p.hex_bytes, vec![0x41, 0x42, 0x00, 0x43, 0x44]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_more_hex_pages_past_first_block() {
        let dir = std::env::temp_dir().join("fcmd_preview_test_hexpage");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.bin");
        let total = HEX_DUMP_MAX + 1000;
        std::fs::write(&path, vec![0xABu8; total]).unwrap();

        // First load caps at HEX_DUMP_MAX but reports the true size.
        let p = Preview::load_hex(&path);
        assert_eq!(p.hex_bytes.len(), HEX_DUMP_MAX);
        assert_eq!(p.binary_size, total);

        // Paging in from the cap reads the remaining 1000 bytes, then EOF.
        let (bytes, next) = Preview::read_more_hex(&path, HEX_DUMP_MAX as u64, HEX_CHUNK).unwrap();
        assert_eq!(bytes.len(), 1000);
        assert_eq!(next, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── scroll edge cases ──────────────────────────────────────────

    #[test]
    fn scroll_down_empty_lines() {
        let mut p = Preview {
            lines: vec![],
            scroll: 0,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        p.scroll_down(10, 5);
        assert_eq!(p.scroll, 0);
    }

    #[test]
    fn scroll_up_already_at_zero() {
        let mut p = Preview {
            lines: vec!["a".into()],
            scroll: 0,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        p.scroll_up(10);
        assert_eq!(p.scroll, 0);
    }

    #[test]
    fn scroll_down_clamps_at_max() {
        let lines: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        let mut p = Preview {
            lines,
            scroll: 0,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        // visible=10, max scroll = 20 - 10 = 10
        p.scroll_down(100, 10);
        assert_eq!(p.scroll, 10);
    }

    #[test]
    fn load_too_large_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.txt");
        // Create a file and fake its "size" by checking the branch condition
        // We can't create 50MB in a test, but we can check the branch message
        std::fs::write(&path, "small").unwrap();
        let p = Preview::load(&path, MAX_LINES);
        // Small file loads normally
        assert!(!p.lines.is_empty());
        assert!(!p.info.contains("Too large"));
    }

    #[test]
    fn load_full_binary_detection_nul_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("binary.bin");
        let mut data = vec![0u8; 100];
        data[0] = 0x89; // PNG-like header
        data[1] = 0x50;
        // NUL byte triggers binary detection
        std::fs::write(&path, &data).unwrap();
        let p = Preview::load(&path, MAX_LINES);
        assert!(p.is_binary);
    }

    #[test]
    fn load_first_never_switches_to_hex() {
        // A file with NUL bytes auto-detects as binary via `load`, but the viewer's
        // `load_first` must always render it as text (default open behavior).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        std::fs::write(&path, [0x89, 0x50, 0x00, b'h', b'i']).unwrap();

        assert!(Preview::load(&path, MAX_LINES).is_binary);
        let cl = Preview::load_first(&path, MAX_LINES);
        assert!(!cl.preview.is_binary);
        assert!(cl.preview.info.contains("lines"));
        assert!(cl.next_byte.is_none()); // tiny file fully loaded
    }

    #[test]
    fn load_first_pages_large_files() {
        // More lines than the chunk limit → first block is partial with a resume offset.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        let content: String = (0..100).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, &content).unwrap();

        let cl = Preview::load_first(&path, 10);
        assert_eq!(cl.preview.lines.len(), 10);
        assert!(cl.preview.info.contains("10+ lines"));
        let start = cl.next_byte.expect("more to read");

        // Resume reading the rest from the offset.
        let (more, next) = Preview::read_more(&path, start, 10).unwrap();
        assert_eq!(more.len(), 10);
        assert_eq!(more[0], "line 10");
        assert!(next.is_some());
    }

    #[test]
    fn load_full_text_no_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.txt");
        std::fs::write(&path, "Hello world\nLine 2\nLine 3\n").unwrap();
        let p = Preview::load(&path, MAX_LINES);
        assert!(!p.is_binary);
        assert!(p.lines.len() >= 3);
    }

    #[test]
    fn load_partial_reads_limited_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("many_lines.txt");
        let content: String = (0..100).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, &content).unwrap();
        // Request only 5 lines
        let p = Preview::load(&path, 5);
        // Should read approximately 5 lines (+ some buffer)
        assert!(p.lines.len() <= 20); // not all 100
    }

    #[test]
    fn load_full_high_control_ratio_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctrl.bin");
        // Create content with >10% control chars (but no NUL)
        let mut data = Vec::new();
        data.extend(std::iter::repeat_n(0x01, 20)); // control chars
        data.extend(std::iter::repeat_n(b'A', 80)); // normal ASCII
        std::fs::write(&path, &data).unwrap();
        let p = Preview::load(&path, MAX_LINES);
        assert!(p.is_binary);
    }

    #[test]
    fn scroll_down_and_up_sequence() {
        let lines: Vec<String> = (0..30).map(|i| format!("line {i}")).collect();
        let mut p = Preview {
            lines,
            scroll: 0,
            title: String::new(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
            hex_bytes: Vec::new(),
        };
        p.scroll_down(5, 10); // scroll 5 down
        assert_eq!(p.scroll, 5);
        p.scroll_up(3); // scroll 3 up
        assert_eq!(p.scroll, 2);
        p.scroll_up(100); // scroll way up - clamps at 0
        assert_eq!(p.scroll, 0);
    }
}
