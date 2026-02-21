use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;

pub(in crate::ui) fn render_bookmarks(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.yellow;
    let bm = &app.bookmarks;
    let len = bm.len();
    if len == 0 {
        return;
    }

    let max_list = 12usize;
    let list_h = len.min(max_list);
    let h = (list_h as u16 + 4).min(area.height);
    let w = 50u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let title = format!(" \u{f02e6} Bookmarks ({len}) ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;
    let scroll = app.bookmark_scroll.min(len.saturating_sub(list_height.max(1)));

    let home = dirs::home_dir().unwrap_or_default();

    let mut items: Vec<ListItem> = Vec::new();
    for (i, (name, path)) in bm.iter().enumerate().skip(scroll).take(list_height) {
        let is_cursor = i == app.bookmark_cursor;

        let marker = if is_cursor { "\u{25b8} " } else { "  " };

        // Shorten path with ~
        let path_str = path.to_string_lossy();
        let home_str = home.to_string_lossy();
        let short_path = if !home_str.is_empty() && path_str.starts_with(home_str.as_ref()) {
            format!("~{}", &path_str[home_str.len()..])
        } else {
            path_str.into_owned()
        };

        let marker_w = marker.chars().count();
        let name_col = format!("{name}  ");
        let name_w = name_col.chars().count();
        let path_max = iw.saturating_sub(marker_w + name_w);
        let path_display = if short_path.chars().count() > path_max {
            let start = short_path.chars().count() - path_max.saturating_sub(1);
            let tail: String = short_path.chars().skip(start).collect();
            format!("\u{2026}{tail}")
        } else {
            short_path.clone()
        };
        let pad = iw.saturating_sub(marker_w + name_w + path_display.chars().count());

        if is_cursor {
            let cursor_style = Style::default().fg(t.bg).bg(t.blue);
            let line = Line::from(vec![
                Span::styled(marker, cursor_style),
                Span::styled(name_col, cursor_style),
                Span::styled(path_display, cursor_style),
                Span::styled(" ".repeat(pad), cursor_style),
            ]);
            items.push(ListItem::new(line));
        } else {
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_col, Style::default().fg(accent)),
                Span::styled(path_display, Style::default().fg(t.fg_dim)),
                Span::styled(" ".repeat(pad), Style::default()),
            ]);
            items.push(ListItem::new(line));
        }
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
        Span::styled(" \u{23ce}", Style::default().fg(accent)),
        Span::styled(" go  ", Style::default().fg(t.fg_dim)),
        Span::styled("a", Style::default().fg(accent)),
        Span::styled(" add  ", Style::default().fg(t.fg_dim)),
        Span::styled("d", Style::default().fg(accent)),
        Span::styled(" del  ", Style::default().fg(t.fg_dim)),
        Span::styled("e", Style::default().fg(accent)),
        Span::styled(" rename  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
