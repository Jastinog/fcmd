use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::search::{FindScope, FindState};
use crate::theme::Theme;
use crate::util::icons::file_icon;

use super::util::{centered_rect, display_width, truncate_to_width, truncate_to_width_left};

/// Number of result rows shown in the find popup for a given screen area.
/// Shared with the scroll-clamp logic in `ui::render` so the clamp height and the
/// rendered height never disagree (which would let the selection scroll off-screen).
pub(in crate::ui) fn find_results_height(area: Rect) -> usize {
    let popup = centered_rect(80, 75, area);
    let inner_h = popup.height.saturating_sub(2) as usize; // block borders
    inner_h.saturating_sub(4) // input + separator + hint rows (+1 margin)
}

pub(super) fn render_find(f: &mut Frame, fs: &FindState, t: &Theme, area: Rect) {
    let popup = centered_rect(80, 75, area);
    f.render_widget(Clear, popup);

    let scope_label = match fs.scope {
        FindScope::Local => "󰉋 Local",
        FindScope::Global => "󰖟 Global",
    };
    let scope_color = match fs.scope {
        FindScope::Local => t.cyan,
        FindScope::Global => t.yellow,
    };
    let status_part = if fs.loading {
        let spinner = fs.spinner();
        let elapsed = fs.elapsed_str();
        format!("{spinner} {elapsed}")
    } else if fs.total_count() > 0 {
        "\u{2714}".to_string()
    } else {
        String::new()
    };
    let title = format!(
        "  Find [{scope_label}] ({}/{}) {status_part} ",
        fs.filtered_count(),
        fs.total_count()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(scope_color))
        .title(title)
        .title_style(Style::default().fg(scope_color))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    // Split inner into left (results) and right (preview) columns
    let left_w = inner.width / 2;
    let right_w = inner.width - left_w - 1; // -1 for vertical separator
    let left_x = inner.x;
    let sep_x = inner.x + left_w;
    let right_x = sep_x + 1;

    // === LEFT SIDE: input + separator + results + hint ===

    // Input line (cursor stays at the end; text scrolls to show the tail)
    let input_area = Rect::new(left_x, inner.y, left_w, 1);
    let input_line =
        super::overlays::input_field_line(&fs.query, " \u{276f} ", left_w as usize, scope_color, t);
    f.render_widget(Paragraph::new(input_line), input_area);

    // Separator
    let sep_area = Rect::new(left_x, inner.y + 1, left_w, 1);
    let sep = Paragraph::new(Line::from(Span::styled(
        "\u{2500}".repeat(left_w as usize),
        Style::default().fg(t.border_inactive),
    )));
    f.render_widget(sep, sep_area);

    // Results (leave 1 row at bottom for hint). Use the shared helper so this matches
    // the scroll clamp in ui::render exactly.
    let results_height = find_results_height(area);
    let results_area = Rect::new(left_x, inner.y + 2, left_w, results_height as u16);

    let lwidth = results_area.width as usize;

    // Placeholders for global search
    if fs.scope == FindScope::Global && fs.query.is_empty() {
        let placeholder = Paragraph::new(Line::from(Span::styled(
            "  󰍉 Type to search...",
            Style::default().fg(t.fg_dim),
        )));
        f.render_widget(placeholder, results_area);
    } else if fs.loading && fs.filtered_count() == 0 {
        let spinner = fs.spinner();
        let placeholder = Paragraph::new(Line::from(Span::styled(
            format!("  {spinner} Searching..."),
            Style::default().fg(t.fg_dim),
        )));
        f.render_widget(placeholder, results_area);
    } else {
        let items: Vec<ListItem> = (fs.scroll..fs.scroll + results_height)
            .filter_map(|idx| {
                let (rel_path, is_dir) = fs.get_item(idx)?;
                let is_selected = idx == fs.selected;

                let icon = file_icon(rel_path.rsplit('/').next().unwrap_or(rel_path), is_dir);

                let display = if is_dir {
                    format!("{rel_path}/")
                } else {
                    rel_path.to_string()
                };

                // Keep the tail (filename) when a path is too long, by display width.
                let max_display = lwidth.saturating_sub(4 + display_width(icon));
                let truncated = truncate_to_width_left(&display, max_display);

                let prefix = if is_selected { "> " } else { "  " };

                let sel_bg = match fs.scope {
                    FindScope::Local => t.cyan,
                    FindScope::Global => t.yellow,
                };

                let style = if is_selected {
                    Style::default().fg(t.bg_text).bg(sel_bg)
                } else if is_dir {
                    Style::default().fg(t.dir_color)
                } else {
                    Style::default().fg(t.fg)
                };

                let icon_style = if is_selected {
                    style
                } else {
                    Style::default().fg(t.fg_dim)
                };

                let line = Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(icon, icon_style),
                    Span::styled(truncated, style),
                ]);

                Some(ListItem::new(line))
            })
            .collect();

        f.render_widget(List::new(items), results_area);
    }

    // Hint line at the bottom of left side
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(left_x, hint_y, left_w, 1);
    let mut hint_spans = vec![
        Span::styled(" \u{23ce}", Style::default().fg(scope_color)),
        Span::styled(" open  ", Style::default().fg(t.fg_dim)),
        Span::styled("tab", Style::default().fg(scope_color)),
        Span::styled(" scope  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(scope_color)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ];
    // Show search progress on the right side of the hint bar
    if fs.scope == FindScope::Global && !fs.query.is_empty() {
        let used: usize = hint_spans.iter().map(|s| s.content.chars().count()).sum();
        let elapsed = fs.elapsed_str();
        let status_text = if fs.loading {
            format!(" {} {} found", fs.spinner(), fs.total_count())
        } else {
            format!(" \u{2714} {} found ({})", fs.total_count(), elapsed)
        };
        let pad = (left_w as usize).saturating_sub(used + status_text.chars().count());
        if pad > 0 {
            hint_spans.push(Span::styled(" ".repeat(pad), Style::default()));
        }
        let status_color = if fs.loading { t.yellow } else { t.green };
        hint_spans.push(Span::styled(status_text, Style::default().fg(status_color)));
    }
    f.render_widget(Paragraph::new(Line::from(hint_spans)), hint_area);

    // === VERTICAL SEPARATOR ===
    for row in 0..inner.height {
        let sep_rect = Rect::new(sep_x, inner.y + row, 1, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2502}",
                Style::default().fg(t.border_inactive),
            ))),
            sep_rect,
        );
    }

    // === RIGHT SIDE: preview ===
    match &fs.find_preview {
        Some(p) => {
            // Preview title
            let preview_title = format!(" 󰈈 {} [{}] ", p.title, p.info);
            let title_display = truncate_to_width(&preview_title, right_w as usize);
            let title_area = Rect::new(right_x, inner.y, right_w, 1);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    title_display,
                    Style::default().fg(scope_color),
                ))),
                title_area,
            );

            // Preview content
            let content_height = inner.height.saturating_sub(1) as usize;
            let content_area = Rect::new(right_x, inner.y + 1, right_w, content_height as u16);
            let rwidth = right_w as usize;

            let items: Vec<ListItem> = if p.is_binary {
                super::hex::render_rows(
                    p,
                    p.scroll,
                    content_height,
                    crate::preview::HEX_COLS,
                    t,
                    None,
                    &[],
                )
            } else {
                (0..content_height)
                    .filter_map(|i| {
                        let line_idx = i + p.scroll;
                        if line_idx >= p.lines.len() {
                            return None;
                        }
                        let line_num = line_idx + 1;
                        let num_width = 4;
                        let max_content = rwidth.saturating_sub(num_width + 2);
                        let mut spans = vec![Span::styled(
                            format!("{line_num:>num_width$}\u{2502}", num_width = num_width),
                            Style::default().fg(t.fg_dim),
                        )];
                        let line = &p.lines[line_idx];
                        let content = if line.width() > max_content {
                            super::preview::truncate_to_width(line, max_content)
                        } else {
                            line.clone()
                        };
                        spans.push(Span::styled(content, Style::default().fg(t.fg)));
                        Some(ListItem::new(Line::from(spans)))
                    })
                    .collect()
            };

            f.render_widget(List::new(items), content_area);
        }
        None => {
            // No preview placeholder
            let center_y = inner.y + inner.height / 2;
            let placeholder_area = Rect::new(right_x, center_y, right_w, 1);
            let text = "󰈈 No preview";
            let pad = (right_w as usize).saturating_sub(text.chars().count()) / 2;
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{}{text}", " ".repeat(pad)),
                    Style::default().fg(t.fg_dim),
                ))),
                placeholder_area,
            );
        }
    }
}
