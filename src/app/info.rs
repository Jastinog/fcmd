use super::*;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

impl App {
    pub(super) fn enter_info(&mut self) {
        let entry = match self.active_panel().selected_entry().filter(|e| e.name != "..") {
            Some(e) => e,
            None => return,
        };

        let path = entry.path.clone();
        let is_dir = entry.is_dir;
        let mut lines: Vec<(String, String)> = Vec::new();

        lines.push(("Name".into(), entry.name.clone()));

        // Type
        let type_str = if entry.is_symlink {
            let target = std::fs::read_link(&path)
                .map(|t| t.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "?".into());
            format!("Symlink -> {target}")
        } else if is_dir {
            "Directory".into()
        } else {
            "File".into()
        };
        lines.push(("Type".into(), type_str));

        // Full path
        let abs_path = std::fs::canonicalize(&path)
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .into_owned();
        lines.push(("Path".into(), abs_path));

        // Metadata
        let meta = std::fs::metadata(&path).ok();
        let symlink_meta = std::fs::symlink_metadata(&path).ok();

        // Size
        if is_dir {
            // Immediate children count
            if let Ok(rd) = std::fs::read_dir(&path) {
                let count = rd.count();
                lines.push(("Items".into(), format!("{count}")));
            }
            // Placeholder for background calculation
            lines.push(("Size".into(), "Calculating...".into()));
            lines.push(("Files".into(), "Calculating...".into()));
            lines.push(("Subdirs".into(), "Calculating...".into()));

            // Spawn background thread
            let bg_path = path.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let (size, files, dirs) = ops::dir_stats(&bg_path);
                let _ = tx.send((size, files, dirs));
            });
            self.info_du_rx = Some(rx);
        } else if let Some(ref m) = meta {
            lines.push(("Size".into(), format_size_detailed(m.len())));
        }

        // Extension (files only)
        if !is_dir
            && let Some(ext) = path.extension()
        {
            lines.push(("Extension".into(), ext.to_string_lossy().into_owned()));
        }

        // Permissions
        #[cfg(unix)]
        if let Some(ref m) = meta {
            let mode = m.permissions().mode() & 0o7777;
            let rwx = crate::app::chmod::format_rwx(mode);
            lines.push(("Permissions".into(), format!("{rwx} ({mode:04o})")));
        }

        // Owner / Group
        #[cfg(unix)]
        if let Some(ref m) = meta {
            let uid = m.uid();
            let gid = m.gid();
            let user_name = get_user_name(uid).unwrap_or_else(|| uid.to_string());
            let group_name = get_group_name(gid).unwrap_or_else(|| gid.to_string());
            lines.push(("Owner".into(), format!("{user_name} ({uid})")));
            lines.push(("Group".into(), format!("{group_name} ({gid})")));
        }

        // Timestamps
        if let Some(ref m) = meta {
            if let Ok(modified) = m.modified() {
                lines.push(("Modified".into(), format_datetime(modified)));
            }
            if let Ok(created) = m.created() {
                lines.push(("Created".into(), format_datetime(created)));
            }
            if let Ok(accessed) = m.accessed() {
                lines.push(("Accessed".into(), format_datetime(accessed)));
            }
        }

        // Inode, device, hard links (Unix)
        #[cfg(unix)]
        if let Some(m) = symlink_meta.as_ref().or(meta.as_ref()) {
            lines.push(("Inode".into(), format!("{}", m.ino())));
            lines.push(("Hard links".into(), format!("{}", m.nlink())));
            lines.push(("Device".into(), format!("{}", m.dev())));
        }

        // Git status
        if let Some(&status) = self.git_statuses.get(&path) {
            let desc = match status {
                'M' => "Modified",
                'A' => "Added",
                'D' => "Deleted",
                'R' => "Renamed",
                'C' => "Copied",
                '?' => "Untracked",
                '!' => "Ignored",
                _ => "Unknown",
            };
            lines.push(("Git".into(), format!("{status} ({desc})")));
        }

        self.info_lines = lines;
        self.info_scroll = 0;
        self.mode = Mode::Info;
    }

    pub fn poll_info_du(&mut self) {
        let rx = match self.info_du_rx.as_ref() {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok((size, files, dirs)) => {
                // Update the placeholder lines
                for (k, v) in &mut self.info_lines {
                    if k == "Size" && v == "Calculating..." {
                        *v = format_size_detailed(size);
                    } else if k == "Files" && v == "Calculating..." {
                        *v = format!("{files}");
                    } else if k == "Subdirs" && v == "Calculating..." {
                        *v = format!("{dirs}");
                    }
                }
                self.info_du_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Thread died, remove placeholders
                for (_, v) in &mut self.info_lines {
                    if v == "Calculating..." {
                        *v = "Error".into();
                    }
                }
                self.info_du_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(super) fn handle_info(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('i') => {
                self.info_lines.clear();
                self.info_du_rx = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let max = self.info_lines.len().saturating_sub(1);
                self.info_scroll = (self.info_scroll + 1).min(max);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.info_scroll = self.info_scroll.saturating_sub(1);
            }
            KeyCode::Char('d') if ctrl => {
                let half = self.visible_height / 2;
                let max = self.info_lines.len().saturating_sub(1);
                self.info_scroll = (self.info_scroll + half).min(max);
            }
            KeyCode::Char('u') if ctrl => {
                let half = self.visible_height / 2;
                self.info_scroll = self.info_scroll.saturating_sub(half);
            }
            _ => {}
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_size_detailed(bytes: u64) -> String {
    let human = format_size(bytes);
    if bytes >= 1024 {
        format!("{human} ({bytes} bytes)")
    } else {
        human
    }
}

fn format_datetime(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Local, Utc};
    let dt: DateTime<Local> = DateTime::<Utc>::from(time).into();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(unix)]
fn get_user_name(uid: u32) -> Option<String> {
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr((*pw).pw_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(not(unix))]
fn get_user_name(_uid: u32) -> Option<String> {
    None
}

#[cfg(unix)]
fn get_group_name(gid: u32) -> Option<String> {
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr((*gr).gr_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(not(unix))]
fn get_group_name(_gid: u32) -> Option<String> {
    None
}
