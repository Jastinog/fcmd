use std::fs;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 5000;
const MAX_DEPTH: usize = 12;
const SKIP_DIRS: &[&str] = &[
    ".git", ".svn", ".hg", "target", "node_modules",
    "__pycache__", ".cache", "build", "dist", ".next",
];

struct Entry {
    rel_path: String,
    full_path: PathBuf,
    is_dir: bool,
}

pub struct FindState {
    pub query: String,
    entries: Vec<Entry>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
}

impl FindState {
    pub fn new(base_dir: &Path) -> Self {
        let mut entries = Vec::new();
        walk(base_dir, base_dir, &mut entries, 0);
        let filtered = (0..entries.len()).collect();
        FindState {
            query: String::new(),
            entries,
            filtered,
            selected: 0,
            scroll: 0,
        }
    }

    pub fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let mut scored: Vec<(usize, i32)> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| fuzzy_score(&self.query, &e.rel_path).map(|s| (i, s)))
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
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

fn walk(dir: &Path, base: &Path, entries: &mut Vec<Entry>, depth: usize) {
    if depth > MAX_DEPTH || entries.len() >= MAX_ENTRIES {
        return;
    }

    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    let mut items: Vec<_> = read_dir.flatten().collect();
    items.sort_by_key(|e| e.file_name());

    for item in items {
        if entries.len() >= MAX_ENTRIES {
            break;
        }

        let name = item.file_name().to_string_lossy().into_owned();
        let path = item.path();
        let is_dir = path.is_dir();

        if is_dir && SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }

        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();

        entries.push(Entry {
            rel_path: rel,
            full_path: path.clone(),
            is_dir,
        });

        if is_dir {
            walk(&path, base, entries, depth + 1);
        }
    }
}

fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();

    if q.is_empty() {
        return Some(0);
    }

    let mut qi = 0;
    let mut score = 0i32;
    let mut consecutive = 0i32;

    for (ti, &tc) in t.iter().enumerate() {
        if qi < q.len() && tc == q[qi] {
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

    if qi == q.len() {
        Some(score - text.len() as i32 / 4)
    } else {
        None
    }
}
