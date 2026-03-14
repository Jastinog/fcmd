use super::*;

#[derive(Clone)]
pub struct BulkRenameEntry {
    pub original_path: PathBuf,
    pub original_name: String,
    pub new_name: String,
    pub is_dir: bool,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum BulkRenameSubMode {
    Nav,
    Edit,
    FindReplace,
}

pub struct BulkRenameState {
    pub entries: Vec<BulkRenameEntry>,
    pub cursor: usize,
    pub scroll: usize,
    pub sub_mode: BulkRenameSubMode,
    pub edit_input: String,
    pub find_replace_input: String,
    pub error: Option<String>,
}

impl BulkRenameState {
    pub fn new(entries: Vec<BulkRenameEntry>) -> Self {
        Self {
            entries,
            cursor: 0,
            scroll: 0,
            sub_mode: BulkRenameSubMode::Nav,
            edit_input: String::new(),
            find_replace_input: String::new(),
            error: None,
        }
    }

    pub fn changed_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.new_name != e.original_name)
            .count()
    }

    pub fn has_conflicts(&self) -> bool {
        let mut seen = HashSet::new();
        // Collect original names so we know which targets are "already taken" by the batch
        let original_names: HashSet<&str> = self.entries.iter().map(|e| e.original_name.as_str()).collect();
        for e in &self.entries {
            if e.new_name.is_empty() {
                return true;
            }
            if !seen.insert(&e.new_name) {
                return true;
            }
            // Check filesystem: if target name differs and exists on disk,
            // and is not another entry in this batch (which will be renamed away)
            if e.new_name != e.original_name && !original_names.contains(e.new_name.as_str()) {
                if let Some(parent) = e.original_path.parent() {
                    if parent.join(&e.new_name).exists() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Returns indices of entries with conflicts (empty, duplicate, or existing on disk).
    pub fn conflict_indices(&self) -> HashSet<usize> {
        let mut result = HashSet::new();
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let original_names: HashSet<&str> = self.entries.iter().map(|e| e.original_name.as_str()).collect();
        for (i, e) in self.entries.iter().enumerate() {
            if e.new_name.is_empty() {
                result.insert(i);
                continue;
            }
            if let Some(&prev) = seen.get(e.new_name.as_str()) {
                result.insert(prev);
                result.insert(i);
            } else {
                seen.insert(&e.new_name, i);
            }
            // Check filesystem conflict
            if e.new_name != e.original_name && !original_names.contains(e.new_name.as_str()) {
                if let Some(parent) = e.original_path.parent() {
                    if parent.join(&e.new_name).exists() {
                        result.insert(i);
                    }
                }
            }
        }
        result
    }

    /// Apply :%s/pattern/replacement/ to all entries.
    pub fn apply_find_replace(&mut self, input: &str) {
        // Parse: %s/old/new, s/old/new, or /old/new (g is always implied)
        let input = input
            .strip_prefix("%s")
            .or_else(|| input.strip_prefix("s"))
            .unwrap_or(input);
        if input.is_empty() {
            self.error = Some("Usage: :%s/old/new".into());
            return;
        }
        let sep = input.chars().next().unwrap();
        let parts: Vec<&str> = input[sep.len_utf8()..].splitn(3, sep).collect();
        if parts.len() < 2 || parts[0].is_empty() {
            self.error = Some("Usage: :%s/old/new".into());
            return;
        }
        let find = parts[0];
        let replace = parts[1];
        let mut count = 0;
        for entry in &mut self.entries {
            if entry.new_name.contains(find) {
                entry.new_name = entry.new_name.replace(find, replace);
                count += 1;
            }
        }
        if count == 0 {
            self.error = Some(format!("No matches for '{find}'"));
        } else {
            self.error = None;
        }
    }

    fn adjust_scroll(&mut self, visible_h: usize) {
        if visible_h == 0 {
            return;
        }
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + visible_h {
            self.scroll = self.cursor - visible_h + 1;
        }
    }
}

impl App {
    pub(super) fn enter_bulk_rename(&mut self) {
        let panel = self.active_panel();
        let targeted = panel.targeted_register_entries();
        if targeted.is_empty() {
            self.status_message = "Nothing to rename".into();
            return;
        }

        let entries: Vec<BulkRenameEntry> = targeted
            .into_iter()
            .filter_map(|re| {
                let name = re
                    .path
                    .file_name()?
                    .to_string_lossy()
                    .into_owned();
                Some(BulkRenameEntry {
                    original_path: re.path,
                    original_name: name.clone(),
                    new_name: name,
                    is_dir: re.is_dir,
                })
            })
            .collect();

        if entries.is_empty() {
            self.status_message = "Nothing to rename".into();
            return;
        }

        self.bulk_rename = Some(BulkRenameState::new(entries));
        self.mode = Mode::BulkRename;
    }

    pub(super) fn handle_bulk_rename(&mut self, key: KeyEvent) {
        let sub_mode = match self.bulk_rename {
            Some(ref s) => s.sub_mode,
            None => return,
        };

        match sub_mode {
            BulkRenameSubMode::Nav => self.handle_bulk_rename_nav(key),
            BulkRenameSubMode::Edit => self.handle_bulk_rename_edit(key),
            BulkRenameSubMode::FindReplace => self.handle_bulk_rename_find_replace(key),
        }
    }

    fn handle_bulk_rename_nav(&mut self, key: KeyEvent) {
        let state = self.bulk_rename.as_mut().unwrap();
        let len = state.entries.len();

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if state.cursor + 1 < len {
                    state.cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.cursor > 0 {
                    state.cursor -= 1;
                }
            }
            KeyCode::Char('G') => {
                state.cursor = len.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                state.cursor = 0;
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                state.cursor = (state.cursor + half).min(len.saturating_sub(1));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.visible_height / 2;
                state.cursor = state.cursor.saturating_sub(half);
            }
            KeyCode::Char('i') | KeyCode::Char('a') => {
                state.edit_input = state.entries[state.cursor].new_name.clone();
                state.sub_mode = BulkRenameSubMode::Edit;
                state.error = None;
            }
            KeyCode::Char('u') => {
                // Undo current line → reset to original
                let entry = &mut state.entries[state.cursor];
                entry.new_name = entry.original_name.clone();
            }
            KeyCode::Char('d') => {
                // Remove entry from list
                if len > 1 {
                    state.entries.remove(state.cursor);
                    if state.cursor >= state.entries.len() {
                        state.cursor = state.entries.len() - 1;
                    }
                }
            }
            KeyCode::Char(':') => {
                state.find_replace_input.clear();
                state.sub_mode = BulkRenameSubMode::FindReplace;
                state.error = None;
            }
            KeyCode::Enter => {
                self.execute_bulk_rename();
                return;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.bulk_rename = None;
                self.mode = Mode::Normal;
                return;
            }
            _ => {}
        }

        // Adjust scroll
        if let Some(ref mut state) = self.bulk_rename {
            state.adjust_scroll(self.visible_height.saturating_sub(6));
        }
    }

    fn handle_bulk_rename_edit(&mut self, key: KeyEvent) {
        let state = self.bulk_rename.as_mut().unwrap();

        match key.code {
            KeyCode::Esc => {
                // Commit edit to entry
                let input = state.edit_input.trim().to_string();
                if !input.is_empty() {
                    state.entries[state.cursor].new_name = input;
                }
                state.sub_mode = BulkRenameSubMode::Nav;
            }
            KeyCode::Enter => {
                // Commit edit and move to next
                let input = state.edit_input.trim().to_string();
                if !input.is_empty() {
                    state.entries[state.cursor].new_name = input;
                }
                if state.cursor + 1 < state.entries.len() {
                    state.cursor += 1;
                    state.edit_input = state.entries[state.cursor].new_name.clone();
                    state.adjust_scroll(self.visible_height.saturating_sub(6));
                } else {
                    state.sub_mode = BulkRenameSubMode::Nav;
                }
            }
            KeyCode::Tab => {
                // Commit and move to next, entering edit mode
                let input = state.edit_input.trim().to_string();
                if !input.is_empty() {
                    state.entries[state.cursor].new_name = input;
                }
                if state.cursor + 1 < state.entries.len() {
                    state.cursor += 1;
                    state.edit_input = state.entries[state.cursor].new_name.clone();
                    state.adjust_scroll(self.visible_height.saturating_sub(6));
                } else {
                    state.sub_mode = BulkRenameSubMode::Nav;
                }
            }
            KeyCode::BackTab => {
                // Commit and move to previous
                let input = state.edit_input.trim().to_string();
                if !input.is_empty() {
                    state.entries[state.cursor].new_name = input;
                }
                if state.cursor > 0 {
                    state.cursor -= 1;
                    state.edit_input = state.entries[state.cursor].new_name.clone();
                    state.adjust_scroll(self.visible_height.saturating_sub(6));
                } else {
                    state.sub_mode = BulkRenameSubMode::Nav;
                }
            }
            KeyCode::Backspace => {
                state.edit_input.pop();
            }
            KeyCode::Char(c) => {
                state.edit_input.push(c);
            }
            _ => {}
        }
    }

    fn handle_bulk_rename_find_replace(&mut self, key: KeyEvent) {
        let state = self.bulk_rename.as_mut().unwrap();

        match key.code {
            KeyCode::Enter => {
                let input = state.find_replace_input.clone();
                state.apply_find_replace(&input);
                state.sub_mode = BulkRenameSubMode::Nav;
            }
            KeyCode::Esc => {
                state.sub_mode = BulkRenameSubMode::Nav;
            }
            KeyCode::Backspace => {
                if state.find_replace_input.is_empty() {
                    state.sub_mode = BulkRenameSubMode::Nav;
                } else {
                    state.find_replace_input.pop();
                }
            }
            KeyCode::Char(c) => {
                state.find_replace_input.push(c);
            }
            _ => {}
        }
    }

    fn execute_bulk_rename(&mut self) {
        let state = match self.bulk_rename.take() {
            Some(s) => s,
            None => return,
        };

        if state.has_conflicts() {
            self.bulk_rename = Some(state);
            if let Some(ref mut s) = self.bulk_rename {
                s.error = Some("Fix conflicts before applying".into());
            }
            return;
        }

        // Collect only changed entries
        let renames: Vec<(PathBuf, String)> = state
            .entries
            .iter()
            .filter(|e| e.new_name != e.original_name)
            .map(|e| (e.original_path.clone(), e.new_name.clone()))
            .collect();

        if renames.is_empty() {
            self.status_message = "No changes to apply".into();
            self.mode = Mode::Normal;
            return;
        }

        let count = renames.len();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);

        tokio::task::spawn_blocking(move || {
            let mut records = Vec::new();
            let mut errors = Vec::new();

            // Detect conflicts: if a target name matches an existing source,
            // use a temporary name first to avoid collisions (handles swaps).
            let source_names: HashSet<String> = renames.iter().filter_map(|(p, _)| {
                p.file_name().map(|n| n.to_string_lossy().into_owned())
            }).collect();

            // Find entries that need a temp name: their target exists as another source
            let mut temp_renames: Vec<(PathBuf, String, String)> = Vec::new(); // (path, temp_name, final_name)
            let mut direct_renames: Vec<(PathBuf, String)> = Vec::new();

            for (path, new_name) in &renames {
                let old_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                // Check if new_name collides with an existing source that hasn't been renamed yet
                if source_names.contains(new_name) && *new_name != old_name {
                    // Use temporary name to break the cycle
                    let temp = format!(".fcmd_tmp_{}", new_name);
                    temp_renames.push((path.clone(), temp, new_name.clone()));
                } else {
                    direct_renames.push((path.clone(), new_name.clone()));
                }
            }

            // Phase 1: Rename conflict entries to temp names
            for (path, temp_name, _) in &temp_renames {
                match ops::rename_path(path, temp_name) {
                    Ok(_) => {}
                    Err(e) => errors.push(format!("{}: {e}", temp_name)),
                }
            }

            // Phase 2: Direct renames (no conflicts)
            for (path, new_name) in &direct_renames {
                match ops::rename_path(path, new_name) {
                    Ok(record) => records.push(record),
                    Err(e) => errors.push(format!("{}: {e}", new_name)),
                }
            }

            // Phase 3: Rename temp names to final names
            for (path, temp_name, final_name) in &temp_renames {
                let temp_path = path.with_file_name(temp_name);
                match ops::rename_path(&temp_path, final_name) {
                    Ok(_) => {
                        // Record the original→final rename for undo
                        let final_path = path.with_file_name(final_name);
                        records.push(ops::OpRecord::Renamed {
                            from: path.clone(),
                            to: final_path,
                        });
                    }
                    Err(e) => errors.push(format!("{}: {e}", final_name)),
                }
            }

            let _ = tx.send(FileOpResult::BulkRename {
                total: count,
                records,
                errors,
            });
        });

        self.mode = Mode::Normal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bulk_entries(names: &[&str]) -> Vec<BulkRenameEntry> {
        names
            .iter()
            .map(|&name| BulkRenameEntry {
                original_path: PathBuf::from(format!("/test/{name}")),
                original_name: name.to_string(),
                new_name: name.to_string(),
                is_dir: false,
            })
            .collect()
    }

    #[test]
    fn changed_count_initially_zero() {
        let state = BulkRenameState::new(make_bulk_entries(&["a.txt", "b.txt"]));
        assert_eq!(state.changed_count(), 0);
    }

    #[test]
    fn changed_count_after_edit() {
        let mut state = BulkRenameState::new(make_bulk_entries(&["a.txt", "b.txt"]));
        state.entries[0].new_name = "c.txt".into();
        assert_eq!(state.changed_count(), 1);
    }

    #[test]
    fn has_conflicts_duplicate() {
        let mut state = BulkRenameState::new(make_bulk_entries(&["a.txt", "b.txt"]));
        state.entries[1].new_name = "a.txt".into();
        assert!(state.has_conflicts());
    }

    #[test]
    fn has_conflicts_empty() {
        let mut state = BulkRenameState::new(make_bulk_entries(&["a.txt"]));
        state.entries[0].new_name.clear();
        assert!(state.has_conflicts());
    }

    #[test]
    fn no_conflicts_when_valid() {
        let state = BulkRenameState::new(make_bulk_entries(&["a.txt", "b.txt"]));
        assert!(!state.has_conflicts());
    }

    #[test]
    fn conflict_indices_marks_duplicates() {
        let mut state = BulkRenameState::new(make_bulk_entries(&["a.txt", "b.txt", "c.txt"]));
        state.entries[2].new_name = "a.txt".into();
        let conflicts = state.conflict_indices();
        assert!(conflicts.contains(&0));
        assert!(conflicts.contains(&2));
        assert!(!conflicts.contains(&1));
    }

    #[test]
    fn find_replace_basic() {
        let mut state = BulkRenameState::new(make_bulk_entries(&["photo_001.jpg", "photo_002.jpg"]));
        state.apply_find_replace("%s/photo/vacation");
        assert_eq!(state.entries[0].new_name, "vacation_001.jpg");
        assert_eq!(state.entries[1].new_name, "vacation_002.jpg");
    }

    #[test]
    fn find_replace_with_separator() {
        let mut state = BulkRenameState::new(make_bulk_entries(&["a-b.txt"]));
        state.apply_find_replace("s|a-b|c-d");
        assert_eq!(state.entries[0].new_name, "c-d.txt");
    }

    #[test]
    fn find_replace_no_match() {
        let mut state = BulkRenameState::new(make_bulk_entries(&["a.txt"]));
        state.apply_find_replace("%s/zzz/xxx");
        assert!(state.error.is_some());
    }

    #[tokio::test]
    async fn enter_bulk_rename_from_selected() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1; // select a.txt
        app.enter_bulk_rename();
        assert_eq!(app.mode, Mode::BulkRename);
        assert!(app.bulk_rename.is_some());
        assert_eq!(app.bulk_rename.as_ref().unwrap().entries.len(), 1);
    }

    #[tokio::test]
    async fn bulk_rename_nav_movement() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        // Select all and enter bulk rename
        app.select_all();
        app.enter_bulk_rename();

        let state = app.bulk_rename.as_ref().unwrap();
        assert_eq!(state.cursor, 0);

        app.handle_bulk_rename(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.bulk_rename.as_ref().unwrap().cursor, 1);

        app.handle_bulk_rename(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.bulk_rename.as_ref().unwrap().cursor, 0);
    }

    #[tokio::test]
    async fn bulk_rename_esc_exits() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_bulk_rename();
        assert_eq!(app.mode, Mode::BulkRename);

        app.handle_bulk_rename(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.bulk_rename.is_none());
    }

    #[tokio::test]
    async fn bulk_rename_edit_mode() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_bulk_rename();

        // Enter edit mode
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(
            app.bulk_rename.as_ref().unwrap().sub_mode,
            BulkRenameSubMode::Edit
        );

        // Type new name
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        for c in "b.txt".chars() {
            app.handle_bulk_rename(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }

        // Esc commits
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            app.bulk_rename.as_ref().unwrap().sub_mode,
            BulkRenameSubMode::Nav
        );
        assert_eq!(app.bulk_rename.as_ref().unwrap().entries[0].new_name, "b.txt");
    }

    #[tokio::test]
    async fn bulk_rename_undo_line() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.active_panel_mut().selected = 1;
        app.enter_bulk_rename();

        // Modify entry directly
        app.bulk_rename.as_mut().unwrap().entries[0].new_name = "changed.txt".into();

        // Undo
        app.handle_bulk_rename(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        assert_eq!(
            app.bulk_rename.as_ref().unwrap().entries[0].new_name,
            "a.txt"
        );
    }

    #[tokio::test]
    async fn bulk_rename_remove_entry() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        app.select_all();
        app.enter_bulk_rename();
        assert_eq!(app.bulk_rename.as_ref().unwrap().entries.len(), 2);

        app.handle_bulk_rename(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(app.bulk_rename.as_ref().unwrap().entries.len(), 1);
    }
}
