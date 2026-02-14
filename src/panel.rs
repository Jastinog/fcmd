use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub is_symlink: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Name,
    Size,
    Modified,
    Created,
    Extension,
}

impl SortMode {
    pub const ALL: &[SortMode] = &[
        SortMode::Name,
        SortMode::Size,
        SortMode::Modified,
        SortMode::Created,
        SortMode::Extension,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortMode::Name => "name",
            SortMode::Size => "size",
            SortMode::Modified => "mod",
            SortMode::Created => "cre",
            SortMode::Extension => "ext",
        }
    }
}

pub struct Panel {
    pub path: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub offset: usize,
    pub visual_anchor: Option<usize>,
    pub marked: HashSet<PathBuf>,
    pub sort_mode: SortMode,
    pub sort_reverse: bool,
    pub show_hidden: bool,
}

impl Panel {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let mut panel = Panel {
            path,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            visual_anchor: None,
            marked: HashSet::new(),
            sort_mode: SortMode::Name,
            sort_reverse: false,
            show_hidden: false,
        };
        panel.load_dir()?;
        Ok(panel)
    }

    pub fn load_dir(&mut self) -> std::io::Result<()> {
        self.load_dir_with_sizes(&HashMap::new())
    }

    pub fn load_dir_with_sizes(&mut self, dir_sizes: &HashMap<PathBuf, u64>) -> std::io::Result<()> {
        self.entries.clear();

        if let Some(parent) = self.path.parent() {
            self.entries.push(FileEntry {
                name: "..".into(),
                path: parent.to_path_buf(),
                is_dir: true,
                size: 0,
                modified: None,
                created: None,
                is_symlink: false,
            });
        }

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        let read_dir = match fs::read_dir(&self.path) {
            Ok(rd) => rd,
            Err(e) => return Err(e),
        };

        for entry in read_dir.flatten() {
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let symlink_meta = entry.path().symlink_metadata().ok();
            let is_symlink = symlink_meta.map(|m| m.is_symlink()).unwrap_or(false);

            let file_entry = FileEntry {
                name: entry.file_name().to_string_lossy().into(),
                path: entry.path(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
                created: metadata.created().ok(),
                is_symlink,
            };

            if !self.show_hidden && file_entry.name.starts_with('.') {
                continue;
            }

            if file_entry.is_dir {
                dirs.push(file_entry);
            } else {
                files.push(file_entry);
            }
        }

        match self.sort_mode {
            SortMode::Name => {
                dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            SortMode::Size => {
                dirs.sort_by(|a, b| {
                    let sa = dir_sizes.get(&a.path).copied();
                    let sb = dir_sizes.get(&b.path).copied();
                    match (sa, sb) {
                        (Some(sa), Some(sb)) => sa.cmp(&sb),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    }
                });
                files.sort_by(|a, b| a.size.cmp(&b.size));
            }
            SortMode::Modified => {
                dirs.sort_by(|a, b| a.modified.cmp(&b.modified));
                files.sort_by(|a, b| a.modified.cmp(&b.modified));
            }
            SortMode::Created => {
                dirs.sort_by(|a, b| a.created.cmp(&b.created));
                files.sort_by(|a, b| a.created.cmp(&b.created));
            }
            SortMode::Extension => {
                dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                files.sort_by(|a, b| {
                    let ea = Path::new(&a.name)
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    let eb = Path::new(&b.name)
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    ea.cmp(&eb)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
            }
        }

        if self.sort_reverse {
            dirs.reverse();
            files.reverse();
        }

        self.entries.extend(dirs);
        self.entries.extend(files);
        self.clamp_selected();
        Ok(())
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        if !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
        }
    }

    pub fn page_up(&mut self, half_page: usize) {
        self.selected = self.selected.saturating_sub(half_page);
    }

    pub fn page_down(&mut self, half_page: usize) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + half_page).min(self.entries.len() - 1);
        }
    }

    pub fn enter_selected(&mut self) -> std::io::Result<bool> {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.is_dir {
                let new_path = entry.path.clone();
                self.path = new_path;
                self.selected = 0;
                self.offset = 0;
                self.marked.clear();
                self.load_dir()?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn go_parent(&mut self) -> std::io::Result<bool> {
        if let Some(parent) = self.path.parent().map(|p| p.to_path_buf()) {
            let old_name = self.path.file_name().map(|n| n.to_string_lossy().into_owned());
            self.path = parent;
            self.selected = 0;
            self.offset = 0;
            self.marked.clear();
            self.load_dir()?;

            if let Some(name) = old_name {
                if let Some(pos) = self.entries.iter().position(|e| e.name == name) {
                    self.selected = pos;
                }
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub fn go_home(&mut self) -> std::io::Result<()> {
        if let Some(home) = home_dir() {
            self.path = home;
            self.selected = 0;
            self.offset = 0;
            self.marked.clear();
            self.load_dir()?;
        }
        Ok(())
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected)
    }

    /// Returns the visual selection range (lo, hi) inclusive, or None.
    pub fn visual_range(&self) -> Option<(usize, usize)> {
        self.visual_anchor.map(|anchor| {
            (anchor.min(self.selected), anchor.max(self.selected))
        })
    }

    /// Paths of currently "targeted" entries.
    /// Priority: marks > visual range > single selected. Filters out "..".
    pub fn targeted_paths(&self) -> Vec<PathBuf> {
        if !self.marked.is_empty() {
            self.entries
                .iter()
                .filter(|e| e.name != ".." && self.marked.contains(&e.path))
                .map(|e| e.path.clone())
                .collect()
        } else if let Some((lo, hi)) = self.visual_range() {
            self.entries[lo..=hi]
                .iter()
                .filter(|e| e.name != "..")
                .map(|e| e.path.clone())
                .collect()
        } else {
            self.selected_entry()
                .filter(|e| e.name != "..")
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        }
    }

    /// Number of targeted entries (for status display).
    pub fn targeted_count(&self) -> usize {
        if !self.marked.is_empty() {
            self.entries
                .iter()
                .filter(|e| e.name != ".." && self.marked.contains(&e.path))
                .count()
        } else if let Some((lo, hi)) = self.visual_range() {
            self.entries[lo..=hi]
                .iter()
                .filter(|e| e.name != "..")
                .count()
        } else if self.selected_entry().is_some_and(|e| e.name != "..") {
            1
        } else {
            0
        }
    }

    /// Toggle mark on current entry and move cursor down.
    pub fn toggle_mark(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.name != ".." {
                let path = entry.path.clone();
                if !self.marked.remove(&path) {
                    self.marked.insert(path);
                }
            }
        }
        self.move_down();
    }

    pub fn toggle_mark_up(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.name != ".." {
                let path = entry.path.clone();
                if !self.marked.remove(&path) {
                    self.marked.insert(path);
                }
            }
        }
        self.move_up();
    }

    /// Select entry by name after refresh.
    pub fn select_by_name(&mut self, name: &str) {
        if let Some(pos) = self.entries.iter().position(|e| e.name == name) {
            self.selected = pos;
        }
    }

    pub fn adjust_scroll(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + visible_height {
            self.offset = self.selected - visible_height + 1;
        }
    }

    fn clamp_selected(&mut self) {
        if self.entries.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
