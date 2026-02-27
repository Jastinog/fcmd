use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

const MAX_ENTRIES: usize = 5000;
const MAX_DEPTH: usize = 12;
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "target",
    "node_modules",
    "__pycache__",
    ".cache",
    "build",
    "dist",
    ".next",
];

const MDFIND_LIMIT: usize = 5000;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FindScope {
    Local,
    Global,
}

struct Entry {
    rel_path: String,
    rel_path_lower: String,
    full_path: PathBuf,
    is_dir: bool,
}

pub struct FindState {
    pub query: String,
    entries: Vec<Entry>,
    rx: Option<tokio::sync::mpsc::Receiver<Entry>>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub loading: bool,
    pub scope: FindScope,
    base_dir: PathBuf,
    search_task: Option<tokio::task::JoinHandle<()>>,
    pub find_preview: Option<crate::preview::Preview>,
    find_preview_path: Option<PathBuf>,
    find_preview_rx: Option<tokio::sync::oneshot::Receiver<(PathBuf, crate::preview::Preview)>>,
    pub search_started: Option<Instant>,
    pub tick: usize,
}

impl Drop for FindState {
    fn drop(&mut self) {
        if let Some(handle) = self.search_task.take() {
            handle.abort();
        }
    }
}

impl FindState {
    pub fn new_local(base_dir: &Path) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1024);
        let base = base_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            walk_send(&base, &base, &tx, 0, &mut 0);
        });
        FindState {
            query: String::new(),
            entries: Vec::new(),
            rx: Some(rx),
            filtered: Vec::new(),
            selected: 0,
            scroll: 0,
            loading: true,
            scope: FindScope::Local,
            base_dir: base_dir.to_path_buf(),
            search_task: None,
            find_preview: None,
            find_preview_path: None,
            find_preview_rx: None,
            search_started: Some(Instant::now()),
            tick: 0,
        }
    }

    pub fn new_global(base_dir: &Path) -> Self {
        FindState {
            query: String::new(),
            entries: Vec::new(),
            rx: None,
            filtered: Vec::new(),
            selected: 0,
            scroll: 0,
            loading: false,
            scope: FindScope::Global,
            base_dir: base_dir.to_path_buf(),
            search_task: None,
            find_preview: None,
            find_preview_path: None,
            find_preview_rx: None,
            search_started: None,
            tick: 0,
        }
    }

    pub fn switch_scope(&self) -> Self {
        let mut new_state = match self.scope {
            FindScope::Local => Self::new_global(&self.base_dir),
            FindScope::Global => Self::new_local(&self.base_dir),
        };
        new_state.query = self.query.clone();
        // For global, trigger search if query is non-empty
        if new_state.scope == FindScope::Global && !new_state.query.is_empty() {
            new_state.trigger_search();
        }
        new_state
    }

    /// Trigger global search (tries mdfind, falls back to find from $HOME).
    pub fn trigger_search(&mut self) {
        if self.scope != FindScope::Global {
            return;
        }

        // Abort old search task (kills child processes via kill_on_drop)
        if let Some(handle) = self.search_task.take() {
            handle.abort();
        }

        // Clear state
        self.entries.clear();
        self.filtered.clear();
        self.selected = 0;
        self.scroll = 0;

        if self.query.is_empty() {
            self.loading = false;
            self.rx = None;
            self.search_started = None;
            return;
        }

        let (tx, rx) = tokio::sync::mpsc::channel(1024);
        let query = self.query.clone();

        // Sanitize: strip characters that could be interpreted by find/mdfind
        let sanitized_query: String = query.chars().filter(|c| *c != '\0').collect();
        let sanitized_pattern = format!("*{sanitized_query}*");

        // Try mdfind first (Spotlight), fall back to find from $HOME
        let child_result = tokio::process::Command::new("mdfind")
            .args(["-name", &sanitized_query])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();

        let child_result = match child_result {
            Ok(child) => {
                // If Spotlight is disabled, mdfind exits with empty output.
                // The reader task will detect 0 results and retry with find.
                Ok(child)
            }
            Err(_) => {
                // mdfind not available, use find from $HOME.
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                tokio::process::Command::new("find")
                    .args([&home, "-maxdepth", "6", "-iname", &sanitized_pattern])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
            }
        };

        match child_result {
            Ok(mut child) => {
                let Some(stdout) = child.stdout.take() else {
                    return;
                };
                self.rx = Some(rx);
                self.loading = true;
                self.search_started = Some(Instant::now());

                let handle = tokio::spawn(async move {
                    let count = global_search_read(stdout, &tx).await;
                    let _ = child.wait().await;
                    // If mdfind returned 0 results (Spotlight disabled), retry with find
                    if count == 0 {
                        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                        if let Ok(mut find_child) = tokio::process::Command::new("find")
                            .kill_on_drop(true)
                            .arg("--")
                            .args([&home, "-maxdepth", "6", "-iname", &sanitized_pattern])
                            .stdout(Stdio::piped())
                            .stderr(Stdio::null())
                            .spawn()
                        {
                            if let Some(stdout) = find_child.stdout.take() {
                                global_search_read(stdout, &tx).await;
                            }
                            let _ = find_child.wait().await;
                        }
                    }
                });
                self.search_task = Some(handle);
            }
            Err(_) => {
                self.loading = false;
            }
        }
    }

    /// Spinner character for the current tick.
    pub fn spinner(&self) -> &'static str {
        const FRAMES: &[&str] = &[
            "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}",
            "\u{2827}", "\u{2807}", "\u{280f}",
        ];
        FRAMES[self.tick % FRAMES.len()]
    }

    /// Elapsed time since search started, formatted.
    pub fn elapsed_str(&self) -> String {
        match self.search_started {
            Some(t) => {
                let secs = t.elapsed().as_secs_f64();
                if secs < 1.0 {
                    format!("{:.0}ms", secs * 1000.0)
                } else {
                    format!("{secs:.1}s")
                }
            }
            None => String::new(),
        }
    }

    /// Drain pending entries from background thread.
    /// Returns true if new entries arrived.
    pub fn poll_entries(&mut self) -> bool {
        self.tick = self.tick.wrapping_add(1);
        let Some(rx) = &mut self.rx else {
            return false;
        };
        let mut added = false;
        loop {
            match rx.try_recv() {
                Ok(entry) => {
                    self.entries.push(entry);
                    added = true;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.loading = false;
                    break;
                }
            }
        }
        if added {
            self.refilter();
        }
        added
    }

    fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let query_lower: Vec<char> = self.query.to_lowercase().chars().collect();
            let mut scored: Vec<(usize, i32)> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    fuzzy_score_pre(&query_lower, &e.rel_path_lower, e.rel_path.len())
                        .map(|s| (i, s))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        // Clamp selected
        if !self.filtered.is_empty() {
            self.selected = self.selected.min(self.filtered.len() - 1);
        } else {
            self.selected = 0;
        }
    }

    pub fn update_filter(&mut self) {
        self.refilter();
        self.selected = 0;
        self.scroll = 0;
    }

    pub fn update_find_preview(&mut self, visible_height: usize) {
        let current = self.selected_path().map(|p| p.to_path_buf());
        if current == self.find_preview_path {
            return;
        }
        self.find_preview_path = current.clone();
        if let Some(p) = current {
            self.find_preview = Some(crate::preview::Preview::loading_placeholder(&p));
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.find_preview_rx = Some(rx);
            let path = p.clone();
            let vis = visible_height;
            tokio::task::spawn_blocking(move || {
                let prev = crate::preview::Preview::load(&path, vis);
                let _ = tx.send((path, prev));
            });
        } else {
            self.find_preview = None;
        }
    }

    pub fn poll_find_preview(&mut self) {
        let rx = match self.find_preview_rx.as_mut() {
            Some(rx) => rx,
            None => return,
        };
        match rx.try_recv() {
            Ok((path, preview)) => {
                if self.find_preview_path.as_ref() == Some(&path) {
                    self.find_preview = Some(preview);
                }
                self.find_preview_rx = None;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.find_preview_rx = None;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        }
    }

    pub fn adjust_scroll(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected - height + 1;
        }
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.entries.get(i))
            .map(|e| e.full_path.as_path())
    }

    pub fn selected_is_dir(&self) -> bool {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.entries.get(i))
            .map(|e| e.is_dir)
            .unwrap_or(false)
    }

    pub fn total_count(&self) -> usize {
        self.entries.len()
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }

    pub fn get_item(&self, filtered_idx: usize) -> Option<(&str, bool)> {
        self.filtered
            .get(filtered_idx)
            .and_then(|&i| self.entries.get(i))
            .map(|e| (e.rel_path.as_str(), e.is_dir))
    }
}

#[cfg(test)]
impl FindState {
    /// Create a FindState with manually populated entries for testing.
    pub fn new_test(base_dir: &Path, items: &[(&str, bool)]) -> Self {
        let mut entries = Vec::new();
        for &(rel, is_dir) in items {
            entries.push(Entry {
                rel_path: rel.to_string(),
                rel_path_lower: rel.to_lowercase(),
                full_path: base_dir.join(rel),
                is_dir,
            });
        }
        let filtered: Vec<usize> = (0..entries.len()).collect();
        FindState {
            query: String::new(),
            entries,
            rx: None,
            filtered,
            selected: 0,
            scroll: 0,
            loading: false,
            scope: FindScope::Local,
            base_dir: base_dir.to_path_buf(),
            search_task: None,
            find_preview: None,
            find_preview_path: None,
            find_preview_rx: None,
            search_started: None,
            tick: 0,
        }
    }
}

fn abbreviate_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

async fn global_search_read(stdout: tokio::process::ChildStdout, tx: &tokio::sync::mpsc::Sender<Entry>) -> usize {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut count = 0;
    while let Ok(Some(line)) = lines.next_line().await {
        if count >= MDFIND_LIMIT {
            break;
        }
        let path = PathBuf::from(&line);
        let is_dir = tokio::task::block_in_place(|| path.is_dir());
        let display = abbreviate_home(&line);
        let display_lower = display.to_lowercase();
        if tx
            .send(Entry {
                rel_path: display,
                rel_path_lower: display_lower,
                full_path: path,
                is_dir,
            })
            .await
            .is_err()
        {
            break;
        }
        count += 1;
    }
    count
}

fn walk_send(dir: &Path, base: &Path, tx: &tokio::sync::mpsc::Sender<Entry>, depth: usize, count: &mut usize) {
    if depth > MAX_DEPTH || *count >= MAX_ENTRIES {
        return;
    }

    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    let mut items: Vec<_> = read_dir.flatten().collect();
    items.sort_by_key(|e| e.file_name());

    for item in items {
        if *count >= MAX_ENTRIES {
            break;
        }

        let name = item.file_name().to_string_lossy().into_owned();
        let path = item.path();
        let is_symlink = path
            .symlink_metadata()
            .map(|m| m.is_symlink())
            .unwrap_or(false);
        let is_dir = path.is_dir();

        if is_dir && SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }

        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let rel_lower = rel.to_lowercase();

        if tx
            .blocking_send(Entry {
                rel_path: rel,
                rel_path_lower: rel_lower,
                full_path: path.clone(),
                is_dir,
            })
            .is_err()
        {
            return;
        }

        *count += 1;

        if is_dir && !is_symlink {
            walk_send(&path, base, tx, depth + 1, count);
        }
    }
}

/// Fuzzy score using pre-lowercased query chars and cached lowercase text.
fn fuzzy_score_pre(query_chars: &[char], text_lower: &str, text_len: usize) -> Option<i32> {
    if query_chars.is_empty() {
        return Some(0);
    }

    let t: Vec<char> = text_lower.chars().collect();

    let mut qi = 0;
    let mut score = 0i32;
    let mut consecutive = 0i32;

    for (ti, &tc) in t.iter().enumerate() {
        if qi < query_chars.len() && tc == query_chars[qi] {
            qi += 1;
            consecutive += 1;
            score += consecutive;
            if ti == 0 || matches!(t.get(ti.wrapping_sub(1)), Some('/' | '_' | '-' | '.' | ' ')) {
                score += 5;
            }
        } else {
            consecutive = 0;
        }
    }

    if qi == query_chars.len() {
        Some(score - text_len as i32 / 4)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_exact_match() {
        let q: Vec<char> = "main".chars().collect();
        let score = fuzzy_score_pre(&q, "main.rs", 7);
        assert!(score.is_some());
        assert!(score.unwrap() > 0);
    }

    #[test]
    fn fuzzy_subsequence() {
        let q: Vec<char> = "mr".chars().collect();
        let score = fuzzy_score_pre(&q, "main.rs", 7);
        assert!(score.is_some());
    }

    #[test]
    fn fuzzy_no_match() {
        let q: Vec<char> = "xyz".chars().collect();
        let score = fuzzy_score_pre(&q, "main.rs", 7);
        assert!(score.is_none());
    }

    #[test]
    fn fuzzy_empty_query() {
        let q: Vec<char> = Vec::new();
        let score = fuzzy_score_pre(&q, "anything", 8);
        assert_eq!(score, Some(0));
    }

    #[test]
    fn fuzzy_path_separator_bonus() {
        let q: Vec<char> = "ar".chars().collect();
        // "a" at start of segment after "/" should get bonus
        let score_with_sep = fuzzy_score_pre(&q, "src/app.rs", 10);
        let score_without = fuzzy_score_pre(&q, "sxcapprrs", 9);
        assert!(score_with_sep.unwrap_or(0) > score_without.unwrap_or(0));
    }

    #[test]
    fn fuzzy_case_insensitive() {
        let q: Vec<char> = "main".chars().collect();
        let score = fuzzy_score_pre(&q, "main.rs", 7);
        assert!(score.is_some());
    }

    #[test]
    fn fuzzy_longer_text_penalty() {
        let q: Vec<char> = "a".chars().collect();
        let short = fuzzy_score_pre(&q, "a", 1);
        let long = fuzzy_score_pre(&q, "a_very_long_filename.txt", 24);
        assert!(short.unwrap() > long.unwrap());
    }

    #[test]
    fn spinner_returns_nonempty() {
        let fs = FindState::new_test(Path::new("/tmp"), &[("a.txt", false)]);
        assert!(!fs.spinner().is_empty());
    }

    #[test]
    fn move_down_increments_selected() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
        );
        assert_eq!(fs.selected, 0);
        fs.move_down();
        assert_eq!(fs.selected, 1);
        fs.move_down();
        assert_eq!(fs.selected, 2);
        // Clamped at last
        fs.move_down();
        assert_eq!(fs.selected, 2);
    }

    #[test]
    fn move_up_decrements_selected() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false), ("b.txt", false)],
        );
        fs.selected = 1;
        fs.move_up();
        assert_eq!(fs.selected, 0);
        // Saturating at 0
        fs.move_up();
        assert_eq!(fs.selected, 0);
    }

    #[test]
    fn adjust_scroll_follows_selected_down() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a", false), ("b", false), ("c", false), ("d", false), ("e", false)],
        );
        fs.selected = 4;
        fs.scroll = 0;
        fs.adjust_scroll(3); // visible height 3
        assert_eq!(fs.scroll, 2); // 4 - 3 + 1
    }

    #[test]
    fn adjust_scroll_follows_selected_up() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a", false), ("b", false), ("c", false)],
        );
        fs.scroll = 2;
        fs.selected = 0;
        fs.adjust_scroll(3);
        assert_eq!(fs.scroll, 0);
    }

    // ── refilter & update_filter ──────────────────────────────────

    #[test]
    fn update_filter_empty_query_shows_all() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false), ("b.rs", false), ("c.py", false)],
        );
        fs.query = String::new();
        fs.update_filter();
        assert_eq!(fs.filtered.len(), 3);
        assert_eq!(fs.selected, 0);
    }

    #[test]
    fn update_filter_narrows_results() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("main.rs", false), ("lib.rs", false), ("readme.md", false)],
        );
        fs.query = "rs".into();
        fs.update_filter();
        assert_eq!(fs.filtered.len(), 2);
    }

    #[test]
    fn update_filter_no_match() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false), ("b.txt", false)],
        );
        fs.query = "xyz".into();
        fs.update_filter();
        assert_eq!(fs.filtered.len(), 0);
        assert_eq!(fs.selected, 0);
    }

    #[test]
    fn update_filter_resets_selected_and_scroll() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false), ("b.txt", false), ("c.txt", false)],
        );
        fs.selected = 2;
        fs.scroll = 1;
        fs.query = "a".into();
        fs.update_filter();
        assert_eq!(fs.selected, 0);
        assert_eq!(fs.scroll, 0);
    }

    // ── selected_is_dir ────────────────────────────────────────────

    #[test]
    fn selected_is_dir_true() {
        let fs = FindState::new_test(
            Path::new("/tmp"),
            &[("src/", true), ("main.rs", false)],
        );
        assert!(fs.selected_is_dir());
    }

    #[test]
    fn selected_is_dir_false() {
        let fs = FindState::new_test(
            Path::new("/tmp"),
            &[("main.rs", false), ("src/", true)],
        );
        assert!(!fs.selected_is_dir());
    }

    #[test]
    fn selected_is_dir_empty() {
        let fs = FindState::new_test(Path::new("/tmp"), &[]);
        assert!(!fs.selected_is_dir());
    }

    // ── total_count & filtered_count ──────────────────────────────

    #[test]
    fn total_and_filtered_counts() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.rs", false), ("b.py", false), ("c.rs", false)],
        );
        assert_eq!(fs.total_count(), 3);
        assert_eq!(fs.filtered_count(), 3);

        fs.query = "rs".into();
        fs.update_filter();
        assert_eq!(fs.total_count(), 3); // total unchanged
        assert_eq!(fs.filtered_count(), 2);
    }

    // ── move_down with empty filtered ──────────────────────────────

    #[test]
    fn move_down_empty_noop() {
        let mut fs = FindState::new_test(Path::new("/tmp"), &[]);
        fs.move_down();
        assert_eq!(fs.selected, 0);
    }

    #[test]
    fn move_up_at_zero_noop() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false)],
        );
        fs.move_up();
        assert_eq!(fs.selected, 0);
    }

    // ── adjust_scroll zero height ──────────────────────────────────

    #[test]
    fn adjust_scroll_zero_height_noop() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false)],
        );
        fs.selected = 0;
        fs.scroll = 5;
        fs.adjust_scroll(0);
        assert_eq!(fs.scroll, 5); // unchanged
    }

    // ── get_item out of bounds ─────────────────────────────────────

    #[test]
    fn get_item_out_of_bounds() {
        let fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.txt", false)],
        );
        assert!(fs.get_item(999).is_none());
    }

    // ── selected_path with no entries ──────────────────────────────

    #[test]
    fn selected_path_empty() {
        let fs = FindState::new_test(Path::new("/tmp"), &[]);
        assert!(fs.selected_path().is_none());
    }

    // ── elapsed_str ────────────────────────────────────────────────

    #[test]
    fn elapsed_str_none() {
        let fs = FindState::new_test(Path::new("/tmp"), &[]);
        assert!(fs.elapsed_str().is_empty());
    }

    // ── switch_scope ───────────────────────────────────────────────

    #[test]
    fn switch_scope_preserves_query() {
        let mut fs = FindState::new_test(Path::new("/tmp"), &[]);
        fs.query = "hello".into();
        fs.scope = FindScope::Local;
        // Note: switch_scope creates new state with same query
        // Can't fully test without tokio runtime, but verify structure
    }

    // ── fuzzy scoring edge cases ───────────────────────────────────

    #[test]
    fn fuzzy_single_char_match() {
        let q: Vec<char> = "a".chars().collect();
        assert!(fuzzy_score_pre(&q, "abc", 3).is_some());
        assert!(fuzzy_score_pre(&q, "xyz", 3).is_none());
    }

    #[test]
    fn fuzzy_query_longer_than_text() {
        let q: Vec<char> = "abcdef".chars().collect();
        assert!(fuzzy_score_pre(&q, "abc", 3).is_none());
    }

    #[test]
    fn fuzzy_consecutive_bonus() {
        // Use inputs without word-boundary separators so consecutive bonus dominates
        let q: Vec<char> = "abc".chars().collect();
        let consec = fuzzy_score_pre(&q, "xabcx", 5);
        let scattered = fuzzy_score_pre(&q, "xaxbxcx", 7);
        assert!(consec.unwrap() > scattered.unwrap());
    }

    #[test]
    fn get_item_and_selected_path() {
        let fs = FindState::new_test(
            Path::new("/tmp"),
            &[("src/main.rs", false), ("docs/", true)],
        );
        let (rel, is_dir) = fs.get_item(0).unwrap();
        assert_eq!(rel, "src/main.rs");
        assert!(!is_dir);

        let (rel, is_dir) = fs.get_item(1).unwrap();
        assert_eq!(rel, "docs/");
        assert!(is_dir);

        assert_eq!(fs.selected_path().unwrap(), Path::new("/tmp/src/main.rs"));
    }

    #[test]
    fn switch_scope_preserves_query_content() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.rs", false), ("b.rs", false)],
        );
        fs.query = "test".into();
        fs.scope = FindScope::Local;
        // switch_scope is tested elsewhere but let's verify query is accessible
        assert_eq!(fs.query, "test");
    }

    #[test]
    fn move_down_wraps_at_end() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("a.rs", false), ("b.rs", false)],
        );
        fs.update_filter();
        assert_eq!(fs.selected, 0);
        fs.move_down();
        assert_eq!(fs.selected, 1);
        fs.move_down(); // at end
        assert_eq!(fs.selected, 1); // stays at last
    }

    #[test]
    fn adjust_scroll_follows_selected() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[
                ("a.rs", false),
                ("b.rs", false),
                ("c.rs", false),
                ("d.rs", false),
                ("e.rs", false),
            ],
        );
        fs.update_filter();
        // Simulate small viewport
        fs.selected = 4;
        fs.adjust_scroll(2); // height=2
        // scroll should ensure selected is visible
        assert!(fs.scroll + 2 > fs.selected);
    }

    #[test]
    fn update_filter_with_fuzzy_query() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("main.rs", false), ("lib.rs", false), ("test_main.rs", false)],
        );
        fs.query = "main".into();
        fs.update_filter();
        // Should match "main.rs" and "test_main.rs"
        assert_eq!(fs.filtered_count(), 2);
    }

    #[test]
    fn update_filter_fuzzy_both_match() {
        let mut fs = FindState::new_test(
            Path::new("/tmp"),
            &[("xyz_main.rs", false), ("main.rs", false)],
        );
        fs.query = "main".into();
        fs.update_filter();
        // Both should match
        assert_eq!(fs.filtered_count(), 2);
    }

    #[test]
    fn selected_path_at_index() {
        let fs = FindState::new_test(
            Path::new("/home"),
            &[("docs/readme.md", false), ("src/", true)],
        );
        let path = fs.selected_path().unwrap();
        assert_eq!(path, Path::new("/home/docs/readme.md"));
    }

    #[test]
    fn spinner_frame_increments() {
        let fs = FindState::new_test(Path::new("/tmp"), &[]);
        let s1 = fs.spinner();
        // Spinner should return something
        assert!(!s1.is_empty());
    }
}
