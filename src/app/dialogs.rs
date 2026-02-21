use super::*;

impl App {
    pub(super) fn handle_sort(&mut self, key: KeyEvent) {
        let len = SortMode::ALL.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.sort_cursor = (self.sort_cursor + 1).min(len - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.sort_cursor = self.sort_cursor.saturating_sub(1);
            }
            KeyCode::Enter => {
                let mode = SortMode::ALL[self.sort_cursor];
                self.active_panel_mut().sort_mode = mode;
                self.reload_active_panel();
                self.save_current_sort();
                let arrow = if self.active_panel().sort_reverse {
                    "\u{2191}"
                } else {
                    "\u{2193}"
                };
                self.status_message = format!("Sort: {}{arrow}", mode.label());
                self.mode = Mode::Normal;
            }
            KeyCode::Char('r') => {
                let rev = !self.active_panel().sort_reverse;
                self.active_panel_mut().sort_reverse = rev;
                self.reload_active_panel();
                self.save_current_sort();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    pub(super) fn handle_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.execute_delete();
                self.active_panel_mut().marked.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let max = self.confirm_paths.len().saturating_sub(1);
                self.confirm_scroll = (self.confirm_scroll + 1).min(max);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.confirm_scroll = self.confirm_scroll.saturating_sub(1);
            }
            _ => {
                self.confirm_paths.clear();
                self.confirm_scroll = 0;
                // Return to Select mode if marks exist, otherwise Normal
                if self.active_panel().marked.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::Select;
                }
                self.status_message = "Cancelled".into();
            }
        }
    }

    pub(super) fn handle_help(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    pub(super) fn enter_theme_picker(&mut self) {
        if self.theme_list.is_empty() {
            self.theme_list = Theme::list_available();
            if self.theme_list.is_empty() {
                self.status_message = "No themes found".into();
                return;
            }
        }
        self.theme_cursor = self.theme_index.unwrap_or(0).min(self.theme_list.len() - 1);
        self.theme_scroll = self.theme_cursor.saturating_sub(5);
        self.theme_preview = None;
        self.spawn_theme_load();
        self.mode = Mode::ThemePicker;
    }

    pub(super) fn handle_theme_picker(&mut self, key: KeyEvent) {
        let len = self.theme_list.len();
        if len == 0 {
            self.mode = Mode::Normal;
            return;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.theme_cursor = (self.theme_cursor + 1).min(len - 1);
                self.adjust_theme_scroll();
                self.spawn_theme_load();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.theme_cursor = self.theme_cursor.saturating_sub(1);
                self.adjust_theme_scroll();
                self.spawn_theme_load();
            }
            KeyCode::Enter => {
                let Some(name) = self.theme_list.get(self.theme_cursor).cloned() else {
                    return;
                };
                if let Some(preview) = self.theme_preview.take() {
                    self.theme = preview;
                }
                self.theme_index = Some(self.theme_cursor);
                if let Some(ref db) = self.db {
                    let _ = db.save_theme(&name);
                }
                self.status_message = format!("Theme: {name}");
                self.mode = Mode::Normal;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.theme_preview = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn spawn_theme_load(&mut self) {
        if let Some(name) = self.theme_list.get(self.theme_cursor).cloned() {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.theme_load_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let _ = tx.send(Theme::load_by_name(&name));
            });
        }
    }

    fn adjust_theme_scroll(&mut self) {
        // Use visible_height to estimate list area (popup height - 4 for borders/separator/hint)
        let max_h = (self.visible_height * 70 / 100).max(2);
        let list_h = max_h.saturating_sub(4).max(1);
        if self.theme_cursor < self.theme_scroll {
            self.theme_scroll = self.theme_cursor;
        } else if self.theme_cursor >= self.theme_scroll + list_h {
            self.theme_scroll = self.theme_cursor - list_h + 1;
        }
    }
}
