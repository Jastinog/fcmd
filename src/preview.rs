use std::fs;
use std::path::Path;

const MAX_LINES: usize = 500;
const MAX_FILE_SIZE: u64 = 1_048_576;

pub struct Preview {
    pub lines: Vec<String>,
    pub scroll: usize,
    pub title: String,
    pub info: String,
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
                };
            }
        };

        if meta.len() > MAX_FILE_SIZE {
            return Preview {
                lines: vec![format!("[Too large: {} bytes]", meta.len())],
                scroll: 0,
                title,
                info: format!("{} bytes", meta.len()),
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
                    return Preview {
                        lines: vec!["[Binary file]".into()],
                        scroll: 0,
                        title,
                        info: format!("binary, {} bytes", bytes.len()),
                    };
                }

                let text = String::from_utf8_lossy(&bytes);
                let lines: Vec<String> = text.lines().take(MAX_LINES).map(String::from).collect();
                let info = format!("{} lines", lines.len());
                Preview {
                    lines,
                    scroll: 0,
                    title,
                    info,
                }
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read]".into()],
                scroll: 0,
                title,
                info: "error".into(),
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
                names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                let info = format!("{} entries", names.len());
                Preview {
                    lines: names,
                    scroll: 0,
                    title,
                    info,
                }
            }
            Err(_) => Preview {
                lines: vec!["[Cannot read directory]".into()],
                scroll: 0,
                title,
                info: "error".into(),
            },
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: usize, visible: usize) {
        let max = self.lines.len().saturating_sub(visible);
        self.scroll = (self.scroll + n).min(max);
    }
}
