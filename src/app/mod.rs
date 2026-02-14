pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use std::path::PathBuf;
pub(crate) use std::sync::mpsc;
pub(crate) use std::time::Instant;

pub(crate) use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) use crate::find::{FindScope, FindState};
pub(crate) use crate::ops::{self, DuMsg, ProgressMsg, Register, RegisterOp, UndoStack};
pub(crate) use crate::panel::{Panel, SortMode};
pub(crate) use crate::preview::Preview;
pub(crate) use crate::theme::Theme;

mod command;
mod file_ops;
mod find;
mod git;
mod input;
mod marks;
mod navigation;
mod polling;
mod rename;
mod search;
mod tree;
mod visual;

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

pub struct DuProgress {
    pub rx: mpsc::Receiver<DuMsg>,
    pub started_at: Instant,
}

#[derive(PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual,
    Select,
    Command,
    Confirm,
    Search,
    Find,
    Help,
    Sort,
    Rename,
    Create,
    Preview,
    ThemePicker,
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
    pub rename_input: String,
    pub should_quit: bool,
    pub open_editor: Option<PathBuf>,
    pub status_message: String,
    pub pending_key: Option<char>,
    pub pending_key_time: Option<Instant>,
    pub visible_height: usize,
    pub register: Option<Register>,
    pub undo_stack: UndoStack,
    pub confirm_paths: Vec<PathBuf>,
    pub confirm_scroll: usize,
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
    pub(super) preview_path: Option<PathBuf>,
    // File preview popup
    pub file_preview: Option<Preview>,
    pub file_preview_path: Option<PathBuf>,
    // Tree
    pub show_tree: bool,
    pub tree_focused: bool,
    pub tree_selected: usize,
    pub tree_scroll: usize,
    pub start_dir: PathBuf,
    pub tree_data: Vec<crate::tree::TreeLine>,
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
    // Theme
    pub theme: Theme,
    pub theme_list: Vec<String>,
    pub theme_index: Option<usize>,
    pub theme_cursor: usize,
    pub theme_scroll: usize,
    pub theme_preview: Option<Theme>,
    // Per-directory sort preferences
    pub dir_sorts: HashMap<PathBuf, (SortMode, bool)>,
    // Sort popup
    pub sort_cursor: usize,
    // Git status
    pub git_statuses: HashMap<PathBuf, char>,
    pub git_root: Option<PathBuf>,
    pub(super) git_status_dir: Option<PathBuf>,
}

impl App {
    pub fn new() -> std::io::Result<Self> {
        let cwd = std::env::current_dir()?;

        let (db, visual_marks, dir_sorts) = match crate::db::Db::init() {
            Ok(db) => {
                let marks = db.load_visual_marks().unwrap_or_default();
                let raw_sorts = db.load_dir_sorts().unwrap_or_default();
                let dir_sorts: HashMap<PathBuf, (SortMode, bool)> = raw_sorts
                    .into_iter()
                    .filter_map(|(p, (label, rev))| {
                        SortMode::from_label(&label).map(|m| (p, (m, rev)))
                    })
                    .collect();
                (Some(db), marks, dir_sorts)
            }
            Err(e) => {
                eprintln!("Warning: DB init failed: {e}");
                (None, HashMap::new(), HashMap::new())
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

        let saved_theme_name = db.as_ref().and_then(|d| d.load_theme());
        let theme = match saved_theme_name.as_deref().and_then(Theme::load_by_name) {
            Some(t) => t,
            None => Theme::from_config(),
        };
        let theme_index = saved_theme_name.and_then(|name| {
            let list = Theme::list_available();
            list.iter().position(|n| n == &name)
        });

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
            search_query: String::new(),
            search_saved_cursor: 0,
            marks: HashMap::new(),
            find_state: None,
            preview_mode: false,
            preview: None,
            preview_path: None,
            file_preview: None,
            file_preview_path: None,
            show_tree: false,
            tree_focused: false,
            tree_selected: 0,
            tree_scroll: 0,
            start_dir: cwd,
            tree_data: Vec::new(),
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
            theme,
            theme_list: Vec::new(),
            theme_index,
            theme_cursor: 0,
            theme_scroll: 0,
            theme_preview: None,
            sort_cursor: 0,
            git_statuses: HashMap::new(),
            git_root: None,
            git_status_dir: None,
        };
        app.refresh_git_status();
        // Apply saved sort preferences to restored panels
        for tab in &mut app.tabs {
            for panel in [&mut tab.left, &mut tab.right] {
                if let Some(&(mode, rev)) = app.dir_sorts.get(&panel.path) {
                    if panel.sort_mode != mode || panel.sort_reverse != rev {
                        panel.sort_mode = mode;
                        panel.sort_reverse = rev;
                        let _ = panel.load_dir();
                    }
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
            Mode::Select => self.handle_select(key),
            Mode::Command => self.handle_command(key),
            Mode::Confirm => self.handle_confirm(key),
            Mode::Search => self.handle_search(key),
            Mode::Find => self.handle_find(key),
            Mode::Help => self.handle_help(key),
            Mode::Sort => self.handle_sort(key),
            Mode::Rename => self.handle_rename(key),
            Mode::Create => self.handle_create(key),
            Mode::Preview => self.handle_preview(key),
            Mode::ThemePicker => self.handle_theme_picker(key),
        }

        self.update_preview();
        self.ensure_git_status();
    }
}
