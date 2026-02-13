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
