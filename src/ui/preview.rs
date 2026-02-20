use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::preview::Preview;
use crate::theme::Theme;

/// Build styled spans for a hex dump line: offset(dim), hex(fg), ascii(accent).
pub(super) fn build_hex_spans(line: &str, dim: Color, fg: Color, accent: Color) -> Vec<Span<'static>> {
    // Format: "OFFSET  HH HH ...  |ASCII...|"
    if let Some(pipe_start) = line.find('|') {
        let offset_end = line.find("  ").unwrap_or(8).min(10);
        let offset_part: String = line.chars().take(offset_end).collect();
        let hex_part: String = line.chars().skip(offset_end).take(pipe_start - offset_end).collect();
        let ascii_part: String = line.chars().skip(pipe_start).collect();
        vec![
            Span::styled(offset_part, Style::default().fg(dim)),
            Span::styled(hex_part, Style::default().fg(fg)),
            Span::styled(ascii_part, Style::default().fg(accent)),
        ]
    } else {
        // Truncation line or other
        vec![Span::styled(line.to_string(), Style::default().fg(dim))]
    }
}

/// Build content spans for a preview line, using syntax highlighting if available.
pub(super) fn build_content_spans<'a>(
    p: &Preview,
    line_idx: usize,
    max_width: usize,
    default_fg: Color,
) -> Vec<Span<'a>> {
    if let Some(ref styled) = p.styled_lines
        && let Some(segments) = styled.get(line_idx)
    {
        let mut spans = Vec::new();
        let mut chars_used = 0;
        for seg in segments {
            if chars_used >= max_width {
                break;
            }
            let remaining = max_width - chars_used;
            let seg_chars: usize = seg.text.chars().count();
            if seg_chars <= remaining {
                spans.push(Span::styled(seg.text.clone(), seg.style));
                chars_used += seg_chars;
            } else {
                let truncated: String = seg.text.chars().take(remaining).collect();
                spans.push(Span::styled(truncated, seg.style));
                chars_used += remaining;
            }
        }
        return spans;
    }
    // Fallback: plain text
    let line = &p.lines[line_idx];
    let content: String = if line.chars().count() > max_width {
        line.chars().take(max_width).collect()
    } else {
        line.clone()
    };
    vec![Span::styled(content, Style::default().fg(default_fg))]
}

pub(super) fn render_preview(f: &mut Frame, preview: &Option<Preview>, area: Rect, t: &Theme) {
    let (title, info) = match preview {
        Some(p) => (p.title.as_str(), p.info.as_str()),
        None => ("Preview", ""),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(format!(" {title} [{info}] "))
        .title_style(Style::default().fg(t.cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(p) = preview else { return };

    // Right-aligned position in title area for binary
    if p.is_binary {
        let visible = inner.height as usize;
        let (_, last_byte, total, pct) = p.hex_position(visible);
        let size_text = super::overlays::format_binary_size(total);
        let pos_text = format!(" 0x{last_byte:04X} {size_text} {pct}% ");
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

    let visible = inner.height as usize;
    let width = inner.width as usize;

    let items: Vec<ListItem> = if p.is_binary {
        p.lines
            .iter()
            .skip(p.scroll)
            .take(visible)
            .map(|line| {
                let max_content = width;
                let content: String = if line.chars().count() > max_content {
                    line.chars().take(max_content).collect()
                } else {
                    line.clone()
                };
                let spans = build_hex_spans(&content, t.fg_dim, t.fg, t.cyan);
                ListItem::new(Line::from(spans))
            })
            .collect()
    } else {
        (0..visible)
            .map(|i| {
                let line_idx = i + p.scroll;
                if line_idx >= p.lines.len() {
                    return ListItem::new(Line::from(""));
                }
                let line_num = line_idx + 1;
                let num_width = 4;
                let max_content = width.saturating_sub(num_width + 2);
                let mut spans = vec![Span::styled(
                    format!("{line_num:>num_width$} ", num_width = num_width),
                    Style::default().fg(t.fg_dim),
                )];
                spans.extend(build_content_spans(p, line_idx, max_content, t.fg));
                ListItem::new(Line::from(spans))
            })
            .collect()
    };

    f.render_widget(List::new(items), inner);
}
