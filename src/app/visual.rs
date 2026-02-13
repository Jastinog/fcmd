use super::*;

impl App {
    pub(super) fn handle_visual(&mut self, key: KeyEvent) {
        if let Some('g') = { self.pending_key_time = None; self.pending_key.take() } {
            if key.code == KeyCode::Char('g') {
                self.active_panel_mut().go_top();
                return;
            }
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.active_panel_mut().move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.active_panel_mut().move_up(),
            KeyCode::Char('G') => self.active_panel_mut().go_bottom(),
            KeyCode::Char('g') => { self.pending_key = Some('g'); self.pending_key_time = Some(Instant::now()); },
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_down(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_up(half);
            }

            KeyCode::Char('y') => {
                self.exit_visual();
                self.yank_targeted();
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.exit_visual();
                self.request_delete();
            }

            KeyCode::Char('p') => {
                self.exit_visual();
                self.paste(false);
            }

            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                self.exit_visual();
                if let Err(e) = self.active_panel_mut().enter_selected() {
                    self.status_message = format!("Error: {e}");
                }
            }

            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                self.exit_visual();
                if let Err(e) = self.active_panel_mut().go_parent() {
                    self.status_message = format!("Error: {e}");
                }
            }

            KeyCode::Char('v') | KeyCode::Char('V') | KeyCode::Esc => self.exit_visual(),

            KeyCode::Tab => {
                self.exit_visual();
                self.tab_mut().switch_panel();
            }

            _ => {}
        }
    }

    pub(super) fn exit_visual(&mut self) {
        self.active_panel_mut().visual_anchor = None;
        self.mode = Mode::Normal;
    }
}
