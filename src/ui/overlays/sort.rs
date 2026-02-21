use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::panel::SortMode;

pub(in crate::ui) fn render_sort(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let panel = app.tab().active_panel();
    let w = 32u16.min(area.width);
    let h = 12u16.min(area.height);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);
    f.render_widget(Clear, popup);

    let dir_arrow = if panel.sort_reverse {
        "\u{2191}"
    } else {
        "\u{2193}"
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(format!(" 󰒓 Sort {dir_arrow} "))
        .title_style(Style::default().fg(t.cyan));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let icons = [" 󰈔 ", " 󰪶 ", " 󰃰 ", " 󰃰 ", " 󰈔 "];
    let labels = ["Name", "Size", "Modified", "Created", "Extension"];
    let keys = ["n", "s", "m", "c", "e"];
    let iw = inner.width as usize;
    let mut items: Vec<ListItem> = Vec::new();

    for (i, ((&mode, label), (icon, key))) in SortMode::ALL
        .iter()
        .zip(labels.iter())
        .zip(icons.iter().zip(keys.iter()))
        .enumerate()
    {
        let is_current = mode == panel.sort_mode;
        let is_cursor = i == app.sort_cursor;

        // Build the right-side indicator: arrow for active, key hint otherwise
        let right_text = if is_current {
            format!(" {dir_arrow} ")
        } else {
            format!(" {key} ")
        };

        // Marker column
        let marker = if is_current && is_cursor {
            "\u{25b8} "
        } else if is_current {
            "  "
        } else if is_cursor {
            "\u{25b8} "
        } else {
            "  "
        };

        let label_width = iw.saturating_sub(
            marker.chars().count() + icon.chars().count() + right_text.chars().count(),
        );
        let label_pad = label_width.saturating_sub(label.len());
        let label_col = format!("{label}{}", " ".repeat(label_pad));

        if is_cursor {
            let cursor_style = Style::default().fg(t.bg).bg(t.blue);
            let line = Line::from(vec![
                Span::styled(marker, cursor_style),
                Span::styled(*icon, cursor_style),
                Span::styled(label_col, cursor_style),
                Span::styled(right_text, cursor_style),
            ]);
            items.push(ListItem::new(line));
        } else if is_current {
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(t.green)),
                Span::styled(*icon, Style::default().fg(t.green)),
                Span::styled(label_col, Style::default().fg(t.green)),
                Span::styled(right_text, Style::default().fg(t.cyan)),
            ]);
            items.push(ListItem::new(line));
        } else {
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(*icon, Style::default().fg(t.fg_dim)),
                Span::styled(label_col, Style::default().fg(t.fg)),
                Span::styled(right_text, Style::default().fg(t.fg_dim)),
            ]);
            items.push(ListItem::new(line));
        }
    }

    // Separator
    let sep_y = inner.y + items.len().min(inner.height.saturating_sub(2) as usize) as u16;
    let sep_area = Rect::new(inner.x, sep_y, inner.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(iw),
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );

    // Hint line
    let rev_label = if panel.sort_reverse { "asc" } else { "desc" };
    let hint_line = Line::from(vec![
        Span::styled(" r", Style::default().fg(t.cyan)),
        Span::styled(format!(" {rev_label}  "), Style::default().fg(t.fg_dim)),
        Span::styled("\u{23ce}", Style::default().fg(t.cyan)),
        Span::styled(" apply  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(t.cyan)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);

    let list_height = inner.height.saturating_sub(2) as usize;
    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    items.truncate(list_height);
    f.render_widget(List::new(items), list_area);

    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
