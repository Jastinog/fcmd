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
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::F(1) => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    pub(super) fn enter_theme_picker(&mut self) {
        if self.theme_dark_list.is_empty() && self.theme_light_list.is_empty() {
            self.mode = Mode::ThemePicker;
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.file_op_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let (dark, light) = Theme::list_available_classified();
                let _ = tx.send(super::FileOpResult::ThemeList { dark, light });
            });
            return;
        }
        self.position_theme_cursors();
        self.theme_preview = None;
        self.spawn_theme_load();
        self.mode = Mode::ThemePicker;
    }

    pub(super) fn handle_theme_picker(&mut self, key: KeyEvent) {
        let col = self.theme_col;
        let list_len = self.theme_col_len(col);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if list_len > 0 {
                    self.theme_cursors[col] = (self.theme_cursors[col] + 1).min(list_len - 1);
                    self.adjust_theme_scroll();
                    self.spawn_theme_load();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.theme_cursors[col] = self.theme_cursors[col].saturating_sub(1);
                self.adjust_theme_scroll();
                self.spawn_theme_load();
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let other = 1 - col;
                if self.theme_col_len(other) > 0 {
                    self.theme_col = other;
                    self.spawn_theme_load();
                }
            }
            KeyCode::Enter => {
                let name = self.current_theme_name().map(|s| s.to_string());
                let Some(name) = name else { return };
                if let Some(preview) = self.theme_preview.take() {
                    self.theme = preview;
                }
                self.apply_transparency();
                self.theme_active_name = Some(name.clone());
                let n = name.clone();
                self.db_spawn(move |db| { let _ = db.save_theme(&n); });
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
        if let Some(name) = self.current_theme_name().map(|s| s.to_string()) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.theme_load_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let _ = tx.send(Theme::load_by_name(&name));
            });
        }
    }

    fn adjust_theme_scroll(&mut self) {
        let col = self.theme_col;
        let max_h = (self.visible_height * 70 / 100).max(2);
        let list_h = max_h.saturating_sub(4).max(1);
        if self.theme_cursors[col] < self.theme_scrolls[col] {
            self.theme_scrolls[col] = self.theme_cursors[col];
        } else if self.theme_cursors[col] >= self.theme_scrolls[col] + list_h {
            self.theme_scrolls[col] = self.theme_cursors[col] - list_h + 1;
        }
    }

    fn theme_col_len(&self, col: usize) -> usize {
        if col == 0 { self.theme_dark_list.len() } else { self.theme_light_list.len() }
    }

    fn current_theme_name(&self) -> Option<&str> {
        let list = if self.theme_col == 0 { &self.theme_dark_list } else { &self.theme_light_list };
        list.get(self.theme_cursors[self.theme_col]).map(|s| s.as_str())
    }

    /// Position cursors on the active theme after loading lists.
    pub(super) fn position_theme_cursors(&mut self) {
        let active = self.theme_active_name.as_deref();
        // Try to find active theme in dark list
        if let Some(name) = active {
            if let Some(pos) = self.theme_dark_list.iter().position(|n| n == name) {
                self.theme_col = 0;
                self.theme_cursors[0] = pos;
                self.theme_scrolls[0] = pos.saturating_sub(5);
                return;
            }
            if let Some(pos) = self.theme_light_list.iter().position(|n| n == name) {
                self.theme_col = 1;
                self.theme_cursors[1] = pos;
                self.theme_scrolls[1] = pos.saturating_sub(5);
                return;
            }
        }
        // Fallback: first dark theme
        self.theme_col = 0;
        self.theme_cursors = [0; 2];
        self.theme_scrolls = [0; 2];
    }
}
