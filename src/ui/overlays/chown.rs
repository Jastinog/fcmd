use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;

pub(in crate::ui) fn render_chown_picker(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let Some(ref picker) = app.chown_picker else {
        return;
    };

    let accent = t.cyan;

    // Popup size
    let w = 60u16.min(area.width.saturating_sub(4)).max(36);
    let h = 20u16.min(area.height.saturating_sub(2)).max(10);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);
    f.render_widget(Clear, popup);

    // Context info
    let n_paths = picker.paths.len();
    let title = if n_paths > 1 {
        format!("  Owner ({n_paths} items) ")
    } else {
        "  Owner ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 4 || inner.width < 12 {
        return;
    }

    let iw = inner.width as usize;
    let col_w = iw / 2;
    let list_height = inner.height.saturating_sub(2) as usize;

    // Current selection display at top is built into the columns via highlighting

    // --- Users column (left) ---
    let user_area = Rect::new(inner.x, inner.y, col_w as u16, list_height as u16);
    let is_user_active = picker.column == 0;

    let user_scroll = picker.user_scroll;

    let mut user_items: Vec<ListItem> = Vec::new();
    // Column header
    let header_style = if is_user_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(t.fg_dim)
    };
    let user_header_pad = col_w.saturating_sub(6);
    user_items.push(ListItem::new(Line::from(vec![
        Span::styled(" User", header_style),
        Span::styled(" ".repeat(user_header_pad.max(1)), Style::default()),
    ])));

    let user_list_h = list_height.saturating_sub(1); // minus header
    for (i, (name, uid)) in picker
        .users
        .iter()
        .enumerate()
        .skip(user_scroll)
        .take(user_list_h)
    {
        let is_cursor = i == picker.user_cursor;
        let is_current = picker.current_uid == Some(*uid);

        let marker = if is_cursor { "\u{25b8} " } else { "  " };
        let uid_str = format!("{uid}");
        let max_name = col_w.saturating_sub(marker.chars().count() + uid_str.len() + 2);
        let name_display = if name.chars().count() > max_name {
            let trunc: String = name.chars().take(max_name.saturating_sub(1)).collect();
            format!("{trunc}\u{2026}")
        } else {
            name.clone()
        };
        let pad =
            col_w.saturating_sub(marker.chars().count() + name_display.chars().count() + uid_str.len() + 1);

        if is_cursor && is_user_active {
            let s = Style::default().fg(t.bg).bg(t.blue);
            user_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, s),
                Span::styled(name_display, s),
                Span::styled(" ".repeat(pad), s),
                Span::styled(uid_str, s),
                Span::styled(" ", s),
            ])));
        } else if is_current {
            user_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.green)),
                Span::styled(name_display, Style::default().fg(t.green)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(uid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        } else {
            user_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_display, Style::default().fg(t.fg)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(uid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        }
    }

    user_items.truncate(list_height);
    f.render_widget(List::new(user_items), user_area);

    // --- Groups column (right) ---
    let group_x = inner.x + col_w as u16;
    let group_col_w = iw.saturating_sub(col_w);
    let group_area = Rect::new(group_x, inner.y, group_col_w as u16, list_height as u16);
    let is_group_active = picker.column == 1;

    let group_scroll = picker.group_scroll;

    let mut group_items: Vec<ListItem> = Vec::new();
    // Column header
    let header_style = if is_group_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(t.fg_dim)
    };
    let group_header_pad = group_col_w.saturating_sub(7);
    group_items.push(ListItem::new(Line::from(vec![
        Span::styled(" Group", header_style),
        Span::styled(" ".repeat(group_header_pad.max(1)), Style::default()),
    ])));

    let group_list_h = list_height.saturating_sub(1);
    for (i, (name, gid)) in picker
        .groups
        .iter()
        .enumerate()
        .skip(group_scroll)
        .take(group_list_h)
    {
        let is_cursor = i == picker.group_cursor;
        let is_current = picker.current_gid == Some(*gid);

        let marker = if is_cursor { "\u{25b8} " } else { "  " };
        let gid_str = format!("{gid}");
        let max_name = group_col_w.saturating_sub(marker.chars().count() + gid_str.len() + 2);
        let name_display = if name.chars().count() > max_name {
            let trunc: String = name.chars().take(max_name.saturating_sub(1)).collect();
            format!("{trunc}\u{2026}")
        } else {
            name.clone()
        };
        let pad = group_col_w
            .saturating_sub(marker.chars().count() + name_display.chars().count() + gid_str.len() + 1);

        if is_cursor && is_group_active {
            let s = Style::default().fg(t.bg).bg(t.blue);
            group_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, s),
                Span::styled(name_display, s),
                Span::styled(" ".repeat(pad), s),
                Span::styled(gid_str, s),
                Span::styled(" ", s),
            ])));
        } else if is_current {
            group_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.green)),
                Span::styled(name_display, Style::default().fg(t.green)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(gid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        } else {
            group_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_display, Style::default().fg(t.fg)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(gid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        }
    }

    group_items.truncate(list_height);
    f.render_widget(List::new(group_items), group_area);

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
        Span::styled(" apply  ", Style::default().fg(t.fg_dim)),
        Span::styled("tab", Style::default().fg(accent)),
        Span::styled(" switch  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" cancel", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
