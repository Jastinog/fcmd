use super::task_manager::TaskState;
use super::*;

impl App {
    pub(super) fn open_tasks(&mut self) {
        if self.task_manager.tasks().is_empty() {
            self.status_message = "No tasks".into();
            return;
        }
        self.tasks_cursor = 0;
        self.tasks_scroll = 0;
        self.mode = Mode::Tasks;
    }

    pub(super) fn handle_tasks(&mut self, key: KeyEvent) {
        let len = self.task_manager.tasks().len();
        if len == 0 {
            self.mode = Mode::Normal;
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down => {
                self.tasks_cursor = (self.tasks_cursor + 1).min(len - 1);
                self.adjust_tasks_scroll();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tasks_cursor = self.tasks_cursor.saturating_sub(1);
                self.adjust_tasks_scroll();
            }
            KeyCode::Char('g') => {
                self.tasks_cursor = 0;
                self.adjust_tasks_scroll();
            }
            KeyCode::Char('G') => {
                self.tasks_cursor = len - 1;
                self.adjust_tasks_scroll();
            }
            // Cancel the task under the cursor (only meaningful while it's running).
            KeyCode::Char('x') | KeyCode::Char('d') => self.cancel_selected_task(),
            // Clear finished/cancelled tasks from the list.
            KeyCode::Char('c') => self.clear_finished_tasks(),
            _ => {}
        }
    }

    fn cancel_selected_task(&mut self) {
        let Some(task) = self.task_manager.tasks().get(self.tasks_cursor) else {
            return;
        };
        if !matches!(task.state, TaskState::Running { .. }) {
            self.status_message = "Task already finished".into();
            return;
        }
        let id = task.id;
        self.task_manager.cancel(id);
        self.status_message = "Cancelling task...".into();
    }

    fn clear_finished_tasks(&mut self) {
        let before = self.task_manager.tasks().len();
        self.task_manager.remove_finished();
        let remaining = self.task_manager.tasks().len();
        let cleared = before - remaining;
        if remaining == 0 {
            self.mode = Mode::Normal;
        } else {
            self.tasks_cursor = self.tasks_cursor.min(remaining - 1);
            self.adjust_tasks_scroll();
        }
        if cleared > 0 {
            self.status_message = format!("Cleared {cleared} finished task(s)");
        } else {
            self.status_message = "No finished tasks to clear".into();
        }
    }

    fn adjust_tasks_scroll(&mut self) {
        // Mirror the overlay's centered_rect(70%) sizing so the cursor stays visible.
        let max_h = (self.visible_height * 70 / 100).max(2);
        let list_h = max_h.saturating_sub(4).max(1);
        if self.tasks_cursor < self.tasks_scroll {
            self.tasks_scroll = self.tasks_cursor;
        } else if self.tasks_cursor >= self.tasks_scroll + list_h {
            self.tasks_scroll = self.tasks_cursor - list_h + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_tasks_empty_shows_message() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.open_tasks();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_message.contains("No tasks"));
    }

    #[tokio::test]
    async fn open_tasks_with_task_enters_mode() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        app.task_manager
            .add_copy(rx, PathBuf::from("/dst"), vec![], cancel);
        app.open_tasks();
        assert_eq!(app.mode, Mode::Tasks);
        assert_eq!(app.tasks_cursor, 0);
    }

    #[tokio::test]
    async fn cancel_selected_sets_flag() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&cancel);
        app.task_manager
            .add_copy(rx, PathBuf::from("/dst"), vec![], cancel);
        app.open_tasks();
        app.handle_tasks(KeyEvent::from(KeyCode::Char('x')));
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn esc_exits_tasks_mode() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        app.task_manager
            .add_copy(rx, PathBuf::from("/dst"), vec![], cancel);
        app.open_tasks();
        app.handle_tasks(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn clear_finished_removes_completed() {
        let entries = make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        app.task_manager
            .add_copy(rx, PathBuf::from("/dst"), vec![], cancel);
        app.open_tasks();
        // Mark finished, then clear.
        app.task_manager.cancel(1); // no-op flag, still running until polled
        app.handle_tasks(KeyEvent::from(KeyCode::Char('c')));
        // Still running (cancel doesn't finish synchronously), so nothing cleared.
        assert_eq!(app.task_manager.tasks().len(), 1);
    }
}
