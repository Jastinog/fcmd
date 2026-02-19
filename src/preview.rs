use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use ratatui::style::Color;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

const MAX_LINES: usize = 50_000;
const MAX_FILE_SIZE: u64 = 50 * 1_048_576; // 50 MB
pub const HEX_DUMP_MAX: usize = 262_144; // 256 KB

/// Skip syntax highlighting for files above this size (lines)
const HIGHLIGHT_MAX_LINES: usize = 5_000;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static HIGHLIGHT_THEME: LazyLock<syntect::highlighting::Theme> = LazyLock::new(|| {
    let ts = ThemeSet::load_defaults();
    ts.themes["base16-ocean.dark"].clone()
});

pub struct StyledSegment {
    pub text: String,
    pub style: ratatui::style::Style,
}

pub struct Preview {
    pub lines: Vec<String>,
    pub scroll: usize,
    pub title: String,
    pub info: String,
    pub is_binary: bool,
    pub binary_size: usize,
    pub styled_lines: Option<Vec<Vec<StyledSegment>>>,
}

impl Preview {
    pub fn load(path: &Path) -> Self {
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
                    styled_lines: None,
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
                styled_lines: None,
            };
        }

        match fs::read(path) {
            Ok(bytes) => {
                // Check larger sample for non-text bytes (NUL, 0x01-0x07, DEL)
                let sample = &bytes[..bytes.len().min(8192)];
                let non_text = sample
                    .iter()
                    .filter(|&&b| b == 0 || b < 0x08 || b == 0x7f)
                    .count();
                if non_text > 0 {
                    return Preview::load_binary(&bytes, title);
                }

                let text = String::from_utf8_lossy(&bytes);
                let lines: Vec<String> = text.lines().take(MAX_LINES).map(String::from).collect();
                let info = format!("{} lines", lines.len());
                Preview {
                    lines,
                    scroll: 0,
                    title,
                    info,
                    is_binary: false,
                    binary_size: 0,
                    styled_lines: None,
                }
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read]".into()],
                scroll: 0,
                title,
                info: "error".into(),
                is_binary: false,
                binary_size: 0,
                styled_lines: None,
            },
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
                    styled_lines: None,
                }
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read directory]".into()],
                scroll: 0,
                title,
                info: "error".into(),
                is_binary: false,
                binary_size: 0,
                styled_lines: None,
            },
        }
    }

    fn load_binary(bytes: &[u8], title: String) -> Self {
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
        if bytes.len() > HEX_DUMP_MAX {
            lines.push(format!("... truncated ({} total bytes)", bytes.len()));
        }
        let info = format!("binary, {} bytes", bytes.len());
        Preview {
            lines,
            scroll: 0,
            title,
            info,
            is_binary: true,
            binary_size: bytes.len(),
            styled_lines: None,
        }
    }

    pub fn apply_highlighting(&mut self, path: &Path) {
        if self.is_binary || self.lines.is_empty() || self.lines.len() > HIGHLIGHT_MAX_LINES {
            return;
        }
        let syntax = match SYNTAX_SET.find_syntax_for_file(path) {
            Ok(Some(s)) => s,
            _ => return,
        };
        let mut h = syntect::easy::HighlightLines::new(syntax, &HIGHLIGHT_THEME);
        let mut result = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            let regions = match h.highlight_line(line, &SYNTAX_SET) {
                Ok(r) => r,
                Err(_) => {
                    return; // abort on error, fall back to plain
                }
            };
            let segments: Vec<StyledSegment> = regions
                .into_iter()
                .map(|(style, text)| {
                    let fg = style.foreground;
                    StyledSegment {
                        text: text.to_string(),
                        style: ratatui::style::Style::default()
                            .fg(Color::Rgb(fg.r, fg.g, fg.b)),
                    }
                })
                .collect();
            result.push(segments);
        }
        self.styled_lines = Some(result);
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
