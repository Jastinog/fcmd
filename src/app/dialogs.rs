use super::*;

impl App {
    pub(super) fn handle_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.execute_delete();
                self.active_panel_mut().marked.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let max = self.confirm_paths.len().saturating_sub(1);
                self.confirm_scroll = (self.confirm_scroll + 1).min(max);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.confirm_scroll = self.confirm_scroll.saturating_sub(1);
            }
            _ => {
                self.confirm_paths.clear();
                self.confirm_scroll = 0;
                // Return to Select mode if marks exist, otherwise Normal
                if self.active_panel().marked.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::Select;
                }
                self.status_message = "Cancelled".into();
            }
        }
    }

    pub(super) fn handle_conflict(&mut self, key: KeyEvent) {
        use crate::ops::ConflictChoice;
        const BUTTON_COUNT: usize = 6;

        let choice = match key.code {
            // Shortcut keys
            KeyCode::Char('o') | KeyCode::Char('O') => Some(ConflictChoice::Overwrite),
            KeyCode::Char('s') | KeyCode::Char('S') => Some(ConflictChoice::Skip),
            KeyCode::Char('a') | KeyCode::Char('A') => Some(ConflictChoice::OverwriteAll),
            KeyCode::Char('n') | KeyCode::Char('N') => Some(ConflictChoice::SkipAll),
            KeyCode::Char('w') | KeyCode::Char('W') => Some(ConflictChoice::OverwriteNewer),
            KeyCode::Esc => Some(ConflictChoice::Abort),
            // Navigation: 2 rows x 3 cols grid
            KeyCode::Left | KeyCode::Char('h') => {
                if self.conflict_selected > 0 {
                    self.conflict_selected -= 1;
                }
                None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.conflict_selected < BUTTON_COUNT - 1 {
                    self.conflict_selected += 1;
                }
                None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.conflict_selected >= 3 {
                    self.conflict_selected -= 3;
                }
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.conflict_selected + 3 < BUTTON_COUNT {
                    self.conflict_selected += 3;
                }
                None
            }
            KeyCode::Enter => {
                let c = match self.conflict_selected {
                    0 => ConflictChoice::Overwrite,
                    1 => ConflictChoice::Skip,
                    2 => ConflictChoice::OverwriteAll,
                    3 => ConflictChoice::SkipAll,
                    4 => ConflictChoice::OverwriteNewer,
                    _ => ConflictChoice::Abort,
                };
                Some(c)
            }
            _ => None,
        };

        if let Some(choice) = choice {
            if let Some(info) = self.conflict_info.take() {
                let _ = info.response_tx.send(choice);
            }
            self.mode = Mode::Normal;
        }
    }

    pub(super) fn handle_help(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::F(1) => {
                self.help_scroll = 0;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.help_scroll = self.help_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.help_scroll = self.help_scroll.saturating_add(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.help_scroll = self.help_scroll.saturating_sub(half);
            }
            KeyCode::Char('G') => {
                // Go to bottom — render will clamp to actual max
                self.help_scroll = usize::MAX;
            }
            KeyCode::Char('g') => {
                // Go to top
                self.help_scroll = 0;
            }
            _ => {}
        }
    }

    pub(super) fn enter_theme_picker(&mut self) {
        if self.theme_groups.is_empty() {
            self.mode = Mode::ThemePicker;
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.file_op_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let groups = Theme::list_grouped();
                let _ = tx.send(super::FileOpResult::ThemeList { groups });
            });
            return;
        }
        self.position_theme_cursors();
        self.theme_preview = None;
        self.spawn_theme_load();
        self.mode = Mode::ThemePicker;
    }

    pub(super) fn handle_theme_picker(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.theme_active_col == 0 {
                    let max = self.theme_groups.len().saturating_sub(1);
                    self.theme_group_cursor = (self.theme_group_cursor + 1).min(max);
                    self.theme_item_cursor = 0;
                    self.theme_item_scroll = 0;
                    self.adjust_theme_scroll();
                    self.spawn_theme_load();
                } else {
                    let group_len = self.current_group_len();
                    if group_len > 0 {
                        self.theme_item_cursor =
                            (self.theme_item_cursor + 1).min(group_len - 1);
                        self.adjust_theme_scroll();
                        self.spawn_theme_load();
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.theme_active_col == 0 {
                    self.theme_group_cursor = self.theme_group_cursor.saturating_sub(1);
                    self.theme_item_cursor = 0;
                    self.theme_item_scroll = 0;
                    self.adjust_theme_scroll();
                    self.spawn_theme_load();
                } else {
                    self.theme_item_cursor = self.theme_item_cursor.saturating_sub(1);
                    self.adjust_theme_scroll();
                    self.spawn_theme_load();
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.theme_half_page();
                if self.theme_active_col == 0 {
                    let max = self.theme_groups.len().saturating_sub(1);
                    self.theme_group_cursor = (self.theme_group_cursor + half).min(max);
                    self.theme_item_cursor = 0;
                    self.theme_item_scroll = 0;
                } else {
                    let max = self.current_group_len().saturating_sub(1);
                    self.theme_item_cursor = (self.theme_item_cursor + half).min(max);
                }
                self.adjust_theme_scroll();
                self.spawn_theme_load();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.theme_half_page();
                if self.theme_active_col == 0 {
                    self.theme_group_cursor = self.theme_group_cursor.saturating_sub(half);
                    self.theme_item_cursor = 0;
                    self.theme_item_scroll = 0;
                } else {
                    self.theme_item_cursor = self.theme_item_cursor.saturating_sub(half);
                }
                self.adjust_theme_scroll();
                self.spawn_theme_load();
            }
            KeyCode::Char('g') => {
                if self.theme_active_col == 0 {
                    self.theme_group_cursor = 0;
                    self.theme_group_scroll = 0;
                    self.theme_item_cursor = 0;
                    self.theme_item_scroll = 0;
                } else {
                    self.theme_item_cursor = 0;
                    self.theme_item_scroll = 0;
                }
                self.spawn_theme_load();
            }
            KeyCode::Char('G') => {
                if self.theme_active_col == 0 {
                    self.theme_group_cursor = self.theme_groups.len().saturating_sub(1);
                    self.theme_item_cursor = 0;
                    self.theme_item_scroll = 0;
                } else {
                    self.theme_item_cursor = self.current_group_len().saturating_sub(1);
                }
                self.adjust_theme_scroll();
                self.spawn_theme_load();
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.theme_active_col == 0 && self.current_group_len() > 0 {
                    self.theme_active_col = 1;
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.theme_active_col == 1 {
                    self.theme_active_col = 0;
                }
            }
            KeyCode::Tab => {
                if self.theme_active_col == 0 {
                    if self.current_group_len() > 0 {
                        self.theme_active_col = 1;
                    }
                } else {
                    self.theme_active_col = 0;
                }
            }
            KeyCode::BackTab => {
                if self.theme_active_col == 1 {
                    self.theme_active_col = 0;
                }
            }
            KeyCode::Enter => {
                let name = self.current_theme_name().map(|s| s.to_string());
                let Some(name) = name else { return };
                if let Some(preview) = self.theme_preview.take() {
                    self.theme = preview;
                }
                self.apply_transparency();
                self.theme_active_name = Some(name.clone());
                let n = name.clone();
                self.db_spawn(move |db| { let _ = db.save_theme(&n); });
                self.status_message = format!("Theme: {name}");
                self.mode = Mode::Normal;
            }
            KeyCode::Char('t') => {
                self.theme_show_light = !self.theme_show_light;
                self.theme_item_cursor = 0;
                self.theme_item_scroll = 0;
                self.spawn_theme_load();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.theme_preview = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn theme_half_page(&self) -> usize {
        let list_h = (self.visible_height * 70 / 100).saturating_sub(4).max(1);
        (list_h / 2).max(1)
    }

    pub(super) fn spawn_theme_load(&mut self) {
        if let Some(name) = self.current_theme_name().map(|s| s.to_string()) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.theme_load_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let _ = tx.send(Theme::load_by_name(&name));
            });
        }
    }

    fn adjust_theme_scroll(&mut self) {
        let max_h = (self.visible_height * 70 / 100).max(2);
        let list_h = max_h.saturating_sub(4).max(1);
        if self.theme_active_col == 0 {
            if self.theme_group_cursor < self.theme_group_scroll {
                self.theme_group_scroll = self.theme_group_cursor;
            } else if self.theme_group_cursor >= self.theme_group_scroll + list_h {
                self.theme_group_scroll = self.theme_group_cursor - list_h + 1;
            }
        } else {
            if self.theme_item_cursor < self.theme_item_scroll {
                self.theme_item_scroll = self.theme_item_cursor;
            } else if self.theme_item_cursor >= self.theme_item_scroll + list_h {
                self.theme_item_scroll = self.theme_item_cursor - list_h + 1;
            }
        }
    }

    fn current_group_themes(&self) -> &[String] {
        let Some(group) = self.theme_groups.get(self.theme_group_cursor) else {
            return &[];
        };
        if self.theme_show_light {
            &group.light_themes
        } else {
            &group.dark_themes
        }
    }

    fn current_group_len(&self) -> usize {
        self.current_group_themes().len()
    }

    fn current_theme_name(&self) -> Option<&str> {
        self.current_group_themes()
            .get(self.theme_item_cursor)
            .map(|s| s.as_str())
    }

    /// Position cursors on the active theme after loading groups.
    pub(super) fn position_theme_cursors(&mut self) {
        let active = self.theme_active_name.as_deref();
        if let Some(name) = active {
            for (gi, group) in self.theme_groups.iter().enumerate() {
                if let Some(ti) = group.dark_themes.iter().position(|n| n == name) {
                    self.theme_group_cursor = gi;
                    self.theme_group_scroll = gi.saturating_sub(5);
                    self.theme_item_cursor = ti;
                    self.theme_item_scroll = ti.saturating_sub(5);
                    self.theme_active_col = 1;
                    self.theme_show_light = false;
                    return;
                }
                if let Some(ti) = group.light_themes.iter().position(|n| n == name) {
                    self.theme_group_cursor = gi;
                    self.theme_group_scroll = gi.saturating_sub(5);
                    self.theme_item_cursor = ti;
                    self.theme_item_scroll = ti.saturating_sub(5);
                    self.theme_active_col = 1;
                    self.theme_show_light = true;
                    return;
                }
            }
        }
        self.theme_group_cursor = 0;
        self.theme_group_scroll = 0;
        self.theme_item_cursor = 0;
        self.theme_item_scroll = 0;
        self.theme_active_col = 0;
        self.theme_show_light = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn handle_confirm_scroll_down_up() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Confirm;
        app.confirm_paths = (0..10).map(|i| (PathBuf::from(format!("/f{i}")), false)).collect();
        app.confirm_scroll = 0;

        app.handle_confirm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.confirm_scroll, 1);
        app.handle_confirm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.confirm_scroll, 2);
        app.handle_confirm(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.confirm_scroll, 1);
    }

    #[tokio::test]
    async fn handle_confirm_cancel_no_marks() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Confirm;
        app.confirm_paths = vec![(PathBuf::from("/test/a.txt"), false)];

        // Press 'n' to cancel
        app.handle_confirm(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.confirm_paths.is_empty());
        assert_eq!(app.status_message, "Cancelled");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_confirm_cancel_with_marks_returns_select() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        // Mark a file
        let path = app.active_panel().entries[1].path.clone();
        app.active_panel_mut().marked.insert(path);
        app.mode = Mode::Confirm;
        app.confirm_paths = vec![(PathBuf::from("/test/a.txt"), false)];

        app.handle_confirm(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Select);
    }

    #[tokio::test]
    async fn handle_confirm_scroll_clamped() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Confirm;
        app.confirm_paths = vec![(PathBuf::from("/a"), false), (PathBuf::from("/b"), false)];
        app.confirm_scroll = 0;

        // Scroll down past max should clamp
        for _ in 0..5 {
            app.handle_confirm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(app.confirm_scroll, 1); // max is len-1 = 1
    }

    #[tokio::test]
    async fn handle_help_esc_returns_normal() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Help;
        app.handle_help(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn position_theme_cursors_finds_in_group() {
        use crate::theme::ThemeGroup;
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.theme_groups = vec![
            ThemeGroup {
                name: "Classic",
                dark_themes: vec!["gruvbox".into(), "dracula".into(), "nord".into()],
                light_themes: vec!["solarized".into()],
            },
            ThemeGroup {
                name: "Tokyo",
                dark_themes: vec!["tokyo-night".into()],
                light_themes: vec!["github-light".into()],
            },
        ];
        app.theme_active_name = Some("dracula".into());

        app.position_theme_cursors();
        assert_eq!(app.theme_group_cursor, 0);
        assert_eq!(app.theme_item_cursor, 1);
        assert_eq!(app.theme_active_col, 1);
        assert!(!app.theme_show_light);
    }

    #[tokio::test]
    async fn position_theme_cursors_finds_in_second_group() {
        use crate::theme::ThemeGroup;
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.theme_groups = vec![
            ThemeGroup {
                name: "Classic",
                dark_themes: vec!["gruvbox".into()],
                light_themes: vec!["solarized".into()],
            },
            ThemeGroup {
                name: "Tokyo",
                dark_themes: vec!["tokyo-night".into()],
                light_themes: vec!["github-light".into(), "dayfox".into()],
            },
        ];
        app.theme_active_name = Some("dayfox".into());

        app.position_theme_cursors();
        assert_eq!(app.theme_group_cursor, 1);
        assert_eq!(app.theme_item_cursor, 1);
        assert_eq!(app.theme_active_col, 1);
        assert!(app.theme_show_light);
    }
}
