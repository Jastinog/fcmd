use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use tokio::sync::mpsc;

use crate::fs::ops::{OpRecord, ProgressMsg};

use super::messages::{ArchiveMsg, DeleteMsg, PhantomEntry};

/// Cap on retained finished tasks, so completed-task history doesn't grow without
/// bound. Oldest finished entries are pruned first.
const MAX_FINISHED: usize = 50;

pub struct TaskManager {
    tasks: Vec<Task>,
    next_id: u32,
}

#[allow(dead_code)]
pub struct Task {
    pub id: u32,
    pub kind: TaskKind,
    pub started_at: Instant,
    pub state: TaskState,
    /// Shared with the background worker; setting it asks the task to stop between items.
    pub cancel: Arc<AtomicBool>,
}

pub enum TaskKind {
    Copy {
        rx: mpsc::Receiver<ProgressMsg>,
        dst_dir: PathBuf,
        phantoms: Vec<PhantomEntry>,
    },
    Move {
        rx: mpsc::Receiver<ProgressMsg>,
        dst_dir: PathBuf,
        phantoms: Vec<PhantomEntry>,
    },
    Delete {
        rx: mpsc::Receiver<DeleteMsg>,
        permanent: bool,
    },
    Archive {
        rx: mpsc::Receiver<ArchiveMsg>,
        /// True for create, false for extract — controls the displayed verb.
        is_create: bool,
    },
}

#[allow(dead_code)]
pub enum TaskState {
    Running {
        progress_pct: u8,
        status_text: String,
    },
    Finished {
        success: bool,
        cancelled: bool,
        summary: String,
    },
}

// Each variant names the operation that finished; the shared suffix is intentional.
#[allow(clippy::enum_variant_names)]
pub enum TaskEvent {
    PasteFinished {
        records: Vec<OpRecord>,
        error: Option<String>,
        is_copy: bool,
        summary: String,
    },
    DeleteFinished {
        summary: String,
    },
    ArchiveFinished {
        summary: String,
    },
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add_copy(
        &mut self,
        rx: mpsc::Receiver<ProgressMsg>,
        dst_dir: PathBuf,
        phantoms: Vec<PhantomEntry>,
        cancel: Arc<AtomicBool>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task {
            id,
            kind: TaskKind::Copy {
                rx,
                dst_dir,
                phantoms,
            },
            started_at: Instant::now(),
            state: TaskState::Running {
                progress_pct: 0,
                status_text: "Copying...".into(),
            },
            cancel,
        });
        id
    }

    pub fn add_move(
        &mut self,
        rx: mpsc::Receiver<ProgressMsg>,
        dst_dir: PathBuf,
        phantoms: Vec<PhantomEntry>,
        cancel: Arc<AtomicBool>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task {
            id,
            kind: TaskKind::Move {
                rx,
                dst_dir,
                phantoms,
            },
            started_at: Instant::now(),
            state: TaskState::Running {
                progress_pct: 0,
                status_text: "Moving...".into(),
            },
            cancel,
        });
        id
    }

    pub fn add_delete(
        &mut self,
        rx: mpsc::Receiver<DeleteMsg>,
        permanent: bool,
        cancel: Arc<AtomicBool>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let verb = if permanent { "Deleting" } else { "Trashing" };
        self.tasks.push(Task {
            id,
            kind: TaskKind::Delete { rx, permanent },
            started_at: Instant::now(),
            state: TaskState::Running {
                progress_pct: 0,
                status_text: format!("{verb}..."),
            },
            cancel,
        });
        id
    }

    pub fn add_archive(
        &mut self,
        rx: mpsc::Receiver<ArchiveMsg>,
        is_create: bool,
        cancel: Arc<AtomicBool>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let verb = if is_create { "Archiving" } else { "Extracting" };
        self.tasks.push(Task {
            id,
            kind: TaskKind::Archive { rx, is_create },
            started_at: Instant::now(),
            state: TaskState::Running {
                progress_pct: 0,
                status_text: format!("{verb}..."),
            },
            cancel,
        });
        id
    }

    pub fn poll_all(&mut self) -> Vec<TaskEvent> {
        let mut events = Vec::new();

        for task in &mut self.tasks {
            let is_copy = matches!(task.kind, TaskKind::Copy { .. });
            match &mut task.kind {
                TaskKind::Copy { rx, .. } | TaskKind::Move { rx, .. } => {
                    let mut last_progress = None;
                    let mut finished = None;

                    loop {
                        match rx.try_recv() {
                            Ok(msg @ ProgressMsg::Progress { .. }) => {
                                last_progress = Some(msg);
                            }
                            Ok(msg @ ProgressMsg::Finished { .. }) => {
                                finished = Some(msg);
                                break;
                            }
                            Err(_) => break,
                        }
                    }

                    if let Some(ProgressMsg::Progress {
                        bytes_done,
                        bytes_total,
                        item_index,
                        item_total,
                    }) = last_progress
                    {
                        let pct = if bytes_total > 0 {
                            (bytes_done as f64 / bytes_total as f64 * 100.0) as u8
                        } else {
                            0
                        };
                        let verb = if is_copy { "Copying" } else { "Moving" };
                        let size_text = format!(
                            "{}/{}",
                            crate::util::format_bytes(bytes_done),
                            crate::util::format_bytes(bytes_total)
                        );
                        task.state = TaskState::Running {
                            progress_pct: pct,
                            status_text: format!(
                                "{verb} {}/{} ({size_text})",
                                item_index + 1,
                                item_total,
                            ),
                        };
                    }

                    if let Some(ProgressMsg::Finished {
                        records,
                        error,
                        bytes_total,
                        cancelled,
                    }) = finished
                    {
                        let elapsed = task.started_at.elapsed();
                        let verb = if is_copy { "Copied" } else { "Moved" };
                        let n = records.len();
                        let summary = if let Some(ref e) = error {
                            format!("Paste error: {e}")
                        } else if cancelled {
                            format!("Cancelled \u{2014} {verb} {n} item(s) before stop")
                        } else {
                            format!(
                                "{verb} {n} item(s), {} in {}",
                                crate::util::format_bytes(bytes_total),
                                crate::util::format_duration(elapsed),
                            )
                        };
                        task.state = TaskState::Finished {
                            success: error.is_none() && !cancelled,
                            cancelled,
                            summary: summary.clone(),
                        };
                        events.push(TaskEvent::PasteFinished {
                            records,
                            error,
                            is_copy,
                            summary,
                        });
                    }
                }
                TaskKind::Delete { rx, permanent } => {
                    let permanent = *permanent;
                    let mut last_progress = None;
                    let mut finished = None;

                    loop {
                        match rx.try_recv() {
                            Ok(msg @ DeleteMsg::Progress { .. }) => {
                                last_progress = Some(msg);
                            }
                            Ok(msg @ DeleteMsg::Finished { .. }) => {
                                finished = Some(msg);
                                break;
                            }
                            Err(_) => break,
                        }
                    }

                    if let Some(DeleteMsg::Progress {
                        done,
                        total,
                        current,
                    }) = last_progress
                    {
                        let pct = if total > 0 {
                            (done as f64 / total as f64 * 100.0) as u8
                        } else {
                            0
                        };
                        let verb = if permanent { "Deleting" } else { "Trashing" };
                        task.state = TaskState::Running {
                            progress_pct: pct,
                            status_text: format!("{verb} [{}/{}] {current}", done + 1, total),
                        };
                    }

                    if let Some(DeleteMsg::Finished {
                        deleted,
                        errors,
                        permanent,
                        cancelled,
                    }) = finished
                    {
                        let verb = if permanent { "Deleted" } else { "Trashed" };
                        let summary = if cancelled && errors.is_empty() {
                            format!("Cancelled \u{2014} {verb} {deleted} item(s) before stop")
                        } else if errors.is_empty() {
                            format!("{verb} {deleted} item(s)")
                        } else if deleted == 0 {
                            format!("{verb} failed: {}", errors[0])
                        } else {
                            format!("{verb} {deleted}, {} failed: {}", errors.len(), errors[0])
                        };
                        task.state = TaskState::Finished {
                            success: errors.is_empty() && !cancelled,
                            cancelled,
                            summary: summary.clone(),
                        };
                        events.push(TaskEvent::DeleteFinished { summary });
                    }
                }
                TaskKind::Archive { rx, is_create } => {
                    let is_create = *is_create;
                    let mut last_progress = None;
                    let mut finished = None;

                    loop {
                        match rx.try_recv() {
                            Ok(msg @ ArchiveMsg::Progress { .. }) => {
                                last_progress = Some(msg);
                            }
                            Ok(msg @ ArchiveMsg::Finished { .. }) => {
                                finished = Some(msg);
                                break;
                            }
                            Err(_) => break,
                        }
                    }

                    if let Some(ArchiveMsg::Progress {
                        done,
                        total,
                        current,
                    }) = last_progress
                    {
                        let pct = if total > 0 {
                            ((done as f64 / total as f64 * 100.0) as u8).min(100)
                        } else {
                            0
                        };
                        let verb = if is_create { "Archiving" } else { "Extracting" };
                        let count = if total > 0 {
                            format!("[{}/{}] ", (done + 1).min(total), total)
                        } else {
                            String::new()
                        };
                        task.state = TaskState::Running {
                            progress_pct: pct,
                            status_text: format!("{verb} {count}{current}"),
                        };
                    }

                    if let Some(ArchiveMsg::Finished {
                        is_create,
                        processed,
                        skipped,
                        error,
                        cancelled,
                        label,
                    }) = finished
                    {
                        let elapsed = task.started_at.elapsed();
                        let verb = if is_create { "Archived" } else { "Extracted" };
                        let summary = if let Some(ref e) = error {
                            let what = if is_create { "Archive" } else { "Extract" };
                            format!("{what} error: {e}")
                        } else if cancelled {
                            format!("Cancelled \u{2014} {verb} {processed} item(s) before stop")
                        } else {
                            let skip_note = if skipped > 0 {
                                format!(", {skipped} skipped")
                            } else {
                                String::new()
                            };
                            format!(
                                "{verb} {label} ({processed} item(s){skip_note}) in {}",
                                crate::util::format_duration(elapsed),
                            )
                        };
                        task.state = TaskState::Finished {
                            success: error.is_none() && !cancelled,
                            cancelled,
                            summary: summary.clone(),
                        };
                        events.push(TaskEvent::ArchiveFinished { summary });
                    }
                }
            }
        }

        events
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn active_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Running { .. }))
            .count()
    }

    /// Drop all finished/cancelled tasks, keeping only those still running.
    pub fn remove_finished(&mut self) {
        self.tasks
            .retain(|t| matches!(t.state, TaskState::Running { .. }));
    }

    /// Ask the task with `id` to stop. The flag is read by the background worker between
    /// items; the row stays Running until the worker acknowledges and reports Finished.
    pub fn cancel(&mut self, id: u32) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id)
            && let TaskState::Running { status_text, .. } = &mut task.state
        {
            task.cancel.store(true, Ordering::Relaxed);
            *status_text = "Cancelling...".into();
        }
    }

    /// Keep retained finished history bounded: if more than `MAX_FINISHED` finished tasks
    /// are present, drop the oldest ones. Running tasks are never dropped.
    pub fn prune_finished(&mut self) {
        let finished = self
            .tasks
            .iter()
            .filter(|t| matches!(t.state, TaskState::Finished { .. }))
            .count();
        if finished <= MAX_FINISHED {
            return;
        }
        let mut to_drop = finished - MAX_FINISHED;
        self.tasks.retain(|t| {
            if to_drop > 0 && matches!(t.state, TaskState::Finished { .. }) {
                to_drop -= 1;
                false
            } else {
                true
            }
        });
    }

    /// Short label for the operation kind, used in the task-manager overlay.
    pub fn kind_label(task: &Task) -> &'static str {
        match task.kind {
            TaskKind::Copy { .. } => "Copy",
            TaskKind::Move { .. } => "Move",
            TaskKind::Delete {
                permanent: true, ..
            } => "Delete",
            TaskKind::Delete {
                permanent: false, ..
            } => "Trash",
            TaskKind::Archive {
                is_create: true, ..
            } => "Archive",
            TaskKind::Archive {
                is_create: false, ..
            } => "Extract",
        }
    }

    /// Get phantom entries for a given directory (from all running paste tasks targeting it).
    pub fn phantoms_for(&self, dir: &std::path::Path) -> Vec<&PhantomEntry> {
        let mut result = Vec::new();
        for task in &self.tasks {
            match &task.kind {
                TaskKind::Copy {
                    dst_dir, phantoms, ..
                }
                | TaskKind::Move {
                    dst_dir, phantoms, ..
                } if dst_dir == dir && matches!(task.state, TaskState::Running { .. }) => {
                    result.extend(phantoms.iter());
                }
                _ => {}
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn new_starts_empty() {
        let tm = TaskManager::new();
        assert!(tm.tasks().is_empty());
        assert_eq!(tm.active_count(), 0);
    }

    #[test]
    fn add_copy_returns_unique_ids() {
        let mut tm = TaskManager::new();
        let (_tx1, rx1) = mpsc::channel(1);
        let (_tx2, rx2) = mpsc::channel(1);
        let id1 = tm.add_copy(rx1, PathBuf::from("/dst"), vec![], flag());
        let id2 = tm.add_copy(rx2, PathBuf::from("/dst"), vec![], flag());
        assert_ne!(id1, id2);
        assert_eq!(tm.tasks().len(), 2);
        assert_eq!(tm.active_count(), 2);
    }

    #[test]
    fn add_move_creates_task() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        let id = tm.add_move(rx, PathBuf::from("/dst"), vec![], flag());
        assert_eq!(tm.tasks().len(), 1);
        assert_eq!(tm.tasks()[0].id, id);
        assert!(matches!(tm.tasks()[0].state, TaskState::Running { .. }));
    }

    #[test]
    fn add_delete_creates_task() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        let _id = tm.add_delete(rx, false, flag());
        assert_eq!(tm.tasks().len(), 1);
        assert_eq!(tm.active_count(), 1);
    }

    #[test]
    fn remove_finished_clears_completed() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        tm.add_copy(rx, PathBuf::from("/dst"), vec![], flag());
        // Manually mark as finished
        tm.tasks[0].state = TaskState::Finished {
            success: true,
            cancelled: false,
            summary: "done".into(),
        };
        assert_eq!(tm.active_count(), 0);
        tm.remove_finished();
        assert!(tm.tasks().is_empty());
    }

    #[test]
    fn remove_finished_keeps_running() {
        let mut tm = TaskManager::new();
        let (_tx1, rx1) = mpsc::channel(1);
        let (_tx2, rx2) = mpsc::channel(1);
        tm.add_copy(rx1, PathBuf::from("/dst"), vec![], flag());
        tm.add_move(rx2, PathBuf::from("/dst"), vec![], flag());
        // Mark first as finished
        tm.tasks[0].state = TaskState::Finished {
            success: true,
            cancelled: false,
            summary: "done".into(),
        };
        tm.remove_finished();
        assert_eq!(tm.tasks().len(), 1);
        assert!(matches!(tm.tasks()[0].state, TaskState::Running { .. }));
    }

    #[test]
    fn phantoms_for_returns_matching() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        let phantoms = vec![PhantomEntry {
            name: "copied.txt".into(),
            is_dir: false,
        }];
        tm.add_copy(rx, PathBuf::from("/dst"), phantoms, flag());

        let result = tm.phantoms_for(std::path::Path::new("/dst"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "copied.txt");

        // Different dir returns empty
        let result = tm.phantoms_for(std::path::Path::new("/other"));
        assert!(result.is_empty());
    }

    #[test]
    fn phantoms_for_ignores_finished() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        let phantoms = vec![PhantomEntry {
            name: "f.txt".into(),
            is_dir: false,
        }];
        tm.add_copy(rx, PathBuf::from("/dst"), phantoms, flag());
        tm.tasks[0].state = TaskState::Finished {
            success: true,
            cancelled: false,
            summary: "done".into(),
        };

        let result = tm.phantoms_for(std::path::Path::new("/dst"));
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn poll_all_copy_finished() {
        let mut tm = TaskManager::new();
        let (tx, rx) = mpsc::channel(4);
        tm.add_copy(rx, PathBuf::from("/dst"), vec![], flag());

        // Send finished message
        tx.send(ProgressMsg::Finished {
            records: vec![],
            error: None,
            bytes_total: 100,
            cancelled: false,
        })
        .await
        .unwrap();

        let events = tm.poll_all();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            TaskEvent::PasteFinished { is_copy: true, .. }
        ));
        assert!(matches!(tm.tasks()[0].state, TaskState::Finished { .. }));
    }

    #[tokio::test]
    async fn add_archive_creates_task() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        let id = tm.add_archive(rx, true, flag());
        assert_eq!(tm.tasks().len(), 1);
        assert_eq!(tm.tasks()[0].id, id);
        assert!(matches!(tm.tasks()[0].state, TaskState::Running { .. }));
        assert_eq!(TaskManager::kind_label(&tm.tasks()[0]), "Archive");
    }

    #[tokio::test]
    async fn add_archive_extract_labels_extract() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        tm.add_archive(rx, false, flag());
        assert_eq!(TaskManager::kind_label(&tm.tasks()[0]), "Extract");
    }

    #[tokio::test]
    async fn poll_all_archive_finished() {
        let mut tm = TaskManager::new();
        let (tx, rx) = mpsc::channel(4);
        tm.add_archive(rx, false, flag());

        tx.send(ArchiveMsg::Finished {
            is_create: false,
            processed: 3,
            skipped: 1,
            error: None,
            cancelled: false,
            label: "all entries".into(),
        })
        .await
        .unwrap();

        let events = tm.poll_all();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], TaskEvent::ArchiveFinished { .. }));
        assert!(matches!(
            tm.tasks()[0].state,
            TaskState::Finished {
                success: true,
                cancelled: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn poll_all_archive_error_marks_failure() {
        let mut tm = TaskManager::new();
        let (tx, rx) = mpsc::channel(4);
        tm.add_archive(rx, true, flag());

        tx.send(ArchiveMsg::Finished {
            is_create: true,
            processed: 0,
            skipped: 0,
            error: Some("boom".into()),
            cancelled: false,
            label: "out.zip".into(),
        })
        .await
        .unwrap();

        tm.poll_all();
        assert!(matches!(
            tm.tasks()[0].state,
            TaskState::Finished { success: false, .. }
        ));
    }

    #[tokio::test]
    async fn poll_all_delete_finished() {
        let mut tm = TaskManager::new();
        let (tx, rx) = mpsc::channel(4);
        tm.add_delete(rx, false, flag());

        tx.send(DeleteMsg::Finished {
            deleted: 3,
            errors: vec![],
            permanent: false,
            cancelled: false,
        })
        .await
        .unwrap();

        let events = tm.poll_all();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], TaskEvent::DeleteFinished { .. }));
    }
}
