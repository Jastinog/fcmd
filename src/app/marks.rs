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
            if let Some(ref db) = self.db
                && let Err(e) = db.remove_visual_mark(&path)
            {
                self.status_message = format!("Unmark error: {e}");
                return;
            }
            self.status_message = format!("Unmarked: {name}");
        } else {
            self.visual_marks.insert(path.clone(), next_level);
            if let Some(ref db) = self.db
                && let Err(e) = db.set_visual_mark(&path, next_level)
            {
                self.status_message = format!("Mark error: {e}");
                return;
            }
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
            if path.is_dir() {
                let side = self.tab().active;
                self.active_panel_mut().navigate_to(path);
                self.apply_dir_sort_no_reload();
                self.spawn_dir_load(side, None);
            } else {
                self.status_message = format!("Mark '{c}' directory no longer exists");
            }
        } else {
            self.status_message = format!("Mark '{c}' not set");
        }
    }
}
