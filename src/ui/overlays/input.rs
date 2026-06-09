use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{App, Mode};
use crate::ui::util::{display_width, truncate_to_width_left};

use super::input_field_line;

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
        Mode::SelectPattern => (" 󰒅 Select ", t.green, None),
        Mode::UnselectPattern => (" 󰒅 Unselect ", t.red, None),
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
        let max_name = iw.saturating_sub(display_width(label));
        let name_display = truncate_to_width_left(orig, max_name);
        let pad = iw.saturating_sub(display_width(label) + display_width(&name_display));
        let ctx_line = Line::from(vec![
            Span::styled(label, Style::default().fg(t.fg_dim)),
            Span::styled(name_display, Style::default().fg(t.fg)),
            Span::styled(" ".repeat(pad), Style::default()),
        ]);
        let ctx_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
        f.render_widget(Paragraph::new(ctx_line), ctx_area);
        row += 1;
    }

    // Input field (cursor stays at the end; text scrolls to show the tail)
    let input_line = input_field_line(&app.rename_input, " \u{276f} ", iw, accent, t);
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
        Mode::SelectPattern | Mode::UnselectPattern => Line::from(vec![
            Span::styled(" \u{23ce}", Style::default().fg(accent)),
            Span::styled(" select  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(accent)),
            Span::styled(" cancel  ", Style::default().fg(t.fg_dim)),
            Span::styled("*", Style::default().fg(accent)),
            Span::styled(" any  ", Style::default().fg(t.fg_dim)),
            Span::styled("?", Style::default().fg(accent)),
            Span::styled(" char", Style::default().fg(t.fg_dim)),
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
