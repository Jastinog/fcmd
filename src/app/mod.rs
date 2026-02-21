pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use std::path::PathBuf;
pub(crate) use std::time::Instant;

pub(crate) use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) use crate::find::{FindScope, FindState};
pub(crate) use crate::ops::{self, DuMsg, ProgressMsg, Register, RegisterOp, UndoStack};
pub(crate) use crate::panel::{self, DirCache, FileEntry, Panel, SortMode};
pub(crate) use crate::preview::Preview;
pub(crate) use crate::theme::Theme;

mod bookmarks;
pub(crate) mod chmod;
mod command;
mod dialogs;
mod file_ops;
mod find;
mod git;
mod info;
mod input;
mod marks;
pub mod messages;
mod navigation;
mod polling;
mod preview_mode;
mod rename;
mod search;
mod tree;
mod visual;

pub use messages::*;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    Normal,
    Visual,
    Select,
    Command,
    Confirm,
    Search,
    Find,
    Help,
    Rename,
    Create,
    Preview,
    PreviewSearch,
    ThemePicker,
    Bookmarks,
    BookmarkAdd,
    BookmarkRename,
    Chmod,
    Chown,
    Info,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum PanelLayout {
    Single,
    Dual,
    Triple,
}

impl PanelLayout {
    pub fn count(self) -> usize {
        match self {
            PanelLayout::Single => 1,
            PanelLayout::Dual => 2,
            PanelLayout::Triple => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PanelLayout::Single => "single",
            PanelLayout::Dual => "dual",
            PanelLayout::Triple => "triple",
        }
    }

    pub fn from_label(s: &str) -> Option<PanelLayout> {
        match s {
            "single" => Some(PanelLayout::Single),
            "dual" => Some(PanelLayout::Dual),
            "triple" => Some(PanelLayout::Triple),
            _ => None,
        }
    }
}

pub struct Tab {
    pub panels: Vec<Panel>,
    pub active: usize,
}

impl Tab {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        Ok(Tab {
            panels: vec![
                Panel::new(path.clone())?,
                Panel::new(path.clone())?,
                Panel::new(path)?,
            ],
            active: 0,
        })
    }

    pub fn active_panel(&self) -> &Panel {
        &self.panels[self.active]
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        &mut self.panels[self.active]
    }

    pub fn inactive_panel_path(&self, layout: PanelLayout) -> PathBuf {
        let next = (self.active + 1) % layout.count();
        self.panels[next].path.clone()
    }

    pub fn cycle_panel(&mut self, layout: PanelLayout) {
        let count = layout.count();
        self.active = (self.active + 1) % count;
    }

    pub fn clamp_active(&mut self, layout: PanelLayout) {
        if self.active >= layout.count() {
            self.active = 0;
        }
    }
}

pub struct App {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub mode: Mode,
    pub command_input: String,
    pub rename_input: String,
    pub should_quit: bool,
    pub open_editor: Option<PathBuf>,
    pub status_message: String,
    pub pending_key: Option<char>,
    pub pending_key_time: Option<Instant>,
    pub visible_height: usize,
    pub register: Option<Register>,
    pub undo_stack: UndoStack,
    pub confirm_paths: Vec<(PathBuf, bool)>, // (path, is_dir)
    pub confirm_scroll: usize,
    pub confirm_permanent: bool,
    // Search
    pub search_query: String,
    pub search_saved_cursor: usize,
    // Marks
    pub marks: HashMap<char, PathBuf>,
    // Find
    pub find_state: Option<FindState>,
    // Layout
    pub layout: PanelLayout,
    // Preview
    pub preview_mode: bool,
    pub preview: Option<Preview>,
    pub(super) preview_path: Option<PathBuf>,
    // File preview popup
    pub file_preview: Option<Preview>,
    pub file_preview_path: Option<PathBuf>,
    // Preview search
    pub preview_search_query: String,
    pub preview_search_matches: Vec<(usize, usize)>, // (line_idx, char_offset)
    pub preview_search_current: usize,
    // Tree
    pub show_tree: bool,
    pub tree_focused: bool,
    pub tree_selected: usize,
    pub tree_scroll: usize,
    pub start_dir: PathBuf,
    pub tree_data: Vec<crate::tree::TreeLine>,
    pub tree_collapsed: HashSet<PathBuf>,
    pub tree_expanded: HashSet<PathBuf>,
    // Visual marks (persistent colored dots, level 1-3)
    pub visual_marks: HashMap<PathBuf, u8>,
    // Database
    pub db: Option<crate::db::Db>,
    // Paste progress
    pub paste_progress: Option<PasteProgress>,
    // Directory sizes
    pub dir_sizes: HashMap<PathBuf, u64>,
    pub du_progress: Option<DuProgress>,
    pub(super) dir_sizes_loaded: HashSet<PathBuf>,
    // Tree cache invalidation
    pub tree_dirty: bool,
    pub tree_last_path: Option<PathBuf>,
    pub tree_last_hidden: bool,
    pub(super) tree_select_path: Option<PathBuf>,
    // Theme
    pub theme: Theme,
    pub theme_list: Vec<String>,
    pub theme_index: Option<usize>,
    pub theme_cursor: usize,
    pub theme_scroll: usize,
    pub theme_preview: Option<Theme>,
    // Per-directory sort preferences
    pub dir_sorts: HashMap<PathBuf, (SortMode, bool)>,
    // Bookmarks
    pub bookmarks: Vec<(String, PathBuf)>,
    pub bookmark_cursor: usize,
    pub bookmark_scroll: usize,
    pub bookmark_rename_old: Option<String>,
    pub bookmark_add_path: Option<PathBuf>,
    // Chmod/Chown
    pub chmod_paths: Vec<PathBuf>,
    pub chown_picker: Option<chmod::ChownPicker>,
    // Info popup
    pub info_lines: Vec<(String, String)>,
    pub info_scroll: usize,
    pub(super) info_du_rx: Option<tokio::sync::oneshot::Receiver<(u64, usize, usize)>>,
    // Git status (tracked for panels)
    pub git_statuses: HashMap<PathBuf, char>,
    pub(super) git_roots: [Option<PathBuf>; 3],
    pub(super) git_checked_dirs: [Option<PathBuf>; 3],
    pub(super) git_progress: Option<GitProgress>,
    // Delete progress
    pub delete_progress: Option<DeleteProgress>,
    // Directory cache (LRU)
    pub dir_cache: DirCache,
    // Async dir loading (streaming batches + sorted final result)
    pub(super) dir_load_tx: tokio::sync::mpsc::Sender<DirLoadMsg>,
    pub dir_load_rx: tokio::sync::mpsc::Receiver<DirLoadMsg>,
    // Async preview loading
    pub preview_load_rx: Option<tokio::sync::oneshot::Receiver<PreviewLoadResult>>,
    pub file_preview_rx: Option<tokio::sync::oneshot::Receiver<PreviewLoadResult>>,
    // Async tree loading
    pub tree_load_rx: Option<tokio::sync::oneshot::Receiver<TreeLoadResult>>,
    // Async info metadata loading
    pub info_load_rx: Option<tokio::sync::oneshot::Receiver<Vec<(String, String)>>>,
    // Async chown picker loading
    pub chown_load_rx: Option<tokio::sync::oneshot::Receiver<ChownLoadResult>>,
    // Async navigation validation
    pub nav_check_rx: Option<tokio::sync::oneshot::Receiver<NavCheckResult>>,
    // Async file operations (mkdir, touch, rename, chmod, chown, undo)
    pub file_op_rx: Option<tokio::sync::oneshot::Receiver<FileOpResult>>,
    // Async theme loading (for theme picker preview)
    pub theme_load_rx: Option<tokio::sync::oneshot::Receiver<Option<Theme>>>,
}

impl App {
    pub fn new() -> std::io::Result<Self> {
        let cwd = std::env::current_dir()?;

        let (db, visual_marks, dir_sorts, bookmarks) = match crate::db::Db::init() {
            Ok(db) => {
                let marks = db.load_visual_marks().unwrap_or_default();
                let raw_sorts = db.load_dir_sorts().unwrap_or_default();
                let dir_sorts: HashMap<PathBuf, (SortMode, bool)> = raw_sorts
                    .into_iter()
                    .filter_map(|(p, (label, rev))| {
                        SortMode::from_label(&label).map(|m| (p, (m, rev)))
                    })
                    .collect();
                let bookmarks = db.load_bookmarks().unwrap_or_default();
                (Some(db), marks, dir_sorts, bookmarks)
            }
            Err(e) => {
                eprintln!("Warning: DB init failed: {e}");
                (None, HashMap::new(), HashMap::new(), Vec::new())
            }
        };

        // Restore session from DB
        let (tabs, active_tab, saved_layout) = if let Some(ref db) = db {
            let layout = db.load_layout();
            match db.load_session() {
                Ok((saved, at)) if !saved.is_empty() => {
                    let mut tabs = Vec::new();
                    for st in &saved {
                        let paths: Vec<PathBuf> = st.panel_paths.iter().map(|p| {
                            if p.is_dir() { p.clone() } else { cwd.clone() }
                        }).collect();
                        let mut tab = Tab {
                            panels: vec![
                                Panel::new(paths.first().cloned().unwrap_or_else(|| cwd.clone()))?,
                                Panel::new(paths.get(1).cloned().unwrap_or_else(|| cwd.clone()))?,
                                Panel::new(paths.get(2).cloned().unwrap_or_else(|| cwd.clone()))?,
                            ],
                            active: st.active_panel.min(2),
                        };
                        for (i, panel) in tab.panels.iter_mut().enumerate() {
                            let _ = panel.load_dir();
                            let cursor = st.panel_cursors.get(i).copied().unwrap_or(0);
                            panel.selected = cursor.min(panel.entries.len().saturating_sub(1));
                        }
                        tabs.push(tab);
                    }
                    let at = at.min(tabs.len().saturating_sub(1));
                    (tabs, at, layout)
                }
                _ => (vec![Tab::new(cwd.clone())?], 0, layout),
            }
        } else {
            (vec![Tab::new(cwd.clone())?], 0, None)
        };
        let layout = saved_layout
            .and_then(|s| PanelLayout::from_label(&s))
            .unwrap_or(PanelLayout::Dual);

        Theme::ensure_builtin_themes();
        let saved_theme_name = db.as_ref().and_then(|d| d.load_theme());
        let theme = match saved_theme_name.as_deref().and_then(Theme::load_by_name) {
            Some(t) => t,
            None => Theme::from_config(),
        };
        let theme_index = saved_theme_name.and_then(|name| {
            let list = Theme::list_available();
            list.iter().position(|n| n == &name)
        });

        let (dir_load_tx, dir_load_rx) = tokio::sync::mpsc::channel(64);

        let mut app = App {
            tabs,
            active_tab,
            mode: Mode::Normal,
            command_input: String::new(),
            rename_input: String::new(),
            should_quit: false,
            open_editor: None,
            status_message: String::new(),
            pending_key: None,
            pending_key_time: None,
            visible_height: 20,
            register: None,
            undo_stack: UndoStack::new(),
            confirm_paths: Vec::new(),
            confirm_scroll: 0,
            confirm_permanent: false,
            search_query: String::new(),
            search_saved_cursor: 0,
            marks: HashMap::new(),
            find_state: None,
            layout,
            preview_mode: false,
            preview: None,
            preview_path: None,
            file_preview: None,
            file_preview_path: None,
            preview_search_query: String::new(),
            preview_search_matches: Vec::new(),
            preview_search_current: 0,
            show_tree: false,
            tree_focused: false,
            tree_selected: 0,
            tree_scroll: 0,
            start_dir: cwd,
            tree_data: Vec::new(),
            tree_collapsed: HashSet::new(),
            tree_expanded: HashSet::new(),
            visual_marks,
            dir_sorts,
            db,
            paste_progress: None,
            dir_sizes: HashMap::new(),
            du_progress: None,
            dir_sizes_loaded: HashSet::new(),
            tree_dirty: true,
            tree_last_path: None,
            tree_last_hidden: false,
            tree_select_path: None,
            theme,
            theme_list: Vec::new(),
            theme_index,
            theme_cursor: 0,
            theme_scroll: 0,
            theme_preview: None,
            bookmarks,
            bookmark_cursor: 0,
            bookmark_scroll: 0,
            bookmark_rename_old: None,
            bookmark_add_path: None,
            chmod_paths: Vec::new(),
            chown_picker: None,
            info_lines: Vec::new(),
            info_scroll: 0,
            info_du_rx: None,
            git_statuses: HashMap::new(),
            git_roots: [None, None, None],
            git_checked_dirs: [None, None, None],
            git_progress: None,
            delete_progress: None,
            dir_cache: DirCache::new(64),
            dir_load_tx,
            dir_load_rx,
            preview_load_rx: None,
            file_preview_rx: None,
            tree_load_rx: None,
            info_load_rx: None,
            chown_load_rx: None,
            nav_check_rx: None,
            file_op_rx: None,
            theme_load_rx: None,
        };
        app.refresh_git_status();
        // Apply saved sort preferences to restored panels
        for tab in &mut app.tabs {
            for panel in tab.panels.iter_mut() {
                if let Some(&(mode, rev)) = app.dir_sorts.get(&panel.path)
                    && (panel.sort_mode != mode || panel.sort_reverse != rev)
                {
                    panel.sort_mode = mode;
                    panel.sort_reverse = rev;
                    let _ = panel.load_dir();
                }
            }
        }
        Ok(app)
    }

    pub fn save_session(&self) {
        let Some(ref db) = self.db else { return };
        let tabs: Vec<crate::db::SavedTab> = self
            .tabs
            .iter()
            .map(|t| crate::db::SavedTab {
                panel_paths: t.panels.iter().map(|p| p.path.clone()).collect(),
                panel_cursors: t.panels.iter().map(|p| p.selected).collect(),
                active_panel: t.active,
            })
            .collect();
        if let Err(e) = db.save_session(&tabs, self.active_tab) {
            eprintln!("Warning: failed to save session: {e}");
        }
        if let Err(e) = db.save_layout(self.layout.label()) {
            eprintln!("Warning: failed to save layout: {e}");
        }
    }

    /// Spawn an async directory load for the given panel index.
    /// Entries stream in batches, then a sorted final result is sent.
    /// `select_name` optionally re-selects an entry by name after loading.
    pub fn spawn_dir_load(&mut self, panel_idx: usize, select_name: Option<String>) {
        let tab = &mut self.tabs[self.active_tab];
        let panel = &mut tab.panels[panel_idx];
        let path = panel.path.clone();
        let show_hidden = panel.show_hidden;
        let sort_mode = panel.sort_mode;
        let sort_reverse = panel.sort_reverse;
        let dir_sizes = self.dir_sizes.clone();
        let tab_index = self.active_tab;

        let tx = self.dir_load_tx.clone();

        tokio::task::spawn_blocking(move || {
            panel::stream_dir_entries(
                panel::DirLoadRequest {
                    path,
                    show_hidden,
                    sort_mode,
                    sort_reverse,
                    dir_sizes,
                    panel_idx,
                    tab_index,
                    select_name,
                },
                &tx,
            );
        });
    }

    /// Handle a message from the streaming directory loader.
    pub fn handle_dir_load_msg(&mut self, msg: DirLoadMsg) {
        match msg {
            DirLoadMsg::Batch {
                panel_idx,
                tab_index,
                path,
                entries,
            } => {
                if tab_index >= self.tabs.len() {
                    return;
                }
                let tab = &mut self.tabs[tab_index];
                if panel_idx >= tab.panels.len() {
                    return;
                }
                let panel = &mut tab.panels[panel_idx];
                if panel.path != path {
                    return;
                }
                panel.append_entries(entries);
            }
            DirLoadMsg::Finished {
                panel_idx,
                tab_index,
                path,
                entries,
                select_name,
            } => {
                if tab_index >= self.tabs.len() {
                    return;
                }
                let tab = &mut self.tabs[tab_index];
                if panel_idx >= tab.panels.len() {
                    return;
                }
                let panel = &mut tab.panels[panel_idx];
                if panel.path != path {
                    return;
                }
                self.dir_cache.insert(
                    path,
                    panel::DirCacheEntry {
                        entries: entries.clone(),
                        sort_mode: panel.sort_mode,
                        sort_reverse: panel.sort_reverse,
                        show_hidden: panel.show_hidden,
                    },
                );
                panel.apply_entries(entries, select_name.as_deref());
            }
        }
    }

    /// Apply preview loaded asynchronously (side panel preview).
    pub fn apply_preview_load(&mut self, result: PreviewLoadResult) {
        // Only apply if still looking at the same path
        if self.preview_path.as_ref() == Some(&result.path) {
            self.preview = Some(result.preview);
        }
    }

    /// Apply file preview loaded asynchronously (popup preview).
    pub fn apply_file_preview_load(&mut self, result: PreviewLoadResult) {
        if self.file_preview_path.as_ref() == Some(&result.path) {
            self.file_preview = Some(result.preview);
        }
    }

    /// Handle async navigation validation result.
    pub fn apply_nav_check(&mut self, result: NavCheckResult) {
        if !result.exists {
            let label = match result.source {
                NavSource::Cd => "Not a directory",
                NavSource::Bookmark => "Bookmark directory no longer exists",
                NavSource::Mark(c) => {
                    self.status_message = format!("Mark '{c}' directory no longer exists");
                    return;
                }
            };
            self.status_message = label.into();
            return;
        }
        let side = self.tab().active;
        if result.is_dir {
            self.navigate_cached(result.path, side, None);
        } else {
            // File: navigate to parent and select the file
            if let Some(parent) = result.path.parent() {
                let name = result
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
                self.navigate_cached(parent.to_path_buf(), side, name);
            }
        }
    }

    /// Handle async file operation result.
    pub fn apply_file_op(&mut self, result: FileOpResult) {
        match result {
            FileOpResult::Mkdir { name, result } => match result {
                Ok(rec) => {
                    self.undo_stack.push(vec![rec]);
                    self.refresh_panels();
                    self.active_panel_mut().select_by_name(&name);
                    self.status_message = format!("Created directory: {name}");
                }
                Err(e) => self.status_message = format!("mkdir: {e}"),
            },
            FileOpResult::Touch { name, result } => match result {
                Ok(rec) => {
                    self.undo_stack.push(vec![rec]);
                    self.refresh_panels();
                    self.active_panel_mut().select_by_name(&name);
                    self.status_message = format!("Created file: {name}");
                }
                Err(e) => self.status_message = format!("touch: {e}"),
            },
            FileOpResult::Rename { new_name, result } => match result {
                Ok(rec) => {
                    self.undo_stack.push(vec![rec]);
                    self.refresh_panels();
                    self.active_panel_mut().select_by_name(&new_name);
                    self.status_message = format!("Renamed to: {new_name}");
                }
                Err(e) => self.status_message = format!("rename: {e}"),
            },
            FileOpResult::Chmod {
                input,
                count,
                errors,
                last_error,
            } => {
                if errors > 0 {
                    if let Some(e) = last_error {
                        self.status_message = format!("chmod: {e}");
                    }
                } else {
                    self.status_message = format!("chmod {input} ({count} item(s))");
                }
                self.refresh_panels();
            }
            FileOpResult::Chown {
                user_name,
                group_name,
                count,
                errors,
                last_error,
            } => {
                if errors > 0 {
                    if let Some(e) = last_error {
                        self.status_message = format!("chown: {e}");
                    }
                } else {
                    self.status_message =
                        format!("chown {user_name}:{group_name} ({count} item(s))");
                }
                self.refresh_panels();
            }
            FileOpResult::Undo { result } => {
                match result {
                    Ok(msg) => self.status_message = msg,
                    Err(e) => self.status_message = format!("Undo error: {e}"),
                }
                self.refresh_panels();
            }
            FileOpResult::ChmodPrefill { prefill, paths } => {
                if self.mode == Mode::Normal || self.mode == Mode::Visual || self.mode == Mode::Select {
                    self.rename_input = prefill;
                    self.chmod_paths = paths;
                    self.mode = Mode::Chmod;
                }
            }
            FileOpResult::ThemeLoad { name, theme, theme_list } => match theme {
                Some(t) => {
                    self.theme = t;
                    self.theme_list = theme_list;
                    self.theme_index = self.theme_list.iter().position(|n| n == &name);
                    if let Some(ref db) = self.db {
                        let _ = db.save_theme(&name);
                    }
                    self.status_message = format!("Theme: {name}");
                }
                None => self.status_message = format!("Theme not found: {name}"),
            },
            FileOpResult::ThemeList { themes } => {
                if self.mode == Mode::ThemePicker {
                    // Populating theme picker after async list load
                    if themes.is_empty() {
                        self.status_message = "No themes found".into();
                        self.mode = Mode::Normal;
                    } else {
                        self.theme_list = themes;
                        self.theme_cursor = self.theme_index.unwrap_or(0).min(self.theme_list.len() - 1);
                        self.theme_scroll = self.theme_cursor.saturating_sub(5);
                        self.spawn_theme_load();
                    }
                } else {
                    // :theme command with no argument — show list in status
                    if themes.is_empty() {
                        self.status_message = "No themes found".into();
                    } else {
                        self.status_message = themes.join(", ");
                    }
                }
            },
            FileOpResult::Clipboard { label, ok } => {
                self.status_message = if ok {
                    label
                } else {
                    "Clipboard not available".into()
                };
            }
        }
    }

    /// Apply async theme preview load (for theme picker).
    pub fn apply_theme_preview(&mut self, theme: Option<Theme>) {
        if self.mode == Mode::ThemePicker {
            self.theme_preview = theme;
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
        self.tab().inactive_panel_path(self.layout)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.status_message.clear();

        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Visual => self.handle_visual(key),
            Mode::Select => self.handle_select(key),
            Mode::Command => self.handle_command(key),
            Mode::Confirm => self.handle_confirm(key),
            Mode::Search => self.handle_search(key),
            Mode::Find => self.handle_find(key),
            Mode::Help => self.handle_help(key),
            Mode::Rename => self.handle_rename(key),
            Mode::Create => self.handle_create(key),
            Mode::Preview => self.handle_preview(key),
            Mode::PreviewSearch => self.handle_preview_search(key),
            Mode::ThemePicker => self.handle_theme_picker(key),
            Mode::Bookmarks => self.handle_bookmarks(key),
            Mode::BookmarkAdd => self.handle_bookmark_add(key),
            Mode::BookmarkRename => self.handle_bookmark_rename(key),
            Mode::Chmod => self.handle_chmod(key),
            Mode::Chown => self.handle_chown(key),
            Mode::Info => self.handle_info(key),
        }

        self.update_preview();
        self.ensure_git_status();
    }
}
