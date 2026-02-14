use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::App;
use crate::panel::SortMode;
use crate::theme::Theme;

use super::util::centered_rect;

pub(super) fn render_which_key(
    f: &mut Frame,
    hints: &[(&str, &str)],
    leader: char,
    t: &Theme,
    area: Rect,
) {
    let (leader_icon, leader_label) = match leader {
        ' ' => ("󱁐 ", "Space"),
        's' => ("󰒓 ", "Sort"),
        'g' => (" ", "Go"),
        'y' => ("󰆏 ", "Yank"),
        _ => return,
    };

    // Column layout: each entry is "key  description" padded to col_width
    let col_width = 18usize;
    let usable_width = area.width.saturating_sub(2) as usize; // -2 for borders
    let num_cols = (usable_width / col_width).max(1);
    let num_rows = (hints.len() + num_cols - 1) / num_cols;

    // Popup dimensions: rows + 2 border + 2 (separator + hint)
    let popup_h = (num_rows as u16 + 4).min(area.height);
    let popup_w = area.width.min((num_cols * col_width + 2) as u16).max(20);
    let popup_x = (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + area.height.saturating_sub(popup_h + 1); // above status bar

    let popup = Rect::new(area.x + popup_x, popup_y, popup_w, popup_h);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.orange))
        .title(format!(" {leader_icon}{leader_label} "))
        .title_style(Style::default().fg(t.orange));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;

    // Render hints in column-major order (fill columns top-to-bottom, then left-to-right)
    let mut lines: Vec<ListItem> = Vec::new();
    for row in 0..num_rows {
        let mut spans: Vec<Span> = Vec::new();
        for col in 0..num_cols {
            let idx = col * num_rows + row;
            if idx < hints.len() {
                let (key, desc) = hints[idx];
                // Key badge
                spans.push(Span::styled(
                    format!(" {key} "),
                    Style::default().fg(t.bg).bg(t.orange),
                ));
                // Description + padding to fill column
                let desc_text = format!(" {desc}");
                let entry_chars = key.chars().count() + 2 + desc_text.chars().count();
                let pad = col_width.saturating_sub(entry_chars);
                spans.push(Span::styled(
                    desc_text,
                    Style::default().fg(t.fg).bg(t.bg_light),
                ));
                spans.push(Span::styled(
                    " ".repeat(pad),
                    Style::default().bg(t.bg_light),
                ));
            } else {
                // Empty cell
                spans.push(Span::styled(
                    " ".repeat(col_width),
                    Style::default().bg(t.bg_light),
                ));
            }
        }
        // Fill remaining width
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        if used < iw {
            spans.push(Span::styled(
                " ".repeat(iw - used),
                Style::default().bg(t.bg_light),
            ));
        }
        lines.push(ListItem::new(Line::from(spans)));
    }

    let list_height = inner.height.saturating_sub(2) as usize;
    lines.truncate(list_height);
    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(lines), list_area);

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
        Span::styled(" esc", Style::default().fg(t.orange)),
        Span::styled(" cancel", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}

pub(super) fn render_sort(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let panel = app.tab().active_panel();
    let w = 32u16.min(area.width);
    let h = 12u16.min(area.height);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);
    f.render_widget(Clear, popup);

    let dir_arrow = if panel.sort_reverse { "\u{2191}" } else { "\u{2193}" };
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

pub(super) fn render_help(f: &mut Frame, t: &Theme, area: Rect) {
    let popup = centered_rect(60, 80, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(" 󰋖 Help \u{2014} press any key to close ")
        .title_style(Style::default().fg(t.cyan));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let help: &[(&str, &[(&str, &str)])] = &[
        (
            " Navigation",
            &[
                ("j/k", "Move down/up"),
                ("h/l", "Parent / Enter dir"),
                ("gg/G", "Top / Bottom"),
                ("Ctrl-d/u", "Half page down/up"),
                ("Ctrl-l/h", "Focus right/left"),
                ("Tab", "Switch panel"),
                ("~", "Go home"),
            ],
        ),
        (
            " Files",
            &[
                ("yy", "Yank (copy)"),
                ("dd", "Delete (trash)"),
                ("p / P", "Paste / Paste to other"),
                ("yp", "Copy path to clipboard"),
                ("u", "Undo"),
            ],
        ),
        (
            " Modes",
            &[
                ("v", "Visual select"),
                ("/", "Search"),
                (":", "Command mode"),
                ("Space+?", "This help"),
            ],
        ),
        (
            "󱁐 Space leader",
            &[
                ("Space+p", "Toggle preview"),
                ("Space+t", "Toggle tree sidebar"),
                ("Space+h", "Toggle hidden files"),
                ("Space+d", "Directory sizes"),
                ("Space+s", "Sort popup"),
                ("Space+,/.", "Find local/global"),
            ],
        ),
        (
            "󰒓 Sort",
            &[
                ("sn/ss", "Sort by name/size"),
                ("sm/sc", "Sort by modified/created"),
                ("se", "Sort by extension"),
                ("sr", "Reverse sort"),
                ("J/K", "Scroll preview"),
            ],
        ),
        (
            " Tabs",
            &[
                ("gt / gT", "Next / Prev tab"),
                (":tabnew", "New tab"),
                (":tabclose", "Close tab"),
            ],
        ),
        (
            " Other",
            &[
                ("m{a-z}", "Set mark"),
                ("'{a-z}", "Go to mark"),
                ("T", "Cycle theme"),
                ("Ctrl-r", "Refresh"),
            ],
        ),
    ];

    let mut lines: Vec<ListItem> = Vec::new();
    for (section, keys) in help {
        if !lines.is_empty() {
            lines.push(ListItem::new(Line::from("")));
        }
        lines.push(ListItem::new(Line::from(Span::styled(
            format!(" {section}"),
            Style::default()
                .fg(t.cyan),
        ))));
        for (key, desc) in *keys {
            let line = Line::from(vec![
                Span::styled(
                    format!("   {key:<16}"),
                    Style::default().fg(t.yellow),
                ),
                Span::styled(*desc, Style::default().fg(t.fg)),
            ]);
            lines.push(ListItem::new(line));
        }
    }

    let visible = inner.height as usize;
    let items: Vec<ListItem> = lines.into_iter().take(visible).collect();
    f.render_widget(List::new(items), inner);
}

