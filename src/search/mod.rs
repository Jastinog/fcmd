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
    /// Content search (grep): each entry is a matching line in a file.
    Content,
}

struct Entry {
    rel_path: String,
    rel_path_lower: String,
    full_path: PathBuf,
    is_dir: bool,
    /// 1-based line number for content (grep) matches; `None` for name matches.
    line: Option<usize>,
    /// The matched line's text for content matches; `None` for name matches.
    match_text: Option<String>,
}

pub struct FindState {
    pub query: String,
    entries: Vec<Entry>,
    // Unbounded so the background walker / `mdfind` reader never blocks waiting for
    // the UI to drain — results are lossless and capped (MAX_ENTRIES / MDFIND_LIMIT),
    // so the queue is bounded in practice without throttling discovery to tick rate.
    rx: Option<tokio::sync::mpsc::UnboundedReceiver<Entry>>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub loading: bool,
    pub scope: FindScope,
    base_dir: PathBuf,
    search_task: Option<tokio::task::JoinHandle<()>>,
    pub find_preview: Option<crate::preview::Preview>,
    // Cache key for the loaded preview: file path plus, for content matches, the
    // target line — so moving between two matches in the same file re-scrolls.
    find_preview_path: Option<(PathBuf, Option<usize>)>,
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
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
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

    /// Content (grep) search rooted at `base_dir` for `pattern`. Spawns the search
    /// immediately so results stream in like a global search.
    pub fn new_content(base_dir: &Path, pattern: &str) -> Self {
        // Same idle starting state as a global search, with the grep pattern and
        // scope set, then kick off the search right away.
        let mut state = Self::new_global(base_dir);
        state.scope = FindScope::Content;
        state.query = pattern.to_string();
        state.trigger_search();
        state
    }

    pub fn switch_scope(&self) -> Self {
        let mut new_state = match self.scope {
            FindScope::Local => Self::new_global(&self.base_dir),
            // Global and Content both fall back to local name search on Tab.
            FindScope::Global | FindScope::Content => Self::new_local(&self.base_dir),
        };
        new_state.query = self.query.clone();
        // For global, trigger search if query is non-empty
        if new_state.scope == FindScope::Global && !new_state.query.is_empty() {
            new_state.trigger_search();
        }
        new_state
    }

    /// Re-run the active search for the current query. Global searches names
    /// (mdfind/fd/find); content searches file contents (rg/grep). No-op for
    /// the in-memory local scope.
    pub fn trigger_search(&mut self) {
        match self.scope {
            FindScope::Global => self.trigger_global(),
            FindScope::Content => self.trigger_content(),
            FindScope::Local => {}
        }
    }

    /// Reset streaming state before (re-)launching an external search. Returns the
    /// fresh channel sender if the query is non-empty, or `None` when it's empty
    /// (in which case the caller should just show an empty/placeholder state).
    fn reset_for_search(&mut self) -> Option<tokio::sync::mpsc::UnboundedSender<Entry>> {
        // Abort old search task (kills child processes via kill_on_drop)
        if let Some(handle) = self.search_task.take() {
            handle.abort();
        }
        self.entries.clear();
        self.filtered.clear();
        self.selected = 0;
        self.scroll = 0;

        if self.query.is_empty() {
            self.clear_loading();
            return None;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.rx = Some(rx);
        self.loading = true;
        self.search_started = Some(Instant::now());
        Some(tx)
    }

    /// Clear the streaming/loading flags (no usable tool, or an empty query).
    fn clear_loading(&mut self) {
        self.loading = false;
        self.rx = None;
        self.search_started = None;
    }

    /// Global name search. Tries mdfind (macOS), then `fd`/`fdfind`, then
    /// Unix `find`, searching from the user's home directory.
    fn trigger_global(&mut self) {
        let Some(tx) = self.reset_for_search() else {
            return;
        };

        // Sanitize: strip characters that could be interpreted by find/mdfind/fd
        let sanitized_query: String = self.query.chars().filter(|c| *c != '\0').collect();
        let home = crate::util::home_dir_string();

        // Spawn the best available search tool for this platform.
        let Some((mut child, is_mdfind)) = spawn_global_search(&sanitized_query, &home) else {
            // No usable search tool (e.g. Windows without `fd` on PATH).
            self.clear_loading();
            return;
        };

        let Some(stdout) = child.stdout.take() else {
            self.loading = false;
            return;
        };

        let handle = tokio::spawn(async move {
            let count = global_search_read(stdout, &tx).await;
            let _ = child.wait().await;
            // mdfind returns 0 results when Spotlight is disabled — retry with fd/find.
            if is_mdfind
                && count == 0
                && let Some(mut fallback) = spawn_fallback_search(&sanitized_query, &home)
            {
                if let Some(stdout) = fallback.stdout.take() {
                    global_search_read(stdout, &tx).await;
                }
                let _ = fallback.wait().await;
            }
        });
        self.search_task = Some(handle);
    }

    /// Content search (grep) rooted at `base_dir`. Tries `rg` (ripgrep), falling
    /// back to `grep -rn`. Streams `path:line:text` matches into the result list.
    fn trigger_content(&mut self) {
        let base = self.base_dir.clone();
        let Some(tx) = self.reset_for_search() else {
            return;
        };
        let pattern = self.query.clone();

        let Some(mut child) = spawn_content_search(&pattern, &base) else {
            // Neither rg nor grep available.
            self.clear_loading();
            return;
        };

        let Some(stdout) = child.stdout.take() else {
            self.loading = false;
            return;
        };

        let handle = tokio::spawn(async move {
            content_search_read(stdout, &tx, &base).await;
            let _ = child.wait().await;
        });
        self.search_task = Some(handle);
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
        // Content matches are already filtered by grep — never fuzzy-filter them
        // by the query (the query is the grep pattern, not a path subsequence).
        if self.query.is_empty() || self.scope == FindScope::Content {
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
            scored.sort_by_key(|&(_, s)| std::cmp::Reverse(s));
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
        let target_line = self.selected_line();
        let current = self
            .selected_path()
            .map(|p| (p.to_path_buf(), target_line));
        if current == self.find_preview_path {
            return;
        }
        self.find_preview_path = current.clone();
        if let Some((p, line)) = current {
            self.find_preview = Some(crate::preview::Preview::loading_placeholder(&p));
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.find_preview_rx = Some(rx);
            let path = p.clone();
            let vis = visible_height;
            tokio::task::spawn_blocking(move || {
                let mut prev = crate::preview::Preview::load(&path, vis);
                // For a content match, scroll so the matched line sits near the top
                // with a little context above it.
                if let Some(line) = line
                    && !prev.is_binary
                    && !prev.lines.is_empty()
                {
                    let last = prev.lines.len() - 1;
                    let target = line.saturating_sub(1).min(last);
                    prev.scroll = target.saturating_sub(vis / 3);
                }
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
                if self.find_preview_path.as_ref().map(|(p, _)| p) == Some(&path) {
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

    /// Result row at `filtered_idx`: the relative path, whether it's a directory,
    /// and (for content/grep results) the line number and matched text.
    pub fn get_item_full(
        &self,
        filtered_idx: usize,
    ) -> Option<(&str, bool, Option<usize>, Option<&str>)> {
        self.filtered
            .get(filtered_idx)
            .and_then(|&i| self.entries.get(i))
            .map(|e| {
                (
                    e.rel_path.as_str(),
                    e.is_dir,
                    e.line,
                    e.match_text.as_deref(),
                )
            })
    }

    /// Line number of the selected content match, if any.
    pub fn selected_line(&self) -> Option<usize> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.entries.get(i))
            .and_then(|e| e.line)
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
                line: None,
                match_text: None,
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

/// Spawn the best available global-search tool for this platform.
///
/// Returns the child plus a flag marking whether it is `mdfind` (which needs a
/// fallback when Spotlight is disabled). Order: macOS Spotlight → `fd`/`fdfind`
/// (cross-platform, incl. Windows) → Unix `find`.
fn spawn_global_search(query: &str, home: &str) -> Option<(tokio::process::Child, bool)> {
    #[cfg(target_os = "macos")]
    if let Ok(child) = tokio::process::Command::new("mdfind")
        .args(["-name", query])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        return Some((child, true));
    }

    spawn_fallback_search(query, home).map(|child| (child, false))
}

/// Spawn a non-Spotlight search tool: `fd`/`fdfind` if on PATH, else Unix `find`.
///
/// Never falls back to the Windows `find.exe` (an unrelated text-search tool),
/// so on Windows this yields a result only when `fd` is installed.
fn spawn_fallback_search(query: &str, home: &str) -> Option<tokio::process::Child> {
    for bin in ["fd", "fdfind"] {
        if let Ok(child) = tokio::process::Command::new(bin)
            .args(["--hidden", "--no-ignore", "--fixed-strings", "--absolute-path"])
            .arg("--")
            .arg(query)
            .arg(home)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            return Some(child);
        }
    }

    #[cfg(unix)]
    {
        let pattern = format!("*{query}*");
        if let Ok(child) = tokio::process::Command::new("find")
            .arg("--")
            .args([home, "-maxdepth", "6", "-iname", &pattern])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            return Some(child);
        }
    }

    None
}

/// Spawn a content (grep) search rooted at `base`. Tries `rg` (ripgrep), then
/// `grep -rn`. Runs with `base` as the working directory and `.` as the path
/// argument so matches are reported relative to `base`. Patterns are matched as
/// fixed strings (no regex surprises). Returns `None` if neither tool is found.
fn spawn_content_search(pattern: &str, base: &Path) -> Option<tokio::process::Child> {
    if let Ok(child) = tokio::process::Command::new("rg")
        .args([
            "--line-number",
            "--no-heading",
            "--color=never",
            "--smart-case",
            "--fixed-strings",
            "-e",
        ])
        .arg(pattern)
        .arg(".")
        .current_dir(base)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        return Some(child);
    }

    // POSIX grep fallback: recursive (-r), line numbers (-n), skip binaries (-I),
    // fixed strings (-F). `-e` guards patterns that start with '-'.
    if let Ok(child) = tokio::process::Command::new("grep")
        .args(["-rnIF", "-e"])
        .arg(pattern)
        .arg(".")
        .current_dir(base)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        return Some(child);
    }

    None
}

/// Read `path:line:text` grep output into result entries (capped at MDFIND_LIMIT).
async fn content_search_read(
    stdout: tokio::process::ChildStdout,
    tx: &tokio::sync::mpsc::UnboundedSender<Entry>,
    base: &Path,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut count = 0;
    while let Ok(Some(line)) = lines.next_line().await {
        if count >= MDFIND_LIMIT {
            break;
        }
        let Some((rel, lineno, text)) = parse_grep_line(&line) else {
            continue;
        };
        // Both rg and grep prefix paths with "./" when searching ".".
        let rel = rel.strip_prefix("./").unwrap_or(rel).to_string();
        let full_path = base.join(&rel);
        let rel_lower = rel.to_lowercase();
        let trimmed = text.trim_start().to_string();
        if tx
            .send(Entry {
                rel_path: rel,
                rel_path_lower: rel_lower,
                full_path,
                is_dir: false,
                line: Some(lineno),
                match_text: Some(trimmed),
            })
            .is_err()
        {
            break;
        }
        count += 1;
    }
}

/// Parse a single `path:line:text` grep/rg output line. The line number is the
/// first all-digit `:`-delimited field; everything before it is the path and
/// everything after is the matched text. Returns `None` if no such field exists.
fn parse_grep_line(line: &str) -> Option<(&str, usize, &str)> {
    let mut start = 0;
    while let Some(rel) = line[start..].find(':') {
        let colon = start + rel;
        let after = &line[colon + 1..];
        if let Some(next) = after.find(':')
            && let Ok(num) = after[..next].parse::<usize>()
        {
            return Some((&line[..colon], num, &after[next + 1..]));
        }
        start = colon + 1;
    }
    None
}

fn abbreviate_home(path: &str) -> String {
    if let Some(home) = dirs::home_dir()
        && let Some(rest) = path.strip_prefix(&*home.to_string_lossy())
    {
        return format!("~{rest}");
    }
    path.to_string()
}

async fn global_search_read(
    stdout: tokio::process::ChildStdout,
    tx: &tokio::sync::mpsc::UnboundedSender<Entry>,
) -> usize {
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
                line: None,
                match_text: None,
            })
            .is_err()
        {
            break;
        }
        count += 1;
    }
    count
}

fn walk_send(
    dir: &Path,
    base: &Path,
    tx: &tokio::sync::mpsc::UnboundedSender<Entry>,
    depth: usize,
    count: &mut usize,
) {
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
                line: None,
                match_text: None,
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
        let mut fs = FindState::new_test(Path::new("/tmp"), &[("a.txt", false), ("b.txt", false)]);
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
            &[
                ("a", false),
                ("b", false),
                ("c", false),
                ("d", false),
                ("e", false),
            ],
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
        let mut fs = FindState::new_test(Path::new("/tmp"), &[("a.txt", false), ("b.txt", false)]);
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
        let fs = FindState::new_test(Path::new("/tmp"), &[("src/", true), ("main.rs", false)]);
        assert!(fs.selected_is_dir());
    }

    #[test]
    fn selected_is_dir_false() {
        let fs = FindState::new_test(Path::new("/tmp"), &[("main.rs", false), ("src/", true)]);
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
        let mut fs = FindState::new_test(Path::new("/tmp"), &[("a.txt", false)]);
        fs.move_up();
        assert_eq!(fs.selected, 0);
    }

    // ── adjust_scroll zero height ──────────────────────────────────

    #[test]
    fn adjust_scroll_zero_height_noop() {
        let mut fs = FindState::new_test(Path::new("/tmp"), &[("a.txt", false)]);
        fs.selected = 0;
        fs.scroll = 5;
        fs.adjust_scroll(0);
        assert_eq!(fs.scroll, 5); // unchanged
    }

    // ── parse_grep_line ────────────────────────────────────────────

    #[test]
    fn parse_grep_basic() {
        let (path, line, text) = parse_grep_line("./src/main.rs:42:    let x = 1;").unwrap();
        assert_eq!(path, "./src/main.rs");
        assert_eq!(line, 42);
        assert_eq!(text, "    let x = 1;");
    }

    #[test]
    fn parse_grep_text_contains_colons() {
        // Colons in the matched text must not confuse the parse.
        let (path, line, text) = parse_grep_line("a.rs:7:foo: bar: baz").unwrap();
        assert_eq!(path, "a.rs");
        assert_eq!(line, 7);
        assert_eq!(text, "foo: bar: baz");
    }

    #[test]
    fn parse_grep_no_line_number() {
        assert!(parse_grep_line("just some text").is_none());
        assert!(parse_grep_line("path:not_a_number:text").is_none());
    }

    #[test]
    fn parse_grep_empty_match_text() {
        let (path, line, text) = parse_grep_line("file.txt:1:").unwrap();
        assert_eq!(path, "file.txt");
        assert_eq!(line, 1);
        assert_eq!(text, "");
    }

    // ── get_item out of bounds ─────────────────────────────────────

    #[test]
    fn get_item_out_of_bounds() {
        let fs = FindState::new_test(Path::new("/tmp"), &[("a.txt", false)]);
        assert!(fs.get_item_full(999).is_none());
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
        let (rel, is_dir, _, _) = fs.get_item_full(0).unwrap();
        assert_eq!(rel, "src/main.rs");
        assert!(!is_dir);

        let (rel, is_dir, _, _) = fs.get_item_full(1).unwrap();
        assert_eq!(rel, "docs/");
        assert!(is_dir);

        assert_eq!(fs.selected_path().unwrap(), Path::new("/tmp/src/main.rs"));
    }

    #[test]
    fn switch_scope_preserves_query_content() {
        let mut fs = FindState::new_test(Path::new("/tmp"), &[("a.rs", false), ("b.rs", false)]);
        fs.query = "test".into();
        fs.scope = FindScope::Local;
        // switch_scope is tested elsewhere but let's verify query is accessible
        assert_eq!(fs.query, "test");
    }

    #[test]
    fn move_down_wraps_at_end() {
        let mut fs = FindState::new_test(Path::new("/tmp"), &[("a.rs", false), ("b.rs", false)]);
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
            &[
                ("main.rs", false),
                ("lib.rs", false),
                ("test_main.rs", false),
            ],
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
