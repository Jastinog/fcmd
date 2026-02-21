use super::*;

impl App {
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
            // Load theme list asynchronously; picker will be populated in apply_file_op
            self.mode = Mode::ThemePicker;
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.file_op_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let _ = tx.send(super::FileOpResult::ThemeList {
                    themes: Theme::list_available(),
                });
            });
            return;
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

    pub(super) fn spawn_theme_load(&mut self) {
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
