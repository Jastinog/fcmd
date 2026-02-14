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
        if let Some(ref db) = self.db
            && let Err(e) = db.save_bookmark(name, &path)
        {
            self.status_message = format!("Bookmark error: {e}");
            return;
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
        if let Some(ref db) = self.db {
            let _ = db.remove_bookmark(name);
        }
        self.bookmarks.retain(|(n, _)| n != name);
    }

    pub(super) fn goto_bookmark(&mut self) {
        let Some((_, path)) = self.bookmarks.get(self.bookmark_cursor).cloned() else {
            return;
        };
        self.mode = Mode::Normal;
        if path.is_dir() {
            let panel = self.active_panel_mut();
            panel.path = path;
            panel.selected = 0;
            panel.offset = 0;
            panel.marked.clear();
            if let Err(e) = panel.load_dir() {
                self.status_message = format!("Bookmark error: {e}");
            } else {
                self.apply_dir_sort();
            }
        } else {
            self.status_message = "Bookmark directory no longer exists".into();
        }
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
                let name = self.bookmarks[self.bookmark_cursor].0.clone();
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
        if self.bookmarks.is_empty() {
            return;
        }
        let old_name = self.bookmarks[self.bookmark_cursor].0.clone();
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
        if let Some(ref db) = self.db
            && let Err(e) = db.rename_bookmark(old_name, new_name)
        {
            self.status_message = format!("Bookmark error: {e}");
            return;
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
