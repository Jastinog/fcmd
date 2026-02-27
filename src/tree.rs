use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct TreeLine {
    pub prefix: String,
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_current: bool,
    pub is_on_path: bool,
    pub is_expanded: bool,
    pub depth: usize,
}

/// Build a flat list of tree lines from `root` down to `current`,
/// expanding directories along the path and any manually expanded dirs.
pub fn build_tree(
    root: &Path,
    current: &Path,
    show_hidden: bool,
    collapsed: &HashSet<PathBuf>,
    expanded: &HashSet<PathBuf>,
) -> Vec<TreeLine> {
    let mut lines = Vec::new();

    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());

    let is_root_current = root == current;

    lines.push(TreeLine {
        prefix: String::new(),
        name: root_name,
        path: root.to_path_buf(),
        is_dir: true,
        is_current: is_root_current,
        is_on_path: true,
        is_expanded: true,
        depth: 0,
    });

    let rel_components: Vec<String> = match current.strip_prefix(root) {
        Ok(rel) => rel
            .components()
            .filter_map(|c| {
                if let Component::Normal(name) = c {
                    Some(name.to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect(),
        Err(_) => return lines,
    };

    expand_dir(
        root,
        &rel_components,
        &mut lines,
        &[],
        show_hidden,
        collapsed,
        expanded,
    );

    lines
}

fn expand_dir(
    dir: &Path,
    path_ahead: &[String],
    lines: &mut Vec<TreeLine>,
    connector_state: &[bool],
    show_hidden: bool,
    collapsed: &HashSet<PathBuf>,
    expanded: &HashSet<PathBuf>,
) {
    let mut subdirs: Vec<(String, PathBuf, bool)> = Vec::new(); // (name, path, is_symlink)
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
            let is_symlink = entry
                .path()
                .symlink_metadata()
                .map(|m| m.is_symlink())
                .unwrap_or(false);
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            if is_dir {
                subdirs.push((name, entry.path(), is_symlink));
            } else {
                files.push((name, entry.path()));
            }
        }
    }
    subdirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    // Combined: dirs first, then files — needed for correct "is_last" connectors
    let total = subdirs.len() + files.len();
    if total == 0 {
        return;
    }

    let target = path_ahead.first();
    let mut idx = 0;

    for (name, path, is_symlink) in &subdirs {
        idx += 1;
        let is_last = idx == total;
        let on_path = target.map(|t| t == name).unwrap_or(false);
        let is_cur = on_path && path_ahead.len() == 1;
        let manually_expanded = expanded.contains(path.as_path());
        let is_collapsed = collapsed.contains(path.as_path());
        let should_expand = !is_collapsed && (on_path || manually_expanded);

        let prefix = make_prefix(connector_state, is_last);

        lines.push(TreeLine {
            prefix,
            name: name.clone(),
            path: path.clone(),
            is_dir: true,
            is_current: is_cur,
            is_on_path: on_path,
            is_expanded: should_expand,
            depth: connector_state.len() + 1,
        });

        // Expand dirs that should be expanded, but skip symlinks to prevent cycles
        if should_expand && !is_symlink {
            let mut next_connectors = connector_state.to_vec();
            next_connectors.push(!is_last);
            if on_path && path_ahead.len() > 1 {
                expand_dir(
                    path,
                    &path_ahead[1..],
                    lines,
                    &next_connectors,
                    show_hidden,
                    collapsed,
                    expanded,
                );
            } else {
                expand_dir(
                    path,
                    &[],
                    lines,
                    &next_connectors,
                    show_hidden,
                    collapsed,
                    expanded,
                );
            }
        }
    }

    for (name, path) in &files {
        idx += 1;
        let is_last = idx == total;
        let prefix = make_prefix(connector_state, is_last);

        lines.push(TreeLine {
            prefix,
            name: name.clone(),
            path: path.clone(),
            is_dir: false,
            is_current: false,
            is_on_path: false,
            is_expanded: false,
            depth: connector_state.len() + 1,
        });
    }
}

fn make_prefix(connector_state: &[bool], is_last: bool) -> String {
    let mut prefix = String::new();
    for &has_more in connector_state {
        if has_more {
            prefix.push_str("│  ");
        } else {
            prefix.push_str("   ");
        }
    }
    if is_last {
        prefix.push_str("└─ ");
    } else {
        prefix.push_str("├─ ");
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a temp directory structure for tree tests.
    fn setup_tree_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("alpha")).unwrap();
        fs::create_dir_all(root.join("alpha/nested")).unwrap();
        fs::write(root.join("alpha/nested/deep.txt"), "").unwrap();
        fs::create_dir_all(root.join("beta")).unwrap();
        fs::write(root.join("beta/b.txt"), "").unwrap();
        fs::write(root.join("file.txt"), "").unwrap();
        fs::write(root.join(".hidden"), "").unwrap();
        fs::create_dir_all(root.join(".secret")).unwrap();
        tmp
    }

    #[test]
    fn build_tree_root_is_current() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        let lines = build_tree(root, root, false, &HashSet::new(), &HashSet::new());
        // Root line should exist and be marked current
        assert!(!lines.is_empty());
        assert!(lines[0].is_current);
        assert!(lines[0].is_dir);
        assert_eq!(lines[0].depth, 0);
    }

    #[test]
    fn build_tree_expands_path_to_current() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        let current = root.join("alpha/nested");
        let lines = build_tree(root, &current, false, &HashSet::new(), &HashSet::new());

        // Should contain root, alpha, nested (expanded along path)
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"nested"));
        // "nested" should be marked is_current
        let nested = lines.iter().find(|l| l.name == "nested").unwrap();
        assert!(nested.is_current);
    }

    #[test]
    fn build_tree_hides_hidden_files() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        let lines = build_tree(root, root, false, &HashSet::new(), &HashSet::new());
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert!(!names.contains(&".hidden"));
        assert!(!names.contains(&".secret"));
    }

    #[test]
    fn build_tree_shows_hidden_files() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        let lines = build_tree(root, root, true, &HashSet::new(), &HashSet::new());
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&".hidden"));
        assert!(names.contains(&".secret"));
    }

    #[test]
    fn build_tree_collapsed_dir_not_expanded() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        let current = root.join("alpha/nested");
        let mut collapsed = HashSet::new();
        collapsed.insert(root.join("alpha"));
        let lines = build_tree(root, &current, false, &collapsed, &HashSet::new());

        // "alpha" should be present but not expanded (nested should be absent)
        let alpha = lines.iter().find(|l| l.name == "alpha").unwrap();
        assert!(!alpha.is_expanded);
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert!(!names.contains(&"nested"));
    }

    #[test]
    fn build_tree_manually_expanded_dir() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        // Current is root, but manually expand beta
        let mut expanded = HashSet::new();
        expanded.insert(root.join("beta"));
        let lines = build_tree(root, root, false, &HashSet::new(), &expanded);
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        // beta's contents (b.txt) should be visible
        assert!(names.contains(&"b.txt"));
    }

    #[test]
    fn build_tree_dirs_before_files() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        let lines = build_tree(root, root, false, &HashSet::new(), &HashSet::new());
        // At depth 1: dirs (alpha, beta) should come before file (file.txt)
        let depth1: Vec<&TreeLine> = lines.iter().filter(|l| l.depth == 1).collect();
        let first_file_idx = depth1.iter().position(|l| !l.is_dir);
        let last_dir_idx = depth1.iter().rposition(|l| l.is_dir);
        if let (Some(fi), Some(di)) = (first_file_idx, last_dir_idx) {
            assert!(di < fi, "dirs should come before files");
        }
    }

    #[test]
    fn build_tree_prefixes_correct() {
        let tmp = setup_tree_dir();
        let root = tmp.path();
        let lines = build_tree(root, root, false, &HashSet::new(), &HashSet::new());
        // Root has empty prefix
        assert_eq!(lines[0].prefix, "");
        // Depth-1 items should have tree connectors
        for l in &lines[1..] {
            if l.depth == 1 {
                assert!(
                    l.prefix.contains("├─") || l.prefix.contains("└─"),
                    "depth 1 prefix should have connector: {:?}",
                    l.prefix
                );
            }
        }
    }
}
