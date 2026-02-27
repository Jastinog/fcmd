use super::*;

impl App {
    pub(super) fn open_find_local(&mut self) {
        let base = self.active_panel().path.clone();
        self.find_state = Some(FindState::new_local(&base));
        self.mode = Mode::Find;
    }

    pub(super) fn open_find_global(&mut self) {
        let base = self.active_panel().path.clone();
        self.find_state = Some(FindState::new_global(&base));
        self.mode = Mode::Find;
    }

    pub(super) fn handle_find(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Navigation
        let nav_up = matches!(key.code, KeyCode::Up)
            || (ctrl && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('k')));
        let nav_down = matches!(key.code, KeyCode::Down)
            || (ctrl && matches!(key.code, KeyCode::Char('n') | KeyCode::Char('j')));

        if nav_up {
            if let Some(ref mut fs) = self.find_state {
                fs.move_up();
            }
            return;
        }
        if nav_down {
            if let Some(ref mut fs) = self.find_state {
                fs.move_down();
            }
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.find_state = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.accept_find();
            }
            KeyCode::Tab => {
                if let Some(ref fs) = self.find_state {
                    let mut new_state = fs.switch_scope();
                    new_state.update_filter();
                    self.find_state = Some(new_state);
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut fs) = self.find_state {
                    fs.query.pop();
                    if fs.scope == FindScope::Global {
                        fs.trigger_search();
                    } else {
                        fs.update_filter();
                    }
                }
            }
            KeyCode::Char(c) if !ctrl => {
                if let Some(ref mut fs) = self.find_state {
                    fs.query.push(c);
                    if fs.scope == FindScope::Global {
                        fs.trigger_search();
                    } else {
                        fs.update_filter();
                    }
                }
            }
            _ => {}
        }
    }

    fn accept_find(&mut self) {
        let target = self
            .find_state
            .as_ref()
            .and_then(|fs| fs.selected_path())
            .map(|p| p.to_path_buf());
        let is_dir = self
            .find_state
            .as_ref()
            .map(|fs| fs.selected_is_dir())
            .unwrap_or(false);

        self.find_state = None;
        self.mode = Mode::Normal;

        let Some(path) = target else { return };

        let side = self.tab().active;
        if is_dir {
            self.navigate_cached(path, side, None);
        } else if let Some(parent) = path.parent() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            self.navigate_cached(parent.to_path_buf(), side, name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::FindState;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[tokio::test]
    async fn handle_find_esc_closes() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.find_state = Some(FindState::new_test(
            std::path::Path::new("/test"),
            &[("a.rs", false)],
        ));
        app.mode = Mode::Find;
        app.handle_find(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.find_state.is_none());
    }

    #[tokio::test]
    async fn handle_find_char_appends_to_query() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.find_state = Some(FindState::new_test(
            std::path::Path::new("/test"),
            &[("main.rs", false), ("lib.rs", false)],
        ));
        app.mode = Mode::Find;
        app.handle_find(key(KeyCode::Char('m')));
        assert_eq!(app.find_state.as_ref().unwrap().query, "m");
    }

    #[tokio::test]
    async fn handle_find_backspace_pops() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let mut fs = FindState::new_test(
            std::path::Path::new("/test"),
            &[("file.rs", false)],
        );
        fs.query = "abc".into();
        app.find_state = Some(fs);
        app.mode = Mode::Find;
        app.handle_find(key(KeyCode::Backspace));
        assert_eq!(app.find_state.as_ref().unwrap().query, "ab");
    }

    #[tokio::test]
    async fn handle_find_up_down_navigation() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let mut fs = FindState::new_test(
            std::path::Path::new("/test"),
            &[("a.rs", false), ("b.rs", false), ("c.rs", false)],
        );
        fs.update_filter();
        app.find_state = Some(fs);
        app.mode = Mode::Find;

        app.handle_find(key(KeyCode::Down));
        assert_eq!(app.find_state.as_ref().unwrap().selected, 1);

        app.handle_find(key(KeyCode::Up));
        assert_eq!(app.find_state.as_ref().unwrap().selected, 0);
    }

    #[tokio::test]
    async fn handle_find_ctrl_j_k_navigation() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let mut fs = FindState::new_test(
            std::path::Path::new("/test"),
            &[("a.rs", false), ("b.rs", false)],
        );
        fs.update_filter();
        app.find_state = Some(fs);
        app.mode = Mode::Find;

        app.handle_find(ctrl('j'));
        assert_eq!(app.find_state.as_ref().unwrap().selected, 1);

        app.handle_find(ctrl('k'));
        assert_eq!(app.find_state.as_ref().unwrap().selected, 0);
    }

    #[tokio::test]
    async fn handle_find_tab_switches_scope() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let fs = FindState::new_test(
            std::path::Path::new("/test"),
            &[("a.rs", false)],
        );
        app.find_state = Some(fs);
        app.mode = Mode::Find;
        app.handle_find(key(KeyCode::Tab));
        // Scope should have switched
        let new_scope = app.find_state.as_ref().unwrap().scope;
        assert_eq!(new_scope, FindScope::Global);
    }

    #[tokio::test]
    async fn accept_find_empty_is_noop() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        // Find state with no entries selected
        app.find_state = Some(FindState::new_test(
            std::path::Path::new("/test"),
            &[],
        ));
        app.mode = Mode::Find;
        app.handle_find(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.find_state.is_none());
    }
}
