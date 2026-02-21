use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;

pub(in crate::ui) fn render_chmod_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.cyan;
    let input = &app.rename_input;

    // Parse current input as octal for live preview
    let parsed_mode = if !input.is_empty() && input.chars().all(|c| c.is_ascii_digit() && c <= '7')
    {
        u32::from_str_radix(input, 8).ok()
    } else {
        None
    };

    // Context: file name or item count
    let n_paths = app.chmod_paths.len();
    let file_ctx = if n_paths > 1 {
        format!("{n_paths} items")
    } else {
        app.chmod_paths
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    // border(2) + file(1) + input(1) + thin_sep(1) + 3 perm rows(3) + sep(1) + hint(1) = 10
    let h = 10u16.min(area.height);
    let w = 46u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(" 󰌑 Permissions ")
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let mut row = 0u16;

    // -- File context line --
    {
        let icon = " 󰈔 ";
        let icon_w = icon.chars().count();
        let max_name = iw.saturating_sub(icon_w);
        let name_display = if file_ctx.chars().count() > max_name {
            let start = file_ctx.chars().count() - max_name.saturating_sub(1);
            let tail: String = file_ctx.chars().skip(start).collect();
            format!("\u{2026}{tail}")
        } else {
            file_ctx
        };
        let pad = iw.saturating_sub(icon_w + name_display.chars().count());
        let line = Line::from(vec![
            Span::styled(icon, Style::default().fg(t.fg_dim)),
            Span::styled(name_display, Style::default().fg(t.fg)),
            Span::styled(" ".repeat(pad), Style::default()),
        ]);
        f.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + row, inner.width, 1),
        );
        row += 1;
    }

    // -- Input field --
    {
        let prefix = " \u{276f} ";
        let prefix_w = prefix.chars().count();
        let input_w = input.chars().count();

        // Build rwx colored spans for suffix
        let mut suffix_spans: Vec<Span> = Vec::new();
        if let Some(mode) = parsed_mode {
            suffix_spans.push(Span::raw("  "));
            // Render each rwx char with color: r=green, w=yellow, x=red, -=dim
            let rwx_str = crate::app::chmod::format_rwx(mode);
            for ch in rwx_str.chars() {
                let color = match ch {
                    'r' => t.green,
                    'w' => t.yellow,
                    'x' => t.red,
                    _ => t.fg_dim,
                };
                suffix_spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(color),
                ));
            }
        }
        let suffix_len: usize = suffix_spans.iter().map(|s| s.content.chars().count()).sum();

        let used = prefix_w + input_w + 1 + suffix_len;
        let pad = iw.saturating_sub(used);

        let mut spans = vec![
            Span::styled(prefix, Style::default().fg(accent)),
            Span::styled(input.clone(), Style::default().fg(t.fg).bg(t.bg_light)),
            Span::styled("\u{2588}", Style::default().fg(accent).bg(t.bg_light)),
            Span::styled(" ".repeat(pad), Style::default().bg(t.bg_light)),
        ];
        spans.extend(suffix_spans);

        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, inner.y + row, inner.width, 1),
        );
        row += 1;
    }

    // -- Thin separator --
    {
        let line = Line::from(Span::styled(
            "\u{254c}".repeat(iw),
            Style::default().fg(t.border_inactive),
        ));
        f.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + row, inner.width, 1),
        );
        row += 1;
    }

    // -- Permission breakdown: owner / group / other --
    {
        // Extract the 3 octal digits (owner, group, other)
        let digits: [Option<u32>; 3] = if let Some(mode) = parsed_mode {
            match input.len() {
                4 | 3 => [
                    Some((mode >> 6) & 7),
                    Some((mode >> 3) & 7),
                    Some(mode & 7),
                ],
                2 => [None, Some((mode >> 3) & 7), Some(mode & 7)],
                1 => [None, None, Some(mode & 7)],
                _ => [None, None, None],
            }
        } else {
            [None, None, None]
        };

        let labels = [" 󰀄 owner ", " 󰡉 group ", " 󰀈 other "];
        let label_w = 9; // all labels are 9 display chars

        for i in 0..3 {
            let label = labels[i];
            let digit = digits[i];
            let active = digit.is_some();

            let d = digit.unwrap_or(0);
            let r = d & 4 != 0;
            let w = d & 2 != 0;
            let x = d & 1 != 0;

            // rwx column: 5 chars " rwx "
            let rwx_col = format!(
                " {}{}{}",
                if r { 'r' } else { '-' },
                if w { 'w' } else { '-' },
                if x { 'x' } else { '-' },
            );

            // Description
            let desc = if !active {
                String::new()
            } else {
                let mut parts = Vec::new();
                if r { parts.push("read"); }
                if w { parts.push("write"); }
                if x { parts.push("execute"); }
                if parts.is_empty() {
                    "  no access".to_string()
                } else {
                    format!("  {}", parts.join(", "))
                }
            };

            let used = label_w + rwx_col.chars().count() + desc.chars().count();
            let pad = iw.saturating_sub(used);

            let dim = Style::default().fg(t.fg_dim);

            let label_style = if active {
                Style::default().fg(accent)
            } else {
                dim
            };

            let mut spans = vec![Span::styled(label, label_style)];

            // Colored rwx chars
            if active {
                spans.push(Span::styled(" ", Style::default()));
                for ch in [if r { 'r' } else { '-' }, if w { 'w' } else { '-' }, if x { 'x' } else { '-' }] {
                    let color = match ch {
                        'r' => t.green,
                        'w' => t.yellow,
                        'x' => t.red,
                        _ => t.fg_dim,
                    };
                    spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
                }
            } else {
                spans.push(Span::styled(rwx_col, dim));
            }

            let desc_style = if active {
                Style::default().fg(t.fg_dim)
            } else {
                dim
            };
            spans.push(Span::styled(desc, desc_style));
            spans.push(Span::styled(" ".repeat(pad), Style::default()));

            f.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(inner.x, inner.y + row, inner.width, 1),
            );
            row += 1;
        }
    }

    // -- Separator --
    {
        let line = Line::from(Span::styled(
            "\u{2500}".repeat(iw),
            Style::default().fg(t.border_inactive),
        ));
        f.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, inner.y + row, inner.width, 1),
        );
        row += 1;
    }

    // -- Hint line --
    {
        let valid = input.len() >= 3 && parsed_mode.is_some();
        let enter_style = if valid {
            Style::default().fg(accent)
        } else {
            Style::default().fg(t.fg_dim)
        };
        let hint_line = Line::from(vec![
            Span::styled(" \u{23ce}", enter_style),
            Span::styled(" apply  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(accent)),
            Span::styled(" cancel  ", Style::default().fg(t.fg_dim)),
            Span::styled("0-7", Style::default().fg(accent)),
            Span::styled(" octal", Style::default().fg(t.fg_dim)),
        ]);
        f.render_widget(
            Paragraph::new(hint_line),
            Rect::new(inner.x, inner.y + row, inner.width, 1),
        );
    }
}
