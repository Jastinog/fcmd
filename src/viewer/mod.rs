//! Full-screen content viewer.
//!
//! A self-contained submodule responsible for *viewing* the contents of a single
//! file (or directory listing): navigation, in-file search, soft-wrap, and the
//! different view representations. Content loading itself is shared with the
//! side-panel preview and lives in [`crate::preview`]; this module owns the
//! interactive view state on top of that content.
//!
//! Scroll position ([`Viewer::content`]`.scroll`) is expressed in *display rows*
//! (see [`layout`]), so it stays valid whether or not soft-wrap is on.

use std::path::PathBuf;

use crate::preview::Preview;

mod highlight;
mod layout;
mod search;

pub use highlight::{HlCache, HlSpan, highlight};
pub use layout::Layout;
pub use search::Search;

/// Interactive state for the full-screen viewer.
pub struct Viewer {
    /// File (or directory) being viewed.
    pub path: PathBuf,
    /// Loaded content + scroll position (in display rows) + title/info. Shared
    /// loader with the side panel preview (text, hex dump, directory, error).
    pub content: Preview,
    /// In-file search state.
    pub search: Search,
    /// Soft-wrap toggle (only applies to non-binary content).
    pub wrap: bool,
    /// Whether to show the line-number gutter (only applies to text content).
    pub line_numbers: bool,
    /// Force a hex view regardless of detected content type (toggled with `x`).
    pub force_hex: bool,
    /// Horizontal scroll offset in chars, used when wrap is off.
    pub hcol: usize,
    /// Display-row layout for the current width/wrap; rebuilt during render.
    pub layout: Layout,
    /// Set when content or wrap changes so the next render rebuilds the layout.
    pub layout_dirty: bool,
    /// Syntax-highlight colors per logical line; loaded asynchronously after the
    /// content lands. `None` until ready or when not applicable (binary/plain).
    pub highlight: Option<HlCache>,
    /// Byte offset to resume reading from when the file is loaded incrementally;
    /// `None` means the whole file is already loaded.
    pub next_byte: Option<u64>,
    /// True while an incremental chunk load is in flight (de-dupes triggers).
    pub loading_more: bool,
}

impl Viewer {
    /// Create a viewer showing a loading placeholder until the async load lands.
    pub fn loading(path: PathBuf) -> Self {
        let content = Preview::loading_placeholder(&path);
        Viewer {
            path,
            content,
            search: Search::new(),
            wrap: false,
            line_numbers: true,
            force_hex: false,
            hcol: 0,
            layout: Layout::empty(),
            layout_dirty: true,
            highlight: None,
            next_byte: None,
            loading_more: false,
        }
    }

    /// Replace the content (e.g. when the async load lands or the view mode
    /// changes) and reset the viewport: scroll, horizontal offset, and search.
    pub fn set_content(&mut self, content: Preview) {
        self.content = content;
        self.content.scroll = 0;
        self.hcol = 0;
        self.search.clear();
        self.highlight = None;
        self.next_byte = None;
        self.loading_more = false;
        self.layout_dirty = true;
    }

    /// Append an incrementally-loaded block of lines and update paging state.
    pub fn append_lines(&mut self, mut lines: Vec<String>, next_byte: Option<u64>) {
        self.content.lines.append(&mut lines);
        self.content.info = crate::preview::Preview::lines_info(
            self.content.lines.len(),
            next_byte.is_some(),
        );
        self.next_byte = next_byte;
        self.loading_more = false;
        self.layout_dirty = true;
    }

    /// Whether more content should be paged in for the current viewport. True
    /// when there's more on disk, nothing is already loading, and the viewport
    /// is within `PREFETCH` rows of the end of what's loaded.
    pub fn wants_more(&self, visible: usize) -> bool {
        const PREFETCH: usize = 2_000;
        self.next_byte.is_some()
            && !self.loading_more
            && self.content.scroll + visible + PREFETCH >= self.total_rows()
    }

    pub fn total_rows(&self) -> usize {
        self.layout.total_rows()
    }

    // ── Layout ────────────────────────────────────────────────────────

    /// Rebuild the display-row layout if width, wrap, or content changed,
    /// preserving the top logical line across the rebuild. Called from render
    /// with the actual content width (gutter excluded).
    pub fn refresh_layout(&mut self, content_width: usize) {
        let wrap = self.wrap && !self.content.is_binary;
        if !self.layout_dirty && self.layout.width == content_width && self.layout.wrap == wrap {
            return;
        }
        // Keep the top logical line stable across the rebuild.
        let anchor = self.layout.rows.get(self.content.scroll).map(|r| r.logical);
        self.layout = Layout::build(&self.content.lines, content_width, wrap);
        if let Some(a) = anchor {
            self.content.scroll = self.layout.row_of_line(a);
        }
        let max = self.total_rows().saturating_sub(1);
        self.content.scroll = self.content.scroll.min(max);
        self.layout_dirty = false;
    }

    // ── Navigation (display-row based) ─────────────────────────────────

    pub fn scroll_down(&mut self, n: usize, visible: usize) {
        let max = self.total_rows().saturating_sub(visible);
        self.content.scroll = (self.content.scroll + n).min(max);
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.content.scroll = self.content.scroll.saturating_sub(n);
    }

    pub fn goto_top(&mut self) {
        self.content.scroll = 0;
    }

    pub fn goto_bottom(&mut self, visible: usize) {
        self.content.scroll = self.total_rows().saturating_sub(visible);
    }

    pub fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
        self.hcol = 0;
        self.layout_dirty = true;
    }

    /// Toggle the line-number gutter. Changes content width, so the layout will
    /// rebuild on the next render.
    pub fn toggle_line_numbers(&mut self) {
        self.line_numbers = !self.line_numbers;
    }

    pub fn scroll_right(&mut self, n: usize) {
        if !self.wrap {
            self.hcol = self.hcol.saturating_add(n);
        }
    }

    pub fn scroll_left(&mut self, n: usize) {
        self.hcol = self.hcol.saturating_sub(n);
    }

    pub fn scroll_line_start(&mut self) {
        self.hcol = 0;
    }

    // ── Search ────────────────────────────────────────────────────────

    /// Recompute matches for the current query against the loaded content and
    /// scroll the first match into view.
    pub fn update_search(&mut self, visible: usize) {
        self.search.recompute(&self.content.lines);
        self.scroll_to_current_match(visible);
    }

    /// Advance to the next match (wrapping) and scroll it into view.
    pub fn search_next(&mut self, visible: usize) {
        if self.search.advance() {
            self.scroll_to_current_match(visible);
        }
    }

    /// Step to the previous match (wrapping) and scroll it into view.
    pub fn search_prev(&mut self, visible: usize) {
        if self.search.retreat() {
            self.scroll_to_current_match(visible);
        }
    }

    /// Bring the current match into view if it's off-screen, leaving it a third
    /// of the way down the viewport. Positions on the match's first display row.
    fn scroll_to_current_match(&mut self, visible: usize) {
        if let Some((line_idx, _)) = self.search.current_match() {
            let target = self.layout.row_of_line(line_idx);
            let scroll = &mut self.content.scroll;
            if target < *scroll || target >= *scroll + visible {
                *scroll = target.saturating_sub(visible / 3);
            }
        }
    }
}
