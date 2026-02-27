use super::*;

impl App {
    pub(super) fn toggle_visual_mark(&mut self) {
        let Some(entry) = self.active_panel().selected_entry() else {
            return;
        };
        if entry.name == ".." {
            return;
        }
        let path = entry.path.clone();
        let name = entry.name.clone();
        let current_level = self.visual_marks.get(&path).copied().unwrap_or(0);
        let next_level = if current_level >= 3 {
            0
        } else {
            current_level + 1
        };

        if next_level == 0 {
            self.visual_marks.remove(&path);
            let p = path;
            self.db_spawn(move |db| { let _ = db.remove_visual_mark(&p); });
            self.status_message = format!("Unmarked: {name}");
        } else {
            self.visual_marks.insert(path.clone(), next_level);
            self.db_spawn(move |db| { let _ = db.set_visual_mark(&path, next_level); });
            let label = match next_level {
                1 => "●1",
                2 => "●2",
                3 => "●3",
                _ => "●",
            };
            self.status_message = format!("{label} {name}");
        }
    }

    pub(super) fn jump_next_visual_mark(&mut self) {
        let panel = self.active_panel();
        let len = panel.entries.len();
        if len == 0 {
            return;
        }
        let start = (panel.selected + 1) % len;
        let pos = (0..len)
            .map(|i| (start + i) % len)
            .find(|&i| self.visual_marks.contains_key(&panel.entries[i].path));
        match pos {
            Some(pos) => self.active_panel_mut().selected = pos,
            None => self.status_message = "No marks".into(),
        }
    }

    pub(super) fn select_all(&mut self) {
        let panel = self.active_panel_mut();
        let mut count = 0;
        for entry in &panel.entries {
            if entry.name != ".." {
                panel.marked.insert(entry.path.clone());
                count += 1;
            }
        }
        self.status_message = format!("Selected {count} items");
    }

    pub(super) fn select_all_and_enter_select(&mut self) {
        self.select_all();
        self.mode = Mode::Select;
    }

    pub(super) fn unselect_all(&mut self) {
        self.active_panel_mut().marked.clear();
        self.status_message = "Selection cleared".into();
    }

    pub(super) fn set_mark(&mut self, c: char) {
        let path = self.active_panel().path.clone();
        self.marks.insert(c, path);
        self.status_message = format!("Mark '{c}' set");
    }

    pub(super) fn goto_mark(&mut self, c: char) {
        if let Some(path) = self.marks.get(&c).cloned() {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.nav_check_rx = Some(rx);
            tokio::task::spawn_blocking(move || {
                let exists = path.exists();
                let is_dir = path.is_dir();
                let _ = tx.send(super::NavCheckResult {
                    path,
                    is_dir,
                    exists,
                    source: super::NavSource::Mark(c),
                });
            });
        } else {
            self.status_message = format!("Mark '{c}' not set");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn toggle_visual_mark_cycles_levels() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // a.txt
        let path = PathBuf::from("/test/a.txt");

        // First toggle → level 1
        app.toggle_visual_mark();
        assert_eq!(app.visual_marks.get(&path), Some(&1));

        // Second toggle → level 2
        app.toggle_visual_mark();
        assert_eq!(app.visual_marks.get(&path), Some(&2));

        // Third toggle → level 3
        app.toggle_visual_mark();
        assert_eq!(app.visual_marks.get(&path), Some(&3));

        // Fourth toggle → removed (cycles back to 0)
        app.toggle_visual_mark();
        assert!(!app.visual_marks.contains_key(&path));
        assert!(app.status_message.contains("Unmarked"));
    }

    #[tokio::test]
    async fn toggle_visual_mark_skips_dotdot() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 0; // ".."
        app.toggle_visual_mark();
        assert!(app.visual_marks.is_empty());
    }

    #[tokio::test]
    async fn jump_next_visual_mark_finds_marked() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.visual_marks.insert(PathBuf::from("/test/c.txt"), 1);
        app.active_panel_mut().selected = 0;
        app.jump_next_visual_mark();
        assert_eq!(app.active_panel().selected, 3); // c.txt
    }

    #[tokio::test]
    async fn jump_next_visual_mark_wraps() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.visual_marks.insert(PathBuf::from("/test/a.txt"), 2);
        app.active_panel_mut().selected = 2; // b.txt, past the marked one
        app.jump_next_visual_mark();
        assert_eq!(app.active_panel().selected, 1); // wraps to a.txt
    }

    #[tokio::test]
    async fn jump_next_visual_mark_none_shows_message() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.jump_next_visual_mark();
        assert!(app.status_message.contains("No marks"));
    }

    #[tokio::test]
    async fn select_all_marks_all_non_dotdot() {
        let entries = make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        assert_eq!(app.active_panel().marked.len(), 3);
        assert!(!app.active_panel().marked.contains(&PathBuf::from("/")));
        assert!(app.status_message.contains("Selected 3"));
    }

    #[tokio::test]
    async fn select_all_and_enter_select_mode() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all_and_enter_select();
        assert_eq!(app.mode, Mode::Select);
        assert_eq!(app.active_panel().marked.len(), 1);
    }

    #[tokio::test]
    async fn unselect_all_clears() {
        let entries = make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        app.unselect_all();
        assert!(app.active_panel().marked.is_empty());
        assert!(app.status_message.contains("cleared"));
    }

    #[tokio::test]
    async fn set_mark_stores_path() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.set_mark('a');
        assert_eq!(app.marks.get(&'a'), Some(&PathBuf::from("/test")));
        assert!(app.status_message.contains("Mark 'a' set"));
    }

    #[tokio::test]
    async fn goto_mark_not_set_shows_message() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.goto_mark('z');
        assert!(app.status_message.contains("Mark 'z' not set"));
    }

    #[tokio::test]
    async fn goto_mark_set_spawns_nav_check() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.marks.insert('a', PathBuf::from("/tmp"));
        app.goto_mark('a');
        assert!(app.nav_check_rx.is_some());
    }
}
