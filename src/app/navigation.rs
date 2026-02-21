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
        match Tab::new(path) {
            Ok(tab) => {
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
                self.preview_path = None;
                self.status_message = format!("Tab {}", self.tabs.len());
            }
            Err(e) => self.status_message = format!("Tab error: {e}"),
        }
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
        self.reload_active_panel();
        self.save_current_sort();
        let arrow = if rev { "\u{2191}" } else { "\u{2193}" };
        let label = self.active_panel().sort_mode.label();
        self.status_message = format!("Sort: {label}{arrow}");
    }

    pub(super) fn set_sort(&mut self, mode: SortMode) {
        self.active_panel_mut().sort_mode = mode;
        self.reload_active_panel();
        self.save_current_sort();
        self.status_message = format!("Sort: {}", mode.label());
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
        if let Some(ref db) = self.db {
            let _ = db.save_dir_sort(&path, mode.label(), rev);
        }
    }

    pub(super) fn refresh_current_panel(&mut self) {
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
            let mut preview = Preview::load(&path, vis);
            preview.apply_highlighting(&path, vis);
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
        let vis = self.visible_height;

        tokio::task::spawn_blocking(move || {
            let mut preview = Preview::load(&path, crate::preview::MAX_LINES);
            preview.apply_highlighting(&path, vis);
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
        let new_path = entry.path.clone();
        let idx = self.tab().active;
        self.active_panel_mut().navigate_to(new_path);
        self.apply_dir_sort_no_reload();
        self.spawn_dir_load(idx, None);
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
        self.active_panel_mut().navigate_to(parent);
        self.apply_dir_sort_no_reload();
        self.spawn_dir_load(idx, old_name);
    }

    /// Go to home directory on the active panel (async).
    pub(super) fn go_home_async(&mut self) {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let idx = self.tab().active;
        self.active_panel_mut().navigate_to(home);
        self.apply_dir_sort_no_reload();
        self.spawn_dir_load(idx, None);
    }

    /// Apply dir sort preference without reloading entries (used before async load).
    pub(super) fn apply_dir_sort_no_reload(&mut self) {
        let path = self.active_panel().path.clone();
        let (new_mode, new_rev) = self
            .dir_sorts
            .get(&path)
            .copied()
            .unwrap_or((SortMode::Name, false));
        let panel = self.active_panel_mut();
        panel.sort_mode = new_mode;
        panel.sort_reverse = new_rev;
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

    pub fn which_key_hints(&self) -> Option<&[(&str, &str)]> {
        const LEADER_HINTS: &[(&str, &str)] = &[
            ("", "Toggle"),
            ("t", "tree"),
            ("h", "hidden"),
            ("p", "preview"),
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
        const SORT_NAME: &[(&str, &str)] = &[("n", "● name"), ("s", "  size"), ("m/d", "  modified"), ("c", "  created"), ("e", "  extension"), ("r", "  reverse ↓")];
        const SORT_NAME_R: &[(&str, &str)] = &[("n", "● name"), ("s", "  size"), ("m/d", "  modified"), ("c", "  created"), ("e", "  extension"), ("r", "● reverse ↑")];
        const SORT_SIZE: &[(&str, &str)] = &[("n", "  name"), ("s", "● size"), ("m/d", "  modified"), ("c", "  created"), ("e", "  extension"), ("r", "  reverse ↓")];
        const SORT_SIZE_R: &[(&str, &str)] = &[("n", "  name"), ("s", "● size"), ("m/d", "  modified"), ("c", "  created"), ("e", "  extension"), ("r", "● reverse ↑")];
        const SORT_MOD: &[(&str, &str)] = &[("n", "  name"), ("s", "  size"), ("m/d", "● modified"), ("c", "  created"), ("e", "  extension"), ("r", "  reverse ↓")];
        const SORT_MOD_R: &[(&str, &str)] = &[("n", "  name"), ("s", "  size"), ("m/d", "● modified"), ("c", "  created"), ("e", "  extension"), ("r", "● reverse ↑")];
        const SORT_CRE: &[(&str, &str)] = &[("n", "  name"), ("s", "  size"), ("m/d", "  modified"), ("c", "● created"), ("e", "  extension"), ("r", "  reverse ↓")];
        const SORT_CRE_R: &[(&str, &str)] = &[("n", "  name"), ("s", "  size"), ("m/d", "  modified"), ("c", "● created"), ("e", "  extension"), ("r", "● reverse ↑")];
        const SORT_EXT: &[(&str, &str)] = &[("n", "  name"), ("s", "  size"), ("m/d", "  modified"), ("c", "  created"), ("e", "● extension"), ("r", "  reverse ↓")];
        const SORT_EXT_R: &[(&str, &str)] = &[("n", "  name"), ("s", "  size"), ("m/d", "  modified"), ("c", "  created"), ("e", "● extension"), ("r", "● reverse ↑")];

        const GOTO_HINTS: &[(&str, &str)] = &[("g", "top"), ("t", "next tab"), ("T", "prev tab")];
        const YANK_HINTS: &[(&str, &str)] = &[("y", "yank"), ("p", "yank path"), ("n", "yank name")];
        const DELETE_HINTS: &[(&str, &str)] = &[("d", "trash"), ("D", "permanent")];
        const CHANGE_HINTS: &[(&str, &str)] = &[("p", "permissions"), ("o", "owner")];
        const MARK_HINTS: &[(&str, &str)] = &[("a-z", "go to mark")];
        const LAYOUT_SINGLE: &[(&str, &str)] = &[
            ("1", "● single"),
            ("2", "  dual"),
            ("3", "  triple"),
        ];
        const LAYOUT_DUAL: &[(&str, &str)] = &[
            ("1", "  single"),
            ("2", "● dual"),
            ("3", "  triple"),
        ];
        const LAYOUT_TRIPLE: &[(&str, &str)] = &[
            ("1", "  single"),
            ("2", "  dual"),
            ("3", "● triple"),
        ];
        let pending = self.pending_key?;
        let time = self.pending_key_time?;
        if time.elapsed() < std::time::Duration::from_millis(400) {
            return None;
        }
        match pending {
            ' ' => Some(LEADER_HINTS),
            's' => {
                let rev = self.active_panel().sort_reverse;
                Some(match (self.active_panel().sort_mode, rev) {
                    (SortMode::Name, false) => SORT_NAME,
                    (SortMode::Name, true) => SORT_NAME_R,
                    (SortMode::Size, false) => SORT_SIZE,
                    (SortMode::Size, true) => SORT_SIZE_R,
                    (SortMode::Modified, false) => SORT_MOD,
                    (SortMode::Modified, true) => SORT_MOD_R,
                    (SortMode::Created, false) => SORT_CRE,
                    (SortMode::Created, true) => SORT_CRE_R,
                    (SortMode::Extension, false) => SORT_EXT,
                    (SortMode::Extension, true) => SORT_EXT_R,
                })
            }
            'g' => Some(GOTO_HINTS),
            'y' => Some(YANK_HINTS),
            'd' => Some(DELETE_HINTS),
            'c' => Some(CHANGE_HINTS),
            '\'' => Some(MARK_HINTS),
            'w' => Some(match self.layout {
                PanelLayout::Single => LAYOUT_SINGLE,
                PanelLayout::Dual => LAYOUT_DUAL,
                PanelLayout::Triple => LAYOUT_TRIPLE,
            }),
            _ => None,
        }
    }
}
