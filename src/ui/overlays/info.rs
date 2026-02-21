use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;

pub(in crate::ui) fn render_info_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let lines = &app.info_lines;
    if lines.is_empty() {
        return;
    }

    let accent = t.cyan;

    // Title from first line (Name)
    let title = lines
        .first()
        .map(|(_, v)| {
            let is_dir = lines
                .iter()
                .any(|(k, v)| k == "Type" && v == "Directory");
            let icon = if is_dir { " " } else { " 󰈔 " };
            format!(" {icon}{v} ")
        })
        .unwrap_or_else(|| " Info ".into());

    // Popup dimensions
    let w = 56u16.min(area.width.saturating_sub(4)).max(30);
    // lines count minus Name (shown in title) + border(2) + separator(1) + hint(1)
    let content_lines = lines.len().saturating_sub(1);
    let h = (content_lines as u16 + 4).min(area.height.saturating_sub(2)).max(6);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;

    // Find max key width for alignment
    let key_width = lines
        .iter()
        .skip(1) // skip Name (in title)
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);

    let scroll = app.info_scroll.min(content_lines.saturating_sub(list_height.max(1)));

    let mut items: Vec<ListItem> = Vec::new();
    for (k, v) in lines.iter().skip(1).skip(scroll).take(list_height) {
        let pad = key_width.saturating_sub(k.chars().count());
        let key_col = format!(" {k}{} ", " ".repeat(pad));
        let key_w = key_col.chars().count();
        let val_max = iw.saturating_sub(key_w);
        let val_display = if v.chars().count() > val_max {
            let trunc: String = v.chars().take(val_max.saturating_sub(1)).collect();
            format!("{trunc}\u{2026}")
        } else {
            v.clone()
        };
        let val_pad = iw.saturating_sub(key_w + val_display.chars().count());

        // Color permissions specially
        let val_spans = if k == "Permissions" {
            let mut spans = Vec::new();
            for ch in val_display.chars() {
                let color = match ch {
                    'r' => t.green,
                    'w' => t.yellow,
                    'x' => t.red,
                    '-' => t.fg_dim,
                    _ => t.fg,
                };
                spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
            }
            spans.push(Span::styled(" ".repeat(val_pad), Style::default()));
            spans
        } else {
            vec![
                Span::styled(val_display, Style::default().fg(t.fg)),
                Span::styled(" ".repeat(val_pad), Style::default()),
            ]
        };

        let mut all_spans = vec![Span::styled(key_col, Style::default().fg(accent))];
        all_spans.extend(val_spans);
        items.push(ListItem::new(Line::from(all_spans)));
    }

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

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
        Span::styled(" esc", Style::default().fg(accent)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
