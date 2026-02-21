use super::*;

impl App {
    pub(super) fn handle_visual(&mut self, key: KeyEvent) {
        if let Some(pending) = {
            self.pending_key_time = None;
            self.pending_key.take()
        } {
            match (pending, key.code) {
                ('g', KeyCode::Char('g')) => {
                    self.active_panel_mut().go_top();
                    return;
                }
                ('c', KeyCode::Char('p')) => {
                    self.exit_visual();
                    self.enter_chmod();
                    return;
                }
                ('c', KeyCode::Char('o')) => {
                    self.exit_visual();
                    self.enter_chown();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.active_panel_mut().move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.active_panel_mut().move_up(),
            KeyCode::Char('G') => self.active_panel_mut().go_bottom(),
            KeyCode::Char(c @ ('g' | 'c')) => {
                self.pending_key = Some(c);
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_down(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_up(half);
            }

            KeyCode::Char('y') => {
                let entries = self.active_panel().targeted_register_entries();
                self.exit_visual();
                if entries.is_empty() {
                    self.status_message = "Nothing to yank".into();
                } else {
                    let n = entries.len();
                    self.register = Some(Register {
                        entries,
                        op: RegisterOp::Yank,
                    });
                    self.status_message = format!("Yanked {n} item(s)");
                }
            }
            KeyCode::Char('d') => {
                let items = self.targeted_path_types();
                self.exit_visual();
                self.confirm_permanent = false;
                self.request_delete_paths(items);
            }
            KeyCode::Char('D') => {
                let items = self.targeted_path_types();
                self.exit_visual();
                self.request_permanent_delete_paths(items);
            }

            KeyCode::Char('p') => {
                self.exit_visual();
                self.paste(false);
            }

            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                self.exit_visual();
                self.enter_dir_async();
            }

            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                self.exit_visual();
                self.go_parent_async();
            }

            KeyCode::Char('v') | KeyCode::Char('V') | KeyCode::Esc => self.exit_visual(),

            KeyCode::Tab => {
                self.exit_visual();
                { let l = self.layout; self.tab_mut().cycle_panel(l); }
            }

            _ => {}
        }
    }

    pub(super) fn exit_visual(&mut self) {
        self.active_panel_mut().visual_anchor = None;
        self.mode = Mode::Normal;
    }

    // ── Select mode ─────────────────────────────────────────────────

    pub(super) fn enter_select_and_mark(&mut self) {
        if self.mode != Mode::Select {
            self.mode = Mode::Select;
        }
        self.active_panel_mut().toggle_mark();
    }

    pub(super) fn enter_select_and_mark_up(&mut self) {
        if self.mode != Mode::Select {
            self.mode = Mode::Select;
        }
        self.active_panel_mut().toggle_mark_up();
    }

    pub(super) fn handle_select(&mut self, key: KeyEvent) {
        if let Some(pending) = {
            self.pending_key_time = None;
            self.pending_key.take()
        } {
            match (pending, key.code) {
                ('g', KeyCode::Char('g')) => {
                    self.active_panel_mut().go_top();
                    return;
                }
                ('c', KeyCode::Char('p')) => {
                    self.exit_select();
                    self.enter_chmod();
                    return;
                }
                ('c', KeyCode::Char('o')) => {
                    self.exit_select();
                    self.enter_chown();
                    return;
                }
                _ => {}
            }
        }

        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Down if shift => self.active_panel_mut().toggle_mark(),
            KeyCode::Up if shift => self.active_panel_mut().toggle_mark_up(),

            KeyCode::Char('j') | KeyCode::Down => self.active_panel_mut().move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.active_panel_mut().move_up(),
            KeyCode::Char('G') => self.active_panel_mut().go_bottom(),
            KeyCode::Char(c @ ('g' | 'c')) => {
                self.pending_key = Some(c);
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_down(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_up(half);
            }

            KeyCode::Char('y') => {
                let entries = self.active_panel().targeted_register_entries();
                self.active_panel_mut().marked.clear();
                self.exit_select();
                if entries.is_empty() {
                    self.status_message = "Nothing to yank".into();
                } else {
                    let n = entries.len();
                    self.register = Some(Register {
                        entries,
                        op: RegisterOp::Yank,
                    });
                    self.status_message = format!("Yanked {n} item(s)");
                }
            }
            KeyCode::Char('d') => {
                let items = self.targeted_path_types();
                self.confirm_permanent = false;
                self.request_delete_paths(items);
            }
            KeyCode::Char('D') => {
                let items = self.targeted_path_types();
                self.request_permanent_delete_paths(items);
            }

            KeyCode::Char('p') => {
                self.exit_select();
                self.paste(false);
            }

            KeyCode::Char('v') => {
                self.exit_select();
                self.enter_visual();
            }

            KeyCode::Esc => {
                self.active_panel_mut().marked.clear();
                self.exit_select();
            }

            KeyCode::Tab => {
                self.exit_select();
                { let l = self.layout; self.tab_mut().cycle_panel(l); }
            }

            _ => {}
        }
    }

    pub(super) fn exit_select(&mut self) {
        // Keep marks intact — user can clear with Space+n if needed
        self.mode = Mode::Normal;
    }
}
