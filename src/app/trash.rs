use super::*;
use crate::fs::trash::TrashedItem;

impl App {
    /// Open the trash-restore overlay, listing everything trashed this session
    /// that is still recoverable.
    pub(super) fn open_trash(&mut self) {
        if self.undo_stack.trashed().is_empty() {
            self.status_message = "Nothing to restore (nothing trashed this session)".into();
            return;
        }
        self.trash_cursor = 0;
        self.trash_scroll = 0;
        self.mode = Mode::Trash;
    }

    pub(super) fn handle_trash(&mut self, key: KeyEvent) {
        let items = self.undo_stack.trashed();
        let len = items.len();
        if len == 0 {
            self.mode = Mode::Normal;
            return;
        }
        self.trash_cursor = self.trash_cursor.min(len - 1);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.trash_cursor = (self.trash_cursor + 1).min(len - 1);
                self.adjust_trash_scroll();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.trash_cursor = self.trash_cursor.saturating_sub(1);
                self.adjust_trash_scroll();
            }
            KeyCode::Char('g') => {
                self.trash_cursor = 0;
                self.adjust_trash_scroll();
            }
            KeyCode::Char('G') => {
                self.trash_cursor = len - 1;
                self.adjust_trash_scroll();
            }
            KeyCode::Enter | KeyCode::Char('r') => {
                let id = items[self.trash_cursor].id;
                self.restore_trashed(vec![id]);
            }
            KeyCode::Char('R') => {
                let ids: Vec<u64> = items.iter().map(|it| it.id).collect();
                self.restore_trashed(ids);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    /// Restore the items with the given ids in the background. The undo history
    /// is pruned only for items that successfully return (see `TrashRestore`).
    fn restore_trashed(&mut self, ids: Vec<u64>) {
        let trashed = self.undo_stack.trashed();
        let items: Vec<TrashedItem> = trashed
            .into_iter()
            .filter(|it| ids.contains(&it.id))
            .collect();
        if items.is_empty() {
            return;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.file_op_rx = Some(rx);
        tokio::task::spawn_blocking(move || {
            let mut restored_ids = Vec::new();
            let mut errors = Vec::new();
            for item in &items {
                match crate::fs::trash::restore(item) {
                    Ok(()) => restored_ids.push(item.id),
                    Err(e) => errors.push(format!("{}: {e}", item.name())),
                }
            }
            let ok = restored_ids.len();
            let result = if errors.is_empty() {
                Ok(format!("Restored {ok} item(s)"))
            } else if ok == 0 {
                Err(errors.join("; "))
            } else {
                Err(format!("restored {ok}, {} failed: {}", errors.len(), errors[0]))
            };
            let _ = tx.send(super::FileOpResult::TrashRestore {
                restored_ids,
                result,
            });
        });
    }

    pub(super) fn adjust_trash_scroll(&mut self) {
        let max_h = (self.visible_height * 70 / 100).max(2);
        let list_h = max_h.saturating_sub(4).max(1);
        if self.trash_cursor < self.trash_scroll {
            self.trash_scroll = self.trash_cursor;
        } else if self.trash_cursor >= self.trash_scroll + list_h {
            self.trash_scroll = self.trash_cursor - list_h + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::trash::TrashedItem;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    /// Build an app with `n` fake trashed items on the undo stack.
    fn app_with_trash(n: usize) -> App {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        let records: Vec<ops::OpRecord> = (0..n)
            .map(|i| {
                ops::OpRecord::Trashed(TrashedItem::new_for_test(
                    PathBuf::from(format!("/tmp/fcmd_trash_{i}")),
                ))
            })
            .collect();
        app.undo_stack.push(records);
        app
    }

    #[tokio::test]
    async fn open_trash_empty_stays_normal() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.open_trash();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_message.contains("Nothing to restore"));
    }

    #[tokio::test]
    async fn open_trash_with_items_enters_mode() {
        let mut app = app_with_trash(3);
        app.open_trash();
        assert_eq!(app.mode, Mode::Trash);
        assert_eq!(app.trash_cursor, 0);
    }

    #[tokio::test]
    async fn handle_trash_navigation() {
        let mut app = app_with_trash(3);
        app.open_trash();
        app.handle_trash(key('j'));
        assert_eq!(app.trash_cursor, 1);
        app.handle_trash(key('G'));
        assert_eq!(app.trash_cursor, 2);
        // clamp at bottom
        app.handle_trash(key('j'));
        assert_eq!(app.trash_cursor, 2);
        app.handle_trash(key('g'));
        assert_eq!(app.trash_cursor, 0);
        app.handle_trash(key('k'));
        assert_eq!(app.trash_cursor, 0);
    }

    #[tokio::test]
    async fn handle_trash_esc_exits() {
        let mut app = app_with_trash(2);
        app.open_trash();
        app.handle_trash(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn trashed_listing_is_newest_first() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.undo_stack.push(vec![ops::OpRecord::Trashed(
            TrashedItem::new_for_test(PathBuf::from("/tmp/old")),
        )]);
        app.undo_stack.push(vec![ops::OpRecord::Trashed(
            TrashedItem::new_for_test(PathBuf::from("/tmp/new")),
        )]);
        let listed = app.undo_stack.trashed();
        assert_eq!(listed[0].original_path, PathBuf::from("/tmp/new"));
        assert_eq!(listed[1].original_path, PathBuf::from("/tmp/old"));
    }

    #[tokio::test]
    async fn remove_trashed_drops_empty_batch() {
        let mut app = app_with_trash(1);
        let id = app.undo_stack.trashed()[0].id;
        assert!(app.undo_stack.remove_trashed(id).is_some());
        assert!(app.undo_stack.trashed().is_empty());
        // Nothing left to undo either.
        assert!(app.undo_stack.pop().is_none());
    }
}
