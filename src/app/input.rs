use super::*;

impl App {
    pub(super) fn handle_normal(&mut self, key: KeyEvent) {
        // Delegate to tree handler when tree is focused
        if self.tree_focused && self.show_tree {
            self.handle_tree_input(key);
            return;
        }

        if let Some(pending) = {
            self.pending_key_time = None;
            self.pending_key.take()
        } && self.handle_pending_sequence(pending, key)
        {
            return;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::F(1) => self.mode = Mode::Help,
            KeyCode::F(2) => self.enter_rename(),
            KeyCode::F(5) => self.copy_to_other_panel(),
            KeyCode::F(6) => self.move_to_other_panel(),
            KeyCode::F(7) => self.enter_create(),
            KeyCode::F(8) => self.request_delete(),
            KeyCode::F(10) => self.should_quit = true,
            KeyCode::F(4) => {
                if let Some(entry) = self.active_panel().selected_entry()
                    && !entry.is_dir
                    && entry.name != ".."
                {
                    let path = entry.path.clone();
                    self.request_open_editor(path);
                }
            }
            KeyCode::F(3) => {
                if let Some(entry) = self.active_panel().selected_entry()
                    && !entry.is_dir
                    && entry.name != ".."
                {
                    let path = entry.path.clone();
                    self.file_preview_path = Some(path.clone());
                    self.file_preview = Some(Preview::loading_placeholder(&path));
                    self.mode = Mode::Preview;
                    self.spawn_file_preview_load(path);
                }
            }
            KeyCode::Esc => {
                self.active_panel_mut().marked.clear();
            }

            // Selection with Shift+arrows → enters Select mode
            KeyCode::Down if shift => self.enter_select_and_mark(),
            KeyCode::Up if shift => self.enter_select_and_mark_up(),
            KeyCode::Char('A') if !ctrl => self.select_all_and_enter_select(),
            KeyCode::Char('+') => self.enter_select_pattern(),
            KeyCode::Char('-') if !ctrl => self.enter_unselect_pattern(),
            KeyCode::Char('*') => self.invert_selection(),

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
                let entry_info = self
                    .active_panel()
                    .selected_entry()
                    .map(|e| (e.is_dir, e.name == ".."));
                match entry_info {
                    Some((false, _)) if key.code == KeyCode::Enter => {
                        // Enter on file → open preview
                        if let Some(entry) = self.active_panel().selected_entry() {
                            let path = entry.path.clone();
                            self.file_preview_path = Some(path.clone());
                            self.file_preview = Some(Preview::loading_placeholder(&path));
                            self.mode = Mode::Preview;
                            self.spawn_file_preview_load(path);
                        }
                    }
                    Some((_, true)) if key.code == KeyCode::Enter => {
                        // Enter on ".." → go to parent
                        self.go_parent_async();
                    }
                    _ => {
                        // l/Right/Enter on directory → enter it
                        self.enter_dir_async();
                    }
                }
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace | KeyCode::Char('-') => {
                self.go_parent_async();
            }
            KeyCode::Char('~') => {
                self.go_home_async();
            }
            KeyCode::Tab => {
                let layout = self.layout;
                self.tab_mut().cycle_panel(layout);
            }
            KeyCode::Char('t') if ctrl => self.new_tab(),
            KeyCode::Char('w') if ctrl => self.close_tab(),

            // Pending key sequences
            KeyCode::Char('g') => {
                self.pending_key = Some('g');
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('d') => {
                self.pending_key = Some('d');
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('y') => {
                self.pending_key = Some('y');
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('s') => {
                self.pending_key = Some('s');
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('c') => {
                self.pending_key = Some('c');
                self.pending_key_time = Some(Instant::now());
            }
            KeyCode::Char('\'') => {
                self.pending_key = Some('\'');
                self.pending_key_time = Some(Instant::now());
            }

            // File operations
            KeyCode::Char('p') => self.paste(false),
            KeyCode::Char('P') => self.paste(true),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char(' ') => {
                self.pending_key = Some(' ');
                self.pending_key_time = Some(Instant::now());
            }

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

            // Bookmarks
            KeyCode::Char('b') => self.add_bookmark_prompt(),
            KeyCode::Char('B') => self.open_bookmarks(),

            // Info
            KeyCode::Char('i') => self.enter_info(),

            // Toggles & settings
            KeyCode::Char('r') if ctrl => self.refresh_current_panel(),
            KeyCode::Char('T') => self.enter_theme_picker(),

            // Open in editor
            KeyCode::Char('o') => {
                if let Some(entry) = self.active_panel().selected_entry()
                    && !entry.is_dir
                    && entry.name != ".."
                {
                    let path = entry.path.clone();
                    self.request_open_editor(path);
                }
            }

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
            ('d', KeyCode::Char('D')) => self.request_permanent_delete(),
            ('y', KeyCode::Char('y')) => self.yank_targeted(),
            ('y', KeyCode::Char('p')) => self.yank_path(),
            ('y', KeyCode::Char('n')) => self.yank_name(),
            ('\'', KeyCode::Char(c)) if c.is_ascii_lowercase() => self.goto_mark(c),
            ('s', KeyCode::Char('n')) => self.set_sort(SortMode::Name),
            ('s', KeyCode::Char('s')) => self.set_sort(SortMode::Size),
            ('s', KeyCode::Char('d')) | ('s', KeyCode::Char('m')) => {
                self.set_sort(SortMode::Modified)
            }
            ('s', KeyCode::Char('c')) => self.set_sort(SortMode::Created),
            ('s', KeyCode::Char('e')) => self.set_sort(SortMode::Extension),
            ('s', KeyCode::Char('r')) => self.toggle_sort_reverse(),
            ('u', KeyCode::Char('t')) => self.toggle_transparent(),
            ('c', KeyCode::Char('p')) => self.enter_chmod(),
            ('c', KeyCode::Char('o')) => self.enter_chown(),
            // Layout
            ('w', KeyCode::Char('1')) => self.set_layout(PanelLayout::Single),
            ('w', KeyCode::Char('2')) => self.set_layout(PanelLayout::Dual),
            ('w', KeyCode::Char('3')) => self.set_layout(PanelLayout::Triple),
            // Space as leader key
            (' ', KeyCode::Char('t')) => self.toggle_tree(),
            (' ', KeyCode::Char('h')) => self.toggle_hidden(),
            (' ', KeyCode::Char('p')) => self.preview_mode = !self.preview_mode,
            (' ', KeyCode::Char('w')) => {
                self.pending_key = Some('w');
                self.pending_key_time = Some(Instant::now());
            }
            (' ', KeyCode::Char('d')) => self.start_du(),
            (' ', KeyCode::Char(',')) => self.open_find_local(),
            (' ', KeyCode::Char('.')) => self.open_find_global(),
            (' ', KeyCode::Char('s')) => {
                self.pending_key = Some('s');
                self.pending_key_time = Some(Instant::now());
            }
            (' ', KeyCode::Char('u')) => {
                self.pending_key = Some('u');
                self.pending_key_time = Some(Instant::now());
            }
            (' ', KeyCode::Char('a')) => self.select_all(),
            (' ', KeyCode::Char('n')) => self.unselect_all(),
            (' ', KeyCode::Char('b')) => self.open_bookmarks(),
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
        let entry = match self
            .active_panel()
            .selected_entry()
            .filter(|e| e.name != "..")
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn handle_normal_q_quits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.handle_normal(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn handle_normal_f1_opens_help() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.handle_normal(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Help);
    }

    #[tokio::test]
    async fn handle_normal_slash_enters_search() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.handle_normal(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Search);
    }

    #[tokio::test]
    async fn handle_normal_colon_enters_command() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.handle_normal(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Command);
    }

    #[tokio::test]
    async fn handle_normal_v_enters_visual() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.handle_normal(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Visual);
        assert_eq!(app.active_panel().visual_anchor, Some(1));
    }

    #[tokio::test]
    async fn handle_normal_j_moves_down() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0;
        app.handle_normal(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 1);
    }

    #[tokio::test]
    async fn handle_normal_k_moves_up() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 2;
        app.handle_normal(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 1);
    }

    #[tokio::test]
    async fn handle_normal_G_goes_bottom() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.handle_normal(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 3); // "..", a, b, c → last is 3
    }

    #[tokio::test]
    async fn handle_normal_tab_cycles_panel() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert_eq!(app.tab().active, 0);
        app.handle_normal(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.tab().active, 1);
    }

    #[tokio::test]
    async fn handle_normal_esc_clears_marks() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        assert!(!app.active_panel().marked.is_empty());
        app.handle_normal(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.active_panel().marked.is_empty());
    }

    #[tokio::test]
    async fn handle_normal_a_enters_create() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.handle_normal(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Create);
    }

    #[tokio::test]
    async fn handle_normal_i_enters_info() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // select "a.txt" (not "..")
        app.handle_normal(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Info);
    }

    #[tokio::test]
    async fn handle_normal_tree_focused_delegates() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.show_tree = true;
        app.tree_focused = true;
        app.tree_data = vec![crate::tree::TreeLine {
            prefix: String::new(),
            name: "test".into(),
            path: PathBuf::from("/test"),
            is_dir: true,
            is_current: true,
            is_on_path: true,
            is_expanded: false,
            depth: 0,
        }];
        // 'q' should reach tree handler and set should_quit
        app.handle_normal(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    // ── Pending sequence tests ───────────────────────────────────────

    #[tokio::test]
    async fn pending_gg_goes_top() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 3;

        // First 'g' sets pending
        app.handle_normal(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.pending_key, Some('g'));

        // Second 'g' triggers go_top
        app.handle_normal(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 0);
    }

    #[tokio::test]
    async fn pending_sn_sets_sort_name() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().sort_mode = SortMode::Size;
        app.handle_normal(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        app.handle_normal(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().sort_mode, SortMode::Name);
    }

    #[tokio::test]
    async fn pending_ss_sets_sort_size() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.handle_normal(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        app.handle_normal(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().sort_mode, SortMode::Size);
    }

    #[tokio::test]
    async fn enter_visual_sets_anchor() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 2;
        app.enter_visual();
        assert_eq!(app.mode, Mode::Visual);
        assert_eq!(app.active_panel().visual_anchor, Some(2));
    }

    #[tokio::test]
    async fn enter_search_saves_cursor() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_search();
        assert_eq!(app.mode, Mode::Search);
        assert_eq!(app.search_saved_cursor, 1);
        assert!(app.search_query.is_empty());
    }

    #[tokio::test]
    async fn enter_command_clears_input() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.command_input = "old".into();
        app.enter_command();
        assert_eq!(app.mode, Mode::Command);
        assert!(app.command_input.is_empty());
    }

    #[tokio::test]
    async fn enter_rename_prefills_name() {
        let entries = crate::app::make_test_entries(&["hello.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "hello.txt"
        app.enter_rename();
        assert_eq!(app.mode, Mode::Rename);
        assert_eq!(app.rename_input, "hello.txt");
    }

    #[tokio::test]
    async fn enter_rename_skips_dotdot() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".."
        app.enter_rename();
        assert_eq!(app.mode, Mode::Normal); // stayed Normal
    }

    #[tokio::test]
    async fn enter_create_clears_input() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.rename_input = "old".into();
        app.enter_create();
        assert_eq!(app.mode, Mode::Create);
        assert!(app.rename_input.is_empty());
    }
}
