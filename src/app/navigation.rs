use super::*;
use crate::util::copy_to_clipboard;

impl App {
    pub(super) fn focus_next(&mut self) {
        if self.tree_focused && self.show_tree {
            // Tree → first panel
            self.tree_focused = false;
        } else {
            let max = self.layout.count().saturating_sub(1);
            let tab = self.tab_mut();
            if tab.active < max {
                tab.active += 1;
            }
        }
    }

    pub(super) fn focus_prev(&mut self) {
        if self.tree_focused {
            return; // already leftmost
        }
        let active = self.tab().active;
        if active > 0 {
            self.tab_mut().active = active - 1;
        } else if self.show_tree {
            self.tree_focused = true;
        }
    }

    pub(super) fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.exit_mode_on_tab_switch();
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            self.preview_path = None;
        }
    }

    pub(super) fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.exit_mode_on_tab_switch();
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
            self.preview_path = None;
        }
    }

    fn exit_mode_on_tab_switch(&mut self) {
        match self.mode {
            Mode::Visual => {
                self.active_panel_mut().visual_anchor = None;
                self.mode = Mode::Normal;
            }
            Mode::Select => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    pub(super) fn new_tab(&mut self) {
        let path = self.active_panel().path.clone();
        let mut tab = Tab::new(path);
        // Apply sort prefs before spawning loads
        for panel in tab.panels.iter_mut() {
            if let Some(&(mode, rev)) = self.dir_sorts.get(&panel.path) {
                panel.sort_mode = mode;
                panel.sort_reverse = rev;
            }
        }
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.preview_path = None;
        for panel_idx in 0..3 {
            self.spawn_dir_load(panel_idx, None);
        }
        self.status_message = format!("Tab {}", self.tabs.len());
    }

    pub(super) fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            self.status_message = "Cannot close last tab".into();
            return;
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.preview_path = None;
    }

    pub(super) fn toggle_hidden(&mut self) {
        let hidden = !self.active_panel().show_hidden;
        {
            let tab = &mut self.tabs[self.active_tab];
            for panel in tab.panels.iter_mut() {
                panel.show_hidden = hidden;
            }
        }
        self.dir_cache.clear();
        // Load all panels async
        for i in 0..3 {
            self.spawn_dir_load(i, None);
        }
        self.tree_dirty = true;
        self.status_message = if hidden {
            "Hidden files: shown".into()
        } else {
            "Hidden files: hidden".into()
        };
    }

    pub(super) fn toggle_tree(&mut self) {
        self.show_tree = !self.show_tree;
        if self.show_tree {
            self.tree_focused = true;
        } else {
            self.tree_focused = false;
        }
    }

    pub(super) fn toggle_sort_reverse(&mut self) {
        let rev = !self.active_panel().sort_reverse;
        self.active_panel_mut().sort_reverse = rev;
        self.resort_from_cache_or_reload();
        self.save_current_sort();
        let arrow = if rev { "\u{25b2}" } else { "\u{25bc}" };
        let mode = self.active_panel().sort_mode;
        self.status_message = format!("Sort: {} {arrow}", mode.display_label());
    }

    pub(super) fn set_sort(&mut self, mode: SortMode) {
        self.active_panel_mut().sort_mode = mode;
        self.resort_from_cache_or_reload();
        self.save_current_sort();
        self.status_message = format!("Sort: {}", mode.display_label());
    }

    fn resort_from_cache_or_reload(&mut self) {
        let path = self.active_panel().path.clone();
        let sort_mode = self.active_panel().sort_mode;
        let sort_reverse = self.active_panel().sort_reverse;
        let show_hidden = self.active_panel().show_hidden;

        if let Some(cached) = self.dir_cache.get(&path) {
            if cached.show_hidden == show_hidden {
                let mut entries = cached.entries.clone();
                panel::resort_entries(&mut entries, sort_mode, sort_reverse, &self.dir_sizes);
                self.active_panel_mut().apply_entries(entries, None);
                return;
            }
        }
        self.reload_active_panel();
    }

    pub(super) fn save_current_sort(&mut self) {
        let path = self.active_panel().path.clone();
        let mode = self.active_panel().sort_mode;
        let rev = self.active_panel().sort_reverse;
        if mode == SortMode::Name && !rev {
            self.dir_sorts.remove(&path);
        } else {
            self.dir_sorts.insert(path.clone(), (mode, rev));
        }
        let label = mode.label().to_string();
        self.db_spawn(move |db| { let _ = db.save_dir_sort(&path, &label, rev); });
    }

    pub(super) fn refresh_current_panel(&mut self) {
        let path = self.active_panel().path.clone();
        self.dir_cache.remove(&path);
        let idx = self.tab().active;
        self.spawn_dir_load(idx, None);
        self.tree_dirty = true;
        self.git_checked_dirs = [None, None, None]; // force re-fetch
        self.refresh_git_status();
    }

    pub(super) fn update_preview(&mut self) {
        if !self.preview_mode {
            self.preview = None;
            self.preview_path = None;
            return;
        }

        let current_path = self.active_panel().selected_entry().map(|e| e.path.clone());

        if current_path == self.preview_path {
            return;
        }

        self.preview_path = current_path.clone();
        if let Some(p) = current_path {
            self.preview = Some(Preview::loading_placeholder(&p));
            self.spawn_preview_load(p);
        } else {
            self.preview = None;
        }
    }

    /// Spawn async preview load for the side panel preview.
    fn spawn_preview_load(&mut self, path: PathBuf) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        // Drop old receiver (cancels stale load)
        self.preview_load_rx = Some(rx);
        let vis = self.visible_height;

        tokio::task::spawn_blocking(move || {
            let preview = Preview::load(&path, vis);
            let _ = tx.send(super::PreviewLoadResult {
                path,
                preview,
            });
        });
    }

    /// Spawn async preview load for the file preview popup.
    pub(super) fn spawn_file_preview_load(&mut self, path: PathBuf) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        // Drop old receiver (cancels stale load)
        self.file_preview_rx = Some(rx);

        tokio::task::spawn_blocking(move || {
            let preview = Preview::load(&path, crate::preview::MAX_LINES);
            let _ = tx.send(super::PreviewLoadResult {
                path,
                preview,
            });
        });
    }

    /// Enter selected directory on the active panel (async).
    pub(super) fn enter_dir_async(&mut self) {
        let panel = self.active_panel();
        let entry = match panel.entries.get(panel.selected) {
            Some(e) if e.is_dir => e,
            _ => return,
        };
        if entry.name == ".." {
            return;
        }
        let new_path = entry.path.clone();
        let idx = self.tab().active;
        self.navigate_cached(new_path, idx, None);
    }

    /// Go to parent directory on the active panel (async).
    pub(super) fn go_parent_async(&mut self) {
        let parent = match self.active_panel().path.parent().map(|p| p.to_path_buf()) {
            Some(p) => p,
            None => return,
        };
        let old_name = self
            .active_panel()
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let idx = self.tab().active;
        self.navigate_cached(parent, idx, old_name);
    }

    /// Go to home directory on the active panel (async).
    pub(super) fn go_home_async(&mut self) {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let idx = self.tab().active;
        self.navigate_cached(home, idx, None);
    }

    /// Navigate to a directory using the cache if available, then spawn a background refresh.
    pub(super) fn navigate_cached(&mut self, path: PathBuf, panel_idx: usize, select_name: Option<String>) {
        let panel = &mut self.tabs[self.active_tab].panels[panel_idx];
        panel.path = path.clone();
        panel.selected = 0;
        panel.offset = 0;
        panel.marked.clear();
        panel.loading = true;

        // Apply sort prefs for this directory
        let (sort_mode, sort_reverse) = self
            .dir_sorts
            .get(&path)
            .copied()
            .unwrap_or((SortMode::Name, false));
        let panel = &mut self.tabs[self.active_tab].panels[panel_idx];
        panel.sort_mode = sort_mode;
        panel.sort_reverse = sort_reverse;

        let show_hidden = panel.show_hidden;

        // Try cache hit
        if let Some(cached) = self.dir_cache.get(&path) {
            if cached.show_hidden == show_hidden {
                let mut entries = cached.entries.clone();
                // Re-sort if sort mode differs from cached
                if cached.sort_mode != sort_mode || cached.sort_reverse != sort_reverse {
                    panel::resort_entries(&mut entries, sort_mode, sort_reverse, &self.dir_sizes);
                }
                let panel = &mut self.tabs[self.active_tab].panels[panel_idx];
                panel.apply_entries(entries, select_name.as_deref());
                panel.loading = false;
            }
        }

        // Always refresh from disk in background
        self.spawn_dir_load(panel_idx, select_name);
    }

    pub(super) fn request_open_editor(&mut self, path: PathBuf) {
        self.open_editor = Some(path);
        // If we're in preview mode, close it
        if self.mode == Mode::Preview {
            self.file_preview = None;
            self.file_preview_path = None;
            self.mode = Mode::Normal;
        }
    }

    pub(super) fn yank_path(&mut self) {
        let path_str = match self.active_panel().selected_entry() {
            Some(e) => e.path.to_string_lossy().into_owned(),
            None => return,
        };
        let label = format!("Path: {path_str}");
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);
        tokio::spawn(async move {
            let ok = copy_to_clipboard(&path_str).await.is_ok();
            let _ = tx.send(super::FileOpResult::Clipboard { label, ok });
        });
    }

    pub(super) fn yank_name(&mut self) {
        let name = match self.active_panel().selected_entry().filter(|e| e.name != "..") {
            Some(e) => e.name.clone(),
            None => return,
        };
        let label = format!("Name: {name}");
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);
        tokio::spawn(async move {
            let ok = copy_to_clipboard(&name).await.is_ok();
            let _ = tx.send(super::FileOpResult::Clipboard { label, ok });
        });
    }

    pub(super) fn set_layout(&mut self, layout: PanelLayout) {
        self.layout = layout;
        self.tab_mut().clamp_active(layout);
        self.status_message = format!("Layout: {}", layout.label());
    }

    pub(super) fn toggle_transparent(&mut self) {
        self.transparent = !self.transparent;
        if self.transparent {
            self.theme.bg = ratatui::style::Color::Reset;
            self.theme.status_bg = ratatui::style::Color::Reset;
        } else {
            self.theme.bg = self.theme.bg_text;
            self.theme.status_bg = self.theme.status_bg_orig;
        }
        let transparent = self.transparent;
        self.db_spawn(move |db| { let _ = db.save_transparent(transparent); });
        self.status_message = if self.transparent {
            "Background: transparent".into()
        } else {
            "Background: opaque".into()
        };
    }

    pub fn which_key_hints(&self) -> Option<Vec<(&'static str, &'static str)>> {
        const LEADER_HINTS: &[(&str, &str)] = &[
            ("", "Toggle"),
            ("t", "tree"),
            ("h", "hidden"),
            ("p", "preview"),
            ("u", "ui"),
            ("", "Actions"),
            ("s", "sort"),
            ("d", "dir sizes"),
            ("w", "layout"),
            ("", "Select"),
            ("a", "select all"),
            ("n", "unselect"),
            ("", "Search"),
            (",", "find"),
            (".", "find global"),
            ("", "Other"),
            ("b", "bookmarks"),
            ("?", "help"),
        ];
        const GOTO_HINTS: &[(&str, &str)] = &[("g", "top"), ("t", "next tab"), ("T", "prev tab")];
        const YANK_HINTS: &[(&str, &str)] = &[("y", "yank"), ("p", "yank path"), ("n", "yank name")];
        const DELETE_HINTS: &[(&str, &str)] = &[("d", "trash"), ("D", "permanent")];
        const CHANGE_HINTS: &[(&str, &str)] = &[("p", "permissions"), ("o", "owner")];
        const MARK_HINTS: &[(&str, &str)] = &[("a-z", "go to mark")];

        let pending = self.pending_key?;
        let time = self.pending_key_time?;
        if time.elapsed() < std::time::Duration::from_millis(400) {
            return None;
        }
        match pending {
            ' ' => Some(LEADER_HINTS.to_vec()),
            's' => Some(self.build_sort_hints()),
            'g' => Some(GOTO_HINTS.to_vec()),
            'y' => Some(YANK_HINTS.to_vec()),
            'd' => Some(DELETE_HINTS.to_vec()),
            'c' => Some(CHANGE_HINTS.to_vec()),
            '\'' => Some(MARK_HINTS.to_vec()),
            'w' => Some(self.build_layout_hints()),
            'u' => Some(self.build_ui_hints()),
            _ => None,
        }
    }

    fn build_sort_hints(&self) -> Vec<(&'static str, &'static str)> {
        let mode = self.active_panel().sort_mode;
        let rev = self.active_panel().sort_reverse;

        let m = |active, label, inactive_label| {
            if active { label } else { inactive_label }
        };

        vec![
            ("", "Sort by"),
            ("n", m(mode == SortMode::Name,     "▍name",     " name")),
            ("s", m(mode == SortMode::Size,     "▍size",     " size")),
            ("m/d", m(mode == SortMode::Modified, "▍modified", " modified")),
            ("c", m(mode == SortMode::Created,  "▍created",  " created")),
            ("e", m(mode == SortMode::Extension,"▍extension"," extension")),
            ("", "Direction"),
            ("r", if rev { "▍reverse \u{25b2}" } else { " ascending \u{25bc}" }),
        ]
    }

    fn build_ui_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("", "UI"),
            ("t", if self.transparent { "\u{258a} transparent" } else { "  transparent" }),
        ]
    }

    fn build_layout_hints(&self) -> Vec<(&'static str, &'static str)> {
        let l = self.layout;
        let m = |active, label, inactive_label| {
            if active { label } else { inactive_label }
        };
        vec![
            ("1", m(l == PanelLayout::Single, "▍single", "  single")),
            ("2", m(l == PanelLayout::Dual,   "▍dual",   "  dual")),
            ("3", m(l == PanelLayout::Triple, "▍triple", "  triple")),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn toggle_tree_enables_and_focuses() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(!app.show_tree);
        assert!(!app.tree_focused);
        app.toggle_tree();
        assert!(app.show_tree);
        assert!(app.tree_focused);
        app.toggle_tree();
        assert!(!app.show_tree);
        assert!(!app.tree_focused);
    }

    #[tokio::test]
    async fn toggle_sort_reverse_flips() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(!app.active_panel().sort_reverse);
        app.toggle_sort_reverse();
        assert!(app.active_panel().sort_reverse);
        app.toggle_sort_reverse();
        assert!(!app.active_panel().sort_reverse);
    }

    #[tokio::test]
    async fn set_sort_updates_mode() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.set_sort(SortMode::Extension);
        assert_eq!(app.active_panel().sort_mode, SortMode::Extension);
        assert!(app.status_message.contains("Sort:"));
    }

    #[tokio::test]
    async fn set_layout_updates() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.set_layout(PanelLayout::Triple);
        assert_eq!(app.layout, PanelLayout::Triple);
        assert!(app.status_message.contains("triple"));
    }

    #[tokio::test]
    async fn toggle_transparent_flips() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(!app.transparent);
        app.toggle_transparent();
        assert!(app.transparent);
        assert!(app.status_message.contains("transparent"));
        app.toggle_transparent();
        assert!(!app.transparent);
        assert!(app.status_message.contains("opaque"));
    }

    #[tokio::test]
    async fn request_open_editor_sets_path() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.request_open_editor(PathBuf::from("/test/a.txt"));
        assert_eq!(app.open_editor, Some(PathBuf::from("/test/a.txt")));
    }

    #[tokio::test]
    async fn request_open_editor_closes_preview() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Preview;
        app.file_preview = Some(Preview::loading_placeholder(&PathBuf::from("/x")));
        app.file_preview_path = Some(PathBuf::from("/x"));
        app.request_open_editor(PathBuf::from("/test/a.txt"));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.file_preview.is_none());
    }

    #[tokio::test]
    async fn update_preview_clears_when_disabled() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.preview_mode = false;
        app.preview = Some(Preview::loading_placeholder(&PathBuf::from("/x")));
        app.update_preview();
        assert!(app.preview.is_none());
        assert!(app.preview_path.is_none());
    }

    #[tokio::test]
    async fn save_current_sort_removes_default() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        // Set non-default sort
        app.active_panel_mut().sort_mode = SortMode::Size;
        app.active_panel_mut().sort_reverse = true;
        app.save_current_sort();
        assert!(app.dir_sorts.contains_key(&PathBuf::from("/test")));

        // Set back to default
        app.active_panel_mut().sort_mode = SortMode::Name;
        app.active_panel_mut().sort_reverse = false;
        app.save_current_sort();
        assert!(!app.dir_sorts.contains_key(&PathBuf::from("/test")));
    }

    #[tokio::test]
    async fn which_key_hints_without_pending() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let app = App::new_for_test(entries);
        assert!(app.which_key_hints().is_none());
    }

    #[tokio::test]
    async fn close_tab_single_tab_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert_eq!(app.tabs.len(), 1);
        app.close_tab();
        assert_eq!(app.tabs.len(), 1);
        assert!(app.status_message.contains("Cannot close"));
    }

    #[tokio::test]
    async fn new_tab_creates_and_switches() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert_eq!(app.tabs.len(), 1);
        app.new_tab();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert!(app.status_message.contains("Tab 2"));
    }

    #[tokio::test]
    async fn close_tab_removes_and_clamps() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.new_tab();
        app.new_tab();
        assert_eq!(app.tabs.len(), 3);
        assert_eq!(app.active_tab, 2);
        app.close_tab();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1); // clamped
    }

    #[tokio::test]
    async fn next_tab_prev_tab_wrapping() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.new_tab();
        app.new_tab();
        app.active_tab = 0;
        app.next_tab();
        assert_eq!(app.active_tab, 1);
        app.next_tab();
        assert_eq!(app.active_tab, 2);
        app.next_tab();
        assert_eq!(app.active_tab, 0); // wraps
        app.prev_tab();
        assert_eq!(app.active_tab, 2); // wraps back
    }

    #[tokio::test]
    async fn single_tab_next_prev_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.next_tab();
        assert_eq!(app.active_tab, 0);
        app.prev_tab();
        assert_eq!(app.active_tab, 0);
    }

    #[tokio::test]
    async fn toggle_hidden_toggles_all_panels() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(!app.active_panel().show_hidden);
        app.toggle_hidden();
        // All panels should have hidden=true
        for panel in &app.tabs[0].panels {
            assert!(panel.show_hidden);
        }
        assert!(app.status_message.contains("shown"));
        app.toggle_hidden();
        for panel in &app.tabs[0].panels {
            assert!(!panel.show_hidden);
        }
        assert!(app.status_message.contains("hidden"));
    }

    #[tokio::test]
    async fn focus_next_from_tree_to_panel() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.show_tree = true;
        app.tree_focused = true;
        app.focus_next();
        assert!(!app.tree_focused);
        assert_eq!(app.tab().active, 0);
    }

    #[tokio::test]
    async fn focus_prev_from_panel_to_tree() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.show_tree = true;
        app.tree_focused = false;
        app.tab_mut().active = 0;
        app.focus_prev();
        assert!(app.tree_focused);
    }

    #[tokio::test]
    async fn focus_prev_tree_already_focused_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.show_tree = true;
        app.tree_focused = true;
        app.focus_prev();
        assert!(app.tree_focused); // stays
    }

    #[tokio::test]
    async fn exit_mode_on_tab_switch_visual() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.new_tab();
        app.active_tab = 0;
        app.mode = Mode::Visual;
        app.active_panel_mut().visual_anchor = Some(0);
        app.next_tab();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn exit_mode_on_tab_switch_select() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.new_tab();
        app.active_tab = 0;
        app.mode = Mode::Select;
        app.next_tab();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn navigate_cached_resets_panel_state() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 2;
        app.active_panel_mut().marked.insert(PathBuf::from("/test/a.txt"));
        app.navigate_cached(PathBuf::from("/new_dir"), 0, None);
        assert_eq!(app.active_panel().selected, 0);
        assert!(app.active_panel().marked.is_empty());
        assert_eq!(app.active_panel().path, PathBuf::from("/new_dir"));
    }

    #[tokio::test]
    async fn navigate_cached_applies_sort_prefs() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let target = PathBuf::from("/sorted_dir");
        app.dir_sorts.insert(target.clone(), (SortMode::Size, true));
        app.navigate_cached(target, 0, None);
        assert_eq!(app.active_panel().sort_mode, SortMode::Size);
        assert!(app.active_panel().sort_reverse);
    }

    #[tokio::test]
    async fn refresh_current_panel_marks_tree_dirty() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.tree_dirty = false;
        app.refresh_current_panel();
        assert!(app.tree_dirty);
    }

    #[tokio::test]
    async fn update_preview_skips_same_path() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.preview_mode = true;
        app.active_panel_mut().selected = 1;
        let path = app.active_panel().selected_entry().map(|e| e.path.clone());
        app.preview_path = path;
        // Should be a no-op since path matches
        app.update_preview();
        // preview stays as-is (not replaced with loading_placeholder)
    }

    #[tokio::test]
    async fn which_key_hints_with_pending_too_recent() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.pending_key = Some(' ');
        app.pending_key_time = Some(Instant::now()); // just now, < 400ms
        assert!(app.which_key_hints().is_none());
    }

    #[tokio::test]
    async fn which_key_hints_with_pending_after_delay() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.pending_key = Some(' ');
        app.pending_key_time = Some(Instant::now() - std::time::Duration::from_millis(500));
        let hints = app.which_key_hints();
        assert!(hints.is_some());
        let hints = hints.unwrap();
        assert!(!hints.is_empty());
    }

    #[tokio::test]
    async fn which_key_unknown_pending_returns_none() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.pending_key = Some('z'); // no mapping
        app.pending_key_time = Some(Instant::now() - std::time::Duration::from_millis(500));
        assert!(app.which_key_hints().is_none());
    }
}
