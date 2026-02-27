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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn handle_rename_char_input() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Rename;
        app.rename_input.clear();

        app.handle_rename(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        app.handle_rename(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(app.rename_input, "hi");
    }

    #[tokio::test]
    async fn handle_rename_backspace_and_empty_exits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Rename;
        app.rename_input = "ab".into();

        app.handle_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.rename_input, "a");
        assert_eq!(app.mode, Mode::Rename);

        app.handle_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.rename_input, "");
        assert_eq!(app.mode, Mode::Rename);

        // Empty backspace exits to Normal
        app.handle_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_rename_esc_exits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Rename;
        app.rename_input = "test".into();
        app.handle_rename(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_rename_enter_empty_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Rename;
        app.rename_input.clear();
        app.handle_rename(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_create_char_input() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Create;
        app.rename_input.clear();

        app.handle_create(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.rename_input, "f");
    }

    #[tokio::test]
    async fn handle_create_esc_exits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Create;
        app.handle_create(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_create_enter_empty_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Create;
        app.rename_input.clear();
        app.handle_create(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }
}
