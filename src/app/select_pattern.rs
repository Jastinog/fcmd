use crossterm::event::{KeyCode, KeyEvent};

use crate::util::glob_match;

use super::*;

impl App {
    // + (select by pattern)
    pub(super) fn enter_select_pattern(&mut self) {
        self.rename_input = "*".into();
        self.mode = Mode::SelectPattern;
    }

    pub(super) fn handle_select_pattern(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let pattern = self.rename_input.trim().to_string();
                if pattern.is_empty() {
                    self.mode = Mode::Normal;
                    return;
                }
                let panel = self.active_panel_mut();
                let mut count = 0;
                for entry in &panel.entries {
                    if entry.name != ".." && glob_match(&pattern, &entry.name) {
                        panel.marked.insert(entry.path.clone());
                        count += 1;
                    }
                }
                if count > 0 {
                    self.mode = Mode::Select;
                    self.status_message = format!("Selected {count} items");
                } else {
                    self.mode = Mode::Normal;
                    self.status_message = "No matches".into();
                }
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                if self.rename_input.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.rename_input.pop();
                }
            }
            KeyCode::Char(c) => {
                self.rename_input.push(c);
            }
            _ => {}
        }
    }

    // - (unselect by pattern)
    pub(super) fn enter_unselect_pattern(&mut self) {
        self.rename_input = "*".into();
        self.mode = Mode::UnselectPattern;
    }

    pub(super) fn handle_unselect_pattern(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let pattern = self.rename_input.trim().to_string();
                if pattern.is_empty() {
                    self.mode = Mode::Normal;
                    return;
                }
                let panel = self.active_panel_mut();
                let mut count = 0;
                let paths_to_remove: Vec<PathBuf> = panel
                    .entries
                    .iter()
                    .filter(|e| e.name != ".." && glob_match(&pattern, &e.name))
                    .map(|e| e.path.clone())
                    .collect();
                for path in &paths_to_remove {
                    if panel.marked.remove(path) {
                        count += 1;
                    }
                }
                if panel.marked.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::Select;
                }
                self.status_message = format!("Unselected {count} items");
            }
            KeyCode::Esc => {
                self.mode = if self.active_panel().marked.is_empty() {
                    Mode::Normal
                } else {
                    Mode::Select
                };
            }
            KeyCode::Backspace => {
                if self.rename_input.is_empty() {
                    self.mode = if self.active_panel().marked.is_empty() {
                        Mode::Normal
                    } else {
                        Mode::Select
                    };
                } else {
                    self.rename_input.pop();
                }
            }
            KeyCode::Char(c) => {
                self.rename_input.push(c);
            }
            _ => {}
        }
    }

    // * (invert selection)
    pub(super) fn invert_selection(&mut self) {
        let panel = self.active_panel_mut();
        let mut selected = 0;
        for entry in &panel.entries {
            if entry.name == ".." {
                continue;
            }
            if panel.marked.contains(&entry.path) {
                panel.marked.remove(&entry.path);
            } else {
                panel.marked.insert(entry.path.clone());
                selected += 1;
            }
        }
        if panel.marked.is_empty() {
            self.mode = Mode::Normal;
            self.status_message = "Selection cleared".into();
        } else {
            self.mode = Mode::Select;
            self.status_message = format!("Selected {selected} items");
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
    async fn enter_select_pattern_sets_star_default() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.enter_select_pattern();
        assert_eq!(app.mode, Mode::SelectPattern);
        assert_eq!(app.rename_input, "*");
    }

    #[tokio::test]
    async fn select_pattern_enter_matches_all() {
        let entries = make_test_entries(&["a.txt", "b.rs", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.rename_input = "*.txt".into();
        app.handle_select_pattern(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Select);
        assert_eq!(app.active_panel().marked.len(), 2); // a.txt, c.txt
        assert!(app.status_message.contains("Selected 2"));
    }

    #[tokio::test]
    async fn select_pattern_no_match_returns_normal() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.rename_input = "*.xyz".into();
        app.handle_select_pattern(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_message.contains("No matches"));
    }

    #[tokio::test]
    async fn select_pattern_empty_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.rename_input = String::new();
        app.handle_select_pattern(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn select_pattern_esc_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.rename_input = "*.txt".into();
        app.handle_select_pattern(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn select_pattern_char_appends() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.rename_input = "*.tx".into();
        app.handle_select_pattern(key(KeyCode::Char('t')));
        assert_eq!(app.rename_input, "*.txt");
    }

    #[tokio::test]
    async fn select_pattern_backspace_pops() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.rename_input = "*.txt".into();
        app.handle_select_pattern(key(KeyCode::Backspace));
        assert_eq!(app.rename_input, "*.tx");
    }

    #[tokio::test]
    async fn select_pattern_backspace_empty_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.rename_input = String::new();
        app.handle_select_pattern(key(KeyCode::Backspace));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn enter_unselect_pattern_sets_star() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.enter_unselect_pattern();
        assert_eq!(app.mode, Mode::UnselectPattern);
        assert_eq!(app.rename_input, "*");
    }

    #[tokio::test]
    async fn unselect_pattern_removes_matching() {
        let entries = make_test_entries(&["a.txt", "b.rs", "c.txt"]);
        let mut app = App::new_for_test(entries);
        // Mark all
        app.select_all();
        assert_eq!(app.active_panel().marked.len(), 3);
        app.mode = Mode::UnselectPattern;
        app.rename_input = "*.txt".into();
        app.handle_unselect_pattern(key(KeyCode::Enter));
        // Only b.rs should remain
        assert_eq!(app.active_panel().marked.len(), 1);
        assert!(app.active_panel().marked.contains(&PathBuf::from("/test/b.rs")));
        assert_eq!(app.mode, Mode::Select);
        assert!(app.status_message.contains("Unselected 2"));
    }

    #[tokio::test]
    async fn unselect_pattern_all_cleared_returns_normal() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        app.mode = Mode::UnselectPattern;
        app.rename_input = "*".into();
        app.handle_unselect_pattern(key(KeyCode::Enter));
        assert!(app.active_panel().marked.is_empty());
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn unselect_pattern_esc_returns_to_select_if_marks() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        app.mode = Mode::UnselectPattern;
        app.handle_unselect_pattern(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Select); // has marks
    }

    #[tokio::test]
    async fn unselect_pattern_esc_returns_normal_if_no_marks() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::UnselectPattern;
        app.handle_unselect_pattern(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal); // no marks
    }

    #[tokio::test]
    async fn invert_selection_from_empty() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.invert_selection();
        assert_eq!(app.active_panel().marked.len(), 2);
        assert_eq!(app.mode, Mode::Select);
    }

    #[tokio::test]
    async fn invert_selection_deselects_all() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        app.invert_selection();
        assert!(app.active_panel().marked.is_empty());
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_message.contains("cleared"));
    }

    #[tokio::test]
    async fn invert_selection_partial() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().marked.insert(PathBuf::from("/test/a.txt"));
        app.invert_selection();
        // a.txt deselected, b.txt and c.txt selected
        assert_eq!(app.active_panel().marked.len(), 2);
        assert!(!app.active_panel().marked.contains(&PathBuf::from("/test/a.txt")));
        assert!(app.active_panel().marked.contains(&PathBuf::from("/test/b.txt")));
        assert!(app.active_panel().marked.contains(&PathBuf::from("/test/c.txt")));
    }
}
