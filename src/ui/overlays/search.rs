use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;

pub(in crate::ui) fn render_search_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.blue;

    // Count matches
    let query_lower = app.search_query.to_lowercase();
    let match_count = if query_lower.is_empty() {
        0
    } else {
        app.tab()
            .active_panel()
            .entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&query_lower))
            .count()
    };

    // Height: border(2) + input(1) + separator(1) + hints(1) = 5
    let h = 5u16;
    let w = 50u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    // Title with match count
    let title = if !app.search_query.is_empty() {
        format!(" 󰍉 Search ({match_count}) ")
    } else {
        " 󰍉 Search ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let mut row = 0u16;

    // Input field
    let input = &app.search_query;
    let prefix = " / ";
    let prefix_len = prefix.chars().count();
    let field_w = iw.saturating_sub(prefix_len).max(1);

    let input_chars: Vec<char> = input.chars().collect();
    let input_char_len = input_chars.len();
    let (visible_input, cursor_pos) = if input_char_len < field_w {
        (input.clone(), input_char_len)
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
        Span::styled(prefix, Style::default().fg(accent)),
        Span::styled(before, Style::default().fg(t.fg).bg(t.bg_light)),
        Span::styled("\u{2588}", Style::default().fg(accent).bg(t.bg_light)),
        Span::styled(after, Style::default().fg(t.fg).bg(t.bg_light)),
        Span::styled(" ".repeat(pad), Style::default().bg(t.bg_light)),
    ]);
    let input_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
    f.render_widget(Paragraph::new(input_line), input_area);
    row += 1;

    // Separator
    let sep_line = Line::from(Span::styled(
        "\u{2500}".repeat(iw),
        Style::default().fg(t.border_inactive),
    ));
    let sep_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
    f.render_widget(Paragraph::new(sep_line), sep_area);
    row += 1;

    // Hint line
    let hint_line = Line::from(vec![
        Span::styled(" \u{23ce}", Style::default().fg(accent)),
        Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" cancel  ", Style::default().fg(t.fg_dim)),
        Span::styled("n/N", Style::default().fg(accent)),
        Span::styled(" next/prev", Style::default().fg(t.fg_dim)),
    ]);
    let hint_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
