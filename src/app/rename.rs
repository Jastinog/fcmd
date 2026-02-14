use super::*;

impl App {
    pub(super) fn handle_rename(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let new_name = self.rename_input.trim().to_string();
                if new_name.is_empty() {
                    self.mode = Mode::Normal;
                    return;
                }
                let path = match self
                    .active_panel()
                    .selected_entry()
                    .filter(|e| e.name != "..")
                {
                    Some(e) => e.path.clone(),
                    None => {
                        self.mode = Mode::Normal;
                        return;
                    }
                };
                match ops::rename_path(&path, &new_name) {
                    Ok(rec) => {
                        self.undo_stack.push(vec![rec]);
                        self.refresh_panels();
                        self.active_panel_mut().select_by_name(&new_name);
                        self.status_message = format!("Renamed to: {new_name}");
                    }
                    Err(e) => self.status_message = format!("rename: {e}"),
                }
                self.mode = Mode::Normal;
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

    pub(super) fn handle_create(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let name = self.rename_input.trim().to_string();
                if name.is_empty() {
                    self.mode = Mode::Normal;
                    return;
                }
                let dir = self.active_panel().path.clone();
                if name.ends_with('/') {
                    let dir_name = name.trim_end_matches('/');
                    match ops::mkdir(&dir, dir_name) {
                        Ok(rec) => {
                            self.undo_stack.push(vec![rec]);
                            self.refresh_panels();
                            self.active_panel_mut().select_by_name(dir_name);
                            self.status_message = format!("Created directory: {dir_name}");
                        }
                        Err(e) => self.status_message = format!("mkdir: {e}"),
                    }
                } else {
                    match ops::touch(&dir, &name) {
                        Ok(rec) => {
                            self.undo_stack.push(vec![rec]);
                            self.refresh_panels();
                            self.active_panel_mut().select_by_name(&name);
                            self.status_message = format!("Created file: {name}");
                        }
                        Err(e) => self.status_message = format!("touch: {e}"),
                    }
                }
                self.mode = Mode::Normal;
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
}
