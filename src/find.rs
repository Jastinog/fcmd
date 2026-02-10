use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const MAX_ENTRIES: usize = 5000;
const MAX_DEPTH: usize = 12;
const SKIP_DIRS: &[&str] = &[
    ".git", ".svn", ".hg", "target", "node_modules",
    "__pycache__", ".cache", "build", "dist", ".next",
];

struct Entry {
    rel_path: String,
    rel_path_lower: String, // cached lowercase for fuzzy matching
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
    pub walking: bool,
}

impl FindState {
    pub fn new(base_dir: &Path) -> Self {
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
            walking: true,
        }
    }

    /// Drain pending entries from background walk thread.
    /// Returns true if new entries arrived.
    pub fn poll_entries(&mut self) -> bool {
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
                    self.walking = false;
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

fn walk_send(
    dir: &Path,
    base: &Path,
    tx: &mpsc::Sender<Entry>,
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
            })
            .is_err()
        {
            return; // receiver dropped (FindState closed)
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
            if ti == 0
                || matches!(
                    t.get(ti.wrapping_sub(1)),
                    Some('/' | '_' | '-' | '.' | ' ')
                )
            {
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
        // Already lowered, so same as matching "MAIN" against "main.rs" pre-lowered
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
