use super::*;

impl App {
    pub(super) fn ensure_git_status(&mut self) {
        let dir = self.active_panel().path.clone();

        // Check if current dir is inside the cached git root — statuses still valid
        if let Some(ref root) = self.git_root {
            if dir.starts_with(root) {
                return;
            }
        }
        // Different repo or not cached yet — need to check
        if self.git_status_dir.as_ref() == Some(&dir) {
            return; // already checked this dir, it's not a git repo
        }
        self.refresh_git_status();
    }

    pub fn refresh_git_status(&mut self) {
        self.git_statuses.clear();
        self.git_root = None;

        let dir = self.active_panel().path.clone();
        self.git_status_dir = Some(dir.clone());

        // Detect git root
        let root_output = std::process::Command::new("git")
            .args(["-C", &dir.to_string_lossy(), "rev-parse", "--show-toplevel"])
            .output();
        let root = match root_output {
            Ok(o) if o.status.success() => {
                PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())
            }
            _ => return,
        };
        self.git_root = Some(root.clone());

        // Get porcelain status
        let status_output = std::process::Command::new("git")
            .args(["-C", &root.to_string_lossy(), "status", "--porcelain=v1", "-uall"])
            .output();
        let output = match status_output {
            Ok(o) if o.status.success() => o,
            _ => return,
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

            // Determine the status char: prefer worktree (Y), fallback to index (X)
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
            self.git_statuses.insert(abs_path.clone(), status);

            // Propagate to parent directories up to root
            let mut parent = abs_path.parent();
            while let Some(p) = parent {
                if !p.starts_with(&root) || p == root {
                    break;
                }
                let existing = self.git_statuses.get(p).copied().unwrap_or('\0');
                if git_priority(status) > git_priority(existing) {
                    self.git_statuses.insert(p.to_path_buf(), status);
                }
                parent = p.parent();
            }
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
