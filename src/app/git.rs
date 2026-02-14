use super::*;

impl App {
    pub(super) fn ensure_git_status(&mut self) {
        let dir = self.active_panel().path.clone();

        // Check if current dir is inside the cached git root — statuses still valid
        if let Some(ref root) = self.git_root
            && dir.starts_with(root)
        {
            return;
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
        let root_output =
            run_git_command(&["-C", &dir.to_string_lossy(), "rev-parse", "--show-toplevel"]);
        let root = match root_output {
            Ok(o) if o.status.success() => PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()),
            _ => return,
        };
        self.git_root = Some(root.clone());

        // Get porcelain status
        let status_output = run_git_command(&[
            "-C",
            &root.to_string_lossy(),
            "status",
            "--porcelain=v1",
            "-uall",
        ]);
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
