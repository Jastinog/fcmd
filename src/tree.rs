use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct TreeLine {
    pub prefix: String,
    pub name: String,
    pub path: PathBuf,
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
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            subdirs.push((name, entry.path()));
        }
    }
    subdirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    if subdirs.is_empty() {
        return;
    }

    let target = path_ahead.first();

    for (i, (name, path)) in subdirs.iter().enumerate() {
        let is_last = i == subdirs.len() - 1;
        let on_path = target.map(|t| t == name).unwrap_or(false);
        let is_cur = on_path && path_ahead.len() == 1;

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

        lines.push(TreeLine {
            prefix,
            name: name.clone(),
            path: path.clone(),
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
                // Show children of current directory
                expand_dir(path, &[], lines, &next_connectors, show_hidden);
            }
        }
    }
}
