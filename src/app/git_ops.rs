//! Git working-tree operations: stage, unstage, and view a file's diff.
//!
//! These complement the read-only status indicators in [`super::git`]. Each
//! operation shells out to `git` off the UI thread and reports back through
//! `file_op_rx` (see [`super::App::apply_file_op`]). Stage/unstage trigger a
//! status refresh so the in-list icons update once the change lands.

use super::*;
use std::path::Path;
use std::time::Duration;

const GIT_OP_TIMEOUT: Duration = Duration::from_secs(10);

impl App {
    /// Stage the targeted file(s) with `git add`.
    pub(super) fn git_stage(&mut self) {
        self.run_git_stage(true);
    }

    /// Unstage the targeted file(s) with `git restore --staged`.
    pub(super) fn git_unstage(&mut self) {
        self.run_git_stage(false);
    }

    fn run_git_stage(&mut self, stage: bool) {
        let paths = self.active_panel().targeted_paths();
        if paths.is_empty() {
            self.status_message = "No file under cursor".into();
            return;
        }
        let dir = self.active_panel().path.clone();
        let count = paths.len();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);
        tokio::spawn(async move {
            let result = git_stage_paths(&dir, &paths, stage).await;
            let _ = tx.send(FileOpResult::GitStage {
                staged: stage,
                count,
                result,
            });
        });
    }

    /// Show `git diff` for the file under the cursor in the full-screen viewer.
    pub(super) fn git_diff(&mut self) {
        let entry = match self
            .active_panel()
            .selected_entry()
            .filter(|e| e.name != "..")
        {
            Some(e) => e,
            None => {
                self.status_message = "No file under cursor".into();
                return;
            }
        };
        let path = entry.path.clone();
        let title = entry.name.clone();
        let dir = self.active_panel().path.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);
        tokio::spawn(async move {
            let text = git_diff_text(&dir, &path).await;
            let _ = tx.send(FileOpResult::GitDiff { title, path, text });
        });
    }
}

/// Run `git` with the given args, capturing both stdout and stderr.
async fn run_git(args: &[String]) -> std::io::Result<std::process::Output> {
    let child = tokio::process::Command::new("git")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    match tokio::time::timeout(GIT_OP_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "git command timed out",
        )),
    }
}

async fn git_stage_paths(dir: &Path, paths: &[PathBuf], stage: bool) -> Result<(), String> {
    let mut args: Vec<String> = vec!["-C".into(), dir.to_string_lossy().into_owned()];
    if stage {
        args.push("add".into());
    } else {
        args.push("restore".into());
        args.push("--staged".into());
    }
    args.push("--".into());
    args.extend(paths.iter().map(|p| p.to_string_lossy().into_owned()));

    let output = run_git(&args).await.map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("git command failed")
            .to_string())
    }
}

/// Diff for a single file: worktree changes first, then staged changes if the
/// worktree is clean. Returns an empty string when there is nothing to show
/// (no changes, untracked file, or not a git repo).
async fn git_diff_text(dir: &Path, path: &Path) -> String {
    let dir_s = dir.to_string_lossy().into_owned();
    let path_s = path.to_string_lossy().into_owned();

    for staged in [false, true] {
        let mut args: Vec<String> = vec!["-C".into(), dir_s.clone(), "diff".into()];
        if staged {
            args.push("--staged".into());
        }
        args.push("--".into());
        args.push(path_s.clone());

        if let Ok(output) = run_git(&args).await
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            if !text.trim().is_empty() {
                return text;
            }
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stage_on_dotdot_is_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".."
        app.git_stage();
        assert!(app.file_op_rx.is_none());
        assert_eq!(app.status_message, "No file under cursor");
    }

    #[tokio::test]
    async fn diff_on_dotdot_is_noop() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".."
        app.git_diff();
        assert!(app.file_op_rx.is_none());
        assert_eq!(app.status_message, "No file under cursor");
    }

    #[tokio::test]
    async fn stage_on_file_spawns_op() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // "a.txt"
        app.git_stage();
        assert!(app.file_op_rx.is_some());
    }

    #[tokio::test]
    async fn apply_git_stage_ok_reports_count() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.apply_file_op(FileOpResult::GitStage {
            staged: true,
            count: 2,
            result: Ok(()),
        });
        assert_eq!(app.status_message, "Staged 2 item(s)");

        app.apply_file_op(FileOpResult::GitStage {
            staged: false,
            count: 1,
            result: Ok(()),
        });
        assert_eq!(app.status_message, "Unstaged 1 item(s)");
    }

    #[tokio::test]
    async fn apply_git_stage_err_reports_message() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.apply_file_op(FileOpResult::GitStage {
            staged: true,
            count: 1,
            result: Err("fatal: not a git repository".into()),
        });
        assert_eq!(app.status_message, "git: fatal: not a git repository");
    }

    #[tokio::test]
    async fn apply_git_diff_empty_shows_no_changes() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.apply_file_op(FileOpResult::GitDiff {
            title: "a.txt".into(),
            path: PathBuf::from("a.txt.diff"),
            text: String::new(),
        });
        assert!(app.viewer.is_none());
        assert_eq!(app.status_message, "No changes: a.txt");
    }

    #[tokio::test]
    async fn apply_git_diff_opens_viewer() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let diff = "diff --git a/a.txt b/a.txt\n@@ -1 +1 @@\n-old\n+new\n";
        app.apply_file_op(FileOpResult::GitDiff {
            title: "a.txt".into(),
            path: PathBuf::from("a.txt.diff"),
            text: diff.into(),
        });
        assert_eq!(app.mode, Mode::Viewer);
        let v = app.viewer.as_ref().expect("viewer opened");
        assert_eq!(v.content.lines.len(), 4);
        assert!(v.content.title.contains("git diff"));
    }
}
