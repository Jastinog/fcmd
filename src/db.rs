use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

pub struct SavedTab {
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub active_side: String, // "left" or "right"
    pub left_cursor: usize,
    pub right_cursor: usize,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn init() -> rusqlite::Result<Self> {
        let db_path = crate::util::config_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&db_path).ok();
        let conn = Connection::open(db_path.join("fcmd.db"))?;
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
             );
             CREATE TABLE IF NOT EXISTS dir_sort (
                 path TEXT PRIMARY KEY,
                 sort_mode TEXT NOT NULL,
                 sort_reverse INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS bookmarks (
                 name TEXT PRIMARY KEY,
                 path TEXT NOT NULL
             );",
        )?;
        // Migrate: add level column if missing
        let has_level: bool = conn
            .prepare("SELECT level FROM visual_marks LIMIT 0")
            .is_ok();
        if !has_level {
            conn.execute_batch(
                "ALTER TABLE visual_marks ADD COLUMN level INTEGER NOT NULL DEFAULT 1;",
            )
            .ok();
        }
        // Migrate: add cursor columns to session_tabs
        let has_cursor: bool = conn
            .prepare("SELECT left_cursor FROM session_tabs LIMIT 0")
            .is_ok();
        if !has_cursor {
            conn.execute_batch(
                "ALTER TABLE session_tabs ADD COLUMN left_cursor INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE session_tabs ADD COLUMN right_cursor INTEGER NOT NULL DEFAULT 0;",
            )
            .ok();
        }
        Ok(Db { conn })
    }

    pub fn load_visual_marks(&self) -> rusqlite::Result<HashMap<PathBuf, u8>> {
        let mut stmt = self.conn.prepare("SELECT path, level FROM visual_marks")?;
        let rows = stmt.query_map([], |row| {
            let s: String = row.get(0)?;
            let level: i64 = row.get(1)?;
            Ok((PathBuf::from(s), level as u8))
        })?;
        let mut map = HashMap::new();
        for (p, l) in rows.flatten() {
            map.insert(p, l);
        }
        Ok(map)
    }

    pub fn set_visual_mark(&self, path: &Path, level: u8) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO visual_marks (path, level) VALUES (?1, ?2)",
            params![path.to_string_lossy().as_ref(), level as i64],
        )?;
        Ok(())
    }

    pub fn remove_visual_mark(&self, path: &Path) -> rusqlite::Result<()> {
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
                "INSERT INTO session_tabs (idx, left_path, right_path, active_side, left_cursor, right_cursor) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    i as i64,
                    tab.left_path.to_string_lossy().as_ref(),
                    tab.right_path.to_string_lossy().as_ref(),
                    tab.active_side,
                    tab.left_cursor as i64,
                    tab.right_cursor as i64,
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
        let escaped = dir
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("{escaped}/%");
        let mut stmt = self.conn.prepare(
            "SELECT path, size_bytes FROM dir_sizes WHERE path LIKE ?1 ESCAPE '\\'",
        )?;
        let rows = stmt.query_map(params![pattern], |row| {
            let p: String = row.get(0)?;
            let s: i64 = row.get(1)?;
            Ok((PathBuf::from(p), s as u64))
        })?;
        let mut map = HashMap::new();
        for (p, s) in rows.flatten() {
            map.insert(p, s);
        }
        Ok(map)
    }

    // --- Per-directory sort persistence ---

    pub fn save_dir_sort(
        &self,
        path: &Path,
        mode_label: &str,
        reverse: bool,
    ) -> rusqlite::Result<()> {
        if mode_label == "name" && !reverse {
            // Default sort — remove row to keep table clean
            self.conn.execute(
                "DELETE FROM dir_sort WHERE path = ?1",
                params![path.to_string_lossy().as_ref()],
            )?;
        } else {
            self.conn.execute(
                "INSERT OR REPLACE INTO dir_sort (path, sort_mode, sort_reverse) VALUES (?1, ?2, ?3)",
                params![path.to_string_lossy().as_ref(), mode_label, reverse as i32],
            )?;
        }
        Ok(())
    }

    pub fn load_dir_sorts(&self) -> rusqlite::Result<HashMap<PathBuf, (String, bool)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, sort_mode, sort_reverse FROM dir_sort")?;
        let rows = stmt.query_map([], |row| {
            let p: String = row.get(0)?;
            let m: String = row.get(1)?;
            let r: i32 = row.get(2)?;
            Ok((PathBuf::from(p), (m, r != 0)))
        })?;
        let mut map = HashMap::new();
        for (p, mr) in rows.flatten() {
            map.insert(p, mr);
        }
        Ok(map)
    }

    #[cfg(test)]
    fn init_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS visual_marks (path TEXT PRIMARY KEY, level INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE IF NOT EXISTS session_tabs (
                 idx INTEGER PRIMARY KEY,
                 left_path TEXT NOT NULL,
                 right_path TEXT NOT NULL,
                 active_side TEXT NOT NULL DEFAULT 'left',
                 left_cursor INTEGER NOT NULL DEFAULT 0,
                 right_cursor INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS session_meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS dir_sizes (
                 path TEXT PRIMARY KEY,
                 size_bytes INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS dir_sort (
                 path TEXT PRIMARY KEY,
                 sort_mode TEXT NOT NULL,
                 sort_reverse INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS bookmarks (
                 name TEXT PRIMARY KEY,
                 path TEXT NOT NULL
             );",
        )?;
        Ok(Db { conn })
    }

    // --- Bookmarks ---

    pub fn load_bookmarks(&self) -> rusqlite::Result<Vec<(String, PathBuf)>> {
        let mut stmt = self.conn.prepare("SELECT name, path FROM bookmarks ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let path: String = row.get(1)?;
            Ok((name, PathBuf::from(path)))
        })?;
        Ok(rows.flatten().collect())
    }

    pub fn save_bookmark(&self, name: &str, path: &Path) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO bookmarks (name, path) VALUES (?1, ?2)",
            params![name, path.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }

    pub fn remove_bookmark(&self, name: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM bookmarks WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    pub fn rename_bookmark(&self, old_name: &str, new_name: &str) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let path: String = tx.query_row(
            "SELECT path FROM bookmarks WHERE name = ?1",
            params![old_name],
            |row| row.get(0),
        )?;
        tx.execute("DELETE FROM bookmarks WHERE name = ?1", params![old_name])?;
        tx.execute(
            "INSERT OR REPLACE INTO bookmarks (name, path) VALUES (?1, ?2)",
            params![new_name, path],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_session(&self) -> rusqlite::Result<(Vec<SavedTab>, usize)> {
        let mut stmt = self.conn.prepare(
            "SELECT left_path, right_path, active_side, left_cursor, right_cursor FROM session_tabs ORDER BY idx",
        )?;
        let tabs: Vec<SavedTab> = stmt
            .query_map([], |row| {
                Ok(SavedTab {
                    left_path: PathBuf::from(row.get::<_, String>(0)?),
                    right_path: PathBuf::from(row.get::<_, String>(1)?),
                    active_side: row.get(2)?,
                    left_cursor: row.get::<_, i64>(3).unwrap_or(0) as usize,
                    right_cursor: row.get::<_, i64>(4).unwrap_or(0) as usize,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_marks_crud() {
        let db = Db::init_in_memory().unwrap();
        let path = PathBuf::from("/tmp/test_file");

        // Initially empty
        let marks = db.load_visual_marks().unwrap();
        assert!(marks.is_empty());

        // Set mark
        db.set_visual_mark(&path, 2).unwrap();
        let marks = db.load_visual_marks().unwrap();
        assert_eq!(marks.get(&path), Some(&2));

        // Update mark level
        db.set_visual_mark(&path, 3).unwrap();
        let marks = db.load_visual_marks().unwrap();
        assert_eq!(marks.get(&path), Some(&3));

        // Remove mark
        db.remove_visual_mark(&path).unwrap();
        let marks = db.load_visual_marks().unwrap();
        assert!(marks.is_empty());
    }

    #[test]
    fn theme_save_load() {
        let db = Db::init_in_memory().unwrap();

        // No theme initially
        assert_eq!(db.load_theme(), None);

        // Save and load
        db.save_theme("dracula").unwrap();
        assert_eq!(db.load_theme(), Some("dracula".into()));

        // Overwrite
        db.save_theme("nord").unwrap();
        assert_eq!(db.load_theme(), Some("nord".into()));
    }

    #[test]
    fn session_save_load() {
        let db = Db::init_in_memory().unwrap();

        let tabs = vec![
            SavedTab {
                left_path: PathBuf::from("/home"),
                right_path: PathBuf::from("/tmp"),
                active_side: "left".into(),
                left_cursor: 5,
                right_cursor: 10,
            },
            SavedTab {
                left_path: PathBuf::from("/usr"),
                right_path: PathBuf::from("/var"),
                active_side: "right".into(),
                left_cursor: 0,
                right_cursor: 3,
            },
        ];

        db.save_session(&tabs, 1).unwrap();
        let (loaded, active) = db.load_session().unwrap();

        assert_eq!(active, 1);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].left_path, PathBuf::from("/home"));
        assert_eq!(loaded[0].right_path, PathBuf::from("/tmp"));
        assert_eq!(loaded[0].active_side, "left");
        assert_eq!(loaded[0].left_cursor, 5);
        assert_eq!(loaded[1].right_cursor, 3);
    }

    #[test]
    fn dir_sizes_save_load() {
        let db = Db::init_in_memory().unwrap();
        let entries = vec![
            (PathBuf::from("/home/user/docs"), 1024u64),
            (PathBuf::from("/home/user/pics"), 2048u64),
            (PathBuf::from("/tmp/other"), 512u64),
        ];

        db.save_dir_sizes(&entries).unwrap();

        // Load sizes for /home/user — should get docs and pics
        let sizes = db.load_dir_sizes(Path::new("/home/user")).unwrap();
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[&PathBuf::from("/home/user/docs")], 1024);
        assert_eq!(sizes[&PathBuf::from("/home/user/pics")], 2048);

        // /tmp should only have "other"
        let sizes = db.load_dir_sizes(Path::new("/tmp")).unwrap();
        assert_eq!(sizes.len(), 1);
    }

    #[test]
    fn dir_sort_save_load() {
        let db = Db::init_in_memory().unwrap();

        db.save_dir_sort(Path::new("/home"), "size", true).unwrap();
        db.save_dir_sort(Path::new("/tmp"), "mod", false).unwrap();

        let sorts = db.load_dir_sorts().unwrap();
        assert_eq!(sorts[&PathBuf::from("/home")], ("size".into(), true));
        assert_eq!(sorts[&PathBuf::from("/tmp")], ("mod".into(), false));
    }

    #[test]
    fn dir_sort_default_removes_row() {
        let db = Db::init_in_memory().unwrap();

        // Save non-default sort
        db.save_dir_sort(Path::new("/home"), "size", true).unwrap();
        assert_eq!(db.load_dir_sorts().unwrap().len(), 1);

        // Saving default sort (name, not reversed) removes the row
        db.save_dir_sort(Path::new("/home"), "name", false).unwrap();
        assert!(db.load_dir_sorts().unwrap().is_empty());
    }

    #[test]
    fn bookmarks_crud() {
        let db = Db::init_in_memory().unwrap();

        // Initially empty
        let bm = db.load_bookmarks().unwrap();
        assert!(bm.is_empty());

        // Add bookmarks
        db.save_bookmark("projects", Path::new("/home/user/projects")).unwrap();
        db.save_bookmark("downloads", Path::new("/home/user/downloads")).unwrap();
        let bm = db.load_bookmarks().unwrap();
        assert_eq!(bm.len(), 2);
        assert_eq!(bm[0].0, "downloads");
        assert_eq!(bm[1].0, "projects");

        // Update bookmark path
        db.save_bookmark("projects", Path::new("/opt/projects")).unwrap();
        let bm = db.load_bookmarks().unwrap();
        assert_eq!(bm.len(), 2);
        assert_eq!(bm[1].1, PathBuf::from("/opt/projects"));

        // Remove bookmark
        db.remove_bookmark("downloads").unwrap();
        let bm = db.load_bookmarks().unwrap();
        assert_eq!(bm.len(), 1);
        assert_eq!(bm[0].0, "projects");
    }

    #[test]
    fn bookmarks_rename() {
        let db = Db::init_in_memory().unwrap();

        db.save_bookmark("old", Path::new("/home/user/old")).unwrap();
        db.save_bookmark("other", Path::new("/tmp/other")).unwrap();

        // Rename old -> new
        db.rename_bookmark("old", "new").unwrap();
        let bm = db.load_bookmarks().unwrap();
        assert_eq!(bm.len(), 2);
        assert!(bm.iter().any(|(n, p)| n == "new" && p == Path::new("/home/user/old")));
        assert!(!bm.iter().any(|(n, _)| n == "old"));

        // Rename non-existent fails
        assert!(db.rename_bookmark("nonexistent", "x").is_err());
    }

    #[test]
    fn session_save_preserves_theme() {
        let db = Db::init_in_memory().unwrap();

        // Save theme, then save session — theme should survive
        db.save_theme("dracula").unwrap();
        db.save_session(&[], 0).unwrap();
        assert_eq!(db.load_theme(), Some("dracula".into()));
    }
}
