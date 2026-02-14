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

        self.find_state = None;
        self.mode = Mode::Normal;

        let Some(path) = target else { return };

        if path.is_dir() {
            let panel = self.active_panel_mut();
            panel.path = path;
            panel.selected = 0;
            panel.offset = 0;
            panel.marked.clear();
            let _ = panel.load_dir();
        } else if let Some(parent) = path.parent() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            let panel = self.active_panel_mut();
            panel.path = parent.to_path_buf();
            panel.selected = 0;
            panel.offset = 0;
            panel.marked.clear();
            let _ = panel.load_dir();
            if let Some(name) = name {
                panel.select_by_name(&name);
            }
        }
    }
}
