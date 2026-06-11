use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::archive::{self, ArchiveEntry, ArchiveFormat, ConflictDecision};
use crate::fs::ops::{ConflictChoice, ConflictInfo};

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
    /// Set after the first `X` press; the second `X` confirms extract-all.
    pub confirm_extract_all: bool,
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
            confirm_extract_all: false,
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
        if let Some(node) = self.tree.get(self.cursor)
            && node.is_dir {
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

        // Any key other than a second 'X' cancels a pending extract-all confirmation.
        if !matches!(key.code, KeyCode::Char('X')) {
            state.confirm_extract_all = false;
        }

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
                if state.confirm_extract_all {
                    self.extract_archive_all();
                } else {
                    state.confirm_extract_all = true;
                    let n = state.file_count;
                    let dest = self.active_panel().path.display();
                    self.status_message = format!(
                        "Extract all {n} file(s) to {dest} \u{2014} press X again to confirm"
                    );
                }
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
        let Some(state) = self.archive_state.as_ref() else {
            return;
        };
        let Some(entry_path) = state.selected_entry_path() else {
            return;
        };
        let entry_path = entry_path.to_string();
        let archive_path = state.archive_path.clone();
        let total = matching_entry_count(&state.entries, Some(&entry_path));
        let label = entry_path.clone();
        self.start_archive_extract(archive_path, Some(entry_path), total, label);
    }

    fn extract_archive_all(&mut self) {
        let Some(state) = self.archive_state.as_ref() else {
            return;
        };
        let archive_path = state.archive_path.clone();
        let total = state.entries.len();
        self.start_archive_extract(archive_path, None, total, "all entries".into());
    }

    /// Spawn a streaming extract task: progress flows through the task manager and
    /// per-file overwrite conflicts route through the shared conflict dialog
    /// (`conflict_rxs` / `Mode::Conflict`), exactly like `ops::paste`.
    fn start_archive_extract(
        &mut self,
        archive_path: PathBuf,
        filter: Option<String>,
        total: usize,
        label: String,
    ) {
        let dest = self.active_panel().path.clone();

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let (conflict_tx, conflict_rx) = tokio::sync::mpsc::channel(4);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = Arc::clone(&cancel);
        // Each task owns its conflict channel so a second extract/paste can't make an
        // earlier task silently skip conflicts (see poll_conflicts).
        self.conflict_rxs.push(conflict_rx);

        tokio::task::spawn_blocking(move || {
            // Throttled, non-blocking progress: a full channel (drained at UI tick
            // rate) must never throttle extraction. Progress is lossy; the final
            // counts arrive via ArchiveMsg::Finished.
            let mut last_report: Option<std::time::Instant> = None;
            let mut on_progress = |done: usize, total: usize, current: &str| {
                let now = std::time::Instant::now();
                if last_report.is_none_or(|t| now.duration_since(t) >= crate::fs::ops::PROGRESS_INTERVAL) {
                    last_report = Some(now);
                    let _ = tx.try_send(ArchiveMsg::Progress {
                        done,
                        total,
                        current: current.to_string(),
                    });
                }
            };

            let mut overwrite_all = false;
            let mut skip_all = false;
            let mut resolve = |name: &str,
                               dst: &std::path::Path,
                               src_size: u64,
                               src_mod: Option<std::time::SystemTime>|
             -> ConflictDecision {
                if overwrite_all {
                    return ConflictDecision::Overwrite;
                }
                if skip_all {
                    return ConflictDecision::Skip;
                }
                let dst_meta = std::fs::symlink_metadata(dst).ok();
                let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                let info = ConflictInfo {
                    src_path: PathBuf::from(name),
                    dst_path: dst.to_path_buf(),
                    src_size,
                    dst_size: dst_meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    src_modified: src_mod,
                    dst_modified: dst_meta.as_ref().and_then(|m| m.modified().ok()),
                    is_dir: false,
                    response_tx: resp_tx,
                };
                if conflict_tx.blocking_send(info).is_err() {
                    return ConflictDecision::Abort;
                }
                match resp_rx.blocking_recv().unwrap_or(ConflictChoice::Abort) {
                    ConflictChoice::Overwrite => ConflictDecision::Overwrite,
                    ConflictChoice::Skip => ConflictDecision::Skip,
                    ConflictChoice::OverwriteAll => {
                        overwrite_all = true;
                        ConflictDecision::Overwrite
                    }
                    ConflictChoice::SkipAll => {
                        skip_all = true;
                        ConflictDecision::Skip
                    }
                    ConflictChoice::OverwriteNewer => {
                        let dst_mod = dst_meta.as_ref().and_then(|m| m.modified().ok());
                        match (src_mod, dst_mod) {
                            (Some(s), Some(d)) if s <= d => ConflictDecision::Skip,
                            _ => ConflictDecision::Overwrite,
                        }
                    }
                    ConflictChoice::Abort => ConflictDecision::Abort,
                }
            };

            let result = archive::extract_stream(
                &archive_path,
                filter.as_deref(),
                &dest,
                total,
                &mut on_progress,
                &mut resolve,
                &cancel_worker,
            );
            let msg = match result {
                Ok(outcome) => ArchiveMsg::Finished {
                    is_create: false,
                    processed: outcome.extracted.len(),
                    skipped: outcome.skipped,
                    error: None,
                    cancelled: outcome.cancelled,
                    label,
                },
                Err(e) => ArchiveMsg::Finished {
                    is_create: false,
                    processed: 0,
                    skipped: 0,
                    error: Some(e.to_string()),
                    cancelled: false,
                    label,
                },
            };
            let _ = tx.blocking_send(msg);
        });

        self.task_manager.add_archive(rx, false, cancel);
        self.status_message = "Extracting (see Tasks: Space j)...".into();
        self.archive_state = None;
        self.mode = Mode::Normal;
    }

    /// Create archive from selected files. Called via `:archive <name>`.
    /// When `force` is false, refuses to overwrite an existing file.
    pub(super) fn create_archive(&mut self, name: &str, force: bool) {
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
        if !force && output.exists() {
            self.status_message =
                format!("{name} already exists \u{2014} use :archive! to overwrite");
            return;
        }

        let base_dir = self.active_panel().path.clone();
        let paths: Vec<PathBuf> = targeted.into_iter().map(|e| e.path).collect();
        let name_owned = name.to_string();

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = Arc::clone(&cancel);

        tokio::task::spawn_blocking(move || {
            // See start_archive_extract: throttled, lossy, non-blocking progress.
            let mut last_report: Option<std::time::Instant> = None;
            let mut on_progress = |done: usize, total: usize, current: &str| {
                let now = std::time::Instant::now();
                if last_report.is_none_or(|t| now.duration_since(t) >= crate::fs::ops::PROGRESS_INTERVAL) {
                    last_report = Some(now);
                    let _ = tx.try_send(ArchiveMsg::Progress {
                        done,
                        total,
                        current: current.to_string(),
                    });
                }
            };
            let result =
                archive::create_stream(&paths, &base_dir, &output, &mut on_progress, &cancel_worker);
            let msg = match result {
                Ok((written, cancelled)) => ArchiveMsg::Finished {
                    is_create: true,
                    processed: written,
                    skipped: 0,
                    error: None,
                    cancelled,
                    label: name_owned,
                },
                Err(e) => ArchiveMsg::Finished {
                    is_create: true,
                    processed: 0,
                    skipped: 0,
                    error: Some(e.to_string()),
                    cancelled: false,
                    label: name_owned,
                },
            };
            let _ = tx.blocking_send(msg);
        });

        self.task_manager.add_archive(rx, true, cancel);
        self.status_message = format!("Creating {name} (see Tasks: Space j)...");
    }
}

/// Count archive entries selected by `filter` — best-effort progress denominator.
fn matching_entry_count(entries: &[ArchiveEntry], filter: Option<&str>) -> usize {
    match filter {
        None => entries.len(),
        Some(f) if f.ends_with('/') => entries.iter().filter(|e| e.path.starts_with(f)).count(),
        Some(f) => entries.iter().filter(|e| e.path == f).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::task_manager::TaskManager;

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

    #[tokio::test]
    async fn extract_all_requires_double_x() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let entries = vec![ArchiveEntry {
            path: "file.txt".into(),
            size: 10,
            is_dir: false,
            modified: None,
        }];
        let mut app = App::new_for_test(crate::app::make_test_entries(&["a.txt"]));
        app.archive_state = Some(ArchiveState::new(
            PathBuf::from("test.zip"),
            ArchiveFormat::Zip,
            entries,
        ));
        app.mode = Mode::Archive;

        // First X arms the confirmation but does not extract.
        app.handle_archive(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE));
        assert!(app.archive_state.as_ref().unwrap().confirm_extract_all);
        assert!(app.status_message.contains("press X again"));

        // Any other key cancels it.
        app.handle_archive(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(!app.archive_state.as_ref().unwrap().confirm_extract_all);
    }

    #[tokio::test]
    async fn extract_entry_spawns_task_and_conflict_channel() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let arch_entries = vec![ArchiveEntry {
            path: "f.txt".into(),
            size: 10,
            is_dir: false,
            modified: None,
        }];
        let mut app = App::new_for_test(crate::app::make_test_entries(&["a.txt"]));
        app.archive_state = Some(ArchiveState::new(
            PathBuf::from("/tmp/nonexistent.zip"),
            ArchiveFormat::Zip,
            arch_entries,
        ));
        app.mode = Mode::Archive;

        // 'x' extracts the selected entry as a streaming task.
        app.handle_archive(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert_eq!(app.task_manager.tasks().len(), 1);
        assert_eq!(TaskManager::kind_label(&app.task_manager.tasks()[0]), "Extract");
        // A conflict channel was queued so overwrite prompts can route through it.
        assert_eq!(app.conflict_rxs.len(), 1);
        assert!(app.archive_state.is_none());
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn create_archive_spawns_task() {
        let mut app = App::new_for_test(crate::app::make_test_entries(&["a.txt"]));
        app.active_panel_mut().selected = 1; // a.txt

        app.create_archive("out.zip", false);

        assert_eq!(app.task_manager.tasks().len(), 1);
        assert_eq!(TaskManager::kind_label(&app.task_manager.tasks()[0]), "Archive");
        assert!(app.status_message.contains("Creating out.zip"));
    }

    #[tokio::test]
    async fn create_archive_unknown_format_rejected() {
        let mut app = App::new_for_test(crate::app::make_test_entries(&["a.txt"]));
        app.active_panel_mut().selected = 1;
        app.create_archive("out.weird", false);
        assert!(app.task_manager.tasks().is_empty());
        assert!(app.status_message.contains("Unknown format"));
    }

    #[test]
    fn matching_entry_count_filters() {
        let entries = vec![
            ArchiveEntry { path: "a.txt".into(), size: 1, is_dir: false, modified: None },
            ArchiveEntry { path: "dir/".into(), size: 0, is_dir: true, modified: None },
            ArchiveEntry { path: "dir/b.txt".into(), size: 2, is_dir: false, modified: None },
        ];
        assert_eq!(matching_entry_count(&entries, None), 3);
        assert_eq!(matching_entry_count(&entries, Some("a.txt")), 1);
        assert_eq!(matching_entry_count(&entries, Some("dir/")), 2);
        assert_eq!(matching_entry_count(&entries, Some("missing")), 0);
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
