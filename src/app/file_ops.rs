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

        self.delete_progress = Some(DeleteProgress {
            rx,
            permanent,
        });
        self.mode = Mode::Normal;
        self.status_message = format!("Deleting {total} item(s)...");
    }

    pub(super) fn paste(&mut self, to_other_panel: bool) {
        if self.paste_progress.is_some() {
            self.status_message = "Operation in progress".into();
            return;
        }

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
