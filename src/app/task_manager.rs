use std::path::PathBuf;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::ops::{OpRecord, ProgressMsg};

use super::messages::{DeleteMsg, PhantomEntry};

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
}

#[allow(dead_code)]
pub enum TaskState {
    Running {
        progress_pct: u8,
        status_text: String,
    },
    Finished {
        success: bool,
        summary: String,
    },
}

pub enum TaskEvent {
    PasteFinished {
        records: Vec<OpRecord>,
        error: Option<String>,
        is_copy: bool,
    },
    DeleteFinished,
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
        });
        id
    }

    pub fn add_move(
        &mut self,
        rx: mpsc::Receiver<ProgressMsg>,
        dst_dir: PathBuf,
        phantoms: Vec<PhantomEntry>,
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
        });
        id
    }

    pub fn add_delete(&mut self, rx: mpsc::Receiver<DeleteMsg>, permanent: bool) -> u32 {
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
                    }) = finished
                    {
                        let elapsed = task.started_at.elapsed();
                        let verb = if is_copy { "Copied" } else { "Moved" };
                        let n = records.len();
                        let summary = if let Some(ref e) = error {
                            format!("Paste error: {e}")
                        } else {
                            format!(
                                "{verb} {n} item(s), {} in {}",
                                crate::util::format_bytes(bytes_total),
                                crate::util::format_duration(elapsed),
                            )
                        };
                        task.state = TaskState::Finished {
                            success: error.is_none(),
                            summary,
                        };
                        events.push(TaskEvent::PasteFinished {
                            records,
                            error,
                            is_copy,
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
                    }) = finished
                    {
                        let verb = if permanent { "Deleted" } else { "Trashed" };
                        let summary = if errors.is_empty() {
                            format!("{verb} {deleted} item(s)")
                        } else if deleted == 0 {
                            format!("{verb} failed: {}", errors[0])
                        } else {
                            format!("{verb} {deleted}, {} failed: {}", errors.len(), errors[0])
                        };
                        task.state = TaskState::Finished {
                            success: errors.is_empty(),
                            summary,
                        };
                        events.push(TaskEvent::DeleteFinished);
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

    pub fn remove_finished(&mut self) {
        self.tasks
            .retain(|t| matches!(t.state, TaskState::Running { .. }));
    }

    /// Get phantom entries for a given directory (from all running paste tasks targeting it).
    pub fn phantoms_for(&self, dir: &std::path::Path) -> Vec<&PhantomEntry> {
        let mut result = Vec::new();
        for task in &self.tasks {
            match &task.kind {
                TaskKind::Copy { dst_dir, phantoms, .. }
                | TaskKind::Move { dst_dir, phantoms, .. }
                    if dst_dir == dir && matches!(task.state, TaskState::Running { .. }) =>
                {
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
        let id1 = tm.add_copy(rx1, PathBuf::from("/dst"), vec![]);
        let id2 = tm.add_copy(rx2, PathBuf::from("/dst"), vec![]);
        assert_ne!(id1, id2);
        assert_eq!(tm.tasks().len(), 2);
        assert_eq!(tm.active_count(), 2);
    }

    #[test]
    fn add_move_creates_task() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        let id = tm.add_move(rx, PathBuf::from("/dst"), vec![]);
        assert_eq!(tm.tasks().len(), 1);
        assert_eq!(tm.tasks()[0].id, id);
        assert!(matches!(tm.tasks()[0].state, TaskState::Running { .. }));
    }

    #[test]
    fn add_delete_creates_task() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        let _id = tm.add_delete(rx, false);
        assert_eq!(tm.tasks().len(), 1);
        assert_eq!(tm.active_count(), 1);
    }

    #[test]
    fn remove_finished_clears_completed() {
        let mut tm = TaskManager::new();
        let (_tx, rx) = mpsc::channel(1);
        tm.add_copy(rx, PathBuf::from("/dst"), vec![]);
        // Manually mark as finished
        tm.tasks[0].state = TaskState::Finished {
            success: true,
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
        tm.add_copy(rx1, PathBuf::from("/dst"), vec![]);
        tm.add_move(rx2, PathBuf::from("/dst"), vec![]);
        // Mark first as finished
        tm.tasks[0].state = TaskState::Finished {
            success: true,
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
        tm.add_copy(rx, PathBuf::from("/dst"), phantoms);

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
        tm.add_copy(rx, PathBuf::from("/dst"), phantoms);
        tm.tasks[0].state = TaskState::Finished {
            success: true,
            summary: "done".into(),
        };

        let result = tm.phantoms_for(std::path::Path::new("/dst"));
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn poll_all_copy_finished() {
        let mut tm = TaskManager::new();
        let (tx, rx) = mpsc::channel(4);
        tm.add_copy(rx, PathBuf::from("/dst"), vec![]);

        // Send finished message
        tx.send(ProgressMsg::Finished {
            records: vec![],
            error: None,
            bytes_total: 100,
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
    async fn poll_all_delete_finished() {
        let mut tm = TaskManager::new();
        let (tx, rx) = mpsc::channel(4);
        tm.add_delete(rx, false);

        tx.send(DeleteMsg::Finished {
            deleted: 3,
            errors: vec![],
            permanent: false,
        })
        .await
        .unwrap();

        let events = tm.poll_all();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], TaskEvent::DeleteFinished));
    }
}
