use super::*;
use crate::archive::{self, ArchiveEntry, ArchiveFormat};

#[derive(Clone)]
pub struct ArchiveTreeNode {
    pub name: String,
    pub full_path: String,
    pub depth: usize,
    pub is_dir: bool,
    pub size: u64,
    pub expanded: bool,
}

pub struct ArchiveState {
    pub archive_path: PathBuf,
    #[allow(dead_code)]
    pub format: ArchiveFormat,
    pub entries: Vec<ArchiveEntry>,
    pub tree: Vec<ArchiveTreeNode>,
    pub cursor: usize,
    pub scroll: usize,
    pub total_size: u64,
    pub file_count: usize,
    pub expanded: HashSet<String>,
    pub search_query: String,
    pub searching: bool,
}

impl ArchiveState {
    pub fn new(
        archive_path: PathBuf,
        format: ArchiveFormat,
        entries: Vec<ArchiveEntry>,
    ) -> Self {
        let total_size: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
        let file_count = entries.iter().filter(|e| !e.is_dir).count();

        // Expand top-level by default
        let mut expanded = HashSet::new();
        for e in &entries {
            if e.is_dir {
                let depth = e.path.matches('/').count();
                // Top-level dirs: "name/" has exactly 1 slash
                if depth <= 1 {
                    expanded.insert(e.path.clone());
                }
            }
        }

        let mut state = Self {
            archive_path,
            format,
            entries,
            tree: Vec::new(),
            cursor: 0,
            scroll: 0,
            total_size,
            file_count,
            expanded,
            search_query: String::new(),
            searching: false,
        };
        state.rebuild_tree();
        state
    }

    pub fn rebuild_tree(&mut self) {
        self.tree.clear();

        for entry in &self.entries {
            let path = &entry.path;
            let depth = if entry.is_dir {
                path.trim_end_matches('/').matches('/').count()
            } else {
                path.matches('/').count()
            };

            // Check if all ancestors are expanded
            let visible = if depth == 0 {
                true
            } else {
                let trimmed = path.trim_end_matches('/');
                let mut all_expanded = true;
                let mut check = trimmed;
                while let Some(pos) = check.rfind('/') {
                    let parent = format!("{}/", &check[..pos]);
                    if !self.expanded.contains(&parent) {
                        all_expanded = false;
                        break;
                    }
                    check = &check[..pos];
                }
                all_expanded
            };

            if !visible {
                continue;
            }

            // Apply search filter
            if !self.search_query.is_empty() {
                let query_lower = self.search_query.to_ascii_lowercase();
                let name_lower = path.to_ascii_lowercase();
                if !name_lower.contains(&query_lower) {
                    if entry.is_dir {
                        // Show directory only if it has matching descendants
                        let has_match = self.entries.iter().any(|e| {
                            !e.is_dir
                                && e.path.starts_with(&entry.path)
                                && e.path.to_ascii_lowercase().contains(&query_lower)
                        });
                        if !has_match {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
            }

            let name = if entry.is_dir {
                let trimmed = path.trim_end_matches('/');
                trimmed
                    .rsplit_once('/')
                    .map(|(_, n)| n)
                    .unwrap_or(trimmed)
                    .to_string()
            } else {
                path.rsplit_once('/')
                    .map(|(_, n)| n)
                    .unwrap_or(path)
                    .to_string()
            };

            self.tree.push(ArchiveTreeNode {
                name,
                full_path: entry.path.clone(),
                depth,
                is_dir: entry.is_dir,
                size: entry.size,
                expanded: entry.is_dir && self.expanded.contains(&entry.path),
            });
        }

        // Clamp scroll/cursor to new tree size
        if !self.tree.is_empty() {
            if self.cursor >= self.tree.len() {
                self.cursor = self.tree.len() - 1;
            }
            if self.scroll > self.tree.len().saturating_sub(1) {
                self.scroll = self.tree.len().saturating_sub(1);
            }
        } else {
            self.cursor = 0;
            self.scroll = 0;
        }
    }

    pub fn toggle_expand(&mut self) {
        if let Some(node) = self.tree.get(self.cursor) {
            if node.is_dir {
                let path = node.full_path.clone();
                if self.expanded.contains(&path) {
                    self.expanded.remove(&path);
                } else {
                    self.expanded.insert(path);
                }
                self.rebuild_tree();
                // Clamp cursor
                if self.cursor >= self.tree.len() {
                    self.cursor = self.tree.len().saturating_sub(1);
                }
            }
        }
    }

    pub fn collapse(&mut self) {
        if let Some(node) = self.tree.get(self.cursor) {
            if node.is_dir && self.expanded.contains(&node.full_path) {
                // Collapse this dir and all children
                let path = node.full_path.clone();
                self.expanded.retain(|p| !p.starts_with(&path));
                self.rebuild_tree();
            } else if node.depth > 0 {
                // Go to parent dir
                let trimmed = node.full_path.trim_end_matches('/');
                if let Some(pos) = trimmed.rfind('/') {
                    let parent = format!("{}/", &trimmed[..pos]);
                    if let Some(idx) = self.tree.iter().position(|n| n.full_path == parent) {
                        self.cursor = idx;
                    }
                }
            }
        }
    }

    pub fn selected_entry_path(&self) -> Option<&str> {
        self.tree.get(self.cursor).map(|n| n.full_path.as_str())
    }

    fn adjust_scroll(&mut self, visible_h: usize) {
        if visible_h == 0 {
            return;
        }
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + visible_h {
            self.scroll = self.cursor - visible_h + 1;
        }
    }
}

impl App {
    /// Open archive overlay for the selected file.
    pub(super) fn open_archive(&mut self) {
        let entry = match self.active_panel().selected_entry() {
            Some(e) if !e.is_dir && e.name != ".." => e,
            _ => return,
        };

        let path = entry.path.clone();
        if !archive::is_archive(&path) {
            return;
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.archive_load_rx = Some(rx);

        tokio::task::spawn_blocking(move || {
            let result = archive::list_archive(&path);
            let _ = tx.send(ArchiveLoadResult {
                archive_path: path,
                result,
            });
        });

        self.status_message = "Loading archive...".into();
    }

    /// Handle the async archive listing result.
    pub fn handle_archive_load(&mut self, result: ArchiveLoadResult) {
        match result.result {
            Ok((format, entries)) => {
                let state = ArchiveState::new(result.archive_path, format, entries);
                self.archive_state = Some(state);
                self.mode = Mode::Archive;
                self.status_message.clear();
            }
            Err(e) => {
                self.status_message = format!("Failed to read archive: {e}");
            }
        }
    }

    pub(super) fn handle_archive(&mut self, key: KeyEvent) {
        let state = match self.archive_state {
            Some(ref s) => s,
            None => return,
        };

        if state.searching {
            self.handle_archive_search(key);
            return;
        }

        let len = state.tree.len();
        let state = self.archive_state.as_mut().unwrap();

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if state.cursor + 1 < len {
                    state.cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.cursor > 0 {
                    state.cursor -= 1;
                }
            }
            KeyCode::Char('G') => {
                state.cursor = len.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                state.cursor = 0;
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                state.cursor = (state.cursor + half).min(len.saturating_sub(1));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                state.cursor = state.cursor.saturating_sub(half);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                state.toggle_expand();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                state.collapse();
            }
            KeyCode::Enter => {
                // Expand/collapse for dirs, no-op for files
                if state.tree.get(state.cursor).is_some_and(|n| n.is_dir) {
                    state.toggle_expand();
                }
            }
            KeyCode::Char('x') => {
                self.extract_archive_entry();
                return;
            }
            KeyCode::Char('X') => {
                self.extract_archive_all();
                return;
            }
            KeyCode::Char('/') => {
                state.searching = true;
                state.search_query.clear();
                return;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.archive_state = None;
                self.mode = Mode::Normal;
                return;
            }
            _ => {}
        }

        if let Some(ref mut state) = self.archive_state {
            state.adjust_scroll(self.visible_height.saturating_sub(6));
        }
    }

    fn handle_archive_search(&mut self, key: KeyEvent) {
        let state = self.archive_state.as_mut().unwrap();
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                state.searching = false;
                if key.code == KeyCode::Esc {
                    state.search_query.clear();
                }
                state.cursor = 0;
                state.rebuild_tree();
            }
            KeyCode::Backspace => {
                state.search_query.pop();
                state.cursor = 0;
                state.rebuild_tree();
            }
            KeyCode::Char(c) => {
                state.search_query.push(c);
                state.cursor = 0;
                state.rebuild_tree();
            }
            _ => {}
        }
    }

    fn extract_archive_entry(&mut self) {
        let state = match self.archive_state {
            Some(ref s) => s,
            None => return,
        };
        let entry_path = match state.selected_entry_path() {
            Some(p) => p.to_string(),
            None => return,
        };
        let archive_path = state.archive_path.clone();
        let dest = self.active_panel().path.clone();

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);

        tokio::task::spawn_blocking(move || {
            let result = archive::extract_entry(&archive_path, &entry_path, &dest);
            let _ = tx.send(FileOpResult::ArchiveExtract {
                entry_path,
                result: result.map(|paths| paths.len()).map_err(|e| e.to_string()),
            });
        });

        self.archive_state = None;
        self.mode = Mode::Normal;
    }

    fn extract_archive_all(&mut self) {
        let state = match self.archive_state {
            Some(ref s) => s,
            None => return,
        };
        let archive_path = state.archive_path.clone();
        let dest = self.active_panel().path.clone();

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);

        tokio::task::spawn_blocking(move || {
            let result = archive::extract_all(&archive_path, &dest);
            let _ = tx.send(FileOpResult::ArchiveExtract {
                entry_path: String::new(),
                result: result.map(|paths| paths.len()).map_err(|e| e.to_string()),
            });
        });

        self.archive_state = None;
        self.mode = Mode::Normal;
    }

    /// Create archive from selected files. Called via `:archive <name>`.
    pub(super) fn create_archive(&mut self, name: &str) {
        let panel = self.active_panel();
        let targeted = panel.targeted_register_entries();
        if targeted.is_empty() {
            self.status_message = "Nothing selected to archive".into();
            return;
        }

        let output = self.active_panel().path.join(name);
        if archive::ArchiveFormat::from_path(&output).is_none() {
            self.status_message =
                "Unknown format. Use .zip, .tar, .tar.gz, .tar.bz2, .tar.xz".into();
            return;
        }

        let base_dir = self.active_panel().path.clone();
        let paths: Vec<PathBuf> = targeted.into_iter().map(|e| e.path).collect();
        let count = paths.len();
        let name_owned = name.to_string();

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);

        tokio::task::spawn_blocking(move || {
            let result = archive::create_archive(&paths, &base_dir, &output)
                .map_err(|e| e.to_string());
            let _ = tx.send(FileOpResult::ArchiveCreate {
                name: name_owned,
                count,
                result,
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_tree_basic() {
        let entries = vec![
            ArchiveEntry {
                path: "src/".into(),
                size: 0,
                is_dir: true,
                modified: None,
            },
            ArchiveEntry {
                path: "src/main.rs".into(),
                size: 100,
                is_dir: false,
                modified: None,
            },
            ArchiveEntry {
                path: "README.md".into(),
                size: 200,
                is_dir: false,
                modified: None,
            },
        ];

        let state = ArchiveState::new(
            PathBuf::from("test.zip"),
            ArchiveFormat::Zip,
            entries,
        );

        // src/ is top-level dir → expanded by default
        assert!(state.expanded.contains("src/"));
        // Tree should have all 3 items visible
        assert_eq!(state.tree.len(), 3);
        assert_eq!(state.file_count, 2);
    }

    #[test]
    fn archive_tree_collapse_expand() {
        let entries = vec![
            ArchiveEntry {
                path: "dir/".into(),
                size: 0,
                is_dir: true,
                modified: None,
            },
            ArchiveEntry {
                path: "dir/file.txt".into(),
                size: 50,
                is_dir: false,
                modified: None,
            },
        ];

        let mut state = ArchiveState::new(
            PathBuf::from("test.zip"),
            ArchiveFormat::Zip,
            entries,
        );

        // Initially expanded → 2 items visible
        assert_eq!(state.tree.len(), 2);

        // Collapse
        state.cursor = 0;
        state.collapse();
        assert_eq!(state.tree.len(), 1); // Only dir/ visible

        // Expand
        state.toggle_expand();
        assert_eq!(state.tree.len(), 2);
    }

    #[test]
    fn archive_tree_search() {
        let entries = vec![
            ArchiveEntry {
                path: "hello.txt".into(),
                size: 10,
                is_dir: false,
                modified: None,
            },
            ArchiveEntry {
                path: "world.rs".into(),
                size: 20,
                is_dir: false,
                modified: None,
            },
        ];

        let mut state = ArchiveState::new(
            PathBuf::from("test.zip"),
            ArchiveFormat::Zip,
            entries,
        );

        assert_eq!(state.tree.len(), 2);

        state.search_query = "hello".into();
        state.rebuild_tree();
        assert_eq!(state.tree.len(), 1);
        assert_eq!(state.tree[0].name, "hello.txt");
    }
}
