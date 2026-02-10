use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::find::FindState;
use crate::ops::{self, ProgressMsg, Register, RegisterOp, UndoStack};
use crate::panel::{Panel, SortMode};
use crate::preview::Preview;

pub struct PhantomEntry {
    pub name: String,
    pub is_dir: bool,
}

pub struct PasteProgress {
    pub rx: mpsc::Receiver<ProgressMsg>,
    pub op: RegisterOp,
    pub started_at: Instant,
    pub dst_dir: PathBuf,
    pub phantoms: Vec<PhantomEntry>,
}

#[derive(PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual,
    Command,
    Confirm,
    Search,
    Find,
    Help,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum PanelSide {
    Left,
    Right,
}

pub struct Tab {
    pub left: Panel,
    pub right: Panel,
    pub active: PanelSide,
}

impl Tab {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        Ok(Tab {
            left: Panel::new(path.clone())?,
            right: Panel::new(path)?,
            active: PanelSide::Left,
        })
    }

    pub fn active_panel(&self) -> &Panel {
        match self.active {
            PanelSide::Left => &self.left,
            PanelSide::Right => &self.right,
        }
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        match self.active {
            PanelSide::Left => &mut self.left,
            PanelSide::Right => &mut self.right,
        }
    }

    pub fn inactive_panel_path(&self) -> PathBuf {
        match self.active {
            PanelSide::Left => self.right.path.clone(),
            PanelSide::Right => self.left.path.clone(),
        }
    }

    pub fn switch_panel(&mut self) {
        self.active = match self.active {
            PanelSide::Left => PanelSide::Right,
            PanelSide::Right => PanelSide::Left,
        };
    }
}

pub struct App {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub mode: Mode,
    pub command_input: String,
    pub should_quit: bool,
    pub status_message: String,
    pub pending_key: Option<char>,
    pub visible_height: usize,
    pub register: Option<Register>,
    pub undo_stack: UndoStack,
    pub confirm_paths: Vec<PathBuf>,
    // Search
    pub search_query: String,
    pub search_saved_cursor: usize,
    // Marks
    pub marks: HashMap<char, PathBuf>,
    // Find
    pub find_state: Option<FindState>,
    // Preview
    pub preview_mode: bool,
    pub preview: Option<Preview>,
    preview_path: Option<PathBuf>,
    // Tree
    pub show_tree: bool,
    pub tree_focused: bool,
    pub tree_selected: usize,
    pub tree_scroll: usize,
    pub start_dir: PathBuf,
    pub tree_data: Vec<crate::tree::TreeLine>,
    // Shell
    pub pending_shell: Option<String>,
    // Visual marks (persistent colored dots)
    pub visual_marks: HashSet<PathBuf>,
    // Database
    pub db: Option<crate::db::Db>,
    // Paste progress
    pub paste_progress: Option<PasteProgress>,
}

impl App {
    pub fn new() -> std::io::Result<Self> {
        let cwd = std::env::current_dir()?;

        let (db, visual_marks) = match crate::db::Db::init() {
            Ok(db) => {
                let marks = db.load_visual_marks().unwrap_or_default();
                (Some(db), marks)
            }
            Err(e) => {
                eprintln!("Warning: DB init failed: {e}");
                (None, HashSet::new())
            }
        };

        // Restore session from DB
        let (tabs, active_tab) = if let Some(ref db) = db {
            match db.load_session() {
                Ok((saved, at)) if !saved.is_empty() => {
                    let mut tabs = Vec::new();
                    for st in &saved {
                        let left_path = if st.left_path.is_dir() {
                            st.left_path.clone()
                        } else {
                            cwd.clone()
                        };
                        let right_path = if st.right_path.is_dir() {
                            st.right_path.clone()
                        } else {
                            cwd.clone()
                        };
                        let mut tab = Tab {
                            left: Panel::new(left_path)?,
                            right: Panel::new(right_path)?,
                            active: if st.active_side == "right" {
                                PanelSide::Right
                            } else {
                                PanelSide::Left
                            },
                        };
                        let _ = tab.left.load_dir();
                        let _ = tab.right.load_dir();
                        tabs.push(tab);
                    }
                    let at = at.min(tabs.len().saturating_sub(1));
                    (tabs, at)
                }
                _ => (vec![Tab::new(cwd.clone())?], 0),
            }
        } else {
            (vec![Tab::new(cwd.clone())?], 0)
        };

        Ok(App {
            tabs,
            active_tab,
            mode: Mode::Normal,
            command_input: String::new(),
            should_quit: false,
            status_message: String::new(),
            pending_key: None,
            visible_height: 20,
            register: None,
            undo_stack: UndoStack::new(),
            confirm_paths: Vec::new(),
            search_query: String::new(),
            search_saved_cursor: 0,
            marks: HashMap::new(),
            find_state: None,
            preview_mode: false,
            preview: None,
            preview_path: None,
            show_tree: false,
            tree_focused: false,
            tree_selected: 0,
            tree_scroll: 0,
            start_dir: cwd,
            tree_data: Vec::new(),
            pending_shell: None,
            visual_marks,
            db,
            paste_progress: None,
        })
    }

    pub fn save_session(&self) {
        let Some(ref db) = self.db else { return };
        let tabs: Vec<crate::db::SavedTab> = self
            .tabs
            .iter()
            .map(|t| crate::db::SavedTab {
                left_path: t.left.path.clone(),
                right_path: t.right.path.clone(),
                active_side: match t.active {
                    PanelSide::Left => "left".into(),
                    PanelSide::Right => "right".into(),
                },
            })
            .collect();
        if let Err(e) = db.save_session(&tabs, self.active_tab) {
            eprintln!("Warning: failed to save session: {e}");
        }
    }

    pub fn phantoms_for(&self, dir: &std::path::Path) -> &[PhantomEntry] {
        match &self.paste_progress {
            Some(p) if p.dst_dir == dir => &p.phantoms,
            _ => &[],
        }
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    pub fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    pub fn active_panel(&self) -> &Panel {
        self.tab().active_panel()
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        self.tab_mut().active_panel_mut()
    }

    fn inactive_panel_path(&self) -> PathBuf {
        self.tab().inactive_panel_path()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.status_message.clear();

        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Visual => self.handle_visual(key),
            Mode::Command => self.handle_command(key),
            Mode::Confirm => self.handle_confirm(key),
            Mode::Search => self.handle_search(key),
            Mode::Find => self.handle_find(key),
            Mode::Help => self.handle_help(key),
        }

        self.update_preview();
    }

    // --- Normal mode ---

    fn handle_normal(&mut self, key: KeyEvent) {
        // Delegate to tree handler when tree is focused
        if self.tree_focused && self.show_tree {
            self.handle_tree_input(key);
            return;
        }

        if let Some(pending) = self.pending_key.take() {
            match (pending, key.code) {
                ('g', KeyCode::Char('g')) => {
                    self.active_panel_mut().go_top();
                    return;
                }
                ('g', KeyCode::Char('t')) => {
                    self.next_tab();
                    return;
                }
                ('g', KeyCode::Char('T')) => {
                    self.prev_tab();
                    return;
                }
                ('d', KeyCode::Char('d')) => {
                    self.request_delete();
                    return;
                }
                ('y', KeyCode::Char('y')) => {
                    self.yank_targeted();
                    return;
                }
                ('y', KeyCode::Char('p')) => {
                    self.yank_path();
                    return;
                }
                ('\'', KeyCode::Char(c)) if c.is_ascii_lowercase() => {
                    self.goto_mark(c);
                    return;
                }
                ('s', KeyCode::Char('n')) => {
                    self.set_sort(SortMode::Name);
                    return;
                }
                ('s', KeyCode::Char('s')) => {
                    self.set_sort(SortMode::Size);
                    return;
                }
                ('s', KeyCode::Char('d')) => {
                    self.set_sort(SortMode::Date);
                    return;
                }
                ('s', KeyCode::Char('e')) => {
                    self.set_sort(SortMode::Extension);
                    return;
                }
                ('s', KeyCode::Char('r')) => {
                    let rev = !self.active_panel().sort_reverse;
                    self.active_panel_mut().sort_reverse = rev;
                    let _ = self.active_panel_mut().load_dir();
                    let arrow = if rev { "\u{2191}" } else { "\u{2193}" };
                    let label = self.active_panel().sort_mode.label();
                    self.status_message = format!("Sort: {label}{arrow}");
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,

            // Focus navigation (Ctrl+h/l)
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_next();
            }
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.focus_prev();
            }

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => self.active_panel_mut().move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.active_panel_mut().move_up(),
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

            // Pending sequences
            KeyCode::Char('g') => self.pending_key = Some('g'),
            KeyCode::Char('G') => self.active_panel_mut().go_bottom(),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_down(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_up(half);
            }
            KeyCode::Char('d') => self.pending_key = Some('d'),
            KeyCode::Char('y') => self.pending_key = Some('y'),

            // Paste
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_find();
            }
            KeyCode::Char('p') => self.paste(false),
            KeyCode::Char('P') => self.paste(true),

            // Undo
            KeyCode::Char('u') => self.undo(),

            // Mark toggle
            KeyCode::Char(' ') => self.active_panel_mut().toggle_mark(),

            // Visual mode
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.mode = Mode::Visual;
                let sel = self.active_panel().selected;
                self.active_panel_mut().visual_anchor = Some(sel);
            }

            // Search
            KeyCode::Char('/') => {
                self.search_saved_cursor = self.active_panel().selected;
                self.search_query.clear();
                self.mode = Mode::Search;
            }
            KeyCode::Char('n') => self.search_next(),
            KeyCode::Char('N') => self.search_prev(),

            // Visual mark toggle
            KeyCode::Char('m') => self.toggle_visual_mark(),
            // Bookmarks
            KeyCode::Char('\'') => self.pending_key = Some('\''),

            // Panel switch
            KeyCode::Tab => self.tab_mut().switch_panel(),

            // Home
            KeyCode::Char('~') => {
                if let Err(e) = self.active_panel_mut().go_home() {
                    self.status_message = format!("Error: {e}");
                }
            }

            // Refresh
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(e) = self.active_panel_mut().load_dir() {
                    self.status_message = format!("Refresh error: {e}");
                }
            }

            // Toggle hidden files
            KeyCode::Char('.') => {
                let hidden = !self.active_panel().show_hidden;
                let tab = self.tab_mut();
                tab.left.show_hidden = hidden;
                tab.right.show_hidden = hidden;
                let _ = tab.left.load_dir();
                let _ = tab.right.load_dir();
                self.status_message = if hidden {
                    "Hidden files: shown".into()
                } else {
                    "Hidden files: hidden".into()
                };
            }

            // Toggle preview
            KeyCode::Char('w') => {
                self.preview_mode = !self.preview_mode;
            }

            // Toggle tree
            KeyCode::Char('t') => {
                self.show_tree = !self.show_tree;
                if !self.show_tree {
                    self.tree_focused = false;
                }
            }

            // Shell
            KeyCode::Char('S') => {
                self.pending_shell = Some(String::new());
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

            // Sort pending
            KeyCode::Char('s') => self.pending_key = Some('s'),

            // Help
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
            }

            // Command mode
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_input.clear();
            }

            _ => {}
        }
    }

    // --- Tree input ---

    fn handle_tree_input(&mut self, key: KeyEvent) {
        // Handle pending key
        if let Some(pending) = self.pending_key.take() {
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
            KeyCode::Char('g') => self.pending_key = Some('g'),
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

    // --- Focus navigation ---

    fn focus_next(&mut self) {
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

    fn focus_prev(&mut self) {
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

    // --- Visual mode ---

    fn handle_visual(&mut self, key: KeyEvent) {
        if let Some('g') = self.pending_key.take() {
            if key.code == KeyCode::Char('g') {
                self.active_panel_mut().go_top();
                return;
            }
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.active_panel_mut().move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.active_panel_mut().move_up(),
            KeyCode::Char('G') => self.active_panel_mut().go_bottom(),
            KeyCode::Char('g') => self.pending_key = Some('g'),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_down(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                self.active_panel_mut().page_up(half);
            }

            KeyCode::Char('y') => {
                self.yank_targeted();
                self.exit_visual();
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.request_delete();
                if self.mode == Mode::Visual {
                    self.exit_visual();
                }
            }

            KeyCode::Char('p') => {
                self.paste(false);
                self.exit_visual();
            }

            KeyCode::Char('v') | KeyCode::Char('V') | KeyCode::Esc => self.exit_visual(),

            KeyCode::Tab => {
                self.exit_visual();
                self.tab_mut().switch_panel();
            }

            _ => {}
        }
    }

    fn exit_visual(&mut self) {
        self.active_panel_mut().visual_anchor = None;
        self.mode = Mode::Normal;
    }

    // --- Help mode ---

    fn handle_help(&mut self, _key: KeyEvent) {
        self.mode = Mode::Normal;
    }

    // --- Tabs ---

    fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            self.preview_path = None;
        }
    }

    fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
            self.preview_path = None;
        }
    }

    fn new_tab(&mut self) {
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

    fn close_tab(&mut self) {
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

    fn yank_path(&mut self) {
        let path_str = match self.active_panel().selected_entry() {
            Some(e) => e.path.to_string_lossy().into_owned(),
            None => return,
        };
        match copy_to_clipboard(&path_str) {
            Ok(()) => self.status_message = format!("Path: {path_str}"),
            Err(_) => self.status_message = "Clipboard not available".into(),
        }
    }

    // --- Search mode ---

    fn handle_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.search_jump_to_match();
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                if self.search_query.is_empty() {
                    self.active_panel_mut().selected = self.search_saved_cursor;
                } else {
                    self.search_jump_to_match();
                }
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                if self.search_query.is_empty() {
                    self.active_panel_mut().selected = self.search_saved_cursor;
                }
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.active_panel_mut().selected = self.search_saved_cursor;
                self.search_query.clear();
            }
            _ => {}
        }
    }

    fn search_jump_to_match(&mut self) {
        let query = self.search_query.to_lowercase();
        let start = self.search_saved_cursor;

        let pos = {
            let panel = self.active_panel();
            let len = panel.entries.len();
            if len == 0 || query.is_empty() {
                None
            } else {
                (0..len)
                    .map(|i| (start + i) % len)
                    .find(|&i| panel.entries[i].name.to_lowercase().contains(&query))
            }
        };

        if let Some(pos) = pos {
            self.active_panel_mut().selected = pos;
        }
    }

    fn search_next(&mut self) {
        if self.search_query.is_empty() {
            self.status_message = "No search pattern \u{2014} use / to search".into();
            return;
        }
        let query = self.search_query.to_lowercase();

        let pos = {
            let panel = self.active_panel();
            let len = panel.entries.len();
            if len == 0 {
                None
            } else {
                let start = (panel.selected + 1) % len;
                (0..len)
                    .map(|i| (start + i) % len)
                    .find(|&i| panel.entries[i].name.to_lowercase().contains(&query))
            }
        };

        if let Some(pos) = pos {
            self.active_panel_mut().selected = pos;
        } else {
            self.status_message = "No match".into();
        }
    }

    fn search_prev(&mut self) {
        if self.search_query.is_empty() {
            self.status_message = "No search pattern \u{2014} use / to search".into();
            return;
        }
        let query = self.search_query.to_lowercase();

        let pos = {
            let panel = self.active_panel();
            let len = panel.entries.len();
            if len == 0 {
                None
            } else {
                let start = if panel.selected == 0 {
                    len - 1
                } else {
                    panel.selected - 1
                };
                (0..len)
                    .map(|i| (start + len - i) % len)
                    .find(|&i| panel.entries[i].name.to_lowercase().contains(&query))
            }
        };

        if let Some(pos) = pos {
            self.active_panel_mut().selected = pos;
        } else {
            self.status_message = "No match".into();
        }
    }

    // --- Visual marks ---

    fn toggle_visual_mark(&mut self) {
        let Some(entry) = self.active_panel().selected_entry() else {
            return;
        };
        if entry.name == ".." {
            return;
        }
        let path = entry.path.clone();
        let name = entry.name.clone();
        if self.visual_marks.remove(&path) {
            if let Some(ref db) = self.db {
                if let Err(e) = db.remove_visual_mark(&path) {
                    self.status_message = format!("Unmarked: {name} (db error: {e})");
                    return;
                }
            }
            self.status_message = format!("Unmarked: {name}");
        } else {
            if let Some(ref db) = self.db {
                if let Err(e) = db.add_visual_mark(&path) {
                    self.status_message = format!("Mark failed: {e}");
                    return;
                }
            }
            self.visual_marks.insert(path);
            self.status_message = format!("Marked: {name}");
        }
    }

    // --- Bookmarks ---

    fn set_mark(&mut self, c: char) {
        let path = self.active_panel().path.clone();
        self.marks.insert(c, path);
        self.status_message = format!("Mark '{c}' set");
    }

    fn goto_mark(&mut self, c: char) {
        if let Some(path) = self.marks.get(&c).cloned() {
            if path.is_dir() {
                let panel = self.active_panel_mut();
                panel.path = path;
                panel.selected = 0;
                panel.offset = 0;
                if let Err(e) = panel.load_dir() {
                    self.status_message = format!("Mark error: {e}");
                }
            } else {
                self.status_message = format!("Mark '{c}' directory no longer exists");
            }
        } else {
            self.status_message = format!("Mark '{c}' not set");
        }
    }

    // --- Sort ---

    fn set_sort(&mut self, mode: SortMode) {
        self.active_panel_mut().sort_mode = mode;
        let _ = self.active_panel_mut().load_dir();
        self.status_message = format!("Sort: {}", mode.label());
    }

    // --- Preview ---

    fn update_preview(&mut self) {
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

    // --- Find mode ---

    fn open_find(&mut self) {
        let base = self.active_panel().path.clone();
        self.find_state = Some(FindState::new(&base));
        self.mode = Mode::Find;
    }

    fn handle_find(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Navigation
        let nav_up = matches!(key.code, KeyCode::Up)
            || (ctrl && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('k')));
        let nav_down = matches!(key.code, KeyCode::Down)
            || (ctrl && matches!(key.code, KeyCode::Char('n') | KeyCode::Char('j')));

        if nav_up {
            if let Some(ref mut fs) = self.find_state {
                fs.move_up();
            }
            return;
        }
        if nav_down {
            if let Some(ref mut fs) = self.find_state {
                fs.move_down();
            }
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.find_state = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.accept_find();
            }
            KeyCode::Backspace => {
                if let Some(ref mut fs) = self.find_state {
                    fs.query.pop();
                    fs.update_filter();
                }
            }
            KeyCode::Char(c) if !ctrl => {
                if let Some(ref mut fs) = self.find_state {
                    fs.query.push(c);
                    fs.update_filter();
                }
            }
            _ => {}
        }
    }

    fn accept_find(&mut self) {
        let target = self
            .find_state
            .as_ref()
            .and_then(|fs| fs.selected_path())
            .map(|p| p.to_path_buf());

        self.find_state = None;
        self.mode = Mode::Normal;

        let Some(path) = target else { return };

        if path.is_dir() {
            let panel = self.active_panel_mut();
            panel.path = path;
            panel.selected = 0;
            panel.offset = 0;
            let _ = panel.load_dir();
        } else if let Some(parent) = path.parent() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            let panel = self.active_panel_mut();
            panel.path = parent.to_path_buf();
            panel.selected = 0;
            panel.offset = 0;
            let _ = panel.load_dir();
            if let Some(name) = name {
                panel.select_by_name(&name);
            }
        }
    }

    // --- Confirm mode ---

    fn handle_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.execute_delete();
                self.mode = Mode::Normal;
            }
            _ => {
                self.confirm_paths.clear();
                self.mode = Mode::Normal;
                self.status_message = "Cancelled".into();
            }
        }
    }

    // --- File operations ---

    fn yank_targeted(&mut self) {
        let paths = self.active_panel().targeted_paths();
        if paths.is_empty() {
            self.status_message = "Nothing to yank".into();
            return;
        }
        let n = paths.len();
        self.register = Some(Register {
            paths,
            op: RegisterOp::Yank,
        });
        self.status_message = format!("Yanked {n} item(s)");
    }

    fn request_delete(&mut self) {
        let paths = self.active_panel().targeted_paths();
        if paths.is_empty() {
            self.status_message = "Nothing to delete".into();
            return;
        }
        self.confirm_paths = paths;
        self.mode = Mode::Confirm;
    }

    fn execute_delete(&mut self) {
        let paths = std::mem::take(&mut self.confirm_paths);
        let mut records = Vec::new();
        for path in &paths {
            match ops::delete_path(path) {
                Ok(rec) => records.push(rec),
                Err(e) => {
                    self.status_message = format!("Delete error: {e}");
                    self.undo_stack.push(records);
                    self.refresh_panels();
                    return;
                }
            }
        }
        let n = records.len();
        self.undo_stack.push(records);
        self.status_message = format!("Deleted {n} item(s) \u{2014} undo with u");
        self.refresh_panels();
    }

    fn paste(&mut self, to_other_panel: bool) {
        if self.paste_progress.is_some() {
            self.status_message = "Operation in progress".into();
            return;
        }

        let (paths, op) = match &self.register {
            Some(r) => (r.paths.clone(), r.op),
            None => {
                self.status_message = "Register empty \u{2014} yy to yank, dd to cut".into();
                return;
            }
        };

        let dst_dir = if to_other_panel {
            self.inactive_panel_path()
        } else {
            self.active_panel().path.clone()
        };

        let phantoms: Vec<PhantomEntry> = paths
            .iter()
            .map(|p| PhantomEntry {
                name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                is_dir: p.is_dir(),
            })
            .collect();

        let (tx, rx) = mpsc::channel();
        ops::paste_in_background(paths, dst_dir.clone(), op, tx);

        let verb = if op == RegisterOp::Yank {
            "Copying"
        } else {
            "Moving"
        };
        self.status_message = format!("{verb}...");

        self.paste_progress = Some(PasteProgress {
            rx,
            op,
            started_at: Instant::now(),
            dst_dir,
            phantoms,
        });
    }

    pub fn poll_progress(&mut self) {
        let progress = match self.paste_progress.as_mut() {
            Some(p) => p,
            None => return,
        };

        // Drain all pending messages, keep the last Progress for display
        let mut last_progress = None;
        let mut finished = None;

        loop {
            match progress.rx.try_recv() {
                Ok(msg @ ProgressMsg::Progress { .. }) => {
                    last_progress = Some(msg);
                }
                Ok(msg @ ProgressMsg::Finished { .. }) => {
                    finished = Some(msg);
                    break;
                }
                Err(_) => break,
            }
        }

        // Update status from latest progress
        if let Some(ProgressMsg::Progress {
            bytes_done,
            bytes_total,
            item_index,
            item_total,
        }) = last_progress
        {
            let verb = if progress.op == RegisterOp::Yank {
                "Copying"
            } else {
                "Moving"
            };

            let pct = if bytes_total > 0 {
                (bytes_done as f64 / bytes_total as f64 * 100.0) as u8
            } else {
                0
            };

            let bar = progress_bar(pct, 20);

            let elapsed = progress.started_at.elapsed();
            let eta = if bytes_done > 0 && bytes_total > bytes_done {
                let rate = bytes_done as f64 / elapsed.as_secs_f64();
                let remaining_bytes = bytes_total - bytes_done;
                let eta_secs = remaining_bytes as f64 / rate;
                format!(
                    " ETA {}",
                    format_duration(std::time::Duration::from_secs_f64(eta_secs))
                )
            } else {
                String::new()
            };

            let size_text = format!(
                "{}/{}",
                format_bytes(bytes_done),
                format_bytes(bytes_total)
            );

            self.status_message = format!(
                "{verb} {bar} {pct}% ({size_text}){eta} [{}/{}]",
                item_index + 1,
                item_total,
            );
        }

        // Handle finish
        if let Some(ProgressMsg::Finished {
            records,
            error,
            bytes_total,
        }) = finished
        {
            let n = records.len();
            let op = progress.op;
            let elapsed = progress.started_at.elapsed();
            self.undo_stack.push(records);

            if let Some(err) = error {
                self.status_message = format!("Paste error: {err}");
            } else {
                let verb = if op == RegisterOp::Yank {
                    "Copied"
                } else {
                    "Moved"
                };
                let dur = format_duration(elapsed);
                let size = format_bytes(bytes_total);
                self.status_message =
                    format!("{verb} {n} item(s), {size} in {dur}");
            }

            if op == RegisterOp::Cut {
                self.register = None;
            }

            self.paste_progress = None;
            self.refresh_panels();
        }
    }

    fn undo(&mut self) {
        if let Some(records) = self.undo_stack.pop() {
            match ops::undo(&records) {
                Ok(msg) => self.status_message = msg,
                Err(e) => self.status_message = format!("Undo error: {e}"),
            }
            self.refresh_panels();
        } else {
            self.status_message = "Nothing to undo".into();
        }
    }

    fn refresh_panels(&mut self) {
        let tab = self.tab_mut();
        let _ = tab.left.load_dir();
        let _ = tab.right.load_dir();
    }

    // --- Command mode ---

    fn handle_command(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.execute_command();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command_input.clear();
            }
            KeyCode::Backspace => {
                if self.command_input.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.command_input.pop();
                }
            }
            KeyCode::Char(c) => {
                self.command_input.push(c);
            }
            _ => {}
        }
    }

    fn execute_command(&mut self) {
        let input = self.command_input.trim().to_string();
        self.command_input.clear();

        // Shell command: :!<cmd>
        if let Some(shell_cmd) = input.strip_prefix('!') {
            if shell_cmd.is_empty() {
                self.status_message = "Usage: :!<command>".into();
            } else {
                self.pending_shell = Some(shell_cmd.to_string());
            }
            return;
        }

        let (cmd, arg) = match input.split_once(' ') {
            Some((c, a)) => (c.trim(), Some(a.trim())),
            None => (input.as_str(), None),
        };

        match cmd {
            "q" | "quit" | "q!" => self.should_quit = true,

            "mkdir" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :mkdir <name>".into();
                        return;
                    }
                };
                let dir = self.active_panel().path.clone();
                match ops::mkdir(&dir, name) {
                    Ok(rec) => {
                        self.undo_stack.push(vec![rec]);
                        self.refresh_panels();
                        self.active_panel_mut().select_by_name(name);
                        self.status_message = format!("Created directory: {name}");
                    }
                    Err(e) => self.status_message = format!("mkdir: {e}"),
                }
            }

            "touch" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :touch <name>".into();
                        return;
                    }
                };
                let dir = self.active_panel().path.clone();
                match ops::touch(&dir, name) {
                    Ok(rec) => {
                        self.undo_stack.push(vec![rec]);
                        self.refresh_panels();
                        self.active_panel_mut().select_by_name(name);
                        self.status_message = format!("Created file: {name}");
                    }
                    Err(e) => self.status_message = format!("touch: {e}"),
                }
            }

            "rename" | "rn" => {
                let new_name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :rename <new_name>".into();
                        return;
                    }
                };
                let path = match self
                    .active_panel()
                    .selected_entry()
                    .filter(|e| e.name != "..")
                {
                    Some(e) => e.path.clone(),
                    None => {
                        self.status_message = "Nothing selected to rename".into();
                        return;
                    }
                };
                match ops::rename_path(&path, new_name) {
                    Ok(rec) => {
                        self.undo_stack.push(vec![rec]);
                        self.refresh_panels();
                        self.active_panel_mut().select_by_name(new_name);
                        self.status_message = format!("Renamed to: {new_name}");
                    }
                    Err(e) => self.status_message = format!("rename: {e}"),
                }
            }

            "cd" => {
                let path_str = match arg.filter(|a| !a.is_empty()) {
                    Some(p) => p,
                    None => {
                        self.status_message = "Usage: :cd <path>".into();
                        return;
                    }
                };
                let target = if path_str.starts_with('/') {
                    PathBuf::from(path_str)
                } else if path_str.starts_with('~') {
                    let home = std::env::var("HOME").unwrap_or_default();
                    PathBuf::from(path_str.replacen('~', &home, 1))
                } else {
                    self.active_panel().path.join(path_str)
                };
                if target.is_dir() {
                    let panel = self.active_panel_mut();
                    panel.path = target;
                    panel.selected = 0;
                    panel.offset = 0;
                    if let Err(e) = panel.load_dir() {
                        self.status_message = format!("cd: {e}");
                    }
                } else {
                    self.status_message = format!("Not a directory: {path_str}");
                }
            }

            "find" => {
                let base = self.active_panel().path.clone();
                let mut fs = FindState::new(&base);
                if let Some(pattern) = arg.filter(|a| !a.is_empty()) {
                    fs.query = pattern.to_string();
                    fs.update_filter();
                }
                self.find_state = Some(fs);
                self.mode = Mode::Find;
            }

            "sort" => {
                match arg.map(|a| a.to_lowercase()).as_deref() {
                    Some("name" | "n") => self.set_sort(SortMode::Name),
                    Some("size" | "s") => self.set_sort(SortMode::Size),
                    Some("date" | "d" | "time" | "t") => self.set_sort(SortMode::Date),
                    Some("ext" | "e" | "extension") => self.set_sort(SortMode::Extension),
                    _ => self.status_message = "Usage: :sort name|size|date|ext".into(),
                }
            }

            "hidden" => {
                let hidden = !self.active_panel().show_hidden;
                let tab = self.tab_mut();
                tab.left.show_hidden = hidden;
                tab.right.show_hidden = hidden;
                let _ = tab.left.load_dir();
                let _ = tab.right.load_dir();
                self.status_message = if hidden {
                    "Hidden files: shown".into()
                } else {
                    "Hidden files: hidden".into()
                };
            }

            "shell" | "sh" => {
                self.pending_shell = Some(String::new());
            }

            "tabnew" => self.new_tab(),
            "tabclose" | "tabc" => self.close_tab(),
            "tabnext" | "tabn" => self.next_tab(),
            "tabprev" | "tabp" | "tabN" => self.prev_tab(),

            "mark" => {
                let c = arg
                    .and_then(|a| a.chars().next())
                    .filter(|c| c.is_ascii_lowercase());
                match c {
                    Some(c) => self.set_mark(c),
                    None => self.status_message = "Usage: :mark <a-z>".into(),
                }
            }

            "marks" => {
                if self.marks.is_empty() {
                    self.status_message = "No marks set".into();
                } else {
                    let list: Vec<String> = self
                        .marks
                        .iter()
                        .map(|(k, v)| format!("'{k}={}", v.display()))
                        .collect();
                    self.status_message = list.join("  ");
                }
            }

            _ => {
                self.status_message = format!("Unknown command: :{cmd}");
            }
        }
    }
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

fn progress_bar(pct: u8, width: usize) -> String {
    let filled = (pct as usize * width / 100).min(width);
    let empty = width - filled;
    format!(
        "\u{2503}{}\u{2591}{}\u{2503}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty),
    )
}

fn format_bytes(b: u64) -> String {
    if b < 1024 {
        format!("{b}B")
    } else if b < 1024 * 1024 {
        format!("{:.1}K", b as f64 / 1024.0)
    } else if b < 1024 * 1024 * 1024 {
        format!("{:.1}M", b as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", b as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut child = if cfg!(target_os = "macos") {
        std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?
    } else {
        std::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()?
    };
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}
