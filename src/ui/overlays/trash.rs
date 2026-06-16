use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::ui::util::{display_width, truncate_to_width_left};

pub(in crate::ui) fn render_trash(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.yellow;
    let items = app.undo_stack.trashed();
    let len = items.len();
    if len == 0 {
        return;
    }

    let max_list = 14usize;
    let list_h = len.min(max_list);
    let h = (list_h as u16 + 4).min(area.height);
    let w = 64u16.min(area.width.saturating_sub(4)).max(34);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let title = format!(" \u{f014} Trash \u{2014} restore ({len}) ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;
    let max_scroll = len.saturating_sub(list_height.max(1));
    let scroll = app.trash_scroll.min(max_scroll);

    let home = dirs::home_dir().unwrap_or_default();
    let home_str = home.to_string_lossy();

    let mut list_items: Vec<ListItem> = Vec::new();
    for (i, item) in items.iter().enumerate().skip(scroll).take(list_height) {
        let is_cursor = i == app.trash_cursor;
        let marker = if is_cursor { "\u{25b8} " } else { "  " };

        // Show the original location (where it will be restored), shortened with ~.
        let path_str = item.original_path.to_string_lossy();
        let short_path = if !home_str.is_empty() && path_str.starts_with(home_str.as_ref()) {
            format!("~{}", &path_str[home_str.len()..])
        } else {
            path_str.into_owned()
        };

        let marker_w = display_width(marker);
        let path_max = iw.saturating_sub(marker_w);
        let path_display = truncate_to_width_left(&short_path, path_max);
        let pad = iw.saturating_sub(marker_w + display_width(&path_display));

        let (marker_style, path_style, pad_style) = if is_cursor {
            let style = Style::default().fg(t.bg_text).bg(t.blue);
            (style, style, style)
        } else {
            (
                Style::default().fg(t.fg_dim),
                Style::default().fg(t.fg),
                Style::default(),
            )
        };
        list_items.push(ListItem::new(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled(path_display, path_style),
            Span::styled(" ".repeat(pad), pad_style),
        ])));
    }

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(list_items), list_area);

    // Separator with scroll indicator
    let sep_y = inner.y + list_height as u16;
    let sep_area = Rect::new(inner.x, sep_y, inner.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            super::scroll_separator(iw, scroll, max_scroll),
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );

    // Hint line
    let hint_line = Line::from(vec![
        Span::styled(" \u{23ce}/r", Style::default().fg(accent)),
        Span::styled(" restore  ", Style::default().fg(t.fg_dim)),
        Span::styled("R", Style::default().fg(accent)),
        Span::styled(" restore all  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
