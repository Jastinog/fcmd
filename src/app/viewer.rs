use super::*;

use crate::viewer::Viewer;

impl App {
    /// Open the full-screen viewer for `path`, showing a loading placeholder while
    /// the content loads asynchronously.
    pub(super) fn open_viewer(&mut self, path: PathBuf) {
        self.viewer = Some(Viewer::loading(path.clone()));
        self.mode = Mode::Viewer;
        self.spawn_viewer_load(path, false);
    }

    /// Spawn the async content load for the viewer. When `force_hex` is set the
    /// content is loaded as a hex dump regardless of its detected type; otherwise
    /// the first block of text is loaded (the rest pages in on scroll).
    pub(super) fn spawn_viewer_load(&mut self, path: PathBuf, force_hex: bool) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        // Replacing the receiver drops any in-flight load (stale result is ignored).
        self.viewer_load_rx = Some(rx);
        // Also cancel any in-flight chunk load from a previous file/mode.
        self.viewer_chunk_rx = None;
        tokio::task::spawn_blocking(move || {
            // The viewer always opens as text; hex is an explicit toggle, so the
            // binary → hex auto-switch in `Preview::load` is intentionally bypassed.
            let (preview, next_byte) = if force_hex {
                let p = Preview::load_hex(&path);
                // Page in the rest of the file as the user scrolls.
                let next = (p.hex_bytes.len() < p.binary_size).then_some(p.hex_bytes.len() as u64);
                (p, next)
            } else {
                let cl = Preview::load_first(&path, crate::preview::MAX_LINES);
                (cl.preview, cl.next_byte)
            };
            let _ = tx.send(super::ViewerLoadResult {
                path,
                preview,
                next_byte,
            });
        });
    }

    /// If the viewport is nearing the end of the loaded portion of a large file,
    /// spawn a load of the next block.
    pub(super) fn viewer_maybe_load_more(&mut self) {
        let visible = self.viewer_visible_height.max(1);
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        if !v.wants_more(visible) {
            return;
        }
        let Some(start) = v.next_byte else { return };
        let is_binary = v.content.is_binary;
        v.loading_more = true;
        let path = v.path.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.viewer_chunk_rx = Some(rx);
        tokio::task::spawn_blocking(move || {
            if is_binary {
                if let Ok((hex_bytes, next_byte)) =
                    Preview::read_more_hex(&path, start, crate::preview::HEX_CHUNK)
                {
                    let _ = tx.send(super::ViewerChunkResult {
                        path,
                        lines: Vec::new(),
                        hex_bytes,
                        next_byte,
                    });
                }
            } else if let Ok((lines, next_byte)) =
                Preview::read_more(&path, start, crate::preview::CHUNK_LINES)
            {
                let _ = tx.send(super::ViewerChunkResult {
                    path,
                    lines,
                    hex_bytes: Vec::new(),
                    next_byte,
                });
            }
        });
    }

    /// Apply an incrementally-loaded block, appending it to the viewer content.
    pub fn apply_viewer_chunk(&mut self, result: ViewerChunkResult) {
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        if v.path != result.path {
            return;
        }
        if v.content.is_binary {
            v.append_hex(result.hex_bytes, result.next_byte);
            v.rescan_hex_search();
        } else {
            v.append_lines(result.lines, result.next_byte);
            self.spawn_viewer_highlight();
        }
        // Keep paging until the viewport's prefetch margin is satisfied.
        self.viewer_maybe_load_more();
    }

    /// Toggle the forced-hex view and reload the content in the new mode.
    fn toggle_hex(&mut self) {
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        v.force_hex = !v.force_hex;
        let path = v.path.clone();
        let force_hex = v.force_hex;
        self.spawn_viewer_load(path, force_hex);
    }

    /// Apply an async viewer content load, guarding against stale results.
    /// Kicks off syntax highlighting and any further paging needed.
    pub fn apply_viewer_load(&mut self, result: ViewerLoadResult) {
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        if v.path != result.path {
            return;
        }
        v.set_content(result.preview);
        v.next_byte = result.next_byte;
        if !v.content.is_binary {
            self.spawn_viewer_highlight();
        }
        // A short first block may not fill the screen — page in more if needed.
        self.viewer_maybe_load_more();
    }

    /// Spawn async syntax highlighting for the current viewer content.
    fn spawn_viewer_highlight(&mut self) {
        let Some(v) = self.viewer.as_ref() else {
            return;
        };
        let path = v.path.clone();
        let lines = v.content.lines.clone();
        let line_count = lines.len();
        let dark = self.theme_is_dark();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.viewer_hl_rx = Some(rx);
        tokio::task::spawn_blocking(move || {
            if let Some(cache) = crate::viewer::highlight(&lines, &path, dark) {
                let _ = tx.send(super::ViewerHlResult {
                    path,
                    line_count,
                    cache,
                });
            }
        });
    }

    /// Apply async syntax-highlight result, guarding against stale content.
    pub fn apply_viewer_highlight(&mut self, result: ViewerHlResult) {
        if let Some(ref mut v) = self.viewer
            && v.path == result.path
            && !v.content.is_binary
            && v.content.lines.len() == result.line_count
        {
            v.highlight = Some(result.cache);
        }
    }

    /// Whether the active theme has a dark background (drives syntect theme choice).
    fn theme_is_dark(&self) -> bool {
        match self.theme.bg {
            ratatui::style::Color::Rgb(r, g, b) => {
                // Rec. 601 luma; below mid-grey counts as dark.
                (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000 < 128
            }
            // Reset/transparent or named: assume a dark terminal.
            _ => true,
        }
    }

    /// Close the viewer and return to normal mode.
    fn close_viewer(&mut self) {
        self.viewer = None;
        self.mode = Mode::Normal;
    }

    /// Effective number of content rows visible in the viewer (set during render).
    fn viewer_visible(&self) -> usize {
        self.viewer_visible_height.max(1)
    }

    pub(super) fn handle_viewer(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let visible = self.viewer_visible();
        let is_binary = self.viewer.as_ref().is_some_and(|v| v.content.is_binary);
        // Motion and search keys are routed by content type inside the `Viewer`
        // helpers (hex moves the byte cursor, text scrolls); arms that change app
        // state (`self`) re-borrow the viewer themselves.
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_viewer(),
            KeyCode::Char('x') | KeyCode::Tab => self.toggle_hex(),
            KeyCode::Char('o') => {
                if let Some(v) = self.viewer.as_ref() {
                    let path = v.path.clone();
                    self.request_open_editor(path);
                }
            }
            KeyCode::Char('/') => {
                if let Some(v) = self.viewer.as_mut() {
                    v.search_clear();
                }
                self.mode = Mode::ViewerSearch;
            }
            // Goto byte offset (hex view only).
            KeyCode::Char(':') if is_binary => {
                if let Some(v) = self.viewer.as_mut() {
                    v.goto.clear();
                }
                self.mode = Mode::ViewerGoto;
            }
            _ => {
                if let Some(v) = self.viewer.as_mut() {
                    match key.code {
                        KeyCode::Char('j') | KeyCode::Down => v.move_down(1, visible),
                        KeyCode::Char('k') | KeyCode::Up => v.move_up(1, visible),
                        KeyCode::Char('d') if ctrl => v.move_down(visible / 2, visible),
                        KeyCode::Char('u') if ctrl => v.move_up(visible / 2, visible),
                        KeyCode::Char('f') if ctrl => v.move_down(visible, visible),
                        KeyCode::PageDown => v.move_down(visible, visible),
                        KeyCode::Char('b') if ctrl => v.move_up(visible, visible),
                        KeyCode::PageUp => v.move_up(visible, visible),
                        KeyCode::Char('G') => v.move_bottom(visible),
                        KeyCode::Char('g') => v.move_top(),
                        KeyCode::Char('w') => v.toggle_wrap(),
                        KeyCode::Char('#') => v.toggle_line_numbers(),
                        KeyCode::Char('l') | KeyCode::Right => v.move_right(visible),
                        KeyCode::Char('h') | KeyCode::Left => v.move_left(visible),
                        KeyCode::Char('0') => v.scroll_line_start(),
                        KeyCode::Char('n') => v.search_advance(visible),
                        KeyCode::Char('N') => v.search_retreat(visible),
                        _ => {}
                    }
                }
            }
        }
        // Page in more of a large file if we scrolled near the end.
        self.viewer_maybe_load_more();
    }

    pub(super) fn handle_viewer_search(&mut self, key: KeyEvent) {
        let visible = self.viewer_visible();
        match key.code {
            KeyCode::Char(c) => {
                if let Some(ref mut v) = self.viewer {
                    v.search_push(c, visible);
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut v) = self.viewer {
                    v.search_pop(visible);
                }
            }
            KeyCode::Enter => {
                self.mode = Mode::Viewer;
            }
            KeyCode::Esc => {
                if let Some(ref mut v) = self.viewer {
                    v.search_clear();
                }
                self.mode = Mode::Viewer;
            }
            _ => {}
        }
    }

    /// Handle input while entering a goto-offset in the hex viewer.
    pub(super) fn handle_viewer_goto(&mut self, key: KeyEvent) {
        let visible = self.viewer_visible();
        match key.code {
            KeyCode::Char(c) => {
                if let Some(ref mut v) = self.viewer {
                    v.goto.push(c);
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut v) = self.viewer {
                    v.goto.pop();
                }
            }
            KeyCode::Enter => {
                if let Some(ref mut v) = self.viewer {
                    if let Some(off) = crate::viewer::parse_offset(&v.goto) {
                        v.goto_offset(off, visible);
                    }
                    v.goto.clear();
                }
                self.mode = Mode::Viewer;
            }
            KeyCode::Esc => {
                if let Some(ref mut v) = self.viewer {
                    v.goto.clear();
                }
                self.mode = Mode::Viewer;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn open_with_lines(lines: Vec<&str>) -> App {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.viewer_visible_height = 3;
        let mut v = Viewer::loading(PathBuf::from("/test/a.txt"));
        v.content.lines = lines.into_iter().map(|s| s.to_string()).collect();
        v.content.info = String::new();
        // Build the display-row layout the nav handlers clamp against.
        v.refresh_layout(80);
        app.viewer = Some(v);
        app.mode = Mode::Viewer;
        app
    }

    #[tokio::test]
    async fn scroll_down_up() {
        let lines: Vec<&str> = (0..20).map(|_| "line").collect();
        let mut app = open_with_lines(lines);
        app.handle_viewer(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.viewer.as_ref().unwrap().content.scroll, 1);
        app.handle_viewer(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.viewer.as_ref().unwrap().content.scroll, 0);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn g_and_G() {
        let lines: Vec<&str> = (0..10).map(|_| "line").collect();
        let mut app = open_with_lines(lines);
        app.viewer_visible_height = 2;
        app.viewer.as_mut().unwrap().content.scroll = 5;
        app.handle_viewer(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.viewer.as_ref().unwrap().content.scroll, 0);
        app.handle_viewer(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.viewer.as_ref().unwrap().content.scroll, 8); // 10 - 2
    }

    #[tokio::test]
    async fn esc_closes() {
        let mut app = open_with_lines(vec!["line"]);
        app.handle_viewer(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.viewer.is_none());
    }

    #[tokio::test]
    async fn slash_enters_search() {
        let mut app = open_with_lines(vec!["line"]);
        app.handle_viewer(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::ViewerSearch);
    }

    #[tokio::test]
    async fn search_char_input_populates_matches() {
        let mut app = open_with_lines(vec!["hello world", "foo bar"]);
        app.mode = Mode::ViewerSearch;
        app.handle_viewer_search(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(app.viewer.as_ref().unwrap().search.query, "h");
        assert!(!app.viewer.as_ref().unwrap().search.matches.is_empty());
    }

    #[tokio::test]
    async fn search_esc_clears() {
        let mut app = open_with_lines(vec!["test line"]);
        app.mode = Mode::ViewerSearch;
        {
            let v = app.viewer.as_mut().unwrap();
            v.search.query = "test".into();
            v.search.matches = vec![(0, 0)];
        }
        app.handle_viewer_search(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Viewer);
        assert!(app.viewer.as_ref().unwrap().search.query.is_empty());
        assert!(app.viewer.as_ref().unwrap().search.matches.is_empty());
    }
}
