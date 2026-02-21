use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;

pub(in crate::ui) fn render_confirm_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let permanent = app.confirm_permanent;
    let accent = if permanent { t.red } else { t.yellow };
    let paths = &app.confirm_paths;
    let n = paths.len();

    // Height: border(2) + file list (capped) + separator(1) + hint(1)
    let max_list = 12usize;
    let list_h = n.min(max_list);
    let h = (list_h as u16 + 4).min(area.height);
    let w = 50u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let title = if permanent {
        format!(" 󰗨 Permanently Delete ({n}) ")
    } else {
        format!("  Move to Trash ({n}) ")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;

    // Adjust scroll so it stays in range
    let scroll = app.confirm_scroll.min(n.saturating_sub(list_height.max(1)));

    let mut items: Vec<ListItem> = Vec::new();
    for (i, (path, is_dir)) in paths.iter().enumerate().skip(scroll).take(list_height) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_dir = *is_dir;
        let icon = if is_dir { " " } else { " 󰈔 " };
        let icon_color = if is_dir { t.blue } else { t.fg_dim };

        let max_name = iw.saturating_sub(icon.chars().count());
        let name_display = if name.chars().count() > max_name {
            let truncated: String = name.chars().take(max_name.saturating_sub(1)).collect();
            format!("{truncated}\u{2026}")
        } else {
            name
        };
        let pad = iw.saturating_sub(icon.chars().count() + name_display.chars().count());

        let bg = if i == scroll && n > list_height {
            // No special highlight needed, just show the list
            t.bg_light
        } else {
            t.bg_light
        };

        let line = Line::from(vec![
            Span::styled(icon, Style::default().fg(icon_color).bg(bg)),
            Span::styled(name_display, Style::default().fg(t.fg).bg(bg)),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]);
        items.push(ListItem::new(line));
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
        Span::styled(" y", Style::default().fg(accent)),
        Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" cancel", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
