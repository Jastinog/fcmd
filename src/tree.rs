use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct TreeLine {
    pub prefix: String,
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_current: bool,
    pub is_on_path: bool,
    pub depth: usize,
}

/// Build a flat list of tree lines from `root` down to `current`,
/// expanding only directories along the path.
pub fn build_tree(root: &Path, current: &Path, show_hidden: bool) -> Vec<TreeLine> {
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

    expand_dir(root, &rel_components, &mut lines, &[], show_hidden);

    lines
}

fn expand_dir(
    dir: &Path,
    path_ahead: &[String],
    lines: &mut Vec<TreeLine>,
    connector_state: &[bool],
    show_hidden: bool,
) {
    let mut subdirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            if is_dir {
                subdirs.push((name, entry.path()));
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

    for (name, path) in &subdirs {
        idx += 1;
        let is_last = idx == total;
        let on_path = target.map(|t| t == name).unwrap_or(false);
        let is_cur = on_path && path_ahead.len() == 1;

        let prefix = make_prefix(connector_state, is_last);

        lines.push(TreeLine {
            prefix,
            name: name.clone(),
            path: path.clone(),
            is_dir: true,
            is_current: is_cur,
            is_on_path: on_path,
            depth: connector_state.len() + 1,
        });

        if on_path {
            let mut next_connectors = connector_state.to_vec();
            next_connectors.push(!is_last);
            if path_ahead.len() > 1 {
                expand_dir(path, &path_ahead[1..], lines, &next_connectors, show_hidden);
            } else {
                expand_dir(path, &[], lines, &next_connectors, show_hidden);
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
