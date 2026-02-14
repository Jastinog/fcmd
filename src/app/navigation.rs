use super::*;
use crate::util::copy_to_clipboard;

impl App {
    pub(super) fn focus_next(&mut self) {
        if self.tree_focused && self.show_tree {
            // Tree → active panel
            self.tree_focused = false;
        } else {
            let tab = self.tab_mut();
            match tab.active {
                PanelSide::Left => tab.active = PanelSide::Right,
                PanelSide::Right => {} // already rightmost
            }
        }
    }

    pub(super) fn focus_prev(&mut self) {
        if self.tree_focused {
            return; // already leftmost
        }
        let current_side = self.tab().active;
        match current_side {
            PanelSide::Right => {
                self.tab_mut().active = PanelSide::Left;
            }
            PanelSide::Left => {
                if self.show_tree {
                    self.tree_focused = true;
                }
            }
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
        let tab = &mut self.tabs[self.active_tab];
        tab.left.show_hidden = hidden;
        tab.right.show_hidden = hidden;
        let _ = tab.left.load_dir_with_sizes(&self.dir_sizes);
        let _ = tab.right.load_dir_with_sizes(&self.dir_sizes);
        self.tree_dirty = true;
        self.status_message = if hidden {
            "Hidden files: shown".into()
        } else {
            "Hidden files: hidden".into()
        };
    }

    pub(super) fn cycle_theme(&mut self, forward: bool) {
        if self.theme_list.is_empty() {
            self.theme_list = Theme::list_available();
            if self.theme_list.is_empty() {
                self.status_message = "No themes found".into();
                return;
            }
        }
        let len = self.theme_list.len();
        let idx = match self.theme_index {
            Some(i) => {
                if forward {
                    (i + 1) % len
                } else {
                    (i + len - 1) % len
                }
            }
            None => 0,
        };
        let name = &self.theme_list[idx];
        match Theme::load_by_name(name) {
            Some(t) => {
                self.theme = t;
                self.theme_index = Some(idx);
                self.status_message = format!("Theme [{}/{}]: {name}", idx + 1, len);
                if let Some(ref db) = self.db {
                    let _ = db.save_theme(name);
                }
            }
            None => self.status_message = format!("Failed to load theme: {name}"),
        }
    }

    pub(super) fn toggle_tree(&mut self) {
        self.show_tree = !self.show_tree;
        if !self.show_tree {
            self.tree_focused = false;
        }
    }

    pub(super) fn toggle_sort_reverse(&mut self) {
        let rev = !self.active_panel().sort_reverse;
        self.active_panel_mut().sort_reverse = rev;
        self.reload_active_panel();
        let arrow = if rev { "\u{2191}" } else { "\u{2193}" };
        let label = self.active_panel().sort_mode.label();
        self.status_message = format!("Sort: {label}{arrow}");
    }

    pub(super) fn set_sort(&mut self, mode: SortMode) {
        self.active_panel_mut().sort_mode = mode;
        self.reload_active_panel();
        self.status_message = format!("Sort: {}", mode.label());
    }

    pub(super) fn refresh_current_panel(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let panel = match tab.active {
            PanelSide::Left => &mut tab.left,
            PanelSide::Right => &mut tab.right,
        };
        if let Err(e) = panel.load_dir_with_sizes(&self.dir_sizes) {
            self.status_message = format!("Refresh error: {e}");
        }
        self.tree_dirty = true;
        self.git_status_dir = None; // force re-fetch
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
        self.preview = current_path.map(|p| Preview::load(&p));
    }

    pub(super) fn yank_path(&mut self) {
        let path_str = match self.active_panel().selected_entry() {
            Some(e) => e.path.to_string_lossy().into_owned(),
            None => return,
        };
        match copy_to_clipboard(&path_str) {
            Ok(()) => self.status_message = format!("Path: {path_str}"),
            Err(_) => self.status_message = "Clipboard not available".into(),
        }
    }

    pub fn which_key_hints(&self) -> Option<&[(&str, &str)]> {
        const LEADER_HINTS: &[(&str, &str)] = &[
            ("t", "tree"),
            ("h", "hidden"),
            ("p", "preview"),
            ("s", "sort"),
            ("d", "dir sizes"),
            ("a", "select all"),
            ("n", "unselect"),
            (",", "find"),
            (".", "find global"),
            ("?", "help"),
        ];
        const SORT_HINTS: &[(&str, &str)] = &[
            ("n", "name"),
            ("s", "size"),
            ("m/d", "modified"),
            ("c", "created"),
            ("e", "extension"),
            ("r", "reverse"),
        ];
        const GOTO_HINTS: &[(&str, &str)] = &[
            ("g", "top"),
            ("t", "next tab"),
            ("T", "prev tab"),
        ];
        const YANK_HINTS: &[(&str, &str)] = &[
            ("y", "yank"),
            ("p", "yank path"),
        ];
        const DELETE_HINTS: &[(&str, &str)] = &[
            ("d", "delete"),
        ];
        const MARK_HINTS: &[(&str, &str)] = &[
            ("a-z", "go to mark"),
        ];
        let pending = self.pending_key?;
        let time = self.pending_key_time?;
        if time.elapsed() < std::time::Duration::from_millis(400) {
            return None;
        }
        match pending {
            ' ' => Some(LEADER_HINTS),
            's' => Some(SORT_HINTS),
            'g' => Some(GOTO_HINTS),
            'y' => Some(YANK_HINTS),
            'd' => Some(DELETE_HINTS),
            '\'' => Some(MARK_HINTS),
            _ => None,
        }
    }
}
