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

pub struct Preview {
    pub lines: Vec<String>,
    pub scroll: usize,
    pub title: String,
    pub info: String,
    pub is_binary: bool,
    pub binary_size: usize,
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

    fn load_full(path: &Path, title: String, file_size: usize) -> Self {
        match fs::read(path) {
            Ok(bytes) => {
                // Check sample for binary content: NUL bytes or high ratio of control chars
                let sample = &bytes[..bytes.len().min(8192)];
                let nul_count = sample.iter().filter(|&&b| b == 0).count();
                if nul_count > 0 {
                    return Preview::load_binary(&bytes, title, file_size);
                }
                let non_text = sample
                    .iter()
                    .filter(|&&b| b < 0x08 || b == 0x7f)
                    .count();
                if !sample.is_empty() && non_text * 100 / sample.len() > 10 {
                    return Preview::load_binary(&bytes, title, file_size);
                }

                let text = String::from_utf8_lossy(&bytes);
                let lines: Vec<String> = text.lines().take(MAX_LINES).map(|l| sanitize_line(l)).collect();
                let info = format!("{} lines", lines.len());
                Preview {
                    lines,
                    scroll: 0,
                    title,
                    info,
                    is_binary: false,
                    binary_size: 0,
                }
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read]".into()],
                scroll: 0,
                title,
                info: "error".into(),
                is_binary: false,
                binary_size: 0,
            },
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
                }
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read directory]".into()],
                scroll: 0,
                title,
                info: "error".into(),
                is_binary: false,
                binary_size: 0,
            },
        }
    }

    fn load_binary(bytes: &[u8], title: String, total_size: usize) -> Self {
        let dump_bytes = &bytes[..bytes.len().min(HEX_DUMP_MAX)];
        let mut lines = Vec::new();
        for chunk_offset in (0..dump_bytes.len()).step_by(16) {
            let chunk = &dump_bytes[chunk_offset..dump_bytes.len().min(chunk_offset + 16)];
            let mut hex_left = String::new();
            let mut hex_right = String::new();
            let mut ascii = String::new();
            for (i, &b) in chunk.iter().enumerate() {
                let hex = format!("{b:02x} ");
                if i < 8 {
                    hex_left.push_str(&hex);
                } else {
                    hex_right.push_str(&hex);
                }
                ascii.push(if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                });
            }
            // Pad incomplete lines
            for _ in chunk.len()..8 {
                hex_left.push_str("   ");
            }
            for i in chunk.len()..16 {
                if i >= 8 {
                    hex_right.push_str("   ");
                }
            }
            for _ in chunk.len()..16 {
                ascii.push(' ');
            }
            lines.push(format!(
                "{chunk_offset:08x}  {hex_left} {hex_right} |{ascii}|"
            ));
        }
        if total_size > HEX_DUMP_MAX {
            lines.push(format!("... truncated ({total_size} total bytes)"));
        }
        let info = format!("binary, {total_size} bytes");
        Preview {
            lines,
            scroll: 0,
            title,
            info,
            is_binary: true,
            binary_size: total_size,
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

    /// For binary previews: returns (first_byte_offset, last_byte_offset, total_bytes, percentage)
    pub fn hex_position(&self, visible: usize) -> (usize, usize, usize, u8) {
        if !self.is_binary || self.binary_size == 0 {
            return (0, 0, 0, 0);
        }
        let first_line = self.scroll;
        let last_line = (self.scroll + visible).min(self.lines.len());
        let first_byte = first_line * 16;
        // Last visible byte: each line is 16 bytes, but last line may be partial
        let dump_size = self.binary_size.min(HEX_DUMP_MAX);
        let last_byte = (last_line * 16).min(dump_size);
        let pct = if dump_size > 0 {
            ((last_byte as u64 * 100) / dump_size as u64) as u8
        } else {
            100
        };
        (first_byte, last_byte, self.binary_size, pct)
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: usize, visible: usize) {
        let max = self.lines.len().saturating_sub(visible);
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
        };
        assert_eq!(p.hex_position(10), (0, 0, 0, 0));
    }

    #[test]
    fn hex_position_binary() {
        let lines: Vec<String> = (0..16).map(|_| "hex line".into()).collect();
        let p = Preview {
            lines,
            scroll: 2,
            title: String::new(),
            info: String::new(),
            is_binary: true,
            binary_size: 256,
        };
        let (first_byte, last_byte, total, pct) = p.hex_position(10);
        assert_eq!(first_byte, 32);  // 2 * 16
        assert_eq!(last_byte, 192);  // (2+10) * 16
        assert_eq!(total, 256);
        assert_eq!(pct, 75);         // 192 * 100 / 256
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
        };
        p.scroll_up(5);
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
        };
        // visible=10, max scroll = 20 - 10 = 10
        p.scroll_down(100, 10);
        assert_eq!(p.scroll, 10);
    }
}
