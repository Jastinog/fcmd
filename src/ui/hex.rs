//! Shared hex-dump rendering.
//!
//! Both the side-panel preview and the full-screen viewer render binary content
//! through here, building one colorized row at a time from the raw byte window
//! stored on [`crate::preview::Preview`]. Each byte is colored by category
//! (null / printable / whitespace / control / high) so the structure of a binary
//! file is legible at a glance, and the renderer optionally highlights a byte
//! cursor and search hits (used by the viewer).

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use crate::preview::Preview;
use crate::theme::Theme;

/// Color for a byte, by category, so the dump reads structurally:
/// null, whitespace, printable, control, and high bytes each stand out.
fn byte_color(b: u8, t: &Theme) -> Color {
    match b {
        0x00 => t.fg_dim,                                   // null padding
        0x09 | 0x0a | 0x0b | 0x0c | 0x0d | 0x20 => t.green, // whitespace
        0x21..=0x7e => t.cyan,                              // printable ASCII
        0x01..=0x1f | 0x7f => t.yellow,                     // other control
        _ => t.magenta,                                     // high / non-ASCII
    }
}

/// Build the spans for one hex row given the row's raw bytes and the file offset
/// of its first byte. `cursor`/`hits` are indices *within* this row (the byte
/// cursor and any search-match byte ranges, the last flagged `true` for the
/// focused match); pass `None`/`&[]` to disable.
fn byte_row_spans(
    abs_offset: usize,
    row: &[u8],
    cols: usize,
    t: &Theme,
    cursor: Option<usize>,
    hits: &[(usize, usize, bool)],
) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(cols * 2 + 4);
    spans.push(Span::styled(
        format!("{abs_offset:08x}  "),
        Style::default().fg(t.fg_dim),
    ));

    // Returns Some(is_current) when byte `i` falls inside a search hit.
    let hit_at = |i: usize| {
        hits.iter()
            .find(|&&(s, e, _)| i >= s && i < e)
            .map(|&(_, _, cur)| cur)
    };
    let cell_style = |i: usize, b: u8| {
        if cursor == Some(i) {
            Style::default().fg(t.bg).bg(t.cyan)
        } else if let Some(cur) = hit_at(i) {
            let bg = if cur { t.orange } else { t.yellow };
            Style::default().fg(t.bg_text).bg(bg)
        } else {
            Style::default().fg(byte_color(b, t))
        }
    };

    // Hex columns, grouped in eights for readability.
    for i in 0..cols {
        if i > 0 && i % 8 == 0 {
            spans.push(Span::raw(" "));
        }
        match row.get(i) {
            Some(&b) => {
                spans.push(Span::styled(format!("{b:02x}"), cell_style(i, b)));
                spans.push(Span::raw(" "));
            }
            None => spans.push(Span::raw("   ")),
        }
    }

    // ASCII gutter.
    spans.push(Span::styled(" |", Style::default().fg(t.fg_dim)));
    for i in 0..cols {
        match row.get(i) {
            Some(&b) => {
                let ch = if (0x20..=0x7e).contains(&b) {
                    b as char
                } else {
                    '.'
                };
                spans.push(Span::styled(ch.to_string(), cell_style(i, b)));
            }
            None => spans.push(Span::raw(" ")),
        }
    }
    spans.push(Span::styled("|", Style::default().fg(t.fg_dim)));
    spans
}

/// Render hex row `row_idx` of `p` (each row is `cols` bytes). `cursor` and
/// `hits` are in absolute file-byte coordinates and are clipped to this row.
/// Rows past the loaded byte window (e.g. a trailing "truncated" notice) fall
/// back to the stored line text.
pub fn render_row(
    p: &Preview,
    row_idx: usize,
    cols: usize,
    t: &Theme,
    cursor: Option<usize>,
    hits: &[(usize, usize, bool)],
) -> Vec<Span<'static>> {
    let byte_start = row_idx * cols;
    if byte_start >= p.hex_bytes.len() {
        // Trailing non-byte line (truncation notice, etc.).
        let line = p.lines.get(row_idx).cloned().unwrap_or_default();
        return vec![Span::styled(line, Style::default().fg(t.fg_dim))];
    }
    let byte_end = (byte_start + cols).min(p.hex_bytes.len());
    let row = &p.hex_bytes[byte_start..byte_end];

    let local_cursor = cursor
        .and_then(|c| c.checked_sub(byte_start))
        .filter(|&c| c < cols);
    let local_hits: Vec<(usize, usize, bool)> = hits
        .iter()
        .filter_map(|&(s, e, cur)| {
            let s = s.max(byte_start);
            let e = e.min(byte_end);
            (s < e).then(|| (s - byte_start, e - byte_start, cur))
        })
        .collect();

    byte_row_spans(byte_start, row, cols, t, local_cursor, &local_hits)
}

/// Build list items for the `height` hex rows visible from `scroll`, blanking
/// rows past the loaded content. `cursor`/`hits` are in absolute file-byte
/// coordinates (pass `None`/`&[]` for the read-only preview).
pub fn render_rows(
    p: &Preview,
    scroll: usize,
    height: usize,
    cols: usize,
    t: &Theme,
    cursor: Option<usize>,
    hits: &[(usize, usize, bool)],
) -> Vec<ListItem<'static>> {
    // Row cutoff follows the actual column width (which may differ from the
    // preview's fixed 16-wide `row_count`), plus any trailing non-byte lines.
    let byte_rows = p.hex_bytes.len().div_ceil(cols);
    let rows = byte_rows.max(p.lines.len());
    (0..height)
        .map(|i| {
            let row_idx = scroll + i;
            if row_idx >= rows {
                ListItem::new(Line::from(""))
            } else {
                ListItem::new(Line::from(render_row(p, row_idx, cols, t, cursor, hits)))
            }
        })
        .collect()
}
