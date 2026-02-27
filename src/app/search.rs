use super::*;

impl App {
    pub(super) fn handle_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.search_jump_to_match();
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                if self.search_query.is_empty() {
                    self.active_panel_mut().selected = self.search_saved_cursor;
                } else {
                    self.search_jump_to_match();
                }
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                if self.search_query.is_empty() {
                    self.active_panel_mut().selected = self.search_saved_cursor;
                }
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.active_panel_mut().selected = self.search_saved_cursor;
                self.search_query.clear();
            }
            _ => {}
        }
    }

    fn search_jump_to_match(&mut self) {
        let query = self.search_query.to_lowercase();
        let start = self.search_saved_cursor;

        let pos = {
            let panel = self.active_panel();
            let len = panel.entries.len();
            if len == 0 || query.is_empty() {
                None
            } else {
                (0..len)
                    .map(|i| (start + i) % len)
                    .find(|&i| panel.entries[i].name.to_lowercase().contains(&query))
            }
        };

        if let Some(pos) = pos {
            self.active_panel_mut().selected = pos;
        }
    }

    pub(super) fn search_next(&mut self) {
        if self.search_query.is_empty() {
            self.status_message = "No search pattern \u{2014} use / to search".into();
            return;
        }
        let query = self.search_query.to_lowercase();

        let pos = {
            let panel = self.active_panel();
            let len = panel.entries.len();
            if len == 0 {
                None
            } else {
                let start = (panel.selected + 1) % len;
                (0..len)
                    .map(|i| (start + i) % len)
                    .find(|&i| panel.entries[i].name.to_lowercase().contains(&query))
            }
        };

        if let Some(pos) = pos {
            self.active_panel_mut().selected = pos;
        } else {
            self.status_message = "No match".into();
        }
    }

    pub(super) fn search_prev(&mut self) {
        if self.search_query.is_empty() {
            self.status_message = "No search pattern \u{2014} use / to search".into();
            return;
        }
        let query = self.search_query.to_lowercase();

        let pos = {
            let panel = self.active_panel();
            let len = panel.entries.len();
            if len == 0 {
                None
            } else {
                let start = if panel.selected == 0 {
                    len - 1
                } else {
                    panel.selected - 1
                };
                (0..len)
                    .map(|i| (start + len - i) % len)
                    .find(|&i| panel.entries[i].name.to_lowercase().contains(&query))
            }
        };

        if let Some(pos) = pos {
            self.active_panel_mut().selected = pos;
        } else {
            self.status_message = "No match".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[tokio::test]
    async fn search_char_appends_and_jumps() {
        let entries = make_test_entries(&["apple.txt", "banana.txt", "cherry.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Search;
        app.search_saved_cursor = 0;
        app.handle_search(key(KeyCode::Char('b')));
        assert_eq!(app.search_query, "b");
        // Should jump to "banana.txt" (index 2)
        assert_eq!(app.active_panel().selected, 2);
    }

    #[tokio::test]
    async fn search_backspace_pops() {
        let entries = make_test_entries(&["apple.txt", "banana.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Search;
        app.search_saved_cursor = 0;
        app.search_query = "ban".into();
        app.handle_search(key(KeyCode::Backspace));
        assert_eq!(app.search_query, "ba");
    }

    #[tokio::test]
    async fn search_backspace_empty_restores_cursor() {
        let entries = make_test_entries(&["apple.txt", "banana.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Search;
        app.search_saved_cursor = 1;
        app.search_query = "a".into();
        app.active_panel_mut().selected = 2;
        app.handle_search(key(KeyCode::Backspace));
        assert!(app.search_query.is_empty());
        assert_eq!(app.active_panel().selected, 1); // restored
    }

    #[tokio::test]
    async fn search_esc_restores_cursor_and_clears() {
        let entries = make_test_entries(&["apple.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Search;
        app.search_saved_cursor = 0;
        app.search_query = "apple".into();
        app.active_panel_mut().selected = 1;
        app.handle_search(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.active_panel().selected, 0);
        assert!(app.search_query.is_empty());
    }

    #[tokio::test]
    async fn search_enter_keeps_position() {
        let entries = make_test_entries(&["apple.txt", "banana.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Search;
        app.search_saved_cursor = 0;
        app.search_query = "banana".into();
        app.active_panel_mut().selected = 2;
        app.handle_search(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.active_panel().selected, 2); // stays at found position
    }

    #[tokio::test]
    async fn search_enter_empty_restores_cursor() {
        let entries = make_test_entries(&["apple.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Search;
        app.search_saved_cursor = 0;
        app.active_panel_mut().selected = 1;
        app.handle_search(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.active_panel().selected, 0);
    }

    #[tokio::test]
    async fn search_next_wraps_around() {
        let entries = make_test_entries(&["a.txt", "ab.txt", "abc.txt"]);
        let mut app = App::new_for_test(entries);
        app.search_query = "a".into();
        app.active_panel_mut().selected = 3; // "abc.txt"
        app.search_next();
        // Should wrap to "a.txt" (index 1) since ".." at 0 doesn't match
        assert_eq!(app.active_panel().selected, 1);
    }

    #[tokio::test]
    async fn search_prev_wraps_backward() {
        let entries = make_test_entries(&["a.txt", "ab.txt", "abc.txt"]);
        let mut app = App::new_for_test(entries);
        app.search_query = "a".into();
        app.active_panel_mut().selected = 1; // "a.txt"
        app.search_prev();
        // Should find "abc.txt" (index 3) going backward
        assert_eq!(app.active_panel().selected, 3);
    }

    #[tokio::test]
    async fn search_no_match_shows_status() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.search_query = "zzz".into();
        app.search_next();
        assert!(app.status_message.contains("No match"));
    }
}
