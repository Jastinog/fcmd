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

/// Parse a single line of `git status --porcelain=v1` into (status_char, relative_path).
fn parse_status_line(line: &str, root: &std::path::Path) -> Option<(char, PathBuf)> {
    if line.len() < 4 {
        return None;
    }
    let x = line.as_bytes()[0] as char;
    let y = line.as_bytes()[1] as char;
    let rel_path = &line[3..];
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

    Some((status, root.join(rel_path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn git_priority_ordering() {
        assert!(git_priority('D') > git_priority('M'));
        assert!(git_priority('M') > git_priority('A'));
        assert!(git_priority('A') > git_priority('R'));
        assert!(git_priority('R') > git_priority('?'));
        assert!(git_priority('?') > git_priority('\0'));
        assert_eq!(git_priority('X'), 0);
    }

    #[test]
    fn parse_modified_file() {
        let root = Path::new("/repo");
        let (status, path) = parse_status_line(" M src/main.rs", root).unwrap();
        assert_eq!(status, 'M');
        assert_eq!(path, PathBuf::from("/repo/src/main.rs"));
    }

    #[test]
    fn parse_added_file() {
        let root = Path::new("/repo");
        let (status, path) = parse_status_line("A  new_file.rs", root).unwrap();
        assert_eq!(status, 'A');
        assert_eq!(path, PathBuf::from("/repo/new_file.rs"));
    }

    #[test]
    fn parse_deleted_file() {
        let root = Path::new("/repo");
        let (status, path) = parse_status_line(" D old.rs", root).unwrap();
        assert_eq!(status, 'D');
        assert_eq!(path, PathBuf::from("/repo/old.rs"));
    }

    #[test]
    fn parse_untracked_file() {
        let root = Path::new("/repo");
        let (status, path) = parse_status_line("?? temp.txt", root).unwrap();
        assert_eq!(status, '?');
        assert_eq!(path, PathBuf::from("/repo/temp.txt"));
    }

    #[test]
    fn parse_rename_uses_new_path() {
        let root = Path::new("/repo");
        let (status, path) = parse_status_line("R  old.rs -> new.rs", root).unwrap();
        assert_eq!(status, 'R');
        assert_eq!(path, PathBuf::from("/repo/new.rs"));
    }

    #[test]
    fn parse_short_line_returns_none() {
        let root = Path::new("/repo");
        assert!(parse_status_line("XY", root).is_none());
        assert!(parse_status_line("", root).is_none());
    }

    #[test]
    fn parse_staged_modified() {
        let root = Path::new("/repo");
        let (status, _) = parse_status_line("M  staged.rs", root).unwrap();
        assert_eq!(status, 'M');
    }

    #[test]
    fn parse_staged_deleted() {
        let root = Path::new("/repo");
        let (status, _) = parse_status_line("D  removed.rs", root).unwrap();
        assert_eq!(status, 'D');
    }

    #[test]
    fn parent_propagation_priority() {
        // Simulate parent propagation: higher priority wins
        let mut statuses = HashMap::new();
        let parent = PathBuf::from("/repo/src");

        // First file: modified
        let existing = statuses.get(&parent).copied().unwrap_or('\0');
        if git_priority('M') > git_priority(existing) {
            statuses.insert(parent.clone(), 'M');
        }
        assert_eq!(statuses[&parent], 'M');

        // Second file: untracked — lower priority, should NOT override
        let existing = statuses.get(&parent).copied().unwrap_or('\0');
        if git_priority('?') > git_priority(existing) {
            statuses.insert(parent.clone(), '?');
        }
        assert_eq!(statuses[&parent], 'M');

        // Third file: deleted — higher priority, should override
        let existing = statuses.get(&parent).copied().unwrap_or('\0');
        if git_priority('D') > git_priority(existing) {
            statuses.insert(parent.clone(), 'D');
        }
        assert_eq!(statuses[&parent], 'D');
    }

    #[tokio::test]
    async fn ensure_git_status_skips_when_root_matches() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        // Set git root for all visible panels so no refresh is needed
        for i in 0..app.layout.count() {
            app.git_roots[i] = Some(PathBuf::from("/test"));
            app.git_checked_dirs[i] = Some(PathBuf::from("/test"));
        }
        app.ensure_git_status();
        assert!(app.git_progress.is_none());
    }

    #[tokio::test]
    async fn refresh_git_skips_if_already_in_progress() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.refresh_git_status();
        assert!(app.git_progress.is_some());
        // Second call should be a no-op
        app.git_checked_dirs = [None, None, None]; // reset to detect re-assignment
        app.refresh_git_status();
        // Should not have re-assigned checked_dirs since progress is in flight
        assert_eq!(app.git_checked_dirs, [None, None, None]);
    }
}
