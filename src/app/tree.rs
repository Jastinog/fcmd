use super::*;

impl App {
    pub(super) fn handle_tree_input(&mut self, key: KeyEvent) {
        // Handle pending key
        if let Some(pending) = {
            self.pending_key_time = None;
            self.pending_key.take()
        } && pending == 'g'
            && key.code == KeyCode::Char('g')
        {
            self.tree_selected = 0;
            return;
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
            KeyCode::Char('g') => {
                self.pending_key = Some('g');
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('G') => {
                if !self.tree_data.is_empty() {
                    self.tree_selected = self.tree_data.len() - 1;
                }
            }

            // Enter: always navigate panel
            KeyCode::Enter => {
                self.tree_enter_selected();
            }

            // l/Right: expand collapsed dir, otherwise navigate panel
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(line) = self.tree_data.get(self.tree_selected) {
                    if line.is_dir && !line.is_expanded {
                        self.tree_toggle_expand(&line.path.clone());
                    } else {
                        self.tree_enter_selected();
                    }
                }
            }

            // h/Left: collapse expanded dir, otherwise go to parent node
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(line) = self.tree_data.get(self.tree_selected) {
                    if line.is_dir && line.is_expanded && line.depth > 0 {
                        self.tree_toggle_collapse(&line.path.clone());
                    } else {
                        self.tree_go_parent();
                    }
                }
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
            let side = self.tab().active;
            if is_dir {
                // Skip if already showing this directory
                if self.active_panel().path == path {
                    return;
                }
                // Ensure navigated dir is not collapsed so its contents show
                self.tree_collapsed.remove(&path);
                self.active_panel_mut().navigate_to(path);
                self.apply_dir_sort_no_reload();
                self.spawn_dir_load(side, None);
                self.spawn_rebuild_tree();
            } else if let Some(parent) = path.parent() {
                let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                self.active_panel_mut().navigate_to(parent.to_path_buf());
                self.apply_dir_sort_no_reload();
                self.spawn_dir_load(side, file_name);
                self.spawn_rebuild_tree();
            }
        }
    }

    fn tree_toggle_expand(&mut self, path: &std::path::Path) {
        self.tree_collapsed.remove(path);
        self.tree_expanded.insert(path.to_path_buf());
        self.tree_select_path = Some(path.to_path_buf());
        self.spawn_rebuild_tree();
    }

    fn tree_toggle_collapse(&mut self, path: &std::path::Path) {
        self.tree_expanded.remove(path);
        self.tree_collapsed.insert(path.to_path_buf());
        self.tree_select_path = Some(path.to_path_buf());
        self.spawn_rebuild_tree();
    }

    fn tree_go_parent(&mut self) {
        if let Some(line) = self.tree_data.get(self.tree_selected) {
            if line.depth == 0 {
                // At root — expand tree upward
                if let Some(parent) = self.start_dir.parent().map(|p| p.to_path_buf()) {
                    let old_root = self.start_dir.clone();
                    self.start_dir = parent;
                    self.tree_select_path = Some(old_root);
                    self.spawn_rebuild_tree();
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

    pub fn spawn_rebuild_tree(&mut self) {
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
        let start_dir = self.start_dir.clone();
        let collapsed = self.tree_collapsed.clone();
        let expanded = self.tree_expanded.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tree_load_rx = Some(rx);

        tokio::task::spawn_blocking(move || {
            let data = crate::tree::build_tree(
                &start_dir,
                &current,
                show_hidden,
                &collapsed,
                &expanded,
            );
            let _ = tx.send(super::TreeLoadResult {
                start_dir,
                current_path: current,
                data,
            });
        });
    }

    pub fn apply_tree_data(&mut self, result: super::TreeLoadResult) {
        // Discard stale results if we've navigated away
        if result.start_dir != self.start_dir {
            return;
        }
        self.tree_data = result.data;
        self.tree_dirty = false;
        self.tree_last_path = Some(result.current_path);
        self.tree_last_hidden = self.active_panel().show_hidden;

        // Position cursor: use pending select path, then is_current, then clamp
        if let Some(target) = self.tree_select_path.take()
            && let Some(idx) = self.tree_data.iter().position(|l| l.path == target)
        {
            self.tree_selected = idx;
            return;
        }
        if let Some(idx) = self.tree_data.iter().position(|l| l.is_current) {
            self.tree_selected = idx;
        } else if self.tree_data.is_empty() {
            self.tree_selected = 0;
        } else if self.tree_selected >= self.tree_data.len() {
            self.tree_selected = self.tree_data.len() - 1;
        }
    }
}
