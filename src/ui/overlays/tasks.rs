use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::app::task_manager::{TaskManager, TaskState};
use crate::ui::SPINNER;
use crate::ui::util::{display_width, fit_truncated};

pub(in crate::ui) fn render_tasks_overlay(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.cyan;
    let tasks = app.task_manager.tasks();
    let len = tasks.len();
    if len == 0 {
        return;
    }

    let popup = crate::ui::util::centered_rect(70, 70, area);
    f.render_widget(Clear, popup);

    let active = app.task_manager.active_count();
    let title = format!(" \u{f0ae} Tasks ({active} running / {len} total) ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 3 || inner.width < 12 {
        return;
    }

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;
    let max_scroll = len.saturating_sub(list_height.max(1));
    let scroll = app.tasks_scroll.min(max_scroll);

    let mut items: Vec<ListItem> = Vec::new();
    for (i, task) in tasks.iter().enumerate().skip(scroll).take(list_height) {
        let is_cursor = i == app.tasks_cursor;
        let marker = if is_cursor { "\u{25b8} " } else { "  " };
        let label = format!("{:<6} ", TaskManager::kind_label(task));

        // State glyph + body text + colour depend on Running vs Finished.
        let (glyph, glyph_color, body, body_color) = match &task.state {
            TaskState::Running { progress_pct, status_text } => {
                let spinner = SPINNER[(app.tick_count % 4) as usize];
                let bar = progress_bar(*progress_pct, 10);
                (
                    spinner.to_string(),
                    accent,
                    format!("{bar} {progress_pct:>3}%  {status_text}"),
                    t.fg,
                )
            }
            TaskState::Finished { success, cancelled, summary } => {
                if *cancelled {
                    ("\u{2298}".to_string(), t.yellow, summary.clone(), t.yellow)
                } else if *success {
                    ("\u{2713}".to_string(), t.green, summary.clone(), t.fg_dim)
                } else {
                    ("\u{2717}".to_string(), t.red, summary.clone(), t.red)
                }
            }
        };

        let glyph_col = format!("{glyph} ");
        let prefix_w = display_width(marker) + display_width(&glyph_col) + display_width(&label);
        let (body_display, pad) = fit_truncated(&body, iw, prefix_w);

        let (marker_c, label_c) = if is_cursor {
            (accent, accent)
        } else {
            (t.fg_dim, t.fg_dim)
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(marker, Style::default().fg(marker_c)),
            Span::styled(glyph_col, Style::default().fg(glyph_color)),
            Span::styled(label, Style::default().fg(label_c)),
            Span::styled(body_display, Style::default().fg(body_color)),
            Span::styled(" ".repeat(pad), Style::default()),
        ])));
    }

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

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
        Span::styled(" x/d", Style::default().fg(accent)),
        Span::styled(" cancel  ", Style::default().fg(t.fg_dim)),
        Span::styled("c", Style::default().fg(accent)),
        Span::styled(" clear done  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}

/// A fixed-width `[####----]` progress bar for the given percentage.
fn progress_bar(pct: u8, width: usize) -> String {
    let filled = (pct as usize * width / 100).min(width);
    let empty = width - filled;
    format!(
        "[{}{}]",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_bounds() {
        assert_eq!(display_width(&progress_bar(0, 10)), 12); // 10 + 2 brackets
        assert_eq!(display_width(&progress_bar(100, 10)), 12);
        assert_eq!(display_width(&progress_bar(255, 10)), 12); // clamps, no overflow
    }

    #[test]
    fn progress_bar_fills_proportionally() {
        let bar = progress_bar(50, 10);
        assert_eq!(bar.matches('\u{2588}').count(), 5);
        assert_eq!(bar.matches('\u{2591}').count(), 5);
    }
}
