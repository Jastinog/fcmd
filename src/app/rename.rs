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
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.file_op_rx = Some(rx);
                let new_name2 = new_name.clone();
                tokio::task::spawn_blocking(move || {
                    let result =
                        ops::rename_path(&path, &new_name2).map_err(|e| e.to_string());
                    let _ = tx.send(super::FileOpResult::Rename {
                        new_name: new_name2,
                        result,
                    });
                });
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
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.file_op_rx = Some(rx);
                if name.ends_with('/') {
                    let dir_name = name.trim_end_matches('/').to_string();
                    tokio::task::spawn_blocking(move || {
                        let result = ops::mkdir(&dir, &dir_name).map_err(|e| e.to_string());
                        let _ = tx.send(super::FileOpResult::Mkdir {
                            name: dir_name,
                            result,
                        });
                    });
                } else {
                    let name2 = name.clone();
                    tokio::task::spawn_blocking(move || {
                        let result = ops::touch(&dir, &name2).map_err(|e| e.to_string());
                        let _ = tx.send(super::FileOpResult::Touch {
                            name: name2,
                            result,
                        });
                    });
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
