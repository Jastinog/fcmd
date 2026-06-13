//! Live filter: narrow the active panel's listing in place as you type.
//!
//! Unlike incremental search (`/`), which only moves the cursor, the filter
//! hides non-matching entries. It is a persistent per-panel state (see
//! [`crate::model::panel::Panel::set_filter`]) that clears on navigation; this
//! module owns the interactive editing of it.

use super::*;

impl App {
    /// Enter filter-edit mode, pre-filled with the panel's current filter so it
    /// can be refined. `filter_prev` remembers the active filter to restore on
    /// cancel.
    pub(super) fn enter_filter(&mut self) {
        let current = self.active_panel().filter.clone();
        self.filter_prev = current.clone();
        self.filter_input = current;
        self.mode = Mode::Filter;
    }

    pub(super) fn handle_filter(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.filter_input.push(c);
                self.apply_live_filter();
            }
            KeyCode::Backspace => {
                self.filter_input.pop();
                self.apply_live_filter();
            }
            KeyCode::Enter => {
                // Keep the live-applied filter; just leave edit mode.
                self.mode = Mode::Normal;
                let active = !self.active_panel().filter.is_empty();
                self.status_message = if active {
                    format!("Filter: {}", self.active_panel().filter)
                } else {
                    String::new()
                };
            }
            KeyCode::Esc => {
                // Revert to whatever filter was active when editing began.
                let prev = std::mem::take(&mut self.filter_prev);
                self.active_panel_mut().set_filter(prev);
                self.filter_input.clear();
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    /// Push the current input buffer into the panel as a live filter.
    fn apply_live_filter(&mut self) {
        let q = self.filter_input.clone();
        self.active_panel_mut().set_filter(q);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[tokio::test]
    async fn typing_narrows_listing() {
        let entries = make_test_entries(&["alpha.rs", "beta.txt", "gamma.rs"]);
        let mut app = App::new_for_test(entries);
        app.enter_filter();
        assert_eq!(app.mode, Mode::Filter);

        app.handle_filter(key(KeyCode::Char('r')));
        app.handle_filter(key(KeyCode::Char('s')));
        // ".." is always kept, plus the two ".rs" files.
        let names: Vec<&str> = app
            .active_panel()
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, vec!["..", "alpha.rs", "gamma.rs"]);
        // Full listing is preserved behind the filter.
        assert_eq!(app.active_panel().full_entries.len(), 4);
    }

    #[tokio::test]
    async fn enter_keeps_filter() {
        let entries = make_test_entries(&["alpha.rs", "beta.txt"]);
        let mut app = App::new_for_test(entries);
        app.enter_filter();
        app.handle_filter(key(KeyCode::Char('a')));
        app.handle_filter(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.active_panel().filter, "a");
    }

    #[tokio::test]
    async fn esc_reverts_to_previous_filter() {
        let entries = make_test_entries(&["alpha.rs", "beta.txt", "gamma.rs"]);
        let mut app = App::new_for_test(entries);
        // Establish an active filter "rs".
        app.active_panel_mut().set_filter("rs".into());
        assert_eq!(app.active_panel().entries.len(), 3); // .. + 2 matches

        // Edit it, then cancel.
        app.enter_filter();
        app.handle_filter(key(KeyCode::Char('x'))); // "rsx" → no file matches
        assert_eq!(app.active_panel().entries.len(), 1); // just ".."
        app.handle_filter(key(KeyCode::Esc));

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.active_panel().filter, "rs");
        assert_eq!(app.active_panel().entries.len(), 3);
    }

    #[tokio::test]
    async fn backspace_to_empty_clears_filter() {
        let entries = make_test_entries(&["alpha.rs", "beta.txt"]);
        let mut app = App::new_for_test(entries);
        app.enter_filter();
        app.handle_filter(key(KeyCode::Char('a')));
        app.handle_filter(key(KeyCode::Backspace));
        assert!(app.active_panel().filter.is_empty());
        assert_eq!(app.active_panel().entries.len(), 3); // full list restored
        assert!(app.active_panel().full_entries.is_empty());
    }
}
