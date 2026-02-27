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
