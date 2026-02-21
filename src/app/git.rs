use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::*;

impl App {
    /// Check whether git status needs refreshing for either panel.
    pub(super) fn ensure_git_status(&mut self) {
        let tab = &self.tabs[self.active_tab];
        let needs_refresh = [&tab.left.path, &tab.right.path]
            .iter()
            .enumerate()
            .any(|(i, dir)| {
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

    /// Spawn a background task to fetch git status for both panels.
    pub fn refresh_git_status(&mut self) {
        // Skip if a git fetch is already in progress
        if self.git_progress.is_some() {
            return;
        }

        let tab = &self.tabs[self.active_tab];
        let dirs = [tab.left.path.clone(), tab.right.path.clone()];

        // Mark checked dirs immediately to prevent re-spawning on every keypress
        self.git_checked_dirs = [Some(dirs[0].clone()), Some(dirs[1].clone())];

        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::task::spawn_blocking(move || {
            let (statuses, roots, checked_dirs) = compute_git_status(dirs);
            let _ = tx.send(GitMsg::Finished {
                statuses,
                roots,
                checked_dirs,
            });
        });

        self.git_progress = Some(GitProgress { rx });
    }
}

type GitResult = (HashMap<PathBuf, char>, [Option<PathBuf>; 2], [Option<PathBuf>; 2]);

/// Compute git status for both panel directories (runs on any thread).
fn compute_git_status(dirs: [PathBuf; 2]) -> GitResult {
    let mut statuses = HashMap::new();
    let mut roots: [Option<PathBuf>; 2] = [None, None];
    let checked_dirs = [Some(dirs[0].clone()), Some(dirs[1].clone())];

    let mut seen_roots = HashSet::new();

    for (i, dir) in dirs.iter().enumerate() {
        let root_output = run_git_command(&[
            "-C",
            &dir.to_string_lossy(),
            "rev-parse",
            "--show-toplevel",
        ]);
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
        ]);
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

use std::time::{Duration, Instant};

const GIT_TIMEOUT: Duration = Duration::from_secs(5);

fn run_git_command(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut child = std::process::Command::new("git")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    // Read stdout in a background thread to avoid pipe deadlock
    let stdout_pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout_pipe {
            let _ = std::io::Read::read_to_end(&mut out, &mut buf);
        }
        buf
    });

    // Poll for completion with timeout
    let deadline = Instant::now() + GIT_TIMEOUT;
    loop {
        match child.try_wait()? {
            Some(status) => {
                let stdout = reader.join().unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr: Vec::new(),
                });
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "git command timed out",
                ));
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
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
