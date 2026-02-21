use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::{App, Mode};
use crate::ui::util::centered_rect;

pub(in crate::ui) fn render_preview_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let Some(ref p) = app.file_preview else {
        return;
    };

    let popup = centered_rect(75, 80, area);
    f.render_widget(Clear, popup);

    let is_searching = app.mode == Mode::PreviewSearch;
    let has_matches = !app.preview_search_matches.is_empty();
    let query = &app.preview_search_query;

    // Title with optional match count or hex position
    let title = if has_matches {
        format!(
            " 󰈈 {} [{}] ({}/{}) ",
            p.title,
            p.info,
            app.preview_search_current + 1,
            app.preview_search_matches.len()
        )
    } else {
        format!(" 󰈈 {} [{}] ", p.title, p.info)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(title)
        .title_style(Style::default().fg(t.cyan))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // Right-aligned position in title area (on the border)
    if p.is_binary {
        let (_, last_byte, total, pct) = p.hex_position(inner.height.saturating_sub(2) as usize);
        let size_text = super::format_binary_size(total);
        let pos_text = format!(" 0x{last_byte:04X} {size_text} {pct}% ");
        let pos_w = pos_text.chars().count() as u16;
        if popup.width > pos_w + 4 {
            let pos_x = popup.x + popup.width - pos_w - 1;
            let pos_area = Rect::new(pos_x, popup.y, pos_w, 1);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    pos_text,
                    Style::default().fg(t.fg_dim),
                ))),
                pos_area,
            );
        }
    } else if !p.lines.is_empty() {
        let reserved: u16 = 2 + if is_searching { 1 } else { 0 };
        let vis = inner.height.saturating_sub(reserved) as usize;
        let (first, total, pct) = p.text_position(vis);
        let pos_text = format!(" {first}/{total} {pct}% ");
        let pos_w = pos_text.chars().count() as u16;
        if popup.width > pos_w + 4 {
            let pos_x = popup.x + popup.width - pos_w - 1;
            let pos_area = Rect::new(pos_x, popup.y, pos_w, 1);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    pos_text,
                    Style::default().fg(t.fg_dim),
                ))),
                pos_area,
            );
        }
    }

    let iw = inner.width as usize;
    // Reserve rows: separator(1) + hint(1) + optional search bar(1)
    let reserved: u16 = 2 + if is_searching { 1 } else { 0 };
    let list_height = inner.height.saturating_sub(reserved) as usize;

    // Build match lookup: for each visible line, collect char offsets of matches
    let query_lower = query.to_lowercase();
    let query_len = query_lower.chars().count();
    let current_match = if has_matches {
        Some(app.preview_search_matches[app.preview_search_current])
    } else {
        None
    };

    let items: Vec<ListItem> = if p.is_binary {
        p.lines
            .iter()
            .skip(p.scroll)
            .take(list_height)
            .map(|line| {
                let max_content = iw;
                let content: String = if line.chars().count() > max_content {
                    line.chars().take(max_content).collect()
                } else {
                    line.clone()
                };
                let spans = crate::ui::preview::build_hex_spans(&content, t.fg_dim, t.fg, t.cyan);
                ListItem::new(Line::from(spans))
            })
            .collect()
    } else {
        (0..list_height)
            .filter_map(|i| {
                let line_idx = i + p.scroll;
                if line_idx >= p.lines.len() {
                    return None;
                }
                let line_num = line_idx + 1;
                let num_width = 4;
                let max_content = iw.saturating_sub(num_width + 2);
                let mut spans = vec![Span::styled(
                    format!("{line_num:>num_width$} ", num_width = num_width),
                    Style::default().fg(t.fg_dim),
                )];

                if !query_lower.is_empty() && query_len > 0 {
                    // Render with search highlights
                    let line = &p.lines[line_idx];
                    let line_chars: Vec<char> = line.chars().collect();
                    let display_len = line_chars.len().min(max_content);
                    let line_lower = line.to_lowercase();
                    // Find all match offsets in this line
                    let mut match_ranges: Vec<(usize, usize, bool)> = Vec::new();
                    let mut start = 0;
                    while let Some(pos) = line_lower[start..].find(&query_lower) {
                        let char_offset = line[..start + pos].chars().count();
                        let is_current = current_match == Some((line_idx, start + pos));
                        match_ranges.push((char_offset, char_offset + query_len, is_current));
                        start += pos + query_lower.len();
                    }

                    if match_ranges.is_empty() {
                        spans.extend(crate::ui::preview::build_content_spans(p, line_idx, max_content, t.fg));
                    } else {
                        let mut pos = 0;
                        for (m_start, m_end, is_cur) in &match_ranges {
                            if pos >= display_len {
                                break;
                            }
                            // Text before match
                            if pos < *m_start {
                                let end = (*m_start).min(display_len);
                                let text: String = line_chars[pos..end].iter().collect();
                                spans.push(Span::styled(text, Style::default().fg(t.fg)));
                                pos = end;
                            }
                            // Match text
                            if pos < display_len && pos < *m_end {
                                let end = (*m_end).min(display_len);
                                let text: String = line_chars[pos..end].iter().collect();
                                let style = if *is_cur {
                                    Style::default().fg(t.bg_text).bg(t.orange)
                                } else {
                                    Style::default().fg(t.bg_text).bg(t.yellow)
                                };
                                spans.push(Span::styled(text, style));
                                pos = end;
                            }
                        }
                        // Remaining text
                        if pos < display_len {
                            let text: String = line_chars[pos..display_len].iter().collect();
                            spans.push(Span::styled(text, Style::default().fg(t.fg)));
                        }
                    }
                } else {
                    spans.extend(crate::ui::preview::build_content_spans(p, line_idx, max_content, t.fg));
                }

                Some(ListItem::new(Line::from(spans)))
            })
            .collect()
    };

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

    let mut row_y = inner.y + list_height as u16;

    // Search input bar (when in PreviewSearch mode)
    if is_searching {
        let prefix = " / ";
        let prefix_len = prefix.chars().count();
        let field_w = iw.saturating_sub(prefix_len).max(1);
        let input_chars: Vec<char> = query.chars().collect();
        let input_char_len = input_chars.len();
        let (visible_input, cursor_pos) = if input_char_len < field_w {
            (query.clone(), input_char_len)
        } else {
            let start = input_char_len + 1 - field_w;
            let s: String = input_chars[start..].iter().collect();
            (s, field_w - 1)
        };
        let before: String = visible_input.chars().take(cursor_pos).collect();
        let after: String = visible_input.chars().skip(cursor_pos).collect();
        let used = prefix_len + before.chars().count() + 1 + after.chars().count();
        let pad = iw.saturating_sub(used);

        let input_line = Line::from(vec![
            Span::styled(prefix, Style::default().fg(t.blue)),
            Span::styled(before, Style::default().fg(t.fg).bg(t.bg_light)),
            Span::styled("\u{2588}", Style::default().fg(t.blue).bg(t.bg_light)),
            Span::styled(after, Style::default().fg(t.fg).bg(t.bg_light)),
            Span::styled(" ".repeat(pad), Style::default().bg(t.bg_light)),
        ]);
        let input_area = Rect::new(inner.x, row_y, inner.width, 1);
        f.render_widget(Paragraph::new(input_line), input_area);
        row_y += 1;
    }

    // Separator
    let sep_area = Rect::new(inner.x, row_y, inner.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(iw),
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );
    row_y += 1;

    // Hint line
    let hint_line = if is_searching {
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
            Span::styled("G/g", Style::default().fg(t.cyan)),
            Span::styled(" top/bottom  ", Style::default().fg(t.fg_dim)),
            Span::styled("/", Style::default().fg(t.cyan)),
            Span::styled(" search  ", Style::default().fg(t.fg_dim)),
        ];
        if has_matches {
            hints.push(Span::styled("n/N", Style::default().fg(t.cyan)));
            hints.push(Span::styled(" next/prev  ", Style::default().fg(t.fg_dim)));
        }
        hints.push(Span::styled("o", Style::default().fg(t.cyan)));
        hints.push(Span::styled(" edit  ", Style::default().fg(t.fg_dim)));
        hints.push(Span::styled("esc", Style::default().fg(t.cyan)));
        hints.push(Span::styled(" close", Style::default().fg(t.fg_dim)));
        Line::from(hints)
    };
    let hint_area = Rect::new(inner.x, row_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
