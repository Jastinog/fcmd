use super::*;
use crate::util::glob_match;

impl App {
    pub(super) fn handle_command(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.execute_command();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command_input.clear();
            }
            KeyCode::Backspace => {
                if self.command_input.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                    self.command_input.pop();
                }
            }
            KeyCode::Char(c) => {
                self.command_input.push(c);
            }
            _ => {}
        }
    }

    fn execute_command(&mut self) {
        let input = self.command_input.trim().to_string();
        self.command_input.clear();

        let (cmd, arg) = match input.split_once(' ') {
            Some((c, a)) => (c.trim(), Some(a.trim())),
            None => (input.as_str(), None),
        };

        match cmd {
            "q" | "quit" | "q!" => self.should_quit = true,

            "mkdir" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :mkdir <name>".into();
                        return;
                    }
                };
                let dir = self.active_panel().path.clone();
                match ops::mkdir(&dir, name) {
                    Ok(rec) => {
                        self.undo_stack.push(vec![rec]);
                        self.refresh_panels();
                        self.active_panel_mut().select_by_name(name);
                        self.status_message = format!("Created directory: {name}");
                    }
                    Err(e) => self.status_message = format!("mkdir: {e}"),
                }
            }

            "touch" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :touch <name>".into();
                        return;
                    }
                };
                let dir = self.active_panel().path.clone();
                match ops::touch(&dir, name) {
                    Ok(rec) => {
                        self.undo_stack.push(vec![rec]);
                        self.refresh_panels();
                        self.active_panel_mut().select_by_name(name);
                        self.status_message = format!("Created file: {name}");
                    }
                    Err(e) => self.status_message = format!("touch: {e}"),
                }
            }

            "rename" | "rn" => {
                let new_name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :rename <new_name>".into();
                        return;
                    }
                };
                let path = match self
                    .active_panel()
                    .selected_entry()
                    .filter(|e| e.name != "..")
                {
                    Some(e) => e.path.clone(),
                    None => {
                        self.status_message = "Nothing selected to rename".into();
                        return;
                    }
                };
                match ops::rename_path(&path, new_name) {
                    Ok(rec) => {
                        self.undo_stack.push(vec![rec]);
                        self.refresh_panels();
                        self.active_panel_mut().select_by_name(new_name);
                        self.status_message = format!("Renamed to: {new_name}");
                    }
                    Err(e) => self.status_message = format!("rename: {e}"),
                }
            }

            "cd" => {
                let path_str = match arg.filter(|a| !a.is_empty()) {
                    Some(p) => p,
                    None => {
                        self.status_message = "Usage: :cd <path>".into();
                        return;
                    }
                };
                let target = if path_str.starts_with('/') {
                    PathBuf::from(path_str)
                } else if path_str.starts_with('~') {
                    let home = std::env::var("HOME").unwrap_or_default();
                    PathBuf::from(path_str.replacen('~', &home, 1))
                } else {
                    self.active_panel().path.join(path_str)
                };
                if target.is_dir() {
                    let panel = self.active_panel_mut();
                    panel.path = target;
                    panel.selected = 0;
                    panel.offset = 0;
                    if let Err(e) = panel.load_dir() {
                        self.status_message = format!("cd: {e}");
                    }
                } else {
                    self.status_message = format!("Not a directory: {path_str}");
                }
            }

            "find" => {
                let base = self.active_panel().path.clone();
                let mut fs = FindState::new_local(&base);
                if let Some(pattern) = arg.filter(|a| !a.is_empty()) {
                    fs.query = pattern.to_string();
                    fs.update_filter();
                }
                self.find_state = Some(fs);
                self.mode = Mode::Find;
            }

            "sort" => {
                match arg.map(|a| a.to_lowercase()).as_deref() {
                    Some("name" | "n") => self.set_sort(SortMode::Name),
                    Some("size" | "s") => self.set_sort(SortMode::Size),
                    Some("mod" | "modified" | "m" | "date" | "d") => self.set_sort(SortMode::Modified),
                    Some("cre" | "created" | "c") => self.set_sort(SortMode::Created),
                    Some("ext" | "e" | "extension") => self.set_sort(SortMode::Extension),
                    _ => self.status_message = "Usage: :sort name|size|mod|cre|ext".into(),
                }
            }

            "hidden" => {
                self.toggle_hidden();
            }

            "tabnew" => self.new_tab(),
            "tabclose" | "tabc" => self.close_tab(),
            "tabnext" | "tabn" => self.next_tab(),
            "tabprev" | "tabp" | "tabN" => self.prev_tab(),

            "mark" => {
                let c = arg
                    .and_then(|a| a.chars().next())
                    .filter(|c| c.is_ascii_lowercase());
                match c {
                    Some(c) => self.set_mark(c),
                    None => self.status_message = "Usage: :mark <a-z>".into(),
                }
            }

            "theme" => match arg.filter(|a| !a.is_empty()) {
                Some(name) => match Theme::load_by_name(name) {
                    Some(t) => {
                        self.theme = t;
                        self.theme_list = Theme::list_available();
                        self.theme_index = self.theme_list.iter().position(|n| n == name);
                        if let Some(ref db) = self.db {
                            let _ = db.save_theme(name);
                        }
                        self.status_message = format!("Theme: {name}");
                    }
                    None => self.status_message = format!("Theme not found: {name}"),
                },
                None => {
                    let themes = Theme::list_available();
                    if themes.is_empty() {
                        self.status_message = "No themes found".into();
                    } else {
                        self.status_message = themes.join(", ");
                    }
                }
            },

            "select" | "sel" => {
                let panel = self.active_panel_mut();
                match arg.filter(|a| !a.is_empty()) {
                    Some(pattern) => {
                        let mut count = 0;
                        for entry in &panel.entries {
                            if entry.name != ".." && glob_match(pattern, &entry.name) {
                                panel.marked.insert(entry.path.clone());
                                count += 1;
                            }
                        }
                        self.status_message = format!("Selected {count} items");
                    }
                    None => {
                        let mut count = 0;
                        for entry in &panel.entries {
                            if entry.name != ".." {
                                panel.marked.insert(entry.path.clone());
                                count += 1;
                            }
                        }
                        self.status_message = format!("Selected {count} items");
                    }
                }
            }

            "unselect" | "unsel" => {
                let panel = self.active_panel_mut();
                match arg.filter(|a| !a.is_empty()) {
                    Some(pattern) => {
                        let to_remove: Vec<PathBuf> = panel
                            .entries
                            .iter()
                            .filter(|e| e.name != ".." && glob_match(pattern, &e.name))
                            .map(|e| e.path.clone())
                            .collect();
                        let count = to_remove.len();
                        for p in &to_remove {
                            panel.marked.remove(p);
                        }
                        self.status_message = format!("Unselected {count} items");
                    }
                    None => {
                        panel.marked.clear();
                        self.status_message = "Selection cleared".into();
                    }
                }
            }

            "du" => self.start_du(),

            "marks" => {
                if self.marks.is_empty() {
                    self.status_message = "No marks set".into();
                } else {
                    let list: Vec<String> = self
                        .marks
                        .iter()
                        .map(|(k, v)| format!("'{k}={}", v.display()))
                        .collect();
                    self.status_message = list.join("  ");
                }
            }

            _ => {
                self.status_message = format!("Unknown command: :{cmd}");
            }
        }
    }
}
