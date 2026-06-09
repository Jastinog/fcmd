use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;

pub(in crate::ui) fn render_archive(f: &mut Frame, app: &App, area: Rect) {
    let state = match app.archive_state {
        Some(ref s) => s,
        None => return,
    };
    let t = &app.theme;

    let w = ((area.width as u32 * 80 / 100) as u16).max(50).min(area.width.saturating_sub(4));
    let h = ((area.height as u32 * 75 / 100) as u16).max(12).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let archive_name = state
        .archive_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let title = format!(
        " \u{f410} {} ({} files, {}) ",
        archive_name,
        state.file_count,
        format_size(state.total_size),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(title)
        .title_style(Style::default().fg(t.cyan))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let iw = inner.width as usize;

    // Reserve bottom lines: search(0-1) + separator(1) + hints(1)
    let search_line = if state.searching { 1u16 } else { 0 };
    let bottom_lines = search_line + 2;
    let list_h = inner.height.saturating_sub(bottom_lines) as usize;

    // Render tree entries
    let size_col_w: usize = 8;

    for (vi, i) in (state.scroll..state.tree.len())
        .take(list_h)
        .enumerate()
    {
        let node = &state.tree[i];
        let is_active = i == state.cursor;

        let bg = if is_active { t.bg_light } else { t.bg };

        // Indent
        let indent = "  ".repeat(node.depth);
        let indent_w = indent.chars().count();

        // Icon + expand indicator
        let (icon, icon_fg) = if node.is_dir {
            let arrow = if node.expanded { "\u{f0078} " } else { "\u{f0054} " };
            (format!("{arrow}\u{f07b} "), t.yellow)
        } else {
            let ext_icon = crate::util::icons::file_icon(&node.name, false);
            (ext_icon.to_string(), t.fg)
        };

        // Size column (right-aligned)
        let size_str = if node.is_dir {
            String::new()
        } else {
            format_size(node.size)
        };
        let size_pad = size_col_w.saturating_sub(size_str.len());

        // Name (fill remaining space)
        let name_max = iw
            .saturating_sub(indent_w)
            .saturating_sub(icon.chars().count())
            .saturating_sub(size_col_w + 1);

        let name_display = if node.name.chars().count() > name_max {
            let mut s: String = node.name.chars().take(name_max.saturating_sub(1)).collect();
            s.push('\u{2026}');
            s
        } else {
            node.name.clone()
        };
        let name_pad = name_max.saturating_sub(name_display.chars().count());

        let name_fg = if node.is_dir {
            t.blue
        } else if is_active {
            t.yellow
        } else {
            t.fg
        };

        let spans = vec![
            Span::styled(&indent, Style::default().bg(bg)),
            Span::styled(&icon, Style::default().fg(icon_fg).bg(bg)),
            Span::styled(name_display, Style::default().fg(name_fg).bg(bg)),
            Span::styled(" ".repeat(name_pad), Style::default().bg(bg)),
            Span::styled(" ".repeat(size_pad), Style::default().bg(bg)),
            Span::styled(size_str, Style::default().fg(t.fg_dim).bg(bg)),
            Span::styled(" ", Style::default().bg(bg)),
        ];

        let row_area = Rect::new(inner.x, inner.y + vi as u16, inner.width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), row_area);
    }

    // Fill remaining list rows
    for vi in state.tree.len().saturating_sub(state.scroll)..list_h {
        let row_area = Rect::new(inner.x, inner.y + vi as u16, inner.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " ".repeat(iw),
                Style::default().bg(t.bg),
            ))),
            row_area,
        );
    }

    let mut bottom_row = inner.y + inner.height.saturating_sub(bottom_lines);

    // Search input
    if state.searching {
        let prefix = " / ";
        let query = &state.search_query;
        let pad = iw.saturating_sub(prefix.len() + query.chars().count() + 1);
        let search_line = Line::from(vec![
            Span::styled(prefix, Style::default().fg(t.yellow)),
            Span::styled(query.as_str(), Style::default().fg(t.fg)),
            Span::styled("\u{2588}", Style::default().fg(t.yellow)),
            Span::styled(" ".repeat(pad), Style::default()),
        ]);
        let search_area = Rect::new(inner.x, bottom_row, inner.width, 1);
        f.render_widget(Paragraph::new(search_line), search_area);
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

    // Hints
    let hints = if state.searching {
        vec![
            Span::styled(" \u{23ce}", Style::default().fg(t.cyan)),
            Span::styled(" accept  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(t.cyan)),
            Span::styled(" clear", Style::default().fg(t.fg_dim)),
        ]
    } else {
        vec![
            Span::styled(" j/k", Style::default().fg(t.cyan)),
            Span::styled(" nav  ", Style::default().fg(t.fg_dim)),
            Span::styled("l/h", Style::default().fg(t.cyan)),
            Span::styled(" expand/collapse  ", Style::default().fg(t.fg_dim)),
            Span::styled("x", Style::default().fg(t.cyan)),
            Span::styled(" extract  ", Style::default().fg(t.fg_dim)),
            Span::styled("X", Style::default().fg(t.cyan)),
            Span::styled(" extract all  ", Style::default().fg(t.fg_dim)),
            Span::styled("/", Style::default().fg(t.cyan)),
            Span::styled(" search  ", Style::default().fg(t.fg_dim)),
            Span::styled("q", Style::default().fg(t.cyan)),
            Span::styled(" close", Style::default().fg(t.fg_dim)),
        ]
    };
    let hint_area = Rect::new(inner.x, bottom_row, inner.width, 1);
    f.render_widget(Paragraph::new(Line::from(hints)), hint_area);
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
