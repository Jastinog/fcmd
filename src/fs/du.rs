//! Recursive directory-size calculation (`du`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::fs::ops::{PROGRESS_INTERVAL, path_size};

pub enum DuMsg {
    Progress {
        done: usize,
        total: usize,
        current: String,
    },
    Finished {
        sizes: Vec<(PathBuf, u64)>,
    },
}

pub fn du_in_background(dirs: Vec<PathBuf>, tx: tokio::sync::mpsc::Sender<DuMsg>) {
    tokio::task::spawn_blocking(move || {
        let total = dirs.len();
        let mut sizes = Vec::new();
        let mut last_report: Option<Instant> = None;
        for (i, dir) in dirs.iter().enumerate() {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // Progress is cosmetic and the receiver only keeps the latest message,
            // so use a throttled, non-blocking send: a full channel must never stall
            // the size calculation (see ProgressCtx::report for the same rationale).
            let now = Instant::now();
            if last_report.is_none_or(|t| now.duration_since(t) >= PROGRESS_INTERVAL) {
                last_report = Some(now);
                let _ = tx.try_send(DuMsg::Progress {
                    done: i,
                    total,
                    current: name,
                });
            }
            let size = path_size(dir);
            sizes.push((dir.clone(), size));
        }
        // Final result must be delivered reliably.
        let _ = tx.blocking_send(DuMsg::Finished { sizes });
    });
}

/// Recursive directory stats: (total_size, file_count, dir_count).
pub fn dir_stats(p: &Path) -> (u64, usize, usize) {
    let mut size = 0u64;
    let mut files = 0usize;
    let mut dirs = 0usize;
    dir_stats_inner(p, &mut size, &mut files, &mut dirs);
    (size, files, dirs)
}

fn dir_stats_inner(p: &Path, size: &mut u64, files: &mut usize, dirs: &mut usize) {
    let rd = match fs::read_dir(p) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            *files += 1;
            *size += fs::symlink_metadata(entry.path())
                .map(|m| m.len())
                .unwrap_or(0);
        } else if ft.is_dir() {
            *dirs += 1;
            dir_stats_inner(&entry.path(), size, files, dirs);
        } else {
            *files += 1;
            *size += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("fc_du_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dir_stats_counts() {
        let dir = tmp_dir();
        fs::write(dir.join("a.txt"), "12345").unwrap();
        fs::write(dir.join("b.txt"), "67").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/c.txt"), "890").unwrap();

        let (size, files, dirs) = dir_stats(&dir);
        assert_eq!(files, 3); // a.txt, b.txt, sub/c.txt
        assert_eq!(dirs, 1); // sub
        assert_eq!(size, 10); // 5 + 2 + 3
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_stats_empty() {
        let dir = tmp_dir();
        let (size, files, dirs) = dir_stats(&dir);
        assert_eq!(size, 0);
        assert_eq!(files, 0);
        assert_eq!(dirs, 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
