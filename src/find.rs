use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
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

#[derive(Clone, Copy, PartialEq)]
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
    rx: Option<mpsc::Receiver<Entry>>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub loading: bool,
    pub scope: FindScope,
    base_dir: PathBuf,
    search_child: Option<std::process::Child>,
    pub find_preview: Option<crate::preview::Preview>,
    find_preview_path: Option<PathBuf>,
    pub search_started: Option<Instant>,
    pub tick: usize,
}

impl Drop for FindState {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.search_child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl FindState {
    pub fn new_local(base_dir: &Path) -> Self {
        let (tx, rx) = mpsc::channel();
        let base = base_dir.to_path_buf();
        std::thread::spawn(move || {
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
            search_child: None,
            find_preview: None,
            find_preview_path: None,
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
            search_child: None,
            find_preview: None,
            find_preview_path: None,
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

        // Kill old search process
        if let Some(ref mut child) = self.search_child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.search_child = None;

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

        let (tx, rx) = mpsc::channel();
        let query = self.query.clone();
        let pattern = format!("*{query}*");

        // Try mdfind first (Spotlight), fall back to find from $HOME
        let child_result = Command::new("mdfind")
            .args(["-name", &query])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        let child_result = match child_result {
            Ok(child) => {
                // If Spotlight is disabled, mdfind exits with empty output.
                // The reader thread will detect 0 results and retry with find.
                Ok(child)
            }
            Err(_) => {
                // mdfind not available, use find
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                Command::new("find")
                    .args([&home, "-maxdepth", "6", "-iname", &pattern])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
            }
        };

        match child_result {
            Ok(mut child) => {
                let Some(stdout) = child.stdout.take() else {
                    return;
                };
                self.search_child = Some(child);
                self.rx = Some(rx);
                self.loading = true;
                self.search_started = Some(Instant::now());

                std::thread::spawn(move || {
                    let count = global_search_read(stdout, &tx);
                    // If mdfind returned 0 results (Spotlight disabled), retry with find
                    if count == 0 {
                        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                        if let Ok(mut child) = Command::new("find")
                            .args([&home, "-maxdepth", "6", "-iname", &pattern])
                            .stdout(Stdio::piped())
                            .stderr(Stdio::null())
                            .spawn()
                        {
                            if let Some(stdout) = child.stdout.take() {
                                global_search_read(stdout, &tx);
                            }
                            let _ = child.wait();
                        }
                    }
                });
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
        let Some(rx) = &self.rx else {
            return false;
        };
        let mut added = false;
        loop {
            match rx.try_recv() {
                Ok(entry) => {
                    self.entries.push(entry);
                    added = true;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
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

    pub fn update_find_preview(&mut self) {
        let current = self.selected_path().map(|p| p.to_path_buf());
        if current == self.find_preview_path {
            return;
        }
        self.find_preview_path = current.clone();
        self.find_preview = current.map(|p| crate::preview::Preview::load(&p));
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

fn abbreviate_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

fn global_search_read(stdout: std::process::ChildStdout, tx: &mpsc::Sender<Entry>) -> usize {
    let reader = std::io::BufReader::new(stdout);
    let mut count = 0;
    for line in reader.lines().map_while(Result::ok) {
        if count >= MDFIND_LIMIT {
            break;
        }
        let path = PathBuf::from(&line);
        let is_dir = path.is_dir();
        let display = abbreviate_home(&line);
        let display_lower = display.to_lowercase();
        if tx
            .send(Entry {
                rel_path: display,
                rel_path_lower: display_lower,
                full_path: path,
                is_dir,
            })
            .is_err()
        {
            break;
        }
        count += 1;
    }
    count
}

fn walk_send(dir: &Path, base: &Path, tx: &mpsc::Sender<Entry>, depth: usize, count: &mut usize) {
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
            .send(Entry {
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
}
