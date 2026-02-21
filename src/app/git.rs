use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use super::*;

impl App {
    /// Check whether git status needs refreshing for any visible panel.
    pub(super) fn ensure_git_status(&mut self) {
        let tab = &self.tabs[self.active_tab];
        let count = self.layout.count();
        let needs_refresh = (0..count).any(|i| {
            let dir = &tab.panels[i].path;
            if let Some(ref root) = self.git_roots[i]
                && dir.starts_with(root)
            {
                return false;
            }
            self.git_checked_dirs[i].as_deref() != Some(dir)
        });
        if needs_refresh {
            self.refresh_git_status();
        }
    }

    /// Spawn a background task to fetch git status for visible panels.
    pub fn refresh_git_status(&mut self) {
        // Skip if a git fetch is already in progress
        if self.git_progress.is_some() {
            return;
        }

        let tab = &self.tabs[self.active_tab];
        let dirs = [
            tab.panels[0].path.clone(),
            tab.panels[1].path.clone(),
            tab.panels[2].path.clone(),
        ];

        // Mark checked dirs immediately to prevent re-spawning on every keypress
        self.git_checked_dirs = [
            Some(dirs[0].clone()),
            Some(dirs[1].clone()),
            Some(dirs[2].clone()),
        ];

        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (statuses, roots, checked_dirs) = compute_git_status(dirs).await;
            let _ = tx.send(GitMsg::Finished {
                statuses,
                roots,
                checked_dirs,
            });
        });

        self.git_progress = Some(GitProgress { rx });
    }
}

type GitResult = (HashMap<PathBuf, char>, [Option<PathBuf>; 3], [Option<PathBuf>; 3]);

const GIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Compute git status for panel directories.
async fn compute_git_status(dirs: [PathBuf; 3]) -> GitResult {
    let mut statuses = HashMap::new();
    let mut roots: [Option<PathBuf>; 3] = [None, None, None];
    let checked_dirs = [
        Some(dirs[0].clone()),
        Some(dirs[1].clone()),
        Some(dirs[2].clone()),
    ];

    let mut seen_roots = HashSet::new();

    for (i, dir) in dirs.iter().enumerate() {
        let root_output = run_git_command(&[
            "-C",
            &dir.to_string_lossy(),
            "rev-parse",
            "--show-toplevel",
        ])
        .await;
        let root = match root_output {
            Ok(o) if o.status.success() => {
                PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())
            }
            _ => continue,
        };

        roots[i] = Some(root.clone());

        // Skip if we already fetched status for this repo root
        if !seen_roots.insert(root.clone()) {
            continue;
        }

        let status_output = run_git_command(&[
            "-C",
            &root.to_string_lossy(),
            "status",
            "--porcelain=v1",
            "-unormal",
        ])
        .await;
        let output = match status_output {
            Ok(o) if o.status.success() => o,
            _ => continue,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }
            let x = line.as_bytes()[0] as char;
            let y = line.as_bytes()[1] as char;
            let rel_path = &line[3..];
            // For renames: "R  old -> new", use the new path
            let rel_path = if let Some(pos) = rel_path.find(" -> ") {
                &rel_path[pos + 4..]
            } else {
                rel_path
            };

            let status = match (x, y) {
                ('?', '?') => '?',
                (_, 'M') => 'M',
                (_, 'D') => 'D',
                ('M', _) => 'M',
                ('A', _) => 'A',
                ('R', _) => 'R',
                ('D', _) => 'D',
                _ => 'M',
            };

            let abs_path = root.join(rel_path);
            statuses.insert(abs_path.clone(), status);

            // Propagate to parent directories up to root
            let mut parent = abs_path.parent();
            while let Some(p) = parent {
                if !p.starts_with(&root) || p == root {
                    break;
                }
                let existing = statuses.get(p).copied().unwrap_or('\0');
                if git_priority(status) > git_priority(existing) {
                    statuses.insert(p.to_path_buf(), status);
                }
                parent = p.parent();
            }
        }
    }

    (statuses, roots, checked_dirs)
}

async fn run_git_command(args: &[&str]) -> std::io::Result<std::process::Output> {
    let child = tokio::process::Command::new("git")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    match tokio::time::timeout(GIT_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "git command timed out",
        )),
    }
}

// Priority ordering for aggregation: D > M > A > R > ?
fn git_priority(c: char) -> u8 {
    match c {
        'D' => 5,
        'M' => 4,
        'A' => 3,
        'R' => 2,
        '?' => 1,
        _ => 0,
    }
}
