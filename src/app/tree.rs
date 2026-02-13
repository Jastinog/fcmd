use super::*;

impl App {
    pub(super) fn handle_tree_input(&mut self, key: KeyEvent) {
        // Handle pending key
        if let Some(pending) = { self.pending_key_time = None; self.pending_key.take() } {
            if pending == 'g' && key.code == KeyCode::Char('g') {
                self.tree_selected = 0;
                return;
            }
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            // Focus navigation
            KeyCode::Char('l') if ctrl => self.focus_next(),
            KeyCode::Char('h') if ctrl => {} // already leftmost

            // Half-page scroll
            KeyCode::Char('d') if ctrl => {
                let half = self.visible_height / 2;
                let max = self.tree_data.len().saturating_sub(1);
                self.tree_selected = (self.tree_selected + half).min(max);
            }
            KeyCode::Char('u') if ctrl => {
                let half = self.visible_height / 2;
                self.tree_selected = self.tree_selected.saturating_sub(half);
            }

            // Tree cursor movement
            KeyCode::Char('j') | KeyCode::Down => {
                if self.tree_selected + 1 < self.tree_data.len() {
                    self.tree_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tree_selected = self.tree_selected.saturating_sub(1);
            }
            KeyCode::Char('g') => { self.pending_key = Some('g'); self.pending_key_time = Some(Instant::now()); },
            KeyCode::Char('G') => {
                if !self.tree_data.is_empty() {
                    self.tree_selected = self.tree_data.len() - 1;
                }
            }

            // Enter directory in active panel
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.tree_enter_selected();
            }

            // Go to parent node in tree
            KeyCode::Char('h') | KeyCode::Left => {
                self.tree_go_parent();
            }

            // Exit tree focus
            KeyCode::Tab => {
                self.tree_focused = false;
            }

            // Global keys
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('t') => {
                self.show_tree = false;
                self.tree_focused = false;
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_input.clear();
            }

            _ => {}
        }
    }

    fn tree_enter_selected(&mut self) {
        if let Some(line) = self.tree_data.get(self.tree_selected) {
            let path = line.path.clone();
            let is_dir = line.is_dir;
            if is_dir {
                let panel = self.active_panel_mut();
                panel.path = path;
                panel.selected = 0;
                panel.offset = 0;
                let _ = panel.load_dir();
                self.rebuild_tree();
                if let Some(idx) = self.tree_data.iter().position(|l| l.is_current) {
                    self.tree_selected = idx;
                }
            } else if let Some(parent) = path.parent() {
                // File: navigate panel to parent dir and select the file
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
                let panel = self.active_panel_mut();
                panel.path = parent.to_path_buf();
                panel.selected = 0;
                panel.offset = 0;
                let _ = panel.load_dir();
                if let Some(name) = file_name {
                    panel.select_by_name(&name);
                }
                self.rebuild_tree();
                if let Some(idx) = self.tree_data.iter().position(|l| l.is_current) {
                    self.tree_selected = idx;
                }
            }
        }
    }

    fn tree_go_parent(&mut self) {
        if let Some(line) = self.tree_data.get(self.tree_selected) {
            if line.depth == 0 {
                // At root — expand tree upward
                if let Some(parent) = self.start_dir.parent().map(|p| p.to_path_buf()) {
                    let old_root = self.start_dir.clone();
                    self.start_dir = parent;
                    self.rebuild_tree();
                    // Position cursor on the old root node
                    if let Some(idx) = self.tree_data.iter().position(|l| l.path == old_root) {
                        self.tree_selected = idx;
                    }
                }
                return;
            }
            let target_depth = line.depth - 1;
            for i in (0..self.tree_selected).rev() {
                if self.tree_data[i].depth == target_depth {
                    self.tree_selected = i;
                    return;
                }
            }
        }
    }

    pub fn rebuild_tree(&mut self) {
        let current = self.active_panel().path.clone();
        let show_hidden = self.active_panel().show_hidden;
        // Auto-expand root upward if panel navigated above start_dir
        while !current.starts_with(&self.start_dir) {
            if let Some(parent) = self.start_dir.parent().map(|p| p.to_path_buf()) {
                self.start_dir = parent;
            } else {
                break;
            }
        }
        self.tree_data = crate::tree::build_tree(&self.start_dir, &current, show_hidden);
        if self.tree_data.is_empty() {
            self.tree_selected = 0;
        } else if self.tree_selected >= self.tree_data.len() {
            self.tree_selected = self.tree_data.len() - 1;
        }
    }
}
