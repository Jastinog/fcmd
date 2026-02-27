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
        if input.is_empty() {
            return;
        }

        let (cmd, arg) = match input.split_once(' ') {
            Some((c, a)) => (c.trim(), Some(a.trim())),
            None => (input.as_str(), None),
        };

        match cmd {
            "q" | "quit" | "q!" => self.should_quit = true,

            "mkdir" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n.to_string(),
                    None => {
                        self.status_message = "Usage: :mkdir <name>".into();
                        return;
                    }
                };
                let dir = self.active_panel().path.clone();
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.file_op_rx = Some(rx);
                let name2 = name.clone();
                tokio::task::spawn_blocking(move || {
                    let result = ops::mkdir(&dir, &name2).map_err(|e| e.to_string());
                    let _ = tx.send(super::FileOpResult::Mkdir { name: name2, result });
                });
            }

            "touch" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n.to_string(),
                    None => {
                        self.status_message = "Usage: :touch <name>".into();
                        return;
                    }
                };
                let dir = self.active_panel().path.clone();
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.file_op_rx = Some(rx);
                let name2 = name.clone();
                tokio::task::spawn_blocking(move || {
                    let result = ops::touch(&dir, &name2).map_err(|e| e.to_string());
                    let _ = tx.send(super::FileOpResult::Touch { name: name2, result });
                });
            }

            "rename" | "rn" => {
                let new_name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n.to_string(),
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
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.file_op_rx = Some(rx);
                let new_name2 = new_name.clone();
                tokio::task::spawn_blocking(move || {
                    let result = ops::rename_path(&path, &new_name2).map_err(|e| e.to_string());
                    let _ = tx.send(super::FileOpResult::Rename {
                        new_name: new_name2,
                        result,
                    });
                });
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
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.nav_check_rx = Some(rx);
                tokio::task::spawn_blocking(move || {
                    let exists = target.exists();
                    let is_dir = target.is_dir();
                    let _ = tx.send(super::NavCheckResult {
                        path: target,
                        is_dir,
                        exists,
                        source: super::NavSource::Cd,
                    });
                });
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

            "sort" => match arg.map(|a| a.to_lowercase()).as_deref() {
                Some("name" | "n") => self.set_sort(SortMode::Name),
                Some("size" | "s") => self.set_sort(SortMode::Size),
                Some("mod" | "modified" | "m" | "date" | "d") => self.set_sort(SortMode::Modified),
                Some("cre" | "created" | "c") => self.set_sort(SortMode::Created),
                Some("ext" | "e" | "extension") => self.set_sort(SortMode::Extension),
                _ => self.status_message = "Usage: :sort name|size|mod|cre|ext".into(),
            },

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
                Some(name) => {
                    let name = name.to_string();
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    self.file_op_rx = Some(rx);
                    tokio::task::spawn_blocking(move || {
                        let theme = Theme::load_by_name(&name);
                        let (dark_list, light_list) = Theme::list_available_classified();
                        let _ = tx.send(super::FileOpResult::ThemeLoad { name, theme, dark_list, light_list });
                    });
                }
                None => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    self.file_op_rx = Some(rx);
                    tokio::task::spawn_blocking(move || {
                        let (dark, light) = Theme::list_available_classified();
                        let _ = tx.send(super::FileOpResult::ThemeList { dark, light });
                    });
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

            "bookmark" | "bm" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :bookmark <name>".into();
                        return;
                    }
                };
                let path = match self
                    .active_panel()
                    .selected_entry()
                    .filter(|e| e.is_dir || e.name == "..")
                {
                    Some(e) => e.path.clone(),
                    None => {
                        self.status_message = "Select a directory to bookmark".into();
                        return;
                    }
                };
                self.add_bookmark(name, path);
            }

            "bookmarks" | "bms" => {
                self.open_bookmarks();
            }

            "brename" | "bmrn" => {
                let parts: Vec<&str> = match arg.filter(|a| !a.is_empty()) {
                    Some(a) => a.splitn(2, ' ').collect(),
                    None => {
                        self.status_message = "Usage: :brename <oldname> <newname>".into();
                        return;
                    }
                };
                if parts.len() < 2 || parts[1].is_empty() {
                    self.status_message = "Usage: :brename <oldname> <newname>".into();
                    return;
                }
                let old_name = parts[0];
                let new_name = parts[1];
                self.rename_bookmark(old_name, new_name);
            }

            "bdel" => {
                let name = match arg.filter(|a| !a.is_empty()) {
                    Some(n) => n,
                    None => {
                        self.status_message = "Usage: :bdel <name>".into();
                        return;
                    }
                };
                if self.bookmarks.iter().any(|(n, _)| n == name) {
                    let name_owned = name.to_string();
                    self.remove_bookmark_by_name(&name_owned);
                    self.status_message = format!("Bookmark removed: {name_owned}");
                } else {
                    self.status_message = format!("Bookmark not found: {name}");
                }
            }

            _ => {
                self.status_message = format!("Unknown command: :{cmd}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn handle_command_char_appends() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Command;
        app.command_input.clear();

        app.handle_command(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(app.command_input, "q");
    }

    #[tokio::test]
    async fn handle_command_backspace_pops() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Command;
        app.command_input = "ab".into();

        app.handle_command(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.command_input, "a");

        app.handle_command(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.command_input, "");

        // Empty backspace → Normal mode
        app.handle_command(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn handle_command_esc_clears_and_exits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Command;
        app.command_input = "something".into();

        app.handle_command(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.command_input.is_empty());
    }

    #[tokio::test]
    async fn handle_command_enter_executes() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mode = Mode::Command;
        app.command_input = "q".into();

        app.handle_command(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn execute_command_quit() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.command_input = "q".into();
        app.execute_command();
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn execute_command_sort_name() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.command_input = "sort name".into();
        app.execute_command();
        assert_eq!(app.active_panel().sort_mode, SortMode::Name);
    }

    #[tokio::test]
    async fn execute_command_sort_size() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.command_input = "sort size".into();
        app.execute_command();
        assert_eq!(app.active_panel().sort_mode, SortMode::Size);
    }

    #[tokio::test]
    async fn execute_command_hidden_toggles() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let before = app.active_panel().show_hidden;
        app.command_input = "hidden".into();
        app.execute_command();
        assert_ne!(app.active_panel().show_hidden, before);
    }

    #[tokio::test]
    async fn execute_command_tabnew() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        assert_eq!(app.tabs.len(), 1);
        app.command_input = "tabnew".into();
        app.execute_command();
        assert_eq!(app.tabs.len(), 2);
    }

    #[tokio::test]
    async fn execute_command_tabclose() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.tabs.push(Tab::new(PathBuf::from("/test2")));
        assert_eq!(app.tabs.len(), 2);
        app.command_input = "tabclose".into();
        app.execute_command();
        assert_eq!(app.tabs.len(), 1);
    }

    #[tokio::test]
    async fn execute_command_select_pattern() {
        let entries = crate::app::make_test_entries(&["a.rs", "b.py", "c.rs"]);
        let mut app = App::new_for_test(entries);
        app.command_input = "select *.rs".into();
        app.execute_command();
        assert_eq!(app.active_panel().marked.len(), 2);
    }

    #[tokio::test]
    async fn execute_command_unselect_clears() {
        let entries = crate::app::make_test_entries(&["a.rs", "b.py"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        assert!(!app.active_panel().marked.is_empty());
        app.command_input = "unselect".into();
        app.execute_command();
        assert!(app.active_panel().marked.is_empty());
    }

    #[tokio::test]
    async fn execute_command_unknown_shows_status() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.command_input = "foobar".into();
        app.execute_command();
        assert!(app.status_message.contains("Unknown command"));
    }
}
