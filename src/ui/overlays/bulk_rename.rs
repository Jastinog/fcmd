use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;
use crate::app::bulk_rename::BulkRenameSubMode;
use crate::ui::util::{display_width, visible_input_tail};

pub(in crate::ui) fn render_bulk_rename(f: &mut Frame, app: &App, area: Rect) {
    let state = match app.bulk_rename {
        Some(ref s) => s,
        None => return,
    };
    let t = &app.theme;

    // Popup dimensions: 80% width, 70% height
    let w = ((area.width as u32 * 80 / 100) as u16)
        .max(40)
        .min(area.width.saturating_sub(4));
    let h = ((area.height as u32 * 70 / 100) as u16)
        .max(10)
        .min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let changed = state.changed_count();
    let total = state.entries.len();
    let title = format!(" \u{f044} Bulk Rename ({changed}/{total} changed) ");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.yellow))
        .title(title)
        .title_style(Style::default().fg(t.yellow))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let iw = inner.width as usize;
    let conflicts = state.conflict_indices();

    // Reserve: 1 line for separator, 1 line for hints, 1 line for error/find-replace
    let bottom_lines: u16 =
        if state.sub_mode == BulkRenameSubMode::FindReplace || state.error.is_some() {
            3
        } else {
            2
        };
    let list_h = inner.height.saturating_sub(bottom_lines) as usize;

    // Render entries
    let arrow = " \u{2192} ";
    let arrow_w = 4; // " → " with spaces
    let name_col_w = iw.saturating_sub(arrow_w) / 2;
    let new_col_w = iw.saturating_sub(name_col_w + arrow_w);

    for (vi, i) in (state.scroll..state.entries.len()).take(list_h).enumerate() {
        let entry = &state.entries[i];
        let is_active = i == state.cursor;
        let is_changed = entry.new_name != entry.original_name;
        let is_conflict = conflicts.contains(&i);

        let icon = if entry.is_dir {
            "\u{f07b} "
        } else {
            "\u{f15b} "
        };

        // Original name (left column)
        let orig_display =
            truncate_with_ellipsis(&format!("{icon}{}", entry.original_name), name_col_w);
        let orig_pad = name_col_w.saturating_sub(char_width(&orig_display));

        let orig_fg = if is_active { t.fg } else { t.fg_dim };
        let orig_bg = if is_active { t.bg_light } else { t.bg };

        // New name (right column)
        let new_display = if is_active && state.sub_mode == BulkRenameSubMode::Edit {
            // Show edit input with cursor
            let input = &state.edit_input;
            truncate_with_ellipsis(input, new_col_w.saturating_sub(1))
        } else {
            truncate_with_ellipsis(&entry.new_name, new_col_w)
        };
        let new_pad = new_col_w.saturating_sub(char_width(&new_display));

        let new_fg = if is_conflict {
            t.red
        } else if is_changed {
            t.green
        } else {
            t.fg_dim
        };
        let new_bg = if is_active { t.bg_light } else { t.bg };

        let arrow_fg = if is_changed { t.yellow } else { t.fg_dim };

        let mut spans = vec![
            Span::styled(orig_display, Style::default().fg(orig_fg).bg(orig_bg)),
            Span::styled(" ".repeat(orig_pad), Style::default().bg(orig_bg)),
            Span::styled(
                arrow,
                Style::default()
                    .fg(arrow_fg)
                    .bg(if is_active { t.bg_light } else { t.bg }),
            ),
        ];

        if is_active && state.sub_mode == BulkRenameSubMode::Edit {
            // Render edit input with cursor block (cursor at end; tail scrolls in).
            let visible = visible_input_tail(&state.edit_input, new_col_w.saturating_sub(1));
            let epad = new_col_w.saturating_sub(display_width(&visible) + 1);

            spans.push(Span::styled(
                visible,
                Style::default().fg(new_fg).bg(new_bg),
            ));
            spans.push(Span::styled(
                "\u{2588}",
                Style::default().fg(t.yellow).bg(new_bg),
            ));
            spans.push(Span::styled(" ".repeat(epad), Style::default().bg(new_bg)));
        } else {
            spans.push(Span::styled(
                new_display,
                Style::default().fg(new_fg).bg(new_bg),
            ));
            spans.push(Span::styled(
                " ".repeat(new_pad),
                Style::default().bg(new_bg),
            ));
        }

        let row_area = Rect::new(inner.x, inner.y + vi as u16, inner.width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), row_area);
    }

    let mut bottom_row = inner.y + inner.height.saturating_sub(bottom_lines);

    // Error or find/replace input
    if state.sub_mode == BulkRenameSubMode::FindReplace {
        let prefix = ":%s";
        let input = &state.find_replace_input;
        let pad = iw.saturating_sub(char_width(prefix) + char_width(input) + 1);
        let fr_line = Line::from(vec![
            Span::styled(prefix, Style::default().fg(t.yellow)),
            Span::styled(input.as_str(), Style::default().fg(t.fg)),
            Span::styled("\u{2588}", Style::default().fg(t.yellow)),
            Span::styled(" ".repeat(pad), Style::default()),
        ]);
        let fr_area = Rect::new(inner.x, bottom_row, inner.width, 1);
        f.render_widget(Paragraph::new(fr_line), fr_area);
        bottom_row += 1;
    } else if let Some(ref err) = state.error {
        let err_display = truncate_with_ellipsis(err, iw);
        let pad = iw.saturating_sub(char_width(&err_display));
        let err_line = Line::from(vec![
            Span::styled(err_display, Style::default().fg(t.red)),
            Span::styled(" ".repeat(pad), Style::default()),
        ]);
        let err_area = Rect::new(inner.x, bottom_row, inner.width, 1);
        f.render_widget(Paragraph::new(err_line), err_area);
        bottom_row += 1;
    }

    // Separator
    let sep = "\u{2500}".repeat(iw);
    let sep_area = Rect::new(inner.x, bottom_row, inner.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            sep,
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );
    bottom_row += 1;

    // Hint line
    let hints = match state.sub_mode {
        BulkRenameSubMode::Nav => vec![
            Span::styled(" j/k", Style::default().fg(t.yellow)),
            Span::styled(" nav  ", Style::default().fg(t.fg_dim)),
            Span::styled("i", Style::default().fg(t.yellow)),
            Span::styled(" edit  ", Style::default().fg(t.fg_dim)),
            Span::styled(":", Style::default().fg(t.yellow)),
            Span::styled("%s find/replace  ", Style::default().fg(t.fg_dim)),
            Span::styled("d", Style::default().fg(t.yellow)),
            Span::styled(" remove  ", Style::default().fg(t.fg_dim)),
            Span::styled("u", Style::default().fg(t.yellow)),
            Span::styled(" undo  ", Style::default().fg(t.fg_dim)),
            Span::styled("\u{23ce}", Style::default().fg(t.yellow)),
            Span::styled(" apply  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(t.yellow)),
            Span::styled(" cancel", Style::default().fg(t.fg_dim)),
        ],
        BulkRenameSubMode::Edit => vec![
            Span::styled(" \u{23ce}/tab", Style::default().fg(t.yellow)),
            Span::styled(" next  ", Style::default().fg(t.fg_dim)),
            Span::styled("S-tab", Style::default().fg(t.yellow)),
            Span::styled(" prev  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(t.yellow)),
            Span::styled(" done", Style::default().fg(t.fg_dim)),
        ],
        BulkRenameSubMode::FindReplace => vec![
            Span::styled(" \u{23ce}", Style::default().fg(t.yellow)),
            Span::styled(" apply  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(t.yellow)),
            Span::styled(" cancel  ", Style::default().fg(t.fg_dim)),
            Span::styled("format: ", Style::default().fg(t.fg_dim)),
            Span::styled("/old/new", Style::default().fg(t.yellow)),
        ],
    };
    let hint_area = Rect::new(inner.x, bottom_row, inner.width, 1);
    f.render_widget(Paragraph::new(Line::from(hints)), hint_area);
}

fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    crate::ui::util::truncate_to_width(s, max)
}

fn char_width(s: &str) -> usize {
    crate::ui::util::display_width(s)
}
