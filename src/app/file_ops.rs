use super::*;

impl App {
    pub(super) fn yank_targeted(&mut self) {
        let entries = self.active_panel().targeted_register_entries();
        if entries.is_empty() {
            self.status_message = "Nothing to yank".into();
            return;
        }
        let n = entries.len();
        self.register = Some(Register {
            entries,
            op: RegisterOp::Yank,
        });
        self.status_message = format!("Yanked {n} item(s)");
    }

    pub(super) fn request_delete(&mut self) {
        let items = self.targeted_path_types();
        self.confirm_permanent = false;
        self.request_delete_paths(items);
    }

    pub(super) fn request_permanent_delete(&mut self) {
        let items = self.targeted_path_types();
        self.confirm_permanent = true;
        self.request_delete_paths(items);
    }

    pub(super) fn request_delete_paths(&mut self, items: Vec<(std::path::PathBuf, bool)>) {
        if items.is_empty() {
            self.status_message = "Nothing to delete".into();
            return;
        }
        self.confirm_paths = items;
        self.confirm_scroll = 0;
        self.mode = Mode::Confirm;
    }

    pub(super) fn request_permanent_delete_paths(&mut self, items: Vec<(std::path::PathBuf, bool)>) {
        self.confirm_permanent = true;
        self.request_delete_paths(items);
    }

    pub(super) fn targeted_path_types(&self) -> Vec<(std::path::PathBuf, bool)> {
        self.active_panel()
            .targeted_register_entries()
            .into_iter()
            .map(|e| (e.path, e.is_dir))
            .collect()
    }

    pub(super) fn execute_delete(&mut self) {
        let items = std::mem::take(&mut self.confirm_paths);
        let permanent = self.confirm_permanent;
        let total = items.len();
        let paths: Vec<PathBuf> = items.into_iter().map(|(p, _)| p).collect();
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        tokio::task::spawn_blocking(move || {
            let mut deleted = 0usize;
            let mut errors = Vec::new();
            for (i, path) in paths.iter().enumerate() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let _ = tx.blocking_send(DeleteMsg::Progress {
                    done: i,
                    total,
                    current: name.clone(),
                });
                let result = if permanent {
                    ops::remove_path(path)
                } else {
                    trash::delete(path).map_err(std::io::Error::other)
                };
                match result {
                    Ok(()) => deleted += 1,
                    Err(e) => errors.push(format!("{name}: {e}")),
                }
            }
            let _ = tx.blocking_send(DeleteMsg::Finished {
                deleted,
                errors,
                permanent,
            });
        });

        self.task_manager.add_delete(rx, permanent);
        self.mode = Mode::Normal;
    }

    pub(super) fn copy_to_other_panel(&mut self) {
        if self.layout == PanelLayout::Single {
            self.status_message = "Cannot copy to same panel in single layout".into();
            return;
        }
        self.yank_targeted();
        self.paste(true);
    }

    pub(super) fn move_to_other_panel(&mut self) {
        if self.layout == PanelLayout::Single {
            self.status_message = "Cannot move to same panel in single layout".into();
            return;
        }
        let entries = self.active_panel().targeted_register_entries();
        if entries.is_empty() {
            self.status_message = "Nothing to move".into();
            return;
        }
        let n = entries.len();
        self.register = Some(Register {
            entries,
            op: RegisterOp::Cut,
        });
        self.status_message = format!("Moving {n} item(s)");
        self.paste(true);
    }

    pub(super) fn paste(&mut self, to_other_panel: bool) {
        let (reg_entries, op) = match &self.register {
            Some(r) => (r.entries.clone(), r.op),
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

        let phantoms: Vec<PhantomEntry> = reg_entries
            .iter()
            .map(|e| PhantomEntry {
                name: e
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                is_dir: e.is_dir,
            })
            .collect();

        let paths: Vec<PathBuf> = reg_entries.iter().map(|e| e.path.clone()).collect();
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        ops::paste_in_background(paths, dst_dir.clone(), op, tx);

        if op == RegisterOp::Yank {
            self.task_manager.add_copy(rx, dst_dir, phantoms);
        } else {
            self.task_manager.add_move(rx, dst_dir, phantoms);
        }
    }

    pub(super) fn undo(&mut self) {
        if let Some(records) = self.undo_stack.pop() {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.file_op_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let result = ops::undo(&records)
                    .map_err(|e| e.to_string());
                let _ = tx.send(super::FileOpResult::Undo { result });
            });
        } else {
            self.status_message = "Nothing to undo".into();
        }
    }

    pub fn reload_active_panel(&mut self) {
        let path = self.active_panel().path.clone();
        self.dir_cache.remove(&path);
        let side = self.tab().active;
        self.spawn_dir_load(side, None);
    }

    /// Like refresh_panels, but passes `select_name` to the active panel
    /// so the cursor moves to the named entry once the async load completes.
    pub(super) fn refresh_panels_select(&mut self, select_name: Option<String>) {
        let tab = &self.tabs[self.active_tab];
        let paths: Vec<PathBuf> = tab.panels.iter().flat_map(|p| {
            let mut v = vec![p.path.clone()];
            if let Some(parent) = p.path.parent() {
                v.push(parent.to_path_buf());
            }
            v
        }).collect();
        for p in paths {
            self.dir_cache.remove(&p);
        }
        let active = self.tab().active;
        for i in 0..3 {
            let sn = if i == active { select_name.clone() } else { None };
            self.spawn_dir_load(i, sn);
        }
        self.tree_dirty = true;
        self.git_checked_dirs = [None, None, None];
        self.refresh_git_status();
    }

    pub(super) fn refresh_panels(&mut self) {
        // Invalidate cache for all visible panel paths and their parents
        let tab = &self.tabs[self.active_tab];
        let paths: Vec<PathBuf> = tab.panels.iter().flat_map(|p| {
            let mut v = vec![p.path.clone()];
            if let Some(parent) = p.path.parent() {
                v.push(parent.to_path_buf());
            }
            v
        }).collect();
        for p in paths {
            self.dir_cache.remove(&p);
        }
        // Load all panels async
        for i in 0..3 {
            self.spawn_dir_load(i, None);
        }
        self.tree_dirty = true;
        self.git_checked_dirs = [None, None, None]; // force re-fetch
        self.refresh_git_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn targeted_path_types_returns_selected() {
        let entries = make_test_entries(&["a.txt", "subdir/"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // a.txt
        let paths = app.targeted_path_types();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, PathBuf::from("/test/a.txt"));
        assert!(!paths[0].1); // not a dir
    }

    #[tokio::test]
    async fn targeted_path_types_dir_entry() {
        let entries = make_test_entries(&["subdir/"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // subdir
        let paths = app.targeted_path_types();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].1); // is a dir
    }

    #[tokio::test]
    async fn request_delete_paths_empty_shows_message() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.request_delete_paths(vec![]);
        assert!(app.status_message.contains("Nothing to delete"));
        assert_ne!(app.mode, Mode::Confirm);
    }

    #[tokio::test]
    async fn request_permanent_delete_paths_sets_permanent() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let items = vec![(PathBuf::from("/test/a.txt"), false)];
        app.request_permanent_delete_paths(items);
        assert!(app.confirm_permanent);
        assert_eq!(app.mode, Mode::Confirm);
    }

    #[tokio::test]
    async fn execute_delete_spawns_task_and_returns_normal() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.confirm_paths = vec![(PathBuf::from("/tmp/nonexistent_test_file"), false)];
        app.confirm_permanent = true;
        app.mode = Mode::Confirm;
        app.execute_delete();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.confirm_paths.is_empty());
        // Task should be added
        assert_eq!(app.task_manager.tasks().len(), 1);
    }

    #[tokio::test]
    async fn yank_targeted_empty_shows_message() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".." — skipped
        app.yank_targeted();
        assert!(app.status_message.contains("Nothing to yank"));
    }

    #[tokio::test]
    async fn yank_targeted_with_file() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // a.txt
        app.yank_targeted();
        assert!(app.register.is_some());
        assert_eq!(app.register.as_ref().unwrap().entries.len(), 1);
        assert!(app.status_message.contains("Yanked 1"));
    }

    #[tokio::test]
    async fn copy_to_other_panel_dual_yanks_and_pastes() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Dual;
        app.active_panel_mut().selected = 1;
        app.copy_to_other_panel();
        // Register should be set
        assert!(app.register.is_some());
        // Task manager should have a task
        assert!(!app.task_manager.tasks().is_empty());
    }

    #[tokio::test]
    async fn move_to_other_panel_dual_with_file() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.layout = PanelLayout::Dual;
        app.active_panel_mut().selected = 1;
        app.move_to_other_panel();
        assert!(app.register.is_some());
        assert_eq!(app.register.as_ref().unwrap().op, RegisterOp::Cut);
        assert!(app.status_message.contains("Moving 1"));
    }

    #[tokio::test]
    async fn paste_to_current_panel() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.register = Some(Register {
            entries: vec![crate::ops::RegisterEntry {
                path: PathBuf::from("/test/a.txt"),
                is_dir: false,
            }],
            op: RegisterOp::Yank,
        });
        app.paste(false); // paste to current
        assert!(!app.task_manager.tasks().is_empty());
    }

    #[tokio::test]
    async fn undo_with_records_spawns_op() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.undo_stack.push(vec![crate::ops::OpRecord::Copied {
            _src: PathBuf::from("/test/src.txt"),
            dst: PathBuf::from("/tmp/undo_test_nonexistent"),
        }]);
        app.undo();
        // file_op_rx should be set
        assert!(app.file_op_rx.is_some());
    }

    #[tokio::test]
    async fn reload_active_panel_removes_cache() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let path = app.active_panel().path.clone();
        // Insert into cache
        app.dir_cache.insert(path.clone(), crate::panel::DirCacheEntry {
            entries: vec![],
            show_hidden: false,
            sort_mode: SortMode::Name,
            sort_reverse: false,
        });
        assert!(app.dir_cache.get(&path).is_some());
        app.reload_active_panel();
        assert!(app.dir_cache.get(&path).is_none());
    }

    #[tokio::test]
    async fn refresh_panels_sets_tree_dirty() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.tree_dirty = false;
        app.refresh_panels();
        assert!(app.tree_dirty);
    }

    #[tokio::test]
    async fn refresh_panels_select_sets_tree_dirty() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.tree_dirty = false;
        app.refresh_panels_select(Some("b.txt".into()));
        assert!(app.tree_dirty);
    }
}
