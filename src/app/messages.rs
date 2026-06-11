use super::*;

pub struct ArchiveLoadResult {
    pub archive_path: PathBuf,
    pub result: std::io::Result<(crate::archive::ArchiveFormat, Vec<crate::archive::ArchiveEntry>)>,
}

pub struct PhantomEntry {
    pub name: String,
    pub is_dir: bool,
}

pub struct DuProgress {
    pub rx: tokio::sync::mpsc::Receiver<DuMsg>,
    pub started_at: Instant,
}

#[derive(Debug)]
pub enum GitMsg {
    Finished {
        statuses: HashMap<PathBuf, char>,
        roots: [Option<PathBuf>; 3],
        checked_dirs: [Option<PathBuf>; 3],
    },
}

pub struct GitProgress {
    pub rx: tokio::sync::oneshot::Receiver<GitMsg>,
}

pub enum DeleteMsg {
    Progress {
        done: usize,
        total: usize,
        current: String,
    },
    Finished {
        deleted: usize,
        errors: Vec<String>,
        permanent: bool,
        /// True when the user cancelled the delete; the items removed so far still count.
        cancelled: bool,
    },
}

pub enum ArchiveMsg {
    Progress {
        done: usize,
        total: usize,
        current: String,
    },
    Finished {
        /// True for archive creation, false for extraction.
        is_create: bool,
        /// Files/dirs successfully written (created) or extracted.
        processed: usize,
        /// Files skipped at an overwrite conflict (extract only).
        skipped: usize,
        error: Option<String>,
        /// True when the user cancelled or aborted; what was done so far still counts.
        cancelled: bool,
        /// Archive name (create) or extracted entry / "all entries" (extract).
        label: String,
    },
}

pub enum DirLoadMsg {
    /// Unsorted batch of entries streaming from the filesystem.
    Batch {
        panel_idx: usize,
        tab_index: usize,
        path: PathBuf,
        entries: Vec<FileEntry>,
    },
    /// All entries read and sorted. Replaces the panel's entries.
    Finished {
        panel_idx: usize,
        tab_index: usize,
        path: PathBuf,
        entries: Vec<FileEntry>,
        select_name: Option<String>,
    },
}

/// Per-directory cached sizes loaded from the DB, keyed by parent directory.
pub type DirSizesLoadResult = Vec<(PathBuf, HashMap<PathBuf, u64>)>;

pub struct PreviewLoadResult {
    pub path: PathBuf,
    pub preview: Preview,
}

/// Initial viewer content load. `next_byte` is `Some` when the file continues
/// past the first block and should be paged in incrementally.
pub struct ViewerLoadResult {
    pub path: PathBuf,
    pub preview: Preview,
    pub next_byte: Option<u64>,
}

/// An incremental block of additional viewer lines appended on scroll.
pub struct ViewerChunkResult {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub next_byte: Option<u64>,
}

/// Async syntax-highlight result for the viewer. `line_count` guards against
/// applying a cache to content that has since changed (e.g. a mode toggle).
pub struct ViewerHlResult {
    pub path: PathBuf,
    pub line_count: usize,
    pub cache: crate::viewer::HlCache,
}

pub struct TreeLoadResult {
    pub start_dir: PathBuf,
    pub current_path: PathBuf,
    pub data: Vec<crate::tree::TreeLine>,
}

pub struct ChownLoadResult {
    pub users: Vec<(String, u32)>,
    pub groups: Vec<(String, u32)>,
    pub current_uid: u32,
    pub current_gid: u32,
    pub paths: Vec<PathBuf>,
}

/// Result of an async path validation for navigation.
pub struct NavCheckResult {
    pub path: PathBuf,
    pub is_dir: bool,
    pub exists: bool,
    pub source: NavSource,
}

pub enum NavSource {
    Cd,
    Bookmark,
    Mark(char),
}

/// Result of an async file operation (mkdir, touch, rename, chmod, chown, undo).
pub enum FileOpResult {
    Mkdir {
        name: String,
        result: Result<ops::OpRecord, String>,
    },
    Touch {
        name: String,
        result: Result<ops::OpRecord, String>,
    },
    Rename {
        new_name: String,
        result: Result<ops::OpRecord, String>,
    },
    Chmod {
        input: String,
        count: usize,
        errors: usize,
        last_error: Option<String>,
    },
    Chown {
        user_name: String,
        group_name: String,
        count: usize,
        errors: usize,
        last_error: Option<String>,
    },
    Undo {
        result: Result<String, String>,
    },
    ChmodPrefill {
        prefill: String,
        paths: Vec<PathBuf>,
    },
    ThemeLoad {
        name: String,
        theme: Option<Theme>,
        groups: Vec<crate::theme::ThemeGroup>,
    },
    ThemeList {
        groups: Vec<crate::theme::ThemeGroup>,
    },
    Clipboard {
        label: String,
        ok: bool,
    },
    BulkRename {
        total: usize,
        records: Vec<ops::OpRecord>,
        errors: Vec<String>,
    },
}
