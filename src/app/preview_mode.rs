use super::*;

impl App {
    pub(super) fn handle_preview(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.file_preview = None;
                self.file_preview_path = None;
                self.preview_search_query.clear();
                self.preview_search_matches.clear();
                self.preview_search_current = 0;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_down(1, self.visible_height);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_up(1);
                }
            }
            KeyCode::Char('d') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    let half = self.visible_height / 2;
                    p.scroll_down(half, self.visible_height);
                }
            }
            KeyCode::Char('u') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    let half = self.visible_height / 2;
                    p.scroll_up(half);
                }
            }
            KeyCode::Char('f') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_down(self.visible_height, self.visible_height);
                }
            }
            KeyCode::Char('b') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_up(self.visible_height);
                }
            }
            KeyCode::Char('G') => {
                if let Some(ref mut p) = self.file_preview {
                    let max = p.lines.len().saturating_sub(self.visible_height);
                    p.scroll = max;
                }
            }
            KeyCode::Char('g') => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll = 0;
                }
            }
            KeyCode::Char('o') => {
                if let Some(path) = self.file_preview_path.clone() {
                    self.request_open_editor(path);
                }
            }
            KeyCode::Char('/') => {
                self.preview_search_query.clear();
                self.preview_search_matches.clear();
                self.preview_search_current = 0;
                self.mode = Mode::PreviewSearch;
            }
            KeyCode::Char('n') => {
                if !self.preview_search_matches.is_empty() {
                    self.preview_search_current =
                        (self.preview_search_current + 1) % self.preview_search_matches.len();
                    self.scroll_preview_to_match();
                }
            }
            KeyCode::Char('N') => {
                if !self.preview_search_matches.is_empty() {
                    let len = self.preview_search_matches.len();
                    self.preview_search_current =
                        (self.preview_search_current + len - 1) % len;
                    self.scroll_preview_to_match();
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_preview_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.preview_search_query.push(c);
                self.update_preview_search_matches();
            }
            KeyCode::Backspace => {
                self.preview_search_query.pop();
                self.update_preview_search_matches();
            }
            KeyCode::Enter => {
                self.mode = Mode::Preview;
            }
            KeyCode::Esc => {
                self.preview_search_query.clear();
                self.preview_search_matches.clear();
                self.preview_search_current = 0;
                self.mode = Mode::Preview;
            }
            _ => {}
        }
    }

    fn update_preview_search_matches(&mut self) {
        const MAX_MATCHES: usize = 10_000;
        self.preview_search_matches.clear();
        self.preview_search_current = 0;
        let query = self.preview_search_query.to_lowercase();
        if query.is_empty() {
            return;
        }
        if let Some(ref p) = self.file_preview {
            'outer: for (line_idx, line) in p.lines.iter().enumerate() {
                let line_lower = line.to_lowercase();
                let mut start = 0;
                while let Some(pos) = line_lower[start..].find(&query) {
                    self.preview_search_matches.push((line_idx, start + pos));
                    if self.preview_search_matches.len() >= MAX_MATCHES {
                        break 'outer;
                    }
                    start += pos + query.len();
                }
            }
        }
        if !self.preview_search_matches.is_empty() {
            self.scroll_preview_to_match();
        }
    }

    fn scroll_preview_to_match(&mut self) {
        if let Some(&(line_idx, _)) = self.preview_search_matches.get(self.preview_search_current)
            && let Some(ref mut p) = self.file_preview
        {
            let visible = self.visible_height;
            if line_idx < p.scroll || line_idx >= p.scroll + visible {
                p.scroll = line_idx.saturating_sub(visible / 3);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_preview(lines: Vec<&str>) -> Preview {
        Preview {
            lines: lines.into_iter().map(|s| s.to_string()).collect(),
            scroll: 0,
            title: "test".into(),
            info: String::new(),
            is_binary: false,
            binary_size: 0,
        }
    }

    #[tokio::test]
    async fn handle_preview_scroll_down_up() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Preview;
        app.visible_height = 3; // small so scroll_down has room
        let lines: Vec<&str> = (0..20).map(|_| "line").collect();
        app.file_preview = Some(make_preview(lines));

        app.handle_preview(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.file_preview.as_ref().unwrap().scroll, 1);

        app.handle_preview(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.file_preview.as_ref().unwrap().scroll, 0);
    }

    #[tokio::test]
    async fn handle_preview_g_and_G() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Preview;
        app.visible_height = 2;
        let lines: Vec<&str> = (0..10).map(|_| "line").collect();
        app.file_preview = Some(make_preview(lines));
        app.file_preview.as_mut().unwrap().scroll = 5;

        // g → scroll to top
        app.handle_preview(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.file_preview.as_ref().unwrap().scroll, 0);

        // G → scroll to bottom
        app.handle_preview(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.file_preview.as_ref().unwrap().scroll, 8); // 10 - 2
    }

    #[tokio::test]
    async fn handle_preview_esc_clears() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Preview;
        app.file_preview = Some(make_preview(vec!["line"]));
        app.file_preview_path = Some(PathBuf::from("/test/a.txt"));

        app.handle_preview(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.file_preview.is_none());
        assert!(app.file_preview_path.is_none());
    }

    #[tokio::test]
    async fn handle_preview_slash_enters_search() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Preview;
        app.file_preview = Some(make_preview(vec!["line"]));

        app.handle_preview(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::PreviewSearch);
    }

    #[tokio::test]
    async fn handle_preview_search_char_input() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::PreviewSearch;
        app.file_preview = Some(make_preview(vec!["hello world", "foo bar"]));
        app.preview_search_query.clear();

        app.handle_preview_search(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(app.preview_search_query, "h");
        // Matches should be populated
        assert!(!app.preview_search_matches.is_empty());
    }

    #[tokio::test]
    async fn handle_preview_search_esc_clears() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::PreviewSearch;
        app.preview_search_query = "test".into();
        app.preview_search_matches = vec![(0, 0)];

        app.handle_preview_search(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Preview);
        assert!(app.preview_search_query.is_empty());
        assert!(app.preview_search_matches.is_empty());
    }

    #[tokio::test]
    async fn update_preview_search_matches_finds_positions() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.file_preview = Some(make_preview(vec![
            "Hello World",
            "hello again",
            "no match here",
        ]));
        app.preview_search_query = "hello".into();
        app.update_preview_search_matches();

        // "hello" appears at line 0 col 0 and line 1 col 0 (case-insensitive)
        assert_eq!(app.preview_search_matches.len(), 2);
        assert_eq!(app.preview_search_matches[0], (0, 0));
        assert_eq!(app.preview_search_matches[1], (1, 0));
    }

    #[tokio::test]
    async fn update_preview_search_matches_empty_query() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.file_preview = Some(make_preview(vec!["some text"]));
        app.preview_search_query.clear();
        app.update_preview_search_matches();
        assert!(app.preview_search_matches.is_empty());
    }
}
