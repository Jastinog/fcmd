use std::ops::Range;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthChar;

use crate::app::{App, Mode};
use crate::viewer::HlSpan;

/// Gutter width (line-number column) for text content, in cells: 4 digits + space.
const GUTTER: usize = 5;

/// Width in cells of the hex data-inspector side panel (borders included).
const INSPECTOR_W: usize = 30;

/// Full-screen content viewer. Takes `&mut App` so it can record the number of
/// content rows currently visible (`viewer_visible_height`) and refresh the
/// wrap layout for the current width — both read by the nav handlers on the next
/// keypress.
pub(in crate::ui) fn render_viewer(f: &mut Frame, app: &mut App, area: Rect) {
    if app.viewer.is_none() {
        return;
    }

    let is_searching = app.mode == Mode::ViewerSearch;
    let is_goto = app.mode == Mode::ViewerGoto;

    let t = &app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .style(Style::default().bg(t.bg));
    let inner = block.inner(area);

    // Reserve rows: separator(1) + hint(1) + optional search/goto bar(1).
    let reserved: u16 = 2 + if is_searching || is_goto { 1 } else { 0 };
    let list_height = inner.height.saturating_sub(reserved) as usize;
    let iw = inner.width as usize;

    let (is_binary, show_gutter, inspector_on) = {
        let vw = app.viewer.as_ref().unwrap();
        (
            vw.content.is_binary,
            !vw.content.is_binary && vw.line_numbers,
            vw.content.is_binary && vw.inspector,
        )
    };
    let gutter_cells = if show_gutter { GUTTER } else { 0 };
    // The data inspector steals a fixed right-hand column from the hex dump; the
    // remaining width drives the hex column count (recomputed in refresh_layout).
    // Suppressed on narrow terminals where it would crowd out the dump.
    let inspector_w = if inspector_on && iw > 50 {
        INSPECTOR_W.min(iw / 2)
    } else {
        0
    };
    let content_width = iw
        .saturating_sub(gutter_cells)
        .saturating_sub(inspector_w)
        .max(1);

    // Record nav inputs for the next keypress, then refresh the wrap layout.
    app.viewer_visible_height = list_height;
    if let Some(v) = app.viewer.as_mut() {
        v.refresh_layout(content_width);
    }

    // From here on everything is read-only.
    let app: &App = app;
    let t = &app.theme;
    let v = app.viewer.as_ref().unwrap();
    let p = &v.content;
    let layout = &v.layout;
    let search = &v.search;
    // Active match counter (n/total): hex search drives it for binary content.
    let match_count: Option<(usize, usize)> = if is_binary {
        v.hex_search
            .is_active()
            .then(|| (v.hex_search.current + 1, v.hex_search.matches.len()))
    } else {
        search
            .is_active()
            .then(|| (search.current + 1, search.matches.len()))
    };
    let query = if is_binary {
        &v.hex_search.query
    } else {
        &search.query
    };
    let scroll = p.scroll;
    let total_rows = v.total_rows();

    f.render_widget(Clear, area);

    // Title: name + info, plus match counter when searching.
    let wrap_tag = if v.wrap && !is_binary {
        " \u{f1290}"
    } else {
        ""
    }; // 󱊐 wrap icon
    let title = if let Some((cur, total)) = match_count {
        format!(
            " \u{f0208} {} [{}]{} ({cur}/{total}) ",
            p.title, p.info, wrap_tag
        )
    } else {
        format!(" \u{f0208} {} [{}]{} ", p.title, p.info, wrap_tag)
    };
    let block = block.title(title).title_style(Style::default().fg(t.cyan));
    f.render_widget(block, area);

    // Right-aligned position indicator on the top border. With a byte cursor we
    // show its offset and value; otherwise the last visible byte offset.
    let pos_text = if p.is_binary {
        let cols = v.hex_cols;
        let total = p.binary_size;
        let last_byte = ((scroll + list_height) * cols).min(p.hex_bytes.len());
        let pct = if total > 0 {
            (last_byte as u64 * 100 / total as u64) as u8
        } else {
            100
        };
        let size_text = super::format_binary_size(total);
        if let Some(c) = v.cursor.filter(|&c| c < p.hex_bytes.len()) {
            let b = p.hex_bytes[c];
            Some(format!(" \u{2316} 0x{c:X}={b:02X}/{b} {size_text} {pct}% "))
        } else {
            Some(format!(" 0x{last_byte:04X} {size_text} {pct}% "))
        }
    } else if total_rows > 0 {
        let first_line = layout.rows.get(scroll).map(|r| r.logical + 1).unwrap_or(0);
        let total_lines = p.lines.len();
        let more = if v.next_byte.is_some() { "+" } else { "" };
        let last_row = (scroll + list_height).min(total_rows);
        let pct = (last_row as u64 * 100 / total_rows as u64) as u8;
        Some(format!(" {first_line}/{total_lines}{more} {pct}% "))
    } else {
        None
    };
    if let Some(pos_text) = pos_text {
        let pos_w = pos_text.chars().count() as u16;
        if area.width > pos_w + 4 {
            let pos_x = area.x + area.width - pos_w - 1;
            let pos_area = Rect::new(pos_x, area.y, pos_w, 1);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    pos_text,
                    Style::default().fg(t.fg_dim),
                ))),
                pos_area,
            );
        }
    }

    let query_key = search.fold(query);
    let style = RowStyle {
        query_len: query_key.chars().count(),
        query_key: &query_key,
        case_sensitive: search.case_sensitive(),
        current_match: search.current_match(),
        t,
    };
    let hl = v.highlight.as_ref();

    let items: Vec<ListItem> = if p.is_binary {
        let cols = v.hex_cols;
        // Search hits intersecting the visible byte window (computed once, then
        // clipped per row inside the renderer).
        let win_from = scroll * cols;
        let win_to = (scroll + list_height) * cols;
        let hits = v.hex_search.hits_in(win_from, win_to);
        crate::ui::hex::render_rows(p, scroll, list_height, cols, t, v.cursor, &hits)
    } else {
        (0..list_height)
            .filter_map(|screen| {
                let row = layout.rows.get(scroll + screen)?;
                let logical = row.logical;
                let line = &p.lines[logical];
                let chars: Vec<char> = line.chars().collect();

                let mut spans = Vec::new();
                if show_gutter {
                    // Line number only on the first display row of the logical line.
                    let is_first = layout.row_of_line(logical) == scroll + screen;
                    let gutter = if is_first {
                        format!("{:>4} ", logical + 1)
                    } else {
                        " ".repeat(GUTTER)
                    };
                    spans.push(Span::styled(gutter, Style::default().fg(t.fg_dim)));
                }

                // Visible char window for this row.
                let begin = if layout.wrap {
                    row.start
                } else {
                    v.hcol.min(chars.len())
                };
                let hard_end = if layout.wrap { row.end } else { chars.len() };
                let mut w = 0usize;
                let mut end = begin;
                while end < hard_end {
                    let cw = UnicodeWidthChar::width(chars[end]).unwrap_or(0);
                    if w + cw > content_width {
                        break;
                    }
                    w += cw;
                    end += 1;
                }

                let line_hl = hl.and_then(|h| h.lines.get(logical)).map(Vec::as_slice);
                push_row_spans(
                    &mut spans,
                    &style,
                    line,
                    &chars,
                    begin..end,
                    logical,
                    line_hl,
                );
                Some(ListItem::new(Line::from(spans)))
            })
            .collect()
    };

    let list_w = inner.width.saturating_sub(inspector_w as u16);
    let list_area = Rect::new(inner.x, inner.y, list_w, list_height as u16);
    f.render_widget(List::new(items), list_area);

    // Data inspector: interpret the bytes at the cursor (hex mode only).
    if inspector_w > 0 {
        let insp_area = Rect::new(
            inner.x + list_w,
            inner.y,
            inspector_w as u16,
            list_height as u16,
        );
        render_inspector(f, p, v.cursor, t, insp_area);
    }

    let mut row_y = inner.y + list_height as u16;

    // Search input bar. In hex mode the prompt reflects how the query is being
    // interpreted (hex byte pattern vs literal ASCII).
    if is_searching {
        let label = if is_binary {
            if crate::viewer::is_hex_query(query) {
                " hex / "
            } else {
                " ascii / "
            }
        } else {
            " / "
        };
        let input_line = super::input_field_line(query, label, iw, t.blue, t);
        let input_area = Rect::new(inner.x, row_y, inner.width, 1);
        f.render_widget(Paragraph::new(input_line), input_area);
        row_y += 1;
    } else if is_goto {
        let input_line =
            super::input_field_line(&v.goto, " goto offset (0x… / dec) ", iw, t.blue, t);
        let input_area = Rect::new(inner.x, row_y, inner.width, 1);
        f.render_widget(Paragraph::new(input_line), input_area);
        row_y += 1;
    }

    // Separator.
    let sep_area = Rect::new(inner.x, row_y, inner.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(iw),
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );
    row_y += 1;

    // Hint line.
    let hint_line = if is_searching || is_goto {
        Line::from(vec![
            Span::styled(" \u{23ce}", Style::default().fg(t.blue)),
            Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(t.blue)),
            Span::styled(" cancel", Style::default().fg(t.fg_dim)),
        ])
    } else {
        let mut hints = vec![
            Span::styled(" j/k", Style::default().fg(t.cyan)),
            Span::styled(" scroll  ", Style::default().fg(t.fg_dim)),
            Span::styled("g/G", Style::default().fg(t.cyan)),
            Span::styled(" top/bot  ", Style::default().fg(t.fg_dim)),
        ];
        if !is_binary {
            let wrap_label = if v.wrap { " nowrap  " } else { " wrap  " };
            hints.push(Span::styled("w", Style::default().fg(t.cyan)));
            hints.push(Span::styled(wrap_label, Style::default().fg(t.fg_dim)));
            hints.push(Span::styled("#", Style::default().fg(t.cyan)));
            hints.push(Span::styled(" numbers  ", Style::default().fg(t.fg_dim)));
        }
        use crate::viewer::ViewMode;
        // Each mode key toggles back to text, so its hint reads "text" when the
        // mode is already active and the mode name otherwise.
        for (key, mode, name) in [
            ("x", ViewMode::Hex, " hex  "),
            ("s", ViewMode::Strings, " strings  "),
            ("S", ViewMode::Struct, " struct  "),
        ] {
            let label = if v.mode == mode { " text  " } else { name };
            hints.push(Span::styled(key, Style::default().fg(t.cyan)));
            hints.push(Span::styled(label, Style::default().fg(t.fg_dim)));
        }
        hints.push(Span::styled("/", Style::default().fg(t.cyan)));
        hints.push(Span::styled(" search  ", Style::default().fg(t.fg_dim)));
        if is_binary {
            hints.push(Span::styled(":", Style::default().fg(t.cyan)));
            hints.push(Span::styled(" goto  ", Style::default().fg(t.fg_dim)));
            hints.push(Span::styled("i", Style::default().fg(t.cyan)));
            hints.push(Span::styled(" inspect  ", Style::default().fg(t.fg_dim)));
        }
        if match_count.is_some() {
            hints.push(Span::styled("n/N", Style::default().fg(t.cyan)));
            hints.push(Span::styled(" next/prev  ", Style::default().fg(t.fg_dim)));
        }
        hints.push(Span::styled("o", Style::default().fg(t.cyan)));
        hints.push(Span::styled(" edit  ", Style::default().fg(t.fg_dim)));
        hints.push(Span::styled("q", Style::default().fg(t.cyan)));
        hints.push(Span::styled(" close", Style::default().fg(t.fg_dim)));
        Line::from(hints)
    };
    let hint_area = Rect::new(inner.x, row_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}

/// Render the hex data-inspector side panel: the bytes at `cursor` interpreted
/// as integers, floats, and timestamps. Shows a prompt when no cursor is placed.
fn render_inspector(
    f: &mut Frame,
    p: &crate::preview::Preview,
    cursor: Option<usize>,
    t: &crate::theme::Theme,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.border_inactive))
        .title(Span::styled(" inspect ", Style::default().fg(t.cyan)))
        .style(Style::default().bg(t.bg));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(c) = cursor.filter(|&c| c < p.hex_bytes.len()) else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "place cursor (hjkl)",
                Style::default().fg(t.fg_dim),
            ))),
            inner,
        );
        return;
    };

    const LABEL_W: usize = 7;
    let rows: Vec<ListItem> = crate::viewer::inspect::describe(&p.hex_bytes, c)
        .into_iter()
        .map(|fld| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<LABEL_W$}", fld.label),
                    Style::default().fg(t.fg_dim),
                ),
                Span::styled(fld.value, Style::default().fg(t.fg)),
            ]))
        })
        .collect();
    f.render_widget(List::new(rows), inner);
}

/// Styling context shared across every rendered row: the active search query
/// (folded for smart-case), the focused match, and the theme.
struct RowStyle<'a> {
    query_key: &'a str,
    query_len: usize,
    case_sensitive: bool,
    current_match: Option<(usize, usize)>,
    t: &'a crate::theme::Theme,
}

/// Append spans for the visible char slice `vis` of a text line, highlighting any
/// search matches. Matches are found (smart-case) in the full `line` in char
/// coordinates, then clipped to `vis`. With no matches, falls back to syntax
/// colors or plain text.
fn push_row_spans(
    spans: &mut Vec<Span<'static>>,
    style: &RowStyle,
    line: &str,
    chars: &[char],
    vis: Range<usize>,
    logical: usize,
    hl: Option<&[HlSpan]>,
) {
    let t = style.t;
    let slice = |r: Range<usize>| -> String { chars[r].iter().collect() };

    if style.query_key.is_empty() || style.query_len == 0 {
        push_syntax_or_plain(spans, chars, &vis, hl, t);
        return;
    }

    // Fold the line the same way as the search engine so highlights line up with
    // the match list (smart-case).
    let line_key = if style.case_sensitive {
        line.to_string()
    } else {
        line.to_lowercase()
    };
    let mut match_ranges: Vec<(usize, usize, bool)> = Vec::new();
    let mut start = 0;
    while let Some(pos) = line_key[start..].find(style.query_key) {
        let char_offset = line[..start + pos].chars().count();
        let is_current = style.current_match == Some((logical, start + pos));
        match_ranges.push((char_offset, char_offset + style.query_len, is_current));
        start += pos + style.query_key.len();
    }

    if match_ranges.is_empty() {
        push_syntax_or_plain(spans, chars, &vis, hl, t);
        return;
    }

    let mut pos = vis.start;
    for (m_start, m_end, is_cur) in &match_ranges {
        let ms = (*m_start).max(vis.start);
        let me = (*m_end).min(vis.end);
        if ms >= me || pos >= vis.end {
            continue;
        }
        if pos < ms {
            spans.push(Span::styled(slice(pos..ms), Style::default().fg(t.fg)));
        }
        let match_style = if *is_cur {
            Style::default().fg(t.bg_text).bg(t.orange)
        } else {
            Style::default().fg(t.bg_text).bg(t.yellow)
        };
        spans.push(Span::styled(slice(ms..me), match_style));
        pos = me;
    }
    if pos < vis.end {
        spans.push(Span::styled(slice(pos..vis.end), Style::default().fg(t.fg)));
    }
}

/// Render the visible char slice `vis` using syntax colors when available,
/// otherwise a single plain-foreground span.
fn push_syntax_or_plain(
    spans: &mut Vec<Span<'static>>,
    chars: &[char],
    vis: &Range<usize>,
    hl: Option<&[HlSpan]>,
    t: &crate::theme::Theme,
) {
    let slice = |r: Range<usize>| -> String { chars[r].iter().collect() };
    match hl {
        Some(hl) if !hl.is_empty() => {
            for s in hl {
                let start = s.start.max(vis.start);
                let end = s.end.min(vis.end);
                if start < end {
                    spans.push(Span::styled(
                        slice(start..end),
                        Style::default().fg(s.color),
                    ));
                }
            }
        }
        _ => spans.push(Span::styled(slice(vis.clone()), Style::default().fg(t.fg))),
    }
}
