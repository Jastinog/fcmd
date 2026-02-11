use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

pub struct SavedTab {
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub active_side: String, // "left" or "right"
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn init() -> rusqlite::Result<Self> {
        let db_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("fc");
        std::fs::create_dir_all(&db_path).ok();
        let conn = Connection::open(db_path.join("fc.db"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS visual_marks (path TEXT PRIMARY KEY);
             CREATE TABLE IF NOT EXISTS session_tabs (
                 idx INTEGER PRIMARY KEY,
                 left_path TEXT NOT NULL,
                 right_path TEXT NOT NULL,
                 active_side TEXT NOT NULL DEFAULT 'left'
             );
             CREATE TABLE IF NOT EXISTS session_meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS dir_sizes (
                 path TEXT PRIMARY KEY,
                 size_bytes INTEGER NOT NULL
             );",
        )?;
        Ok(Db { conn })
    }

    pub fn load_visual_marks(&self) -> rusqlite::Result<HashSet<PathBuf>> {
        let mut stmt = self.conn.prepare("SELECT path FROM visual_marks")?;
        let rows = stmt.query_map([], |row| {
            let s: String = row.get(0)?;
            Ok(PathBuf::from(s))
        })?;
        let mut set = HashSet::new();
        for row in rows {
            if let Ok(p) = row {
                set.insert(p);
            }
        }
        Ok(set)
    }

    pub fn add_visual_mark(&self, path: &PathBuf) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO visual_marks (path) VALUES (?1)",
            params![path.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }

    pub fn remove_visual_mark(&self, path: &PathBuf) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM visual_marks WHERE path = ?1",
            params![path.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }

    // --- Session persistence ---

    pub fn save_session(&self, tabs: &[SavedTab], active_tab: usize) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute("DELETE FROM session_tabs", [])?;
        tx.execute("DELETE FROM session_meta WHERE key != 'theme'", [])?;

        for (i, tab) in tabs.iter().enumerate() {
            tx.execute(
                "INSERT INTO session_tabs (idx, left_path, right_path, active_side) VALUES (?1, ?2, ?3, ?4)",
                params![
                    i as i64,
                    tab.left_path.to_string_lossy().as_ref(),
                    tab.right_path.to_string_lossy().as_ref(),
                    tab.active_side,
                ],
            )?;
        }

        tx.execute(
            "INSERT INTO session_meta (key, value) VALUES ('active_tab', ?1)",
            params![active_tab.to_string()],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn save_theme(&self, name: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO session_meta (key, value) VALUES ('theme', ?1)",
            params![name],
        )?;
        Ok(())
    }

    pub fn load_theme(&self) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM session_meta WHERE key = 'theme'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
    }

    // --- Directory sizes ---

    pub fn save_dir_sizes(&self, entries: &[(PathBuf, u64)]) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (path, size) in entries {
            tx.execute(
                "INSERT OR REPLACE INTO dir_sizes (path, size_bytes) VALUES (?1, ?2)",
                params![path.to_string_lossy().as_ref(), *size as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_dir_sizes(&self, dir: &Path) -> rusqlite::Result<HashMap<PathBuf, u64>> {
        let pattern = format!("{}/%", dir.to_string_lossy());
        let mut stmt = self
            .conn
            .prepare("SELECT path, size_bytes FROM dir_sizes WHERE path LIKE ?1")?;
        let rows = stmt.query_map(params![pattern], |row| {
            let p: String = row.get(0)?;
            let s: i64 = row.get(1)?;
            Ok((PathBuf::from(p), s as u64))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            if let Ok((p, s)) = row {
                map.insert(p, s);
            }
        }
        Ok(map)
    }

    pub fn load_session(&self) -> rusqlite::Result<(Vec<SavedTab>, usize)> {
        let mut stmt = self.conn.prepare(
            "SELECT left_path, right_path, active_side FROM session_tabs ORDER BY idx",
        )?;
        let tabs: Vec<SavedTab> = stmt
            .query_map([], |row| {
                Ok(SavedTab {
                    left_path: PathBuf::from(row.get::<_, String>(0)?),
                    right_path: PathBuf::from(row.get::<_, String>(1)?),
                    active_side: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let active_tab: usize = self
            .conn
            .query_row(
                "SELECT value FROM session_meta WHERE key = 'active_tab'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "0".into())
            .parse()
            .unwrap_or(0);

        Ok((tabs, active_tab))
    }
}
