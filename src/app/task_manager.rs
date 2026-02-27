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
