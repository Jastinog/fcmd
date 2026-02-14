use super::*;

impl App {
    pub(super) fn yank_targeted(&mut self) {
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

    pub(super) fn request_delete(&mut self) {
        let paths = self.active_panel().targeted_paths();
        self.request_delete_paths(paths);
    }

    pub(super) fn request_delete_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        if paths.is_empty() {
            self.status_message = "Nothing to delete".into();
            return;
        }
        self.confirm_paths = paths;
        self.confirm_scroll = 0;
        self.mode = Mode::Confirm;
    }

    pub(super) fn execute_delete(&mut self) {
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

    pub(super) fn paste(&mut self, to_other_panel: bool) {
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

    pub(super) fn undo(&mut self) {
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

    pub(super) fn reload_active_panel(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let panel = match tab.active {
            PanelSide::Left => &mut tab.left,
            PanelSide::Right => &mut tab.right,
        };
        let _ = panel.load_dir_with_sizes(&self.dir_sizes);
    }

    pub(super) fn refresh_panels(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let _ = tab.left.load_dir_with_sizes(&self.dir_sizes);
        let _ = tab.right.load_dir_with_sizes(&self.dir_sizes);
        self.tree_dirty = true;
        self.git_checked_dirs = [None, None]; // force re-fetch
        self.refresh_git_status();
    }
}
