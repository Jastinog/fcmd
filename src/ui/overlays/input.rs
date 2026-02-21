use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{App, Mode};

pub(in crate::ui) fn render_input_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;

    let (title, accent, context) = match app.mode {
        Mode::Rename => {
            let ctx = app
                .tab()
                .active_panel()
                .selected_entry()
                .filter(|e| e.name != "..")
                .map(|e| e.name.clone());
            (" 󰑕 Rename ", t.yellow, ctx)
        }
        Mode::BookmarkAdd => {
            let ctx = app
                .bookmark_add_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            (" 󰃀 Bookmark ", t.yellow, ctx)
        }
        Mode::BookmarkRename => {
            let ctx = app.bookmark_rename_old.clone();
            (" 󰃀 Rename Bookmark ", t.yellow, ctx)
        }
        _ => (" 󰝒 New ", t.cyan, None),
    };

    // Height: border(2) + context(0-1) + input(1) + separator(1) + hints(1)
    let has_context = context.is_some();
    let h = if has_context { 6u16 } else { 5u16 };
    let w = 50u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let mut row = 0u16;

    // Context line (original name for rename)
    if let Some(ref orig) = context {
        let label = " 󰈔 ";
        let max_name = iw.saturating_sub(label.chars().count());
        let orig_chars: Vec<char> = orig.chars().collect();
        let name_display = if orig_chars.len() > max_name {
            let start = orig_chars.len() - max_name.saturating_sub(1);
            let tail: String = orig_chars[start..].iter().collect();
            format!("\u{2026}{tail}")
        } else {
            orig.clone()
        };
        let pad = iw.saturating_sub(label.chars().count() + name_display.chars().count());
        let ctx_line = Line::from(vec![
            Span::styled(label, Style::default().fg(t.fg_dim)),
            Span::styled(name_display, Style::default().fg(t.fg)),
            Span::styled(" ".repeat(pad), Style::default()),
        ]);
        let ctx_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
        f.render_widget(Paragraph::new(ctx_line), ctx_area);
        row += 1;
    }

    // Input field
    let input = &app.rename_input;
    let prefix = " \u{276f} ";
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

    // Build input spans: prefix + text before cursor + cursor char + text after cursor + padding
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
    let hint_line = match app.mode {
        Mode::Create => Line::from(vec![
            Span::styled(" \u{23ce}", Style::default().fg(accent)),
            Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(accent)),
            Span::styled(" cancel  ", Style::default().fg(t.fg_dim)),
            Span::styled("name/", Style::default().fg(accent)),
            Span::styled(" = dir", Style::default().fg(t.fg_dim)),
        ]),
        _ => Line::from(vec![
            Span::styled(" \u{23ce}", Style::default().fg(accent)),
            Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(accent)),
            Span::styled(" cancel", Style::default().fg(t.fg_dim)),
        ]),
    };
    let hint_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
