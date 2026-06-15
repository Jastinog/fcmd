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

mod hexsearch;
mod highlight;
pub mod inspect;
mod layout;
mod search;

pub use hexsearch::{HexSearch, is_hex_query};
pub use highlight::{HlCache, HlSpan, highlight};
pub use layout::Layout;
pub use search::Search;

/// How the viewer renders the current file. Mutually exclusive; toggled with
/// `x` (hex) and `s` (strings), each falling back to [`ViewMode::Text`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewMode {
    /// Decoded text (the default), with soft-wrap and syntax highlighting.
    #[default]
    Text,
    /// Raw hex + ASCII dump with a byte cursor.
    Hex,
    /// Extracted printable strings (ASCII + UTF-16LE), one per line.
    Strings,
    /// Parsed executable structure (PE/ELF/Mach-O headers, sections, imports).
    Struct,
}

/// Interactive state for the full-screen viewer.
pub struct Viewer {
    /// File (or directory) being viewed.
    pub path: PathBuf,
    /// Loaded content + scroll position (in display rows) + title/info. Shared
    /// loader with the side panel preview (text, hex dump, directory, error).
    pub content: Preview,
    /// In-file search state (text content).
    pub search: Search,
    /// Byte-level search state (hex content).
    pub hex_search: HexSearch,
    /// Pending goto-offset input buffer (hex content).
    pub goto: String,
    /// Byte cursor offset in the hex view; `None` when no cursor is placed.
    pub cursor: Option<usize>,
    /// Bytes per hex row, chosen to fit the viewport width (8/16/24/…).
    pub hex_cols: usize,
    /// Soft-wrap toggle (only applies to non-binary content).
    pub wrap: bool,
    /// Whether to show the line-number gutter (only applies to text content).
    pub line_numbers: bool,
    /// Which representation the content is currently loaded as. Drives the
    /// reload on toggle and the title/hint labels.
    pub mode: ViewMode,
    /// Whether the hex data-inspector side panel is shown (hex mode only).
    pub inspector: bool,
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
    /// 1-based logical line to scroll to once content lands (e.g. opened from a
    /// grep match). Applied and cleared on the first content load.
    pub pending_goto_line: Option<usize>,
}

impl Viewer {
    /// Create a viewer showing a loading placeholder until the async load lands.
    pub fn loading(path: PathBuf) -> Self {
        let content = Preview::loading_placeholder(&path);
        Viewer {
            path,
            content,
            search: Search::new(),
            hex_search: HexSearch::new(),
            goto: String::new(),
            cursor: None,
            hex_cols: crate::preview::HEX_COLS,
            wrap: false,
            line_numbers: true,
            mode: ViewMode::Text,
            inspector: false,
            hcol: 0,
            layout: Layout::empty(),
            layout_dirty: true,
            highlight: None,
            next_byte: None,
            loading_more: false,
            pending_goto_line: None,
        }
    }

    /// Replace the content (e.g. when the async load lands or the view mode
    /// changes) and reset the viewport: scroll, horizontal offset, and search.
    pub fn set_content(&mut self, content: Preview) {
        self.content = content;
        self.content.scroll = 0;
        self.hcol = 0;
        self.search.clear();
        self.hex_search.clear();
        self.goto.clear();
        self.cursor = None;
        self.highlight = None;
        self.next_byte = None;
        self.loading_more = false;
        self.layout_dirty = true;
    }

    /// Append an incrementally-loaded block of lines and update paging state.
    pub fn append_lines(&mut self, mut lines: Vec<String>, next_byte: Option<u64>) {
        self.content.lines.append(&mut lines);
        self.content.info =
            crate::preview::Preview::lines_info(self.content.lines.len(), next_byte.is_some());
        self.next_byte = next_byte;
        self.loading_more = false;
        self.layout_dirty = true;
    }

    /// Append an incrementally-loaded block of raw bytes to the hex window.
    pub fn append_hex(&mut self, mut bytes: Vec<u8>, next_byte: Option<u64>) {
        self.content.hex_bytes.append(&mut bytes);
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
        if self.content.is_binary {
            self.content.hex_bytes.len().div_ceil(self.hex_cols)
        } else {
            self.layout.total_rows()
        }
    }

    // ── Layout ────────────────────────────────────────────────────────

    /// Rebuild the display-row layout if width, wrap, or content changed,
    /// preserving the top logical line across the rebuild. Called from render
    /// with the actual content width (gutter excluded). Binary content needs no
    /// row layout — its rows derive directly from the byte window.
    pub fn refresh_layout(&mut self, content_width: usize) {
        if self.content.is_binary {
            // Adapt bytes-per-row to the width; re-anchor scroll on the top byte
            // so the viewport stays put when the column count changes.
            let cols = hex_cols_for_width(content_width);
            if cols != self.hex_cols {
                let top_byte = self.content.scroll * self.hex_cols;
                self.hex_cols = cols;
                self.content.scroll = top_byte / cols;
            }
            let max = self.total_rows().saturating_sub(1);
            self.content.scroll = self.content.scroll.min(max);
            self.layout_dirty = false;
            return;
        }
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

    // ── Unified motion (text scroll vs. hex byte cursor) ───────────────
    //
    // In hex mode vertical/horizontal keys move the byte cursor (one row is
    // `hex_cols` bytes) and the viewport follows; in text mode they scroll.
    // These wrappers let the key handler stay mode-agnostic.

    /// Move down `rows` rows.
    pub fn move_down(&mut self, rows: usize, visible: usize) {
        if self.content.is_binary {
            self.hex_cursor_step((rows * self.hex_cols) as isize, visible);
        } else {
            self.scroll_down(rows, visible);
        }
    }

    /// Move up `rows` rows.
    pub fn move_up(&mut self, rows: usize, visible: usize) {
        if self.content.is_binary {
            self.hex_cursor_step(-((rows * self.hex_cols) as isize), visible);
        } else {
            self.scroll_up(rows);
        }
    }

    /// Move right one column (one byte in hex, eight cells of horizontal scroll
    /// in text).
    pub fn move_right(&mut self, visible: usize) {
        if self.content.is_binary {
            self.hex_cursor_step(1, visible);
        } else {
            self.scroll_right(8);
        }
    }

    /// Move left one column (see [`Self::move_right`]).
    pub fn move_left(&mut self, visible: usize) {
        if self.content.is_binary {
            self.hex_cursor_step(-1, visible);
        } else {
            self.scroll_left(8);
        }
    }

    /// Jump to the top of the content.
    pub fn move_top(&mut self) {
        if self.content.is_binary {
            self.hex_cursor_home();
        } else {
            self.goto_top();
        }
    }

    /// Jump to the bottom of the content.
    pub fn move_bottom(&mut self, visible: usize) {
        if self.content.is_binary {
            self.hex_cursor_end(visible);
        } else {
            self.goto_bottom(visible);
        }
    }

    // ── Unified search (text vs. hex), routed by content type ──────────

    /// Reset the active search (hex or text) ahead of a new query.
    pub fn search_clear(&mut self) {
        if self.content.is_binary {
            self.hex_search.clear();
        } else {
            self.search.clear();
        }
    }

    /// Append a character to the active query and re-run the search.
    pub fn search_push(&mut self, c: char, visible: usize) {
        if self.content.is_binary {
            self.hex_search.query.push(c);
            self.update_hex_search(visible);
        } else {
            self.search.query.push(c);
            self.update_search(visible);
        }
    }

    /// Delete the last character of the active query and re-run the search.
    pub fn search_pop(&mut self, visible: usize) {
        if self.content.is_binary {
            self.hex_search.query.pop();
            self.update_hex_search(visible);
        } else {
            self.search.query.pop();
            self.update_search(visible);
        }
    }

    /// Jump to the next match (hex or text).
    pub fn search_advance(&mut self, visible: usize) {
        if self.content.is_binary {
            self.hex_search_next(visible);
        } else {
            self.search_next(visible);
        }
    }

    /// Jump to the previous match (hex or text).
    pub fn search_retreat(&mut self, visible: usize) {
        if self.content.is_binary {
            self.hex_search_prev(visible);
        } else {
            self.search_prev(visible);
        }
    }

    // ── Hex search (byte-offset based) ─────────────────────────────────

    /// Recompute hex matches for the current query and scroll the first into view.
    pub fn update_hex_search(&mut self, visible: usize) {
        self.hex_search.recompute(&self.content.hex_bytes);
        self.scroll_to_hex_match(visible);
    }

    /// Re-run the hex search over the (now larger) byte window after paging,
    /// keeping the focus on the match nearest the previously focused offset.
    pub fn rescan_hex_search(&mut self) {
        if self.hex_search.query.is_empty() {
            return;
        }
        let prev = self.hex_search.current_offset();
        self.hex_search.recompute(&self.content.hex_bytes);
        if let Some(p) = prev
            && let Some(idx) = self.hex_search.matches.iter().position(|&m| m >= p)
        {
            self.hex_search.current = idx;
        }
    }

    pub fn hex_search_next(&mut self, visible: usize) {
        if self.hex_search.advance() {
            self.scroll_to_hex_match(visible);
        }
    }

    pub fn hex_search_prev(&mut self, visible: usize) {
        if self.hex_search.retreat() {
            self.scroll_to_hex_match(visible);
        }
    }

    /// Bring the focused hex match into view (a third of the way down) if it's
    /// off-screen.
    fn scroll_to_hex_match(&mut self, visible: usize) {
        if let Some(offset) = self.hex_search.current_offset() {
            self.cursor = Some(offset);
            let target = offset / self.hex_cols;
            let scroll = &mut self.content.scroll;
            if target < *scroll || target >= *scroll + visible {
                *scroll = target.saturating_sub(visible / 3);
            }
        }
    }

    // ── Goto offset ────────────────────────────────────────────────────

    /// Jump to byte `offset`: place the cursor there and scroll its row to the
    /// top of the viewport (clamped to the loaded window).
    pub fn goto_offset(&mut self, offset: usize, visible: usize) {
        let last = self.content.hex_bytes.len().saturating_sub(1);
        let off = offset.min(last);
        self.cursor = Some(off);
        let target_row = off / self.hex_cols;
        let max_scroll = self.total_rows().saturating_sub(visible);
        self.content.scroll = target_row.min(max_scroll);
    }

    // ── Hex byte cursor ────────────────────────────────────────────────

    /// Move the byte cursor by `delta` bytes (negative = backwards), clamped to
    /// the loaded window, then scroll to keep it visible. A first move with no
    /// cursor anchors it at the top-left visible byte.
    pub fn hex_cursor_step(&mut self, delta: isize, visible: usize) {
        let last = self.content.hex_bytes.len().saturating_sub(1);
        let anchor = self.cursor.unwrap_or(self.content.scroll * self.hex_cols);
        let next = (anchor as isize + delta).clamp(0, last as isize) as usize;
        self.cursor = Some(next);
        self.scroll_cursor_into_view(visible);
    }

    /// Move the cursor to the first byte (top of file).
    pub fn hex_cursor_home(&mut self) {
        self.cursor = Some(0);
        self.content.scroll = 0;
    }

    /// Move the cursor to the last loaded byte (bottom of file).
    pub fn hex_cursor_end(&mut self, visible: usize) {
        let last = self.content.hex_bytes.len().saturating_sub(1);
        self.cursor = Some(last);
        self.scroll_cursor_into_view(visible);
    }

    /// Scroll the minimum amount so the cursor's row is within the viewport.
    fn scroll_cursor_into_view(&mut self, visible: usize) {
        if let Some(c) = self.cursor {
            let row = c / self.hex_cols;
            if row < self.content.scroll {
                self.content.scroll = row;
            } else if visible > 0 && row >= self.content.scroll + visible {
                self.content.scroll = row + 1 - visible;
            }
        }
    }
}

/// Largest bytes-per-row (a multiple of 8) whose rendered hex row fits in `w`
/// cells, clamped to `[8, 64]`. A row is: offset(8)+2 gap + 3·cols hex chars +
/// one group gap per 8 bytes + " |" + cols ASCII + "|".
pub fn hex_cols_for_width(w: usize) -> usize {
    let fits = |cols: usize| 13 + 4 * cols + (cols - 1) / 8 <= w;
    let mut best = 8;
    for cols in (8..=64).step_by(8) {
        if fits(cols) {
            best = cols;
        } else {
            break;
        }
    }
    best
}

/// Parse a goto-offset string: `0x`-prefixed or bare-hex (containing a–f) is
/// hexadecimal; an all-decimal string is decimal. Returns `None` if invalid.
pub fn parse_offset(s: &str) -> Option<usize> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return usize::from_str_radix(hex, 16).ok();
    }
    if s.bytes()
        .any(|b| b.is_ascii_hexdigit() && !b.is_ascii_digit())
    {
        usize::from_str_radix(s, 16).ok()
    } else {
        s.parse::<usize>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_offset_forms() {
        assert_eq!(parse_offset("0x1f40"), Some(0x1f40));
        assert_eq!(parse_offset("0X10"), Some(16));
        assert_eq!(parse_offset("100"), Some(100)); // decimal
        assert_eq!(parse_offset("1f"), Some(0x1f)); // bare hex (has a letter)
        assert_eq!(parse_offset("  20  "), Some(20));
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("zz"), None);
    }

    #[test]
    fn goto_offset_places_cursor_and_clamps() {
        let mut v = Viewer::loading(PathBuf::from("/x.bin"));
        v.content.is_binary = true;
        v.content.hex_bytes = (0..=255u8).collect(); // 256 bytes, 16 rows
        v.content.lines = vec![String::new(); 16];
        v.refresh_layout(80);
        v.goto_offset(0x20, 4); // byte 32 -> row 2
        assert_eq!(v.cursor, Some(0x20));
        assert_eq!(v.content.scroll, 2);
        // Past the end clamps to the last byte.
        v.goto_offset(9999, 4);
        assert_eq!(v.cursor, Some(255));
    }

    #[test]
    fn hex_cursor_step_moves_and_scrolls() {
        let mut v = Viewer::loading(PathBuf::from("/x.bin"));
        v.content.is_binary = true;
        v.content.hex_bytes = (0..=255u8).collect(); // 16 rows
        v.content.lines = vec![String::new(); 16];
        v.refresh_layout(80);
        let visible = 4;
        // No cursor yet: first horizontal step anchors at top-left (byte 0) + 1.
        v.hex_cursor_step(1, visible);
        assert_eq!(v.cursor, Some(1));
        assert_eq!(v.content.scroll, 0);
        // Step down by 6 rows (6*16=96) -> row 6, off-screen, scroll follows.
        v.hex_cursor_step(16 * 6, visible);
        assert_eq!(v.cursor, Some(1 + 96));
        assert_eq!(v.content.scroll, 6 + 1 - visible); // row 6 visible at bottom
        // Home/end.
        v.hex_cursor_home();
        assert_eq!((v.cursor, v.content.scroll), (Some(0), 0));
        v.hex_cursor_end(visible);
        assert_eq!(v.cursor, Some(255));
        // Backwards clamps at 0.
        v.hex_cursor_home();
        v.hex_cursor_step(-100, visible);
        assert_eq!(v.cursor, Some(0));
    }

    #[test]
    fn hex_cols_for_width_picks_multiple_of_8() {
        assert_eq!(hex_cols_for_width(78), 16); // exact fit for 16
        assert_eq!(hex_cols_for_width(80), 16);
        assert_eq!(hex_cols_for_width(44), 8); // 16 needs 78, only 8 fits
        assert_eq!(hex_cols_for_width(10), 8); // clamp floor
        assert_eq!(hex_cols_for_width(111), 24); // 24 needs 111
        assert!(hex_cols_for_width(10_000) <= 64); // clamp ceiling
    }

    #[test]
    fn width_change_reanchors_scroll_by_byte() {
        let mut v = Viewer::loading(PathBuf::from("/x.bin"));
        v.content.is_binary = true;
        v.content.hex_bytes = vec![0u8; 16 * 100]; // 1600 bytes
        v.refresh_layout(80); // 16 cols
        assert_eq!(v.hex_cols, 16);
        v.content.scroll = 10; // top byte = 160
        // Widen so 32 cols fit (needs 13+128+3 = 144).
        v.refresh_layout(150);
        assert_eq!(v.hex_cols, 32);
        // Top byte 160 should now be on row 160/32 = 5.
        assert_eq!(v.content.scroll, 5);
    }
}
