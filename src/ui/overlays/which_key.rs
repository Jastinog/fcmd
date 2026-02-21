use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::theme::Theme;

pub(in crate::ui) fn render_which_key(
    f: &mut Frame,
    hints: &[(&str, &str)],
    leader: char,
    t: &Theme,
    area: Rect,
) {
    let (leader_icon, leader_label) = match leader {
        ' ' => ("󱁐 ", "Space"),
        's' => ("󰒓 ", "Sort"),
        'g' => (" ", "Go"),
        'y' => ("󰆏 ", "Yank"),
        'd' => ("󰗨 ", "Delete"),
        'c' => ("󰌑 ", "Change"),
        '\'' => (" ", "Mark"),
        'w' => ("󰕰 ", "Layout"),
        'u' => ("󰔃 ", "UI"),
        _ => return,
    };

    // Parse hints into groups: entries with empty key are section headers
    let mut groups: Vec<Vec<(&str, &str)>> = Vec::new();
    for &(key, desc) in hints {
        if key.is_empty() {
            groups.push(Vec::new());
        } else {
            if groups.is_empty() {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push((key, desc));
        }
    }
    // Remove empty groups
    groups.retain(|g| !g.is_empty());
    let has_sections = groups.len() > 1;

    // Layout
    let col_width = 18usize;
    let usable_width = area.width.saturating_sub(2) as usize;
    let num_cols = (usable_width / col_width).max(1);

    // Calculate total content rows (items + dashed separators between groups)
    let total_rows: usize = if has_sections {
        let item_rows: usize = groups
            .iter()
            .map(|items| items.len().div_ceil(num_cols))
            .sum();
        item_rows + groups.len() - 1
    } else {
        let n: usize = groups.iter().map(|items| items.len()).sum();
        n.div_ceil(num_cols)
    };

    // Popup dimensions
    let popup_h = (total_rows as u16 + 4).min(area.height);
    let popup_w = area
        .width
        .min((num_cols * col_width + 2) as u16)
        .max(20);
    let popup_x = (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + area.height.saturating_sub(popup_h + 1);

    let popup = Rect::new(area.x + popup_x, popup_y, popup_w, popup_h);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.orange))
        .title(format!(" {leader_icon}{leader_label} "))
        .title_style(Style::default().fg(t.orange))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;

    // Build render lines
    let mut lines: Vec<ListItem> = Vec::new();

    if has_sections {
        // Grouped layout with dashed separators between groups
        for (gi, items) in groups.iter().enumerate() {
            let group_rows = items.len().div_ceil(num_cols);
            for r in 0..group_rows {
                let mut spans: Vec<Span> = Vec::new();
                for c in 0..num_cols {
                    let idx = r * num_cols + c;
                    if idx < items.len() {
                        let (key, desc) = items[idx];
                        spans.push(Span::styled(
                            format!(" {key} "),
                            Style::default().fg(t.bg_text).bg(t.orange),
                        ));
                        let desc_text = format!(" {desc}");
                        let entry_chars = key.chars().count() + 2 + desc_text.chars().count();
                        let pad = col_width.saturating_sub(entry_chars);
                        spans.push(Span::styled(
                            desc_text,
                            Style::default().fg(t.fg),
                        ));
                        spans.push(Span::raw(" ".repeat(pad)));
                    } else {
                        spans.push(Span::raw(" ".repeat(col_width)));
                    }
                }
                let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                if used < iw {
                    spans.push(Span::raw(" ".repeat(iw - used)));
                }
                lines.push(ListItem::new(Line::from(spans)));
            }
            // Dashed separator between groups (not after last)
            if gi < groups.len() - 1 {
                lines.push(ListItem::new(Line::from(Span::styled(
                    "\u{254c}".repeat(iw),
                    Style::default().fg(t.border_inactive),
                ))));
            }
        }
    } else {
        // Flat column-major layout (for small leaders without sections)
        let all_items: Vec<(&str, &str)> =
            groups.into_iter().flatten().collect();
        let num_rows = all_items.len().div_ceil(num_cols);

        for row in 0..num_rows {
            let mut spans: Vec<Span> = Vec::new();
            for col in 0..num_cols {
                let idx = col * num_rows + row;
                if idx < all_items.len() {
                    let (key, desc) = all_items[idx];
                    spans.push(Span::styled(
                        format!(" {key} "),
                        Style::default().fg(t.bg_text).bg(t.orange),
                    ));
                    let desc_text = format!(" {desc}");
                    let entry_chars = key.chars().count() + 2 + desc_text.chars().count();
                    let pad = col_width.saturating_sub(entry_chars);
                    spans.push(Span::styled(
                        desc_text,
                        Style::default().fg(t.fg).bg(t.bg_light),
                    ));
                    spans.push(Span::styled(
                        " ".repeat(pad),
                        Style::default().bg(t.bg_light),
                    ));
                } else {
                    spans.push(Span::styled(
                        " ".repeat(col_width),
                        Style::default().bg(t.bg_light),
                    ));
                }
            }
            let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            if used < iw {
                spans.push(Span::styled(
                    " ".repeat(iw - used),
                    Style::default().bg(t.bg_light),
                ));
            }
            lines.push(ListItem::new(Line::from(spans)));
        }
    }

    let list_height = inner.height.saturating_sub(2) as usize;
    lines.truncate(list_height);
    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(lines), list_area);

    // Separator
    let sep_y = inner.y + list_height as u16;
    let sep_area = Rect::new(inner.x, sep_y, inner.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(iw),
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );

    // Hint line
    let hint_line = Line::from(vec![
        Span::styled(" esc", Style::default().fg(t.orange)),
        Span::styled(" cancel", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
