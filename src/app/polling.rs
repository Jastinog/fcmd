use super::*;
use crate::util::{format_bytes, format_duration, progress_bar};

impl App {
    pub fn poll_find(&mut self) {
        if let Some(ref mut fs) = self.find_state {
            fs.poll_entries();
            fs.update_find_preview();
        }
    }

    pub fn poll_progress(&mut self) {
        let progress = match self.paste_progress.as_mut() {
            Some(p) => p,
            None => return,
        };

        // Drain all pending messages, keep the last Progress for display
        let mut last_progress = None;
        let mut finished = None;

        loop {
            match progress.rx.try_recv() {
                Ok(msg @ ProgressMsg::Progress { .. }) => {
                    last_progress = Some(msg);
                }
                Ok(msg @ ProgressMsg::Finished { .. }) => {
                    finished = Some(msg);
                    break;
                }
                Err(_) => break,
            }
        }

        // Update status from latest progress
        if let Some(ProgressMsg::Progress {
            bytes_done,
            bytes_total,
            item_index,
            item_total,
        }) = last_progress
        {
            let verb = if progress.op == RegisterOp::Yank {
                "Copying"
            } else {
                "Moving"
            };

            let pct = if bytes_total > 0 {
                (bytes_done as f64 / bytes_total as f64 * 100.0) as u8
            } else {
                0
            };

            let bar = progress_bar(pct, 20);

            let elapsed = progress.started_at.elapsed();
            let eta = if bytes_done > 0 && bytes_total > bytes_done {
                let rate = bytes_done as f64 / elapsed.as_secs_f64();
                let remaining_bytes = bytes_total - bytes_done;
                let eta_secs = remaining_bytes as f64 / rate;
                format!(
                    " ETA {}",
                    format_duration(std::time::Duration::from_secs_f64(eta_secs))
                )
            } else {
                String::new()
            };

            let size_text = format!(
                "{}/{}",
                format_bytes(bytes_done),
                format_bytes(bytes_total)
            );

            self.status_message = format!(
                "{verb} {bar} {pct}% ({size_text}){eta} [{}/{}]",
                item_index + 1,
                item_total,
            );
        }

        // Handle finish
        if let Some(ProgressMsg::Finished {
            records,
            error,
            bytes_total,
        }) = finished
        {
            let n = records.len();
            let op = progress.op;
            let elapsed = progress.started_at.elapsed();
            self.undo_stack.push(records);

            if let Some(err) = error {
                self.status_message = format!("Paste error: {err}");
            } else {
                let verb = if op == RegisterOp::Yank {
                    "Copied"
                } else {
                    "Moved"
                };
                let dur = format_duration(elapsed);
                let size = format_bytes(bytes_total);
                self.status_message =
                    format!("{verb} {n} item(s), {size} in {dur}");
            }

            if op == RegisterOp::Cut {
                self.register = None;
            }

            self.paste_progress = None;
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
        let (tx, rx) = mpsc::channel();
        ops::du_in_background(dirs, tx);
        self.du_progress = Some(DuProgress {
            rx,
            started_at: Instant::now(),
        });
        self.status_message = format!("Calculating sizes for {n} directories...");
    }

    pub fn poll_du(&mut self) {
        self.ensure_dir_sizes_loaded();

        let progress = match self.du_progress.as_ref() {
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
            let elapsed = self.du_progress.as_ref().unwrap().started_at.elapsed();
            let count = sizes.len();
            let total: u64 = sizes.iter().map(|(_, s)| s).sum();

            // Update in-memory cache
            for &(ref path, size) in &sizes {
                self.dir_sizes.insert(path.clone(), size);
            }

            // Save to DB
            if let Some(ref db) = self.db {
                if let Err(e) = db.save_dir_sizes(&sizes) {
                    self.status_message = format!("Sizes calculated but DB save failed: {e}");
                    self.du_progress = None;
                    return;
                }
            }

            let secs = elapsed.as_secs_f64();
            let total_str = format_bytes(total);
            self.status_message =
                format!("{count} dirs measured: {total_str} total ({secs:.1}s)");
            self.du_progress = None;
        }
    }

    fn ensure_dir_sizes_loaded(&mut self) {
        let Some(ref db) = self.db else { return };
        let left = self.tab().left.path.clone();
        let right = self.tab().right.path.clone();
        for dir in [left, right] {
            if !self.dir_sizes_loaded.contains(&dir) {
                if let Ok(sizes) = db.load_dir_sizes(&dir) {
                    self.dir_sizes.extend(sizes);
                }
                self.dir_sizes_loaded.insert(dir);
            }
        }
    }
}
