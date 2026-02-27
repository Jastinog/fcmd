use super::*;

impl App {
    pub(super) fn open_bookmarks(&mut self) {
        if self.bookmarks.is_empty() {
            self.status_message = "No bookmarks set. Use B to add one.".into();
            return;
        }
        self.bookmark_cursor = 0;
        self.bookmark_scroll = 0;
        self.mode = Mode::Bookmarks;
    }

    pub(super) fn add_bookmark_prompt(&mut self) {
        let entry = match self.active_panel().selected_entry() {
            Some(e) if e.is_dir || e.name == ".." => e,
            _ => {
                self.status_message = "Select a directory to bookmark".into();
                return;
            }
        };
        let path = entry.path.clone();
        let default_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.bookmark_add_path = Some(path);
        self.rename_input = default_name;
        self.mode = Mode::BookmarkAdd;
    }

    pub(super) fn add_bookmark(&mut self, name: &str, path: PathBuf) {
        {
            let n = name.to_string();
            let p = path.clone();
            self.db_spawn(move |db| { let _ = db.save_bookmark(&n, &p); });
        }
        // Update in-memory list (keep sorted)
        if let Some(pos) = self.bookmarks.iter().position(|(n, _)| n == name) {
            self.bookmarks[pos].1 = path;
        } else {
            self.bookmarks.push((name.to_string(), path));
            self.bookmarks.sort_by(|a, b| a.0.cmp(&b.0));
        }
        self.status_message = format!("Bookmark added: {name}");
    }

    pub(super) fn remove_bookmark_by_name(&mut self, name: &str) {
        let n = name.to_string();
        self.db_spawn(move |db| { let _ = db.remove_bookmark(&n); });
        self.bookmarks.retain(|(n, _)| n != name);
    }

    pub(super) fn goto_bookmark(&mut self) {
        let Some((_, path)) = self.bookmarks.get(self.bookmark_cursor).cloned() else {
            return;
        };
        self.mode = Mode::Normal;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.nav_check_rx = Some(rx);
        tokio::task::spawn_blocking(move || {
            let exists = path.exists();
            let is_dir = path.is_dir();
            let _ = tx.send(super::NavCheckResult {
                path,
                is_dir,
                exists,
                source: super::NavSource::Bookmark,
            });
        });
    }

    pub(super) fn handle_bookmarks(&mut self, key: KeyEvent) {
        let len = self.bookmarks.len();
        if len == 0 {
            self.mode = Mode::Normal;
            return;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.bookmark_cursor = (self.bookmark_cursor + 1).min(len - 1);
                self.adjust_bookmark_scroll();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.bookmark_cursor = self.bookmark_cursor.saturating_sub(1);
                self.adjust_bookmark_scroll();
            }
            KeyCode::Char('G') => {
                self.bookmark_cursor = len - 1;
                self.adjust_bookmark_scroll();
            }
            KeyCode::Char('g') => {
                self.bookmark_cursor = 0;
                self.adjust_bookmark_scroll();
            }
            KeyCode::Enter => {
                self.goto_bookmark();
            }
            KeyCode::Char('d') => {
                let Some((name, _)) = self.bookmarks.get(self.bookmark_cursor).cloned() else {
                    return;
                };
                self.remove_bookmark_by_name(&name);
                self.status_message = format!("Bookmark removed: {name}");
                if self.bookmarks.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.bookmark_cursor = self.bookmark_cursor.min(self.bookmarks.len() - 1);
                    self.adjust_bookmark_scroll();
                }
            }
            KeyCode::Char('e') => {
                self.rename_bookmark_prompt();
            }
            KeyCode::Char('a') => {
                self.mode = Mode::Normal;
                self.add_bookmark_prompt();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    pub(super) fn rename_bookmark_prompt(&mut self) {
        let Some((old_name, _)) = self.bookmarks.get(self.bookmark_cursor).cloned() else {
            return;
        };
        self.rename_input = old_name.clone();
        self.bookmark_rename_old = Some(old_name);
        self.mode = Mode::BookmarkRename;
    }

    pub(super) fn handle_bookmark_add(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let name = self.rename_input.trim().to_string();
                self.rename_input.clear();
                let path = self.bookmark_add_path.take();
                if name.is_empty() {
                    self.mode = Mode::Normal;
                    return;
                }
                if let Some(path) = path {
                    self.add_bookmark(&name, path);
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.rename_input.clear();
                self.bookmark_add_path = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                if self.rename_input.is_empty() {
                    self.bookmark_add_path = None;
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

    pub(super) fn handle_bookmark_rename(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let new_name = self.rename_input.trim().to_string();
                self.rename_input.clear();
                let old_name = self.bookmark_rename_old.take();
                if new_name.is_empty() {
                    self.mode = Mode::Normal;
                    return;
                }
                if let Some(old) = old_name {
                    self.rename_bookmark(&old, &new_name);
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.rename_input.clear();
                self.bookmark_rename_old = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                if self.rename_input.is_empty() {
                    self.bookmark_rename_old = None;
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

    pub(super) fn rename_bookmark(&mut self, old_name: &str, new_name: &str) {
        if !self.bookmarks.iter().any(|(n, _)| n == old_name) {
            self.status_message = format!("Bookmark not found: {old_name}");
            return;
        }
        {
            let old = old_name.to_string();
            let new = new_name.to_string();
            self.db_spawn(move |db| { let _ = db.rename_bookmark(&old, &new); });
        }
        if let Some(pos) = self.bookmarks.iter().position(|(n, _)| n == old_name) {
            self.bookmarks[pos].0 = new_name.to_string();
            self.bookmarks.sort_by(|a, b| a.0.cmp(&b.0));
        }
        self.status_message = format!("Bookmark renamed: {old_name} -> {new_name}");
    }

    fn adjust_bookmark_scroll(&mut self) {
        let max_h = (self.visible_height * 70 / 100).max(2);
        let list_h = max_h.saturating_sub(4).max(1);
        if self.bookmark_cursor < self.bookmark_scroll {
            self.bookmark_scroll = self.bookmark_cursor;
        } else if self.bookmark_cursor >= self.bookmark_scroll + list_h {
            self.bookmark_scroll = self.bookmark_cursor - list_h + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn app_with_bookmarks() -> App {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("alpha", PathBuf::from("/a"));
        app.add_bookmark("beta", PathBuf::from("/b"));
        app.add_bookmark("gamma", PathBuf::from("/c"));
        app
    }

    #[tokio::test]
    async fn handle_bookmarks_j_k_navigation() {
        let mut app = app_with_bookmarks();
        app.mode = Mode::Bookmarks;
        app.bookmark_cursor = 0;

        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 1);
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 2);
        // Clamped at max
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 2);

        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 1);
    }

    #[tokio::test]
    async fn handle_bookmarks_G_g() {
        let mut app = app_with_bookmarks();
        app.mode = Mode::Bookmarks;

        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 2);

        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 0);
    }

    #[tokio::test]
    async fn handle_bookmarks_esc_exits() {
        let mut app = app_with_bookmarks();
        app.mode = Mode::Bookmarks;
        app.handle_bookmarks(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_bookmarks_d_deletes() {
        let mut app = app_with_bookmarks();
        app.mode = Mode::Bookmarks;
        app.bookmark_cursor = 0; // "alpha"
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(app.bookmarks.len(), 2);
        assert!(app.status_message.contains("removed"));
    }

    #[tokio::test]
    async fn handle_bookmarks_d_last_exits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("only", PathBuf::from("/only"));
        app.mode = Mode::Bookmarks;
        app.bookmark_cursor = 0;
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(app.bookmarks.is_empty());
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_bookmarks_empty_exits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Bookmarks;
        // No bookmarks → should exit immediately
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_bookmark_add_char_and_esc() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::BookmarkAdd;
        app.rename_input.clear();
        app.bookmark_add_path = Some(PathBuf::from("/test"));

        app.handle_bookmark_add(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(app.rename_input, "x");

        app.handle_bookmark_add(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.rename_input.is_empty());
        assert!(app.bookmark_add_path.is_none());
    }

    #[tokio::test]
    async fn handle_bookmark_add_enter_creates() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::BookmarkAdd;
        app.rename_input = "test_bm".into();
        app.bookmark_add_path = Some(PathBuf::from("/test/dir"));

        app.handle_bookmark_add(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.bookmarks.len(), 1);
        assert_eq!(app.bookmarks[0].0, "test_bm");
    }

    #[tokio::test]
    async fn handle_bookmark_add_empty_enter_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::BookmarkAdd;
        app.rename_input.clear();
        app.handle_bookmark_add(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.bookmarks.is_empty());
    }

    #[tokio::test]
    async fn handle_bookmark_rename_enter() {
        let mut app = app_with_bookmarks();
        app.mode = Mode::BookmarkRename;
        app.bookmark_rename_old = Some("alpha".into());
        app.rename_input = "alpha_new".into();

        app.handle_bookmark_rename(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.bookmarks.iter().any(|(n, _)| n == "alpha_new"));
        assert!(!app.bookmarks.iter().any(|(n, _)| n == "alpha"));
    }

    #[tokio::test]
    async fn handle_bookmark_rename_esc() {
        let mut app = app_with_bookmarks();
        app.mode = Mode::BookmarkRename;
        app.bookmark_rename_old = Some("alpha".into());
        app.rename_input = "new_name".into();
        app.handle_bookmark_rename(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.bookmark_rename_old.is_none());
        // Original bookmark still there
        assert!(app.bookmarks.iter().any(|(n, _)| n == "alpha"));
    }

    #[tokio::test]
    async fn add_bookmark_prompt_non_dir_rejected() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "a.txt" (file, not dir)
        app.add_bookmark_prompt();
        assert_eq!(app.mode, Mode::Normal); // stayed Normal
        assert!(app.status_message.contains("directory"));
    }

    #[tokio::test]
    async fn rename_bookmark_nonexistent() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.rename_bookmark("nonexistent", "new");
        assert!(app.status_message.contains("not found"));
    }
}
