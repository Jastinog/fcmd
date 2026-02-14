use super::*;

impl App {
    pub(super) fn handle_normal(&mut self, key: KeyEvent) {
        // Delegate to tree handler when tree is focused
        if self.tree_focused && self.show_tree {
            self.handle_tree_input(key);
            return;
        }

        if let Some(pending) = { self.pending_key_time = None; self.pending_key.take() } {
            if self.handle_pending_sequence(pending, key) {
                return;
            }
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,

            // Selection with Shift+arrows → enters Select mode
            KeyCode::Down if shift => self.enter_select_and_mark(),
            KeyCode::Up if shift => self.enter_select_and_mark_up(),

            // Focus & navigation
            KeyCode::Char('l') if ctrl => self.focus_next(),
            KeyCode::Char('h') if ctrl => self.focus_prev(),
            KeyCode::Char('j') | KeyCode::Down => self.active_panel_mut().move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.active_panel_mut().move_up(),
            KeyCode::Char('G') => self.active_panel_mut().go_bottom(),
            KeyCode::Char('d') if ctrl => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_down(half);
            }
            KeyCode::Char('u') if ctrl => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_up(half);
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                if let Err(e) = self.active_panel_mut().enter_selected() {
                    self.status_message = format!("Error: {e}");
                }
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace | KeyCode::Char('-') => {
                if let Err(e) = self.active_panel_mut().go_parent() {
                    self.status_message = format!("Error: {e}");
                }
            }
            KeyCode::Char('~') => {
                if let Err(e) = self.active_panel_mut().go_home() {
                    self.status_message = format!("Error: {e}");
                }
            }
            KeyCode::Tab => self.tab_mut().switch_panel(),

            // Pending key sequences
            KeyCode::Char('g') => { self.pending_key = Some('g'); self.pending_key_time = Some(Instant::now()); },
            KeyCode::Char('d') => { self.pending_key = Some('d'); self.pending_key_time = Some(Instant::now()); },
            KeyCode::Char('y') => { self.pending_key = Some('y'); self.pending_key_time = Some(Instant::now()); },
            KeyCode::Char('s') => { self.pending_key = Some('s'); self.pending_key_time = Some(Instant::now()); },
            KeyCode::Char('\'') => { self.pending_key = Some('\''); self.pending_key_time = Some(Instant::now()); },

            // File operations
            KeyCode::Char('p') => self.paste(false),
            KeyCode::Char('P') => self.paste(true),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char(' ') => { self.pending_key = Some(' '); self.pending_key_time = Some(Instant::now()); },

            // Mode switches
            KeyCode::Char('v') | KeyCode::Char('V') => self.enter_visual(),
            KeyCode::Char('/') => self.enter_search(),
            KeyCode::Char(':') => self.enter_command(),

            // Search navigation
            KeyCode::Char('n') => self.search_next(),
            KeyCode::Char('N') => self.search_prev(),

            // Marks
            KeyCode::Char('m') => self.toggle_visual_mark(),
            KeyCode::Char('M') => self.jump_next_visual_mark(),

            // Find
            KeyCode::Char('f') => self.open_find_local(),
            KeyCode::Char('F') => self.open_find_global(),

            // Rename / Create
            KeyCode::Char('r') if !ctrl => self.enter_rename(),
            KeyCode::Char('a') => self.enter_create(),

            // Toggles & settings
            KeyCode::Char('r') if ctrl => self.refresh_current_panel(),
            KeyCode::Char('T') => self.cycle_theme(true),

            // Preview scroll
            KeyCode::Char('J') => {
                if let Some(ref mut p) = self.preview {
                    p.scroll_down(1, self.visible_height);
                }
            }
            KeyCode::Char('K') => {
                if let Some(ref mut p) = self.preview {
                    p.scroll_up(1);
                }
            }

            _ => {}
        }
    }

    /// Handle two-key sequences (gg, dd, yy, etc). Returns true if consumed.
    fn handle_pending_sequence(&mut self, pending: char, key: KeyEvent) -> bool {
        match (pending, key.code) {
            ('g', KeyCode::Char('g')) => self.active_panel_mut().go_top(),
            ('g', KeyCode::Char('t')) => self.next_tab(),
            ('g', KeyCode::Char('T')) => self.prev_tab(),
            ('d', KeyCode::Char('d')) => self.request_delete(),
            ('y', KeyCode::Char('y')) => self.yank_targeted(),
            ('y', KeyCode::Char('p')) => self.yank_path(),
            ('\'', KeyCode::Char(c)) if c.is_ascii_lowercase() => self.goto_mark(c),
            ('s', KeyCode::Char('n')) => self.set_sort(SortMode::Name),
            ('s', KeyCode::Char('s')) => self.set_sort(SortMode::Size),
            ('s', KeyCode::Char('d')) | ('s', KeyCode::Char('m')) => self.set_sort(SortMode::Modified),
            ('s', KeyCode::Char('c')) => self.set_sort(SortMode::Created),
            ('s', KeyCode::Char('e')) => self.set_sort(SortMode::Extension),
            ('s', KeyCode::Char('r')) => self.toggle_sort_reverse(),
            // Space as leader key
            (' ', KeyCode::Char('t')) => self.toggle_tree(),
            (' ', KeyCode::Char('h')) => self.toggle_hidden(),
            (' ', KeyCode::Char('p')) => self.preview_mode = !self.preview_mode,
            (' ', KeyCode::Char('d')) => self.start_du(),
            (' ', KeyCode::Char(',')) => self.open_find_local(),
            (' ', KeyCode::Char('.')) => self.open_find_global(),
            (' ', KeyCode::Char('s')) => {
                self.sort_cursor = SortMode::ALL.iter()
                    .position(|&m| m == self.active_panel().sort_mode)
                    .unwrap_or(0);
                self.mode = Mode::Sort;
            }
            (' ', KeyCode::Char('a')) => self.select_all(),
            (' ', KeyCode::Char('n')) => self.unselect_all(),
            (' ', KeyCode::Char('?')) => self.mode = Mode::Help,
            _ => return false,
        }
        true
    }

    pub(super) fn enter_visual(&mut self) {
        self.mode = Mode::Visual;
        let sel = self.active_panel().selected;
        self.active_panel_mut().visual_anchor = Some(sel);
    }

    pub(super) fn enter_search(&mut self) {
        self.search_saved_cursor = self.active_panel().selected;
        self.search_query.clear();
        self.mode = Mode::Search;
    }

    pub(super) fn enter_command(&mut self) {
        self.mode = Mode::Command;
        self.command_input.clear();
    }

    pub(super) fn enter_rename(&mut self) {
        let entry = match self.active_panel().selected_entry().filter(|e| e.name != "..") {
            Some(e) => e,
            None => return,
        };
        self.rename_input = entry.name.clone();
        self.mode = Mode::Rename;
    }

    pub(super) fn enter_create(&mut self) {
        self.rename_input.clear();
        self.mode = Mode::Create;
    }
}
