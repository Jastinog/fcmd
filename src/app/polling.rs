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

        let mut needs_refresh = false;

        for event in events {
            match event {
                TaskEvent::PasteFinished { records, error, is_copy } => {
                    self.undo_stack.push(records);
                    if error.is_none() && !is_copy {
                        self.register = None;
                    }
                    needs_refresh = true;
                }
                TaskEvent::DeleteFinished => {
                    needs_refresh = true;
                }
            }
        }

        // Set notification from the last finished task if no running tasks remain
        if self.task_manager.active_count() == 0 {
            if let Some(task) = self.task_manager.tasks().last() {
                if let task_manager::TaskState::Finished { summary, .. } = &task.state {
                    self.task_notification = Some(summary.clone());
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
        self.poll_dir_sizes_load();

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

            // Save to DB (fire-and-forget)
            let sizes_clone = sizes.clone();
            self.db_spawn(move |db| { let _ = db.save_dir_sizes(&sizes_clone); });

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
                let statuses = self.git_statuses.clone();
                self.db_spawn(move |db| { let _ = db.save_git_statuses(&statuses); });
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.git_progress = None;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
        }
    }


    fn ensure_dir_sizes_loaded(&mut self) {
        if self.dir_sizes_load_rx.is_some() {
            return;
        }
        let Some(ref db) = self.db else { return };
        let dirs: Vec<PathBuf> = self
            .tab()
            .panels
            .iter()
            .map(|p| p.path.clone())
            .filter(|d| !self.dir_sizes_loaded.contains(d))
            .collect();
        if dirs.is_empty() {
            return;
        }
        for d in &dirs {
            self.dir_sizes_loaded.insert(d.clone());
        }
        let db = std::sync::Arc::clone(db);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.dir_sizes_load_rx = Some(rx);
        tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();
            if let Ok(db) = db.lock() {
                for dir in dirs {
                    if let Ok(sizes) = db.load_dir_sizes(&dir) {
                        results.push((dir, sizes));
                    }
                }
            }
            let _ = tx.send(results);
        });
    }

    pub(super) fn poll_dir_sizes_load(&mut self) {
        let Some(ref mut rx) = self.dir_sizes_load_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(results) => {
                for (_, sizes) in results {
                    self.dir_sizes.extend(sizes);
                }
                self.dir_sizes_load_rx = None;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.dir_sizes_load_rx = None;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn poll_find_noop_when_no_find_state() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(app.find_state.is_none());
        app.poll_find(); // should not panic
    }

    #[tokio::test]
    async fn poll_tasks_noop_when_empty() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.poll_tasks(); // should not panic
        assert!(app.task_notification.is_none());
    }

    #[tokio::test]
    async fn poll_du_noop_when_no_progress() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(app.du_progress.is_none());
        app.poll_du(); // should not panic
    }

    #[tokio::test]
    async fn poll_git_noop_when_no_progress() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(app.git_progress.is_none());
        app.poll_git(); // should not panic
    }

    #[tokio::test]
    async fn poll_git_handles_closed_channel() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let (tx, rx) = tokio::sync::oneshot::channel::<GitMsg>();
        drop(tx); // close channel
        app.git_progress = Some(GitProgress { rx });
        app.poll_git();
        assert!(app.git_progress.is_none());
    }

    #[tokio::test]
    async fn poll_git_receives_finished() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.git_progress = Some(GitProgress { rx });

        let mut statuses = HashMap::new();
        statuses.insert(PathBuf::from("/repo/file.rs"), 'M');
        let roots = [Some(PathBuf::from("/repo")), None, None];
        let checked = [Some(PathBuf::from("/test")), Some(PathBuf::from("/test")), Some(PathBuf::from("/test"))];
        tx.send(GitMsg::Finished {
            statuses: statuses.clone(),
            roots: roots.clone(),
            checked_dirs: checked.clone(),
        }).unwrap();

        app.poll_git();
        assert!(app.git_progress.is_none());
        assert_eq!(app.git_statuses.get(&PathBuf::from("/repo/file.rs")), Some(&'M'));
        assert_eq!(app.git_roots, roots);
    }

    #[tokio::test]
    async fn start_du_no_dirs_shows_message() {
        // Only ".." entry — no subdirectories
        let entries = make_test_entries(&["file.txt"]);
        let mut app = App::new_for_test(entries);
        app.start_du();
        assert!(app.du_progress.is_none());
        assert!(app.status_message.contains("No subdirectories"));
    }

    #[tokio::test]
    async fn start_du_already_running_shows_message() {
        let entries = make_test_entries(&["subdir/"]);
        let mut app = App::new_for_test(entries);
        // Start first DU
        app.start_du();
        assert!(app.du_progress.is_some());
        // Second start should show "already in progress"
        app.start_du();
        assert!(app.status_message.contains("already in progress"));
    }

    #[tokio::test]
    async fn poll_du_receives_finished() {
        let entries = make_test_entries(&["subdir/"]);
        let mut app = App::new_for_test(entries);
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        app.du_progress = Some(DuProgress {
            rx,
            started_at: Instant::now(),
        });

        tx.send(DuMsg::Finished {
            sizes: vec![(PathBuf::from("/test/subdir"), 4096)],
        }).await.unwrap();

        app.poll_du();
        assert!(app.du_progress.is_none());
        assert_eq!(app.dir_sizes.get(&PathBuf::from("/test/subdir")), Some(&4096));
        assert!(app.status_message.contains("measured"));
    }

    #[tokio::test]
    async fn poll_du_progress_updates_status() {
        let entries = make_test_entries(&["subdir/"]);
        let mut app = App::new_for_test(entries);
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        app.du_progress = Some(DuProgress {
            rx,
            started_at: Instant::now(),
        });

        tx.send(DuMsg::Progress {
            done: 0,
            total: 3,
            current: "subdir".into(),
        }).await.unwrap();

        // Don't finish yet — just progress
        app.poll_du();
        assert!(app.du_progress.is_some());
        assert!(app.status_message.contains("Calculating sizes"));
    }

    #[tokio::test]
    async fn poll_dir_sizes_load_noop_when_none() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert!(app.dir_sizes_load_rx.is_none());
        app.poll_dir_sizes_load(); // should not panic
    }

    #[tokio::test]
    async fn poll_dir_sizes_load_receives_data() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dir_sizes_load_rx = Some(rx);

        let mut sizes = HashMap::new();
        sizes.insert(PathBuf::from("/test/subdir"), 1024u64);
        tx.send(vec![(PathBuf::from("/test"), sizes)]).unwrap();

        app.poll_dir_sizes_load();
        assert!(app.dir_sizes_load_rx.is_none());
        assert_eq!(app.dir_sizes.get(&PathBuf::from("/test/subdir")), Some(&1024));
    }
}
