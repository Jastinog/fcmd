use super::*;
use super::task_manager::TaskEvent;
use crate::util::format_bytes;

impl App {
    pub fn poll_find(&mut self) {
        if let Some(ref mut fs) = self.find_state {
            fs.poll_entries();
            fs.update_find_preview(self.visible_height);
            fs.poll_find_preview();
        }
    }

    pub fn poll_tasks(&mut self) {
        let events = self.task_manager.poll_all();

        // Update status bar with latest running task progress
        if let Some(status) = self.task_manager.status_line() {
            self.status_message = status;
        }

        let mut needs_refresh = false;

        for event in events {
            match event {
                TaskEvent::PasteFinished { records, error } => {
                    self.undo_stack.push(records);
                    if error.is_none() {
                        self.register = None;
                    }
                    needs_refresh = true;
                }
                TaskEvent::DeleteFinished => {
                    needs_refresh = true;
                }
            }
        }

        // Set status from the last finished task if no running tasks remain
        if self.task_manager.active_count() == 0 {
            // Show the last finished task's summary
            if let Some(task) = self.task_manager.tasks().last() {
                if let task_manager::TaskState::Finished { summary, .. } = &task.state {
                    self.status_message = summary.clone();
                }
            }
            self.task_manager.remove_finished();
        }

        if needs_refresh {
            self.refresh_panels();
        }
    }

    pub(super) fn start_du(&mut self) {
        if self.du_progress.is_some() {
            self.status_message = "Directory size calculation already in progress".into();
            return;
        }
        let panel = self.active_panel();
        let dirs: Vec<PathBuf> = panel
            .entries
            .iter()
            .filter(|e| e.is_dir && e.name != "..")
            .map(|e| e.path.clone())
            .collect();
        if dirs.is_empty() {
            self.status_message = "No subdirectories to measure".into();
            return;
        }
        let n = dirs.len();
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        ops::du_in_background(dirs, tx);
        self.du_progress = Some(DuProgress {
            rx,
            started_at: Instant::now(),
        });
        self.status_message = format!("Calculating sizes for {n} directories...");
    }

    pub fn poll_du(&mut self) {
        self.ensure_dir_sizes_loaded();

        let progress = match self.du_progress.as_mut() {
            Some(p) => p,
            None => return,
        };

        let mut last_progress = None;
        let mut finished = None;

        loop {
            match progress.rx.try_recv() {
                Ok(msg @ DuMsg::Progress { .. }) => {
                    last_progress = Some(msg);
                }
                Ok(msg @ DuMsg::Finished { .. }) => {
                    finished = Some(msg);
                    break;
                }
                Err(_) => break,
            }
        }

        if let Some(DuMsg::Progress {
            done,
            total,
            current,
        }) = last_progress
        {
            self.status_message =
                format!("Calculating sizes... [{}/{}] {current}", done + 1, total);
        }

        if let Some(DuMsg::Finished { sizes }) = finished {
            let elapsed = self
                .du_progress
                .as_ref()
                .map(|p| p.started_at.elapsed())
                .unwrap_or_default();
            let count = sizes.len();
            let total: u64 = sizes.iter().map(|(_, s)| s).sum();

            // Update in-memory cache
            for &(ref path, size) in &sizes {
                self.dir_sizes.insert(path.clone(), size);
            }

            // Save to DB
            if let Some(ref db) = self.db
                && let Err(e) = db.save_dir_sizes(&sizes)
            {
                self.status_message = format!("Sizes calculated but DB save failed: {e}");
                self.du_progress = None;
                return;
            }

            let secs = elapsed.as_secs_f64();
            let total_str = format_bytes(total);
            self.status_message = format!("{count} dirs measured: {total_str} total ({secs:.1}s)");
            self.du_progress = None;
        }
    }

    pub fn poll_git(&mut self) {
        let progress = match self.git_progress.as_mut() {
            Some(p) => p,
            None => return,
        };

        match progress.rx.try_recv() {
            Ok(GitMsg::Finished {
                statuses,
                roots,
                checked_dirs,
            }) => {
                // Clear stale entries for repos we just re-fetched, then merge
                // fresh data. Statuses from other repos are preserved so icons
                // remain visible when navigating back.
                for root in roots.iter().flatten() {
                    self.git_statuses.retain(|path, _| !path.starts_with(root));
                }
                self.git_statuses.extend(statuses);
                self.git_roots = roots;
                self.git_checked_dirs = checked_dirs;
                self.git_progress = None;
                if let Some(ref db) = self.db {
                    let _ = db.save_git_statuses(&self.git_statuses);
                }
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.git_progress = None;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
        }
    }


    fn ensure_dir_sizes_loaded(&mut self) {
        let Some(ref db) = self.db else { return };
        let dirs: Vec<PathBuf> = self.tab().panels.iter().map(|p| p.path.clone()).collect();
        for dir in dirs {
            if !self.dir_sizes_loaded.contains(&dir) {
                if let Ok(sizes) = db.load_dir_sizes(&dir) {
                    self.dir_sizes.extend(sizes);
                }
                self.dir_sizes_loaded.insert(dir);
            }
        }
    }
}
