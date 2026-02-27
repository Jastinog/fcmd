pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use std::path::PathBuf;
pub(crate) use std::time::Instant;

pub(crate) use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) use crate::find::{FindScope, FindState};
pub(crate) use crate::ops::{self, DuMsg, Register, RegisterOp, UndoStack};
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
pub(crate) mod task_manager;
mod search;
mod select_pattern;
mod tree;
mod visual;

pub use messages::*;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
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
    SelectPattern,
    UnselectPattern,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
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
    pub fn new(path: PathBuf) -> Self {
        Tab {
            panels: vec![
                Panel::new(path.clone()),
                Panel::new(path.clone()),
                Panel::new(path),
            ],
            active: 0,
        }
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
    pub db: Option<std::sync::Arc<std::sync::Mutex<crate::db::Db>>>,
    // Task manager (copy/move/delete operations)
    pub task_manager: task_manager::TaskManager,
    pub task_notification: Option<String>,
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
    pub transparent: bool,
    pub theme: Theme,
    pub theme_dark_list: Vec<String>,
    pub theme_light_list: Vec<String>,
    pub theme_col: usize,
    pub theme_cursors: [usize; 2],
    pub theme_scrolls: [usize; 2],
    pub theme_active_name: Option<String>,
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
    // Async dir sizes loading from DB
    pub(super) dir_sizes_load_rx: Option<tokio::sync::oneshot::Receiver<Vec<(PathBuf, HashMap<PathBuf, u64>)>>>,
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

        let (db, visual_marks, dir_sorts, bookmarks, git_statuses) = match crate::db::Db::init() {
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
                let git_statuses = db.load_git_statuses().unwrap_or_default();
                (Some(db), marks, dir_sorts, bookmarks, git_statuses)
            }
            Err(e) => {
                eprintln!("Warning: DB init failed: {e}");
                (None, HashMap::new(), HashMap::new(), Vec::new(), HashMap::new())
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
                        let tab = Tab {
                            panels: vec![
                                Panel::new(paths.first().cloned().unwrap_or_else(|| cwd.clone())),
                                Panel::new(paths.get(1).cloned().unwrap_or_else(|| cwd.clone())),
                                Panel::new(paths.get(2).cloned().unwrap_or_else(|| cwd.clone())),
                            ],
                            active: st.active_panel.min(2),
                        };
                        tabs.push(tab);
                    }
                    let at = at.min(tabs.len().saturating_sub(1));
                    (tabs, at, layout)
                }
                _ => (vec![Tab::new(cwd.clone())], 0, layout),
            }
        } else {
            (vec![Tab::new(cwd.clone())], 0, None)
        };
        let layout = saved_layout
            .and_then(|s| PanelLayout::from_label(&s))
            .unwrap_or(PanelLayout::Dual);

        tokio::task::spawn_blocking(Theme::ensure_builtin_themes);
        let transparent = db.as_ref().is_some_and(|d| d.load_transparent());
        let saved_theme_name = db.as_ref().and_then(|d| d.load_theme());
        let theme = match saved_theme_name.as_deref().and_then(Theme::load_by_name) {
            Some(t) => t,
            None => Theme::from_config(),
        };
        let theme_active_name = saved_theme_name;

        let db = db.map(|d| std::sync::Arc::new(std::sync::Mutex::new(d)));

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
            task_manager: task_manager::TaskManager::new(),
            task_notification: None,
            dir_sizes: HashMap::new(),
            du_progress: None,
            dir_sizes_loaded: HashSet::new(),
            tree_dirty: true,
            tree_last_path: None,
            tree_last_hidden: false,
            tree_select_path: None,
            transparent,
            theme,
            theme_dark_list: Vec::new(),
            theme_light_list: Vec::new(),
            theme_col: 0,
            theme_cursors: [0; 2],
            theme_scrolls: [0; 2],
            theme_active_name,
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
            git_statuses,
            git_roots: [None, None, None],
            git_checked_dirs: [None, None, None],
            git_progress: None,
            dir_cache: DirCache::new(64),
            dir_load_tx,
            dir_load_rx,
            preview_load_rx: None,
            file_preview_rx: None,
            tree_load_rx: None,
            info_load_rx: None,
            chown_load_rx: None,
            dir_sizes_load_rx: None,
            nav_check_rx: None,
            file_op_rx: None,
            theme_load_rx: None,
        };
        app.refresh_git_status();
        app.apply_transparency();
        // Apply saved sort preferences to panels before spawning async loads
        for tab in &mut app.tabs {
            for panel in tab.panels.iter_mut() {
                if let Some(&(mode, rev)) = app.dir_sorts.get(&panel.path) {
                    panel.sort_mode = mode;
                    panel.sort_reverse = rev;
                }
            }
        }
        // Spawn async directory loads for all panels in all tabs
        let saved_active_tab = app.active_tab;
        for tab_idx in 0..app.tabs.len() {
            app.active_tab = tab_idx;
            for panel_idx in 0..3 {
                app.spawn_dir_load(panel_idx, None);
            }
        }
        app.active_tab = saved_active_tab;
        Ok(app)
    }

    /// Save session and layout to DB. Synchronous — called on shutdown
    /// where fire-and-forget could lose data if the runtime exits first.
    pub fn save_session(&self) {
        let Some(ref db) = self.db else { return };
        let Ok(db) = db.lock() else { return };
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

    /// Fire-and-forget DB write on the blocking thread pool.
    pub(super) fn db_spawn(&self, f: impl FnOnce(&crate::db::Db) + Send + 'static) {
        if let Some(ref db) = self.db {
            let db = std::sync::Arc::clone(db);
            tokio::task::spawn_blocking(move || {
                if let Ok(db) = db.lock() {
                    f(&db);
                }
            });
        }
    }

    pub fn apply_transparency(&mut self) {
        if self.transparent {
            self.theme.bg = ratatui::style::Color::Reset;
            self.theme.status_bg = ratatui::style::Color::Reset;
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
        let dir_sizes = std::sync::Arc::new(self.dir_sizes.clone());
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
                panel.loading = false;
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
                    self.refresh_panels_select(Some(name.clone()));
                    self.status_message = format!("Created directory: {name}");
                }
                Err(e) => self.status_message = format!("mkdir: {e}"),
            },
            FileOpResult::Touch { name, result } => match result {
                Ok(rec) => {
                    self.undo_stack.push(vec![rec]);
                    self.refresh_panels_select(Some(name.clone()));
                    self.status_message = format!("Created file: {name}");
                }
                Err(e) => self.status_message = format!("touch: {e}"),
            },
            FileOpResult::Rename { new_name, result } => match result {
                Ok(rec) => {
                    self.undo_stack.push(vec![rec]);
                    self.refresh_panels_select(Some(new_name.clone()));
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
            FileOpResult::ThemeLoad { name, theme, dark_list, light_list } => match theme {
                Some(t) => {
                    self.theme = t;
                    self.apply_transparency();
                    self.theme_dark_list = dark_list;
                    self.theme_light_list = light_list;
                    self.theme_active_name = Some(name.clone());
                    let n = name.clone();
                    self.db_spawn(move |db| { let _ = db.save_theme(&n); });
                    self.status_message = format!("Theme: {name}");
                }
                None => self.status_message = format!("Theme not found: {name}"),
            },
            FileOpResult::ThemeList { dark, light } => {
                if self.mode == Mode::ThemePicker {
                    if dark.is_empty() && light.is_empty() {
                        self.status_message = "No themes found".into();
                        self.mode = Mode::Normal;
                    } else {
                        self.theme_dark_list = dark;
                        self.theme_light_list = light;
                        self.position_theme_cursors();
                        self.spawn_theme_load();
                    }
                } else {
                    let mut all: Vec<String> = dark.into_iter().chain(light).collect();
                    all.sort();
                    if all.is_empty() {
                        self.status_message = "No themes found".into();
                    } else {
                        self.status_message = all.join(", ");
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
    pub fn apply_theme_preview(&mut self, mut theme: Option<Theme>) {
        if self.mode == Mode::ThemePicker {
            if self.transparent {
                if let Some(ref mut t) = theme {
                    t.bg = ratatui::style::Color::Reset;
                    t.status_bg = ratatui::style::Color::Reset;
                }
            }
            self.theme_preview = theme;
        }
    }

    pub fn phantoms_for(&self, dir: &std::path::Path) -> Vec<&PhantomEntry> {
        self.task_manager.phantoms_for(dir)
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
        self.task_notification = None;

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
            Mode::SelectPattern => self.handle_select_pattern(key),
            Mode::UnselectPattern => self.handle_unselect_pattern(key),
        }

        self.update_preview();
        self.ensure_git_status();
    }
}

#[cfg(test)]
pub(crate) fn make_test_entries(names: &[&str]) -> Vec<FileEntry> {
    let mut entries = vec![FileEntry {
        name: "..".into(),
        path: PathBuf::from("/"),
        is_dir: true,
        size: 0,
        modified: None,
        created: None,
        is_symlink: false,
    }];
    for &name in names {
        let is_dir = name.ends_with('/');
        let clean = name.trim_end_matches('/');
        entries.push(FileEntry {
            name: clean.to_string(),
            path: PathBuf::from(format!("/test/{clean}")),
            is_dir,
            size: 0,
            modified: None,
            created: None,
            is_symlink: false,
        });
    }
    entries
}

#[cfg(test)]
impl App {
    pub(crate) fn new_for_test(entries: Vec<FileEntry>) -> Self {
        let db = crate::db::Db::init_in_memory().unwrap();
        let db = Some(std::sync::Arc::new(std::sync::Mutex::new(db)));

        let (dir_load_tx, dir_load_rx) = tokio::sync::mpsc::channel(64);

        let mut panel = Panel::new(PathBuf::from("/test"));
        panel.entries = entries;
        panel.loading = false;

        let tab = Tab {
            panels: vec![
                panel,
                Panel::new(PathBuf::from("/test")),
                Panel::new(PathBuf::from("/test")),
            ],
            active: 0,
        };

        App {
            tabs: vec![tab],
            active_tab: 0,
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
            layout: PanelLayout::Dual,
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
            start_dir: PathBuf::from("/test"),
            tree_data: Vec::new(),
            tree_collapsed: HashSet::new(),
            tree_expanded: HashSet::new(),
            visual_marks: HashMap::new(),
            dir_sorts: HashMap::new(),
            db,
            task_manager: task_manager::TaskManager::new(),
            task_notification: None,
            dir_sizes: HashMap::new(),
            du_progress: None,
            dir_sizes_loaded: HashSet::new(),
            tree_dirty: false,
            tree_last_path: None,
            tree_last_hidden: false,
            tree_select_path: None,
            transparent: false,
            theme: Theme::default_theme(),
            theme_dark_list: Vec::new(),
            theme_light_list: Vec::new(),
            theme_col: 0,
            theme_cursors: [0; 2],
            theme_scrolls: [0; 2],
            theme_active_name: None,
            theme_preview: None,
            bookmarks: Vec::new(),
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
            dir_cache: DirCache::new(64),
            dir_load_tx,
            dir_load_rx,
            preview_load_rx: None,
            file_preview_rx: None,
            tree_load_rx: None,
            info_load_rx: None,
            chown_load_rx: None,
            dir_sizes_load_rx: None,
            nav_check_rx: None,
            file_op_rx: None,
            theme_load_rx: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Navigation tests ──────────────────────────────────────────

    #[tokio::test]
    async fn focus_next_cycles_panels() {
        let entries = make_test_entries(&["a", "b"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Triple;
        assert_eq!(app.tab().active, 0);
        app.focus_next();
        assert_eq!(app.tab().active, 1);
        app.focus_next();
        assert_eq!(app.tab().active, 2);
        // At max, stays at max
        app.focus_next();
        assert_eq!(app.tab().active, 2);
    }

    #[tokio::test]
    async fn focus_prev_cycles_panels() {
        let entries = make_test_entries(&["a", "b"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Triple;
        app.tab_mut().active = 2;
        app.focus_prev();
        assert_eq!(app.tab().active, 1);
        app.focus_prev();
        assert_eq!(app.tab().active, 0);
        // At 0 without tree, stays at 0
        app.focus_prev();
        assert_eq!(app.tab().active, 0);
    }

    #[tokio::test]
    async fn focus_prev_enters_tree() {
        let entries = make_test_entries(&["a"]);
        let mut app = App::new_for_test(entries);
        app.show_tree = true;
        app.tree_focused = false;
        app.tab_mut().active = 0;
        app.focus_prev();
        assert!(app.tree_focused);
    }

    #[tokio::test]
    async fn focus_next_exits_tree() {
        let entries = make_test_entries(&["a"]);
        let mut app = App::new_for_test(entries);
        app.show_tree = true;
        app.tree_focused = true;
        app.focus_next();
        assert!(!app.tree_focused);
    }

    #[tokio::test]
    async fn next_tab_wraps() {
        let entries = make_test_entries(&["a"]);
        let mut app = App::new_for_test(entries);
        // Add a second tab
        app.tabs.push(Tab::new(PathBuf::from("/test2")));
        assert_eq!(app.active_tab, 0);
        app.next_tab();
        assert_eq!(app.active_tab, 1);
        app.next_tab();
        assert_eq!(app.active_tab, 0); // wraps
    }

    #[tokio::test]
    async fn prev_tab_wraps() {
        let entries = make_test_entries(&["a"]);
        let mut app = App::new_for_test(entries);
        app.tabs.push(Tab::new(PathBuf::from("/test2")));
        assert_eq!(app.active_tab, 0);
        app.prev_tab();
        assert_eq!(app.active_tab, 1); // wraps to end
        app.prev_tab();
        assert_eq!(app.active_tab, 0);
    }

    #[tokio::test]
    async fn tab_switch_exits_visual() {
        let entries = make_test_entries(&["a", "b"]);
        let mut app = App::new_for_test(entries);
        app.tabs.push(Tab::new(PathBuf::from("/test2")));
        app.mode = Mode::Visual;
        app.active_panel_mut().visual_anchor = Some(0);
        app.next_tab();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn set_layout_clamps_active() {
        let entries = make_test_entries(&["a"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Triple;
        app.tab_mut().active = 2;
        app.set_layout(PanelLayout::Single);
        assert_eq!(app.layout, PanelLayout::Single);
        assert_eq!(app.tab().active, 0);
    }

    #[tokio::test]
    async fn single_tab_no_switch() {
        let entries = make_test_entries(&["a"]);
        let mut app = App::new_for_test(entries);
        // Only 1 tab — next_tab should be a no-op
        app.next_tab();
        assert_eq!(app.active_tab, 0);
        app.prev_tab();
        assert_eq!(app.active_tab, 0);
    }

    // ── Marks tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn toggle_visual_mark_cycles() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "a.txt"
        let path = app.active_panel().entries[1].path.clone();

        app.toggle_visual_mark();
        assert_eq!(app.visual_marks.get(&path), Some(&1));
        app.toggle_visual_mark();
        assert_eq!(app.visual_marks.get(&path), Some(&2));
        app.toggle_visual_mark();
        assert_eq!(app.visual_marks.get(&path), Some(&3));
        app.toggle_visual_mark();
        assert_eq!(app.visual_marks.get(&path), None); // back to 0
    }

    #[tokio::test]
    async fn toggle_visual_mark_skips_dotdot() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".."
        app.toggle_visual_mark();
        assert!(app.visual_marks.is_empty());
    }

    #[tokio::test]
    async fn jump_next_visual_mark_finds_next() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        let path_c = app.active_panel().entries[3].path.clone(); // "c.txt"
        app.visual_marks.insert(path_c, 1);
        app.active_panel_mut().selected = 0;
        app.jump_next_visual_mark();
        assert_eq!(app.active_panel().selected, 3);
    }

    #[tokio::test]
    async fn jump_next_visual_mark_wraps() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        let path_a = app.active_panel().entries[1].path.clone(); // "a.txt"
        app.visual_marks.insert(path_a, 1);
        app.active_panel_mut().selected = 2; // past a.txt
        app.jump_next_visual_mark();
        assert_eq!(app.active_panel().selected, 1); // wrapped
    }

    #[tokio::test]
    async fn jump_next_visual_mark_empty() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.jump_next_visual_mark();
        assert_eq!(app.status_message, "No marks");
    }

    #[tokio::test]
    async fn select_all_marks_everything() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        assert_eq!(app.active_panel().marked.len(), 3); // excludes ".."
    }

    #[tokio::test]
    async fn unselect_all_clears() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        assert!(!app.active_panel().marked.is_empty());
        app.unselect_all();
        assert!(app.active_panel().marked.is_empty());
    }

    #[tokio::test]
    async fn set_mark_stores_path() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.set_mark('a');
        assert_eq!(app.marks.get(&'a'), Some(&PathBuf::from("/test")));
    }

    #[tokio::test]
    async fn set_mark_status_message() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.set_mark('z');
        assert_eq!(app.status_message, "Mark 'z' set");
    }

    // ── Bookmarks tests ──────────────────────────────────────────

    #[tokio::test]
    async fn add_bookmark_sorted_insert() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("zebra", PathBuf::from("/z"));
        app.add_bookmark("alpha", PathBuf::from("/a"));
        assert_eq!(app.bookmarks[0].0, "alpha");
        assert_eq!(app.bookmarks[1].0, "zebra");
    }

    #[tokio::test]
    async fn add_bookmark_update_existing() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("test", PathBuf::from("/old"));
        app.add_bookmark("test", PathBuf::from("/new"));
        assert_eq!(app.bookmarks.len(), 1);
        assert_eq!(app.bookmarks[0].1, PathBuf::from("/new"));
    }

    #[tokio::test]
    async fn remove_bookmark_by_name_removes() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("test", PathBuf::from("/t"));
        app.remove_bookmark_by_name("test");
        assert!(app.bookmarks.is_empty());
    }

    #[tokio::test]
    async fn rename_bookmark_updates_name() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("old", PathBuf::from("/t"));
        app.rename_bookmark("old", "new");
        assert_eq!(app.bookmarks[0].0, "new");
        assert_eq!(app.bookmarks[0].1, PathBuf::from("/t"));
    }

    #[tokio::test]
    async fn open_bookmarks_empty_shows_message() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.open_bookmarks();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_message.contains("No bookmarks"));
    }

    #[tokio::test]
    async fn open_bookmarks_non_empty_enters_mode() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("test", PathBuf::from("/t"));
        app.open_bookmarks();
        assert_eq!(app.mode, Mode::Bookmarks);
        assert_eq!(app.bookmark_cursor, 0);
    }

    #[tokio::test]
    async fn bookmark_cursor_navigation() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("a", PathBuf::from("/a"));
        app.add_bookmark("b", PathBuf::from("/b"));
        app.add_bookmark("c", PathBuf::from("/c"));
        app.open_bookmarks();
        // Navigate down
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 1);
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 2);
        // Clamps at end
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 2);
        // Navigate up
        app.handle_bookmarks(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.bookmark_cursor, 1);
    }

    #[tokio::test]
    async fn bookmark_esc_returns_normal() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.add_bookmark("test", PathBuf::from("/t"));
        app.open_bookmarks();
        assert_eq!(app.mode, Mode::Bookmarks);
        app.handle_bookmarks(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── Search tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn search_jump_finds_match() {
        let entries = make_test_entries(&["alpha.rs", "beta.rs", "gamma.py"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0;
        app.search_saved_cursor = 0;
        app.search_query = "beta".into();
        app.handle_search(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        // search_query is now "betaa" but let's test properly
        // Instead, test the jump directly
        let entries = make_test_entries(&["alpha.rs", "beta.rs", "gamma.py"]);
        let mut app = App::new_for_test(entries);
        app.search_saved_cursor = 0;
        app.mode = Mode::Search;
        // Type "beta"
        app.search_query = "beta".into();
        // Call the private method indirectly via handle_search with Enter
        // Actually, let's just set up and check via search_next
        app.active_panel_mut().selected = 0;
        app.search_next();
        assert_eq!(app.active_panel().selected, 2); // "beta.rs" is at index 2
    }

    #[tokio::test]
    async fn search_case_insensitive() {
        let entries = make_test_entries(&["Alpha.RS", "beta.rs"]);
        let mut app = App::new_for_test(entries);
        app.search_query = "ALPHA".into();
        app.search_saved_cursor = 0;
        app.active_panel_mut().selected = 0;
        app.search_next();
        assert_eq!(app.active_panel().selected, 1); // "Alpha.RS"
    }

    #[tokio::test]
    async fn search_next_wraps() {
        let entries = make_test_entries(&["a.rs", "b.txt", "c.rs"]);
        let mut app = App::new_for_test(entries);
        app.search_query = ".rs".into();
        app.active_panel_mut().selected = 3; // "c.rs" (last .rs)
        app.search_next();
        assert_eq!(app.active_panel().selected, 1); // wraps to "a.rs"
    }

    #[tokio::test]
    async fn search_prev_wraps() {
        let entries = make_test_entries(&["a.rs", "b.txt", "c.rs"]);
        let mut app = App::new_for_test(entries);
        app.search_query = ".rs".into();
        app.active_panel_mut().selected = 1; // "a.rs"
        app.search_prev();
        assert_eq!(app.active_panel().selected, 3); // wraps to "c.rs"
    }

    #[tokio::test]
    async fn search_no_match() {
        let entries = make_test_entries(&["a.rs", "b.rs"]);
        let mut app = App::new_for_test(entries);
        app.search_query = "xyz".into();
        app.search_next();
        assert_eq!(app.status_message, "No match");
    }

    #[tokio::test]
    async fn search_empty_query_shows_hint() {
        let entries = make_test_entries(&["a.rs"]);
        let mut app = App::new_for_test(entries);
        app.search_query.clear();
        app.search_next();
        assert!(app.status_message.contains("No search pattern"));
    }

    #[tokio::test]
    async fn search_prev_empty_query_shows_hint() {
        let entries = make_test_entries(&["a.rs"]);
        let mut app = App::new_for_test(entries);
        app.search_query.clear();
        app.search_prev();
        assert!(app.status_message.contains("No search pattern"));
    }

    // ── Select pattern tests ─────────────────────────────────────

    #[tokio::test]
    async fn select_pattern_rs_marks_only_rs() {
        let entries = make_test_entries(&["a.rs", "b.py", "c.rs"]);
        let mut app = App::new_for_test(entries);
        app.rename_input = "*.rs".into();
        app.mode = Mode::SelectPattern;
        app.handle_select_pattern(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.active_panel().marked.len(), 2);
        assert_eq!(app.mode, Mode::Select);
    }

    #[tokio::test]
    async fn select_pattern_star_marks_all() {
        let entries = make_test_entries(&["a.rs", "b.py", "dir/"]);
        let mut app = App::new_for_test(entries);
        app.rename_input = "*".into();
        app.mode = Mode::SelectPattern;
        app.handle_select_pattern(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.active_panel().marked.len(), 3);
        assert_eq!(app.mode, Mode::Select);
    }

    #[tokio::test]
    async fn select_pattern_no_match_returns_normal() {
        let entries = make_test_entries(&["a.rs", "b.py"]);
        let mut app = App::new_for_test(entries);
        app.rename_input = "*.xyz".into();
        app.mode = Mode::SelectPattern;
        app.handle_select_pattern(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.active_panel().marked.len(), 0);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn unselect_pattern_removes_matching() {
        let entries = make_test_entries(&["a.rs", "b.py", "c.rs"]);
        let mut app = App::new_for_test(entries);
        // First select all
        app.select_all();
        assert_eq!(app.active_panel().marked.len(), 3);
        // Unselect *.rs
        app.rename_input = "*.rs".into();
        app.mode = Mode::UnselectPattern;
        app.handle_unselect_pattern(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.active_panel().marked.len(), 1); // only b.py
        assert_eq!(app.mode, Mode::Select);
    }

    #[tokio::test]
    async fn unselect_pattern_clears_all_returns_normal() {
        let entries = make_test_entries(&["a.rs", "b.rs"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        app.rename_input = "*".into();
        app.mode = Mode::UnselectPattern;
        app.handle_unselect_pattern(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.active_panel().marked.is_empty());
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn invert_selection_from_none() {
        let entries = make_test_entries(&["a.rs", "b.py"]);
        let mut app = App::new_for_test(entries);
        app.invert_selection();
        assert_eq!(app.active_panel().marked.len(), 2);
        assert_eq!(app.mode, Mode::Select);
    }

    #[tokio::test]
    async fn invert_selection_from_all() {
        let entries = make_test_entries(&["a.rs", "b.py"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        app.invert_selection();
        assert!(app.active_panel().marked.is_empty());
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn select_pattern_esc_exits() {
        let entries = make_test_entries(&["a.rs"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::SelectPattern;
        app.handle_select_pattern(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── Visual mode tests ────────────────────────────────────────

    #[tokio::test]
    async fn enter_visual_sets_mode_and_anchor() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 2;
        app.enter_visual();
        assert_eq!(app.mode, Mode::Visual);
        assert_eq!(app.active_panel().visual_anchor, Some(2));
    }

    #[tokio::test]
    async fn exit_visual_clears_anchor() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.enter_visual();
        app.exit_visual();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.active_panel().visual_anchor, None);
    }

    #[tokio::test]
    async fn enter_select_and_mark_sets_mode() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "a.txt"
        app.enter_select_and_mark();
        assert_eq!(app.mode, Mode::Select);
        assert!(app.active_panel().marked.contains(&PathBuf::from("/test/a.txt")));
    }

    #[tokio::test]
    async fn exit_select_preserves_marks() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_select_and_mark();
        app.exit_select();
        assert_eq!(app.mode, Mode::Normal);
        // Marks are preserved after exit_select
        assert!(!app.active_panel().marked.is_empty());
    }

    #[tokio::test]
    async fn visual_esc_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.enter_visual();
        assert_eq!(app.mode, Mode::Visual);
        app.handle_visual(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── Info load / du poll tests ────────────────────────────────────

    #[tokio::test]
    async fn apply_info_load_in_info_mode() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Info;
        let lines = vec![("Name".into(), "test.txt".into()), ("Size".into(), "100 B".into())];
        app.apply_info_load(lines.clone());
        assert_eq!(app.info_lines.len(), 2);
        assert_eq!(app.info_lines[0].1, "test.txt");
    }

    #[tokio::test]
    async fn apply_info_load_not_in_info_mode_ignored() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Normal;
        app.info_lines = vec![("old".into(), "data".into())];
        let lines = vec![("Name".into(), "test.txt".into())];
        app.apply_info_load(lines);
        assert_eq!(app.info_lines[0].0, "old"); // unchanged
    }

    #[tokio::test]
    async fn poll_info_du_replaces_placeholders() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Info;
        app.info_lines = vec![
            ("Name".into(), "dir".into()),
            ("Size".into(), "Calculating...".into()),
            ("Files".into(), "Calculating...".into()),
            ("Subdirs".into(), "Calculating...".into()),
        ];
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.info_du_rx = Some(rx);
        tx.send((2048u64, 10usize, 3usize)).unwrap();
        app.poll_info_du();
        assert!(app.info_lines[1].1.contains("2.0 KB"));
        assert_eq!(app.info_lines[2].1, "10");
        assert_eq!(app.info_lines[3].1, "3");
        assert!(app.info_du_rx.is_none());
    }

    // ── Tree data tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn apply_tree_data_matching_start_dir() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.tree_dirty = true;
        let tree_line = crate::tree::TreeLine {
            prefix: String::new(),
            name: "test".into(),
            path: PathBuf::from("/test"),
            is_dir: true,
            is_current: true,
            is_on_path: true,
            is_expanded: false,
            depth: 0,
        };
        let result = TreeLoadResult {
            start_dir: PathBuf::from("/test"),
            current_path: PathBuf::from("/test"),
            data: vec![tree_line],
        };
        app.apply_tree_data(result);
        assert_eq!(app.tree_data.len(), 1);
        assert!(!app.tree_dirty);
    }

    #[tokio::test]
    async fn apply_tree_data_mismatched_start_dir_discarded() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.tree_dirty = true;
        let result = TreeLoadResult {
            start_dir: PathBuf::from("/other"),
            current_path: PathBuf::from("/other"),
            data: vec![],
        };
        app.apply_tree_data(result);
        assert!(app.tree_data.is_empty());
        assert!(app.tree_dirty); // unchanged
    }

    // ── Theme picker navigation test ─────────────────────────────────

    #[tokio::test]
    async fn handle_theme_picker_jk_moves_cursor() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::ThemePicker;
        app.theme_dark_list = vec!["a".into(), "b".into(), "c".into()];
        app.theme_light_list = vec!["x".into(), "y".into()];
        app.theme_col = 0;
        app.theme_cursors = [0; 2];

        // j moves down in dark column
        app.handle_theme_picker(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.theme_cursors[0], 1);

        // k moves up
        app.handle_theme_picker(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.theme_cursors[0], 0);

        // Tab switches to light column
        app.handle_theme_picker(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.theme_col, 1);
    }

    // ── Visual mode handler tests ────────────────────────────────

    #[tokio::test]
    async fn visual_j_k_moves_cursor() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_visual();

        app.handle_visual(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 2);

        app.handle_visual(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 1);
    }

    #[tokio::test]
    async fn visual_g_g_goes_to_top() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 3;
        app.enter_visual();
        // Pending 'g' then 'g'
        app.handle_visual(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        app.handle_visual(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 0);
    }

    #[tokio::test]
    async fn visual_G_goes_to_bottom() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0;
        app.enter_visual();
        app.handle_visual(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 3);
    }

    #[tokio::test]
    async fn visual_v_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.enter_visual();
        app.handle_visual(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.active_panel().visual_anchor.is_none());
    }

    #[tokio::test]
    async fn visual_tab_exits_and_cycles() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Dual;
        app.enter_visual();
        app.handle_visual(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.tab().active, 1);
    }

    #[tokio::test]
    async fn visual_yank_sets_register() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_visual(); // anchor at 1
        app.active_panel_mut().selected = 2; // extend to 2
        app.handle_visual(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.register.is_some());
        assert_eq!(app.register.as_ref().unwrap().entries.len(), 2);
        assert!(app.status_message.contains("Yanked"));
    }

    // ── Select mode handler tests ────────────────────────────────

    #[tokio::test]
    async fn select_j_k_moves_cursor() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_select_and_mark(); // toggle_mark moves cursor down → selected=2

        app.handle_select(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 3);

        app.handle_select(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 2);
    }

    #[tokio::test]
    async fn select_esc_clears_marks() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_select_and_mark();
        assert!(!app.active_panel().marked.is_empty());

        app.handle_select(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.active_panel().marked.is_empty());
    }

    #[tokio::test]
    async fn select_shift_down_marks_and_moves() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.mode = Mode::Select;
        app.handle_select(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
        // toggle_mark marks current and moves down
        assert!(app.active_panel().marked.contains(&PathBuf::from("/test/a.txt")));
        assert_eq!(app.active_panel().selected, 2);
    }

    #[tokio::test]
    async fn select_yank_clears_marks() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_select_and_mark();
        assert!(!app.active_panel().marked.is_empty());

        app.handle_select(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.active_panel().marked.is_empty());
        assert!(app.register.is_some());
    }

    #[tokio::test]
    async fn select_v_enters_visual() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Select;
        app.handle_select(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Visual);
    }

    #[tokio::test]
    async fn select_G_goes_bottom() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Select;
        app.handle_select(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 3);
    }

    #[tokio::test]
    async fn select_gg_goes_top() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 3;
        app.mode = Mode::Select;
        app.handle_select(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        app.handle_select(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.active_panel().selected, 0);
    }

    // ── Command mode additional tests ────────────────────────────
    // Note: execute_command is private, so we test via handle_command(Enter)

    fn run_command(app: &mut App, cmd: &str) {
        app.mode = Mode::Command;
        app.command_input = cmd.into();
        app.handle_command(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    #[tokio::test]
    async fn cmd_sort_all_modes() {
        let entries = make_test_entries(&["a.txt"]);

        let mut app = App::new_for_test(entries.clone());
        run_command(&mut app, "sort mod");
        assert_eq!(app.active_panel().sort_mode, SortMode::Modified);

        let mut app = App::new_for_test(entries.clone());
        run_command(&mut app, "sort cre");
        assert_eq!(app.active_panel().sort_mode, SortMode::Created);

        let mut app = App::new_for_test(entries.clone());
        run_command(&mut app, "sort ext");
        assert_eq!(app.active_panel().sort_mode, SortMode::Extension);

        let mut app = App::new_for_test(entries.clone());
        run_command(&mut app, "sort d");
        assert_eq!(app.active_panel().sort_mode, SortMode::Modified);

        let mut app = App::new_for_test(entries.clone());
        run_command(&mut app, "sort e");
        assert_eq!(app.active_panel().sort_mode, SortMode::Extension);
    }

    #[tokio::test]
    async fn cmd_sort_invalid() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "sort xyz");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_mark_stores() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "mark a");
        assert!(app.marks.contains_key(&'a'));
    }

    #[tokio::test]
    async fn cmd_mark_invalid() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "mark A");
        assert!(app.status_message.contains("Usage"));

        run_command(&mut app, "mark");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_marks_empty() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "marks");
        assert!(app.status_message.contains("No marks"));
    }

    #[tokio::test]
    async fn cmd_marks_shows_list() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.set_mark('a');
        run_command(&mut app, "marks");
        assert!(app.status_message.contains("'a="));
    }

    #[tokio::test]
    async fn cmd_empty_noop() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "  ");
        assert!(!app.should_quit);
    }

    #[tokio::test]
    async fn cmd_bdel_nonexistent() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "bdel nonexistent");
        assert!(app.status_message.contains("not found"));
    }

    #[tokio::test]
    async fn cmd_du_starts() {
        let entries = make_test_entries(&["dir/"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "du");
    }

    #[tokio::test]
    async fn cmd_mkdir_no_arg() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "mkdir");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_touch_no_arg() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "touch");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_rename_no_arg() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "rename");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_cd_no_arg() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "cd");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_bookmark_no_dir() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "a.txt" (not a dir)
        run_command(&mut app, "bookmark test");
        assert!(app.status_message.contains("directory"));
    }

    #[tokio::test]
    async fn cmd_brename_no_arg() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "brename");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_brename_missing_newname() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "brename old");
        assert!(app.status_message.contains("Usage"));
    }

    #[tokio::test]
    async fn cmd_quit_excl() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        run_command(&mut app, "q!");
        assert!(app.should_quit);
    }

    // ── Chmod handler tests ──────────────────────────────────────

    #[tokio::test]
    async fn handle_chmod_empty_enter_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Chmod;
        app.rename_input.clear();
        app.chmod_paths = vec![PathBuf::from("/test/a.txt")];
        app.handle_chmod(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.chmod_paths.is_empty());
    }

    #[tokio::test]
    async fn handle_chmod_invalid_mode_shows_error() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Chmod;
        app.rename_input = "99".into(); // only 2 digits
        app.chmod_paths = vec![PathBuf::from("/test/a.txt")];
        app.handle_chmod(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.status_message.contains("Invalid"));
        assert_eq!(app.mode, Mode::Chmod); // stays in chmod mode
    }

    #[tokio::test]
    async fn handle_chmod_invalid_digit_shows_error() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Chmod;
        app.rename_input = "898".into(); // 8 and 9 are not octal
        app.chmod_paths = vec![PathBuf::from("/test/a.txt")];
        app.handle_chmod(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.status_message.contains("Invalid"));
    }

    #[tokio::test]
    async fn handle_chmod_backspace_empty_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Chmod;
        app.rename_input.clear();
        app.chmod_paths = vec![PathBuf::from("/test/a.txt")];
        app.handle_chmod(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── Chown handler tests ──────────────────────────────────────

    #[tokio::test]
    async fn handle_chown_navigation() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Chown;
        app.visible_height = 20;
        app.chown_picker = Some(crate::app::chmod::ChownPicker {
            users: vec![("root".into(), 0), ("user".into(), 1000), ("daemon".into(), 1)],
            groups: vec![("wheel".into(), 0), ("staff".into(), 20)],
            user_cursor: 0,
            group_cursor: 0,
            user_scroll: 0,
            group_scroll: 0,
            column: 0,
            paths: vec![PathBuf::from("/test/a.txt")],
            current_uid: Some(0),
            current_gid: Some(0),
        });

        // j moves user cursor down
        app.handle_chown(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.chown_picker.as_ref().unwrap().user_cursor, 1);

        // k moves back
        app.handle_chown(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.chown_picker.as_ref().unwrap().user_cursor, 0);

        // Tab switches to group column
        app.handle_chown(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.chown_picker.as_ref().unwrap().column, 1);

        // j moves group cursor
        app.handle_chown(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.chown_picker.as_ref().unwrap().group_cursor, 1);

        // G goes to end
        app.handle_chown(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(app.chown_picker.as_ref().unwrap().group_cursor, 1); // last item

        // g goes to start
        app.handle_chown(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.chown_picker.as_ref().unwrap().group_cursor, 0);
    }

    #[tokio::test]
    async fn handle_chown_esc_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Chown;
        app.chown_picker = Some(crate::app::chmod::ChownPicker {
            users: vec![],
            groups: vec![],
            user_cursor: 0,
            group_cursor: 0,
            user_scroll: 0,
            group_scroll: 0,
            column: 0,
            paths: vec![],
            current_uid: None,
            current_gid: None,
        });

        app.handle_chown(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.chown_picker.is_none());
    }

    #[tokio::test]
    async fn handle_chown_no_picker_exits() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Chown;
        app.chown_picker = None;
        app.handle_chown(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── File ops tests ──────────────────────────────────────────

    #[tokio::test]
    async fn yank_targeted_empty() {
        let entries = make_test_entries(&[]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".."
        app.yank_targeted();
        assert!(app.register.is_none());
        assert!(app.status_message.contains("Nothing to yank"));
    }

    #[tokio::test]
    async fn yank_targeted_single() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "a.txt"
        app.yank_targeted();
        assert!(app.register.is_some());
        assert_eq!(app.register.as_ref().unwrap().entries.len(), 1);
        assert!(app.status_message.contains("Yanked 1"));
    }

    #[tokio::test]
    async fn copy_to_other_panel_single_layout_blocked() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Single;
        app.copy_to_other_panel();
        assert!(app.status_message.contains("Cannot copy"));
    }

    #[tokio::test]
    async fn move_to_other_panel_single_layout_blocked() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Single;
        app.move_to_other_panel();
        assert!(app.status_message.contains("Cannot move"));
    }

    #[tokio::test]
    async fn move_to_other_panel_empty_nothing() {
        let entries = make_test_entries(&[]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Dual;
        app.active_panel_mut().selected = 0; // ".."
        app.move_to_other_panel();
        assert!(app.status_message.contains("Nothing to move"));
    }

    #[tokio::test]
    async fn paste_empty_register() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.register = None;
        app.paste(false);
        assert!(app.status_message.contains("Register empty"));
    }

    #[tokio::test]
    async fn undo_empty_stack() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.undo();
        assert!(app.status_message.contains("Nothing to undo"));
    }

    #[tokio::test]
    async fn request_delete_empty_shows_message() {
        let entries = make_test_entries(&[]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".."
        app.request_delete();
        assert!(app.status_message.contains("Nothing to delete"));
    }

    #[tokio::test]
    async fn request_delete_enters_confirm() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "a.txt"
        app.request_delete();
        assert_eq!(app.mode, Mode::Confirm);
        assert!(!app.confirm_permanent);
        assert_eq!(app.confirm_paths.len(), 1);
    }

    #[tokio::test]
    async fn request_permanent_delete_sets_flag() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.request_permanent_delete();
        assert_eq!(app.mode, Mode::Confirm);
        assert!(app.confirm_permanent);
    }
}
