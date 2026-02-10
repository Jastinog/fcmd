use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::{Connection, params};

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
            "CREATE TABLE IF NOT EXISTS visual_marks (path TEXT PRIMARY KEY);",
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
}
