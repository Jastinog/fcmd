use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::{App, Mode};
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
        'd' => ("󰗨 ", "Delete"),
        '\'' => (" ", "Mark"),
        _ => return,
    };

    // Column layout: each entry is "key  description" padded to col_width
    let col_width = 18usize;
    let usable_width = area.width.saturating_sub(2) as usize; // -2 for borders
    let num_cols = (usable_width / col_width).max(1);
    let num_rows = hints.len().div_ceil(num_cols);

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

pub(super) fn render_help(f: &mut Frame, t: &Theme, area: Rect) {
    let popup = centered_rect(60, 80, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(" 󰋖 Help ")
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
                ("r", "Rename"),
                ("a", "Create file/dir (/ suffix)"),
                ("u", "Undo"),
            ],
        ),
        (
            " Modes",
            &[
                ("v", "Visual select"),
                ("/", "Search"),
                ("f / F", "Find local / global"),
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
                ("Ctrl+t", "New tab"),
                ("Ctrl+w", "Close tab"),
            ],
        ),
        (
            " Other",
            &[
                ("m{a-z}", "Set mark"),
                ("'{a-z}", "Go to mark"),
                ("T", "Theme picker"),
                ("Ctrl-r", "Refresh"),
            ],
        ),
    ];

    let iw = inner.width as usize;
    let mut lines: Vec<ListItem> = Vec::new();
    for (section, keys) in help {
        if !lines.is_empty() {
            lines.push(ListItem::new(Line::from("")));
        }
        lines.push(ListItem::new(Line::from(Span::styled(
            format!(" {section}"),
            Style::default().fg(t.cyan),
        ))));
        for (key, desc) in *keys {
            let line = Line::from(vec![
                Span::styled(format!("   {key:<16}"), Style::default().fg(t.yellow)),
                Span::styled(*desc, Style::default().fg(t.fg)),
            ]);
            lines.push(ListItem::new(line));
        }
    }

    let list_height = inner.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = lines.into_iter().take(list_height).collect();
    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

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
        Span::styled(" esc", Style::default().fg(t.cyan)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}

pub(super) fn render_input_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;

    let is_rename = app.mode == Mode::Rename;

    let (title, accent) = if is_rename {
        (" 󰑕 Rename ", t.yellow)
    } else {
        (" 󰝒 New ", t.cyan)
    };

    // Context line for rename: show original name
    let context: Option<String> = if is_rename {
        app.tab()
            .active_panel()
            .selected_entry()
            .filter(|e| e.name != "..")
            .map(|e| e.name.clone())
    } else {
        None
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
        .title_style(Style::default().fg(accent));

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
    let field_w = iw.saturating_sub(prefix_len);

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
    let hint_line = if is_rename {
        Line::from(vec![
            Span::styled(" \u{23ce}", Style::default().fg(accent)),
            Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(accent)),
            Span::styled(" cancel", Style::default().fg(t.fg_dim)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" \u{23ce}", Style::default().fg(accent)),
            Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(accent)),
            Span::styled(" cancel  ", Style::default().fg(t.fg_dim)),
            Span::styled("name/", Style::default().fg(accent)),
            Span::styled(" = dir", Style::default().fg(t.fg_dim)),
        ])
    };
    let hint_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}

pub(super) fn render_confirm_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.red;
    let paths = &app.confirm_paths;
    let n = paths.len();

    // Height: border(2) + file list (capped) + separator(1) + hint(1)
    let max_list = 12usize;
    let list_h = n.min(max_list);
    let h = (list_h as u16 + 4).min(area.height);
    let w = 50u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let title = format!(" 󰗨 Delete ({n}) ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;

    // Adjust scroll so it stays in range
    let scroll = app.confirm_scroll.min(n.saturating_sub(list_height.max(1)));

    let mut items: Vec<ListItem> = Vec::new();
    for (i, path) in paths.iter().enumerate().skip(scroll).take(list_height) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_dir = path.is_dir();
        let icon = if is_dir { " " } else { " 󰈔 " };
        let icon_color = if is_dir { t.blue } else { t.fg_dim };

        let max_name = iw.saturating_sub(icon.chars().count());
        let name_display = if name.chars().count() > max_name {
            let truncated: String = name.chars().take(max_name.saturating_sub(1)).collect();
            format!("{truncated}\u{2026}")
        } else {
            name
        };
        let pad = iw.saturating_sub(icon.chars().count() + name_display.chars().count());

        let bg = if i == scroll && n > list_height {
            // No special highlight needed, just show the list
            t.bg_light
        } else {
            t.bg_light
        };

        let line = Line::from(vec![
            Span::styled(icon, Style::default().fg(icon_color).bg(bg)),
            Span::styled(name_display, Style::default().fg(t.fg).bg(bg)),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        ]);
        items.push(ListItem::new(line));
    }

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

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
        Span::styled(" y", Style::default().fg(accent)),
        Span::styled(" confirm  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" cancel", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}

pub(super) fn render_preview_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let Some(ref p) = app.file_preview else {
        return;
    };

    let popup = centered_rect(75, 80, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(format!(" 󰈈 {} [{}] ", p.title, p.info))
        .title_style(Style::default().fg(t.cyan));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;

    let items: Vec<ListItem> = p
        .lines
        .iter()
        .skip(p.scroll)
        .take(list_height)
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + p.scroll + 1;
            let num_width = 4;
            let max_content = iw.saturating_sub(num_width + 2);
            let content: String = if line.chars().count() > max_content {
                line.chars().take(max_content).collect()
            } else {
                line.clone()
            };
            Line::from(vec![
                Span::styled(
                    format!("{line_num:>num_width$} ", num_width = num_width),
                    Style::default().fg(t.fg_dim),
                ),
                Span::styled(content, Style::default().fg(t.fg)),
            ])
        })
        .map(ListItem::new)
        .collect();

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

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
        Span::styled(" j/k", Style::default().fg(t.cyan)),
        Span::styled(" scroll  ", Style::default().fg(t.fg_dim)),
        Span::styled("G/g", Style::default().fg(t.cyan)),
        Span::styled(" top/bottom  ", Style::default().fg(t.fg_dim)),
        Span::styled("o", Style::default().fg(t.cyan)),
        Span::styled(" edit  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(t.cyan)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}

pub(super) fn render_theme_picker(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let list = &app.theme_list;
    let len = list.len();
    if len == 0 {
        return;
    }

    let popup = centered_rect(80, 75, area);
    f.render_widget(Clear, popup);

    // Split into left (40%) and right (60%) panes
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(popup);

    // --- Left pane: theme list ---
    let left_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(" \u{f0394} Themes ")
        .title_style(Style::default().fg(t.cyan));

    let left_inner = left_block.inner(panes[0]);
    f.render_widget(left_block, panes[0]);

    if left_inner.height >= 3 && left_inner.width >= 8 {
        let liw = left_inner.width as usize;
        let list_height = left_inner.height.saturating_sub(2) as usize;
        let cursor = app.theme_cursor;
        let scroll = app.theme_scroll;
        let active_index = app.theme_index;

        let mut items: Vec<ListItem> = Vec::new();
        for (i, name) in list
            .iter()
            .enumerate()
            .take(len.min(scroll + list_height))
            .skip(scroll)
        {
            let is_cursor = i == cursor;
            let is_active = active_index == Some(i);

            let marker = if is_cursor { "\u{25b8} " } else { "  " };
            let max_name = liw.saturating_sub(marker.chars().count());
            let name_display = if name.chars().count() > max_name {
                let truncated: String = name.chars().take(max_name.saturating_sub(1)).collect();
                format!("{truncated}\u{2026}")
            } else {
                name.clone()
            };
            let pad = liw.saturating_sub(marker.chars().count() + name_display.chars().count());

            if is_cursor {
                let cursor_style = Style::default().fg(t.bg).bg(t.blue);
                let line = Line::from(vec![
                    Span::styled(marker, cursor_style),
                    Span::styled(name_display, cursor_style),
                    Span::styled(" ".repeat(pad), cursor_style),
                ]);
                items.push(ListItem::new(line));
            } else if is_active {
                let line = Line::from(vec![
                    Span::styled(marker, Style::default().fg(t.green)),
                    Span::styled(name_display, Style::default().fg(t.green)),
                    Span::styled(" ".repeat(pad), Style::default()),
                ]);
                items.push(ListItem::new(line));
            } else {
                let line = Line::from(vec![
                    Span::styled(marker, Style::default().fg(t.fg_dim)),
                    Span::styled(name_display, Style::default().fg(t.fg)),
                    Span::styled(" ".repeat(pad), Style::default()),
                ]);
                items.push(ListItem::new(line));
            }
        }

        let list_area = Rect::new(
            left_inner.x,
            left_inner.y,
            left_inner.width,
            list_height as u16,
        );
        f.render_widget(List::new(items), list_area);

        // Separator
        let sep_y = left_inner.y + list_height as u16;
        let sep_area = Rect::new(left_inner.x, sep_y, left_inner.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2500}".repeat(liw),
                Style::default().fg(t.border_inactive),
            ))),
            sep_area,
        );

        // Hint line
        let hint_line = Line::from(vec![
            Span::styled(" \u{23ce}", Style::default().fg(t.cyan)),
            Span::styled(" apply  ", Style::default().fg(t.fg_dim)),
            Span::styled("esc", Style::default().fg(t.cyan)),
            Span::styled(" cancel", Style::default().fg(t.fg_dim)),
        ]);
        let hint_y = left_inner.y + left_inner.height.saturating_sub(1);
        let hint_area = Rect::new(left_inner.x, hint_y, left_inner.width, 1);
        f.render_widget(Paragraph::new(hint_line), hint_area);
    }

    // --- Right pane: mock preview ---
    if let Some(ref pt) = app.theme_preview {
        render_preview_panel(f, pt, panes[1]);
    }
}

fn render_preview_panel(f: &mut Frame, pt: &Theme, area: Rect) {
    use crate::icons::file_icon;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pt.border_active))
        .title(" Preview ")
        .title_style(Style::default().fg(pt.border_active));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 12 {
        return;
    }

    struct MockEntry {
        name: &'static str,
        is_dir: bool,
        meta: &'static str,
        date: &'static str,
    }

    let entries: &[MockEntry] = &[
        MockEntry {
            name: "Documents",
            is_dir: true,
            meta: "<DIR>",
            date: "Feb 14",
        },
        MockEntry {
            name: "Downloads",
            is_dir: true,
            meta: "<DIR>",
            date: "Feb 13",
        },
        MockEntry {
            name: "Projects",
            is_dir: true,
            meta: "<DIR>",
            date: "Feb 10",
        },
        MockEntry {
            name: ".config",
            is_dir: true,
            meta: "<DIR>",
            date: "Jan 28",
        },
        MockEntry {
            name: "readme.md",
            is_dir: false,
            meta: "1.2K",
            date: "Feb 14",
        },
        MockEntry {
            name: "setup.sh",
            is_dir: false,
            meta: "840",
            date: "Feb 12",
        },
        MockEntry {
            name: "photo.png",
            is_dir: false,
            meta: "2.4M",
            date: "Feb 01",
        },
        MockEntry {
            name: "notes.txt",
            is_dir: false,
            meta: "512",
            date: "Jan 15",
        },
    ];

    let cursor_row = 2usize; // "Projects/" row
    let iw = inner.width as usize;

    for (i, entry) in entries.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }

        let is_cursor = i == cursor_row;
        let icon = file_icon(entry.name, entry.is_dir);
        let display_name = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.to_string()
        };

        let name_color = if entry.is_dir {
            pt.dir_color
        } else {
            pt.file_color
        };
        let cursor_bg = if is_cursor {
            Some(pt.cursor_line)
        } else {
            None
        };

        // Layout: " icon name     meta   date "
        let meta_date = format!("{}  {}", entry.meta, entry.date);
        let prefix = format!(" {icon}");
        let prefix_w = prefix.chars().count();
        let meta_w = meta_date.chars().count() + 1; // +1 for trailing space
        let name_w = iw.saturating_sub(prefix_w + meta_w);
        let name_display: String = if display_name.chars().count() > name_w {
            display_name
                .chars()
                .take(name_w.saturating_sub(1))
                .chain(std::iter::once('\u{2026}'))
                .collect()
        } else {
            let pad = name_w.saturating_sub(display_name.chars().count());
            format!("{display_name}{}", " ".repeat(pad))
        };

        let mut name_style = Style::default().fg(name_color);
        let mut meta_style = Style::default().fg(pt.fg_dim);
        let mut pad_style = Style::default();
        if let Some(bg) = cursor_bg {
            name_style = name_style.bg(bg);
            meta_style = meta_style.bg(bg);
            pad_style = pad_style.bg(bg);
        }

        let line = Line::from(vec![
            Span::styled(&prefix, name_style),
            Span::styled(name_display, name_style),
            Span::styled(meta_date, meta_style),
            Span::styled(" ", pad_style),
        ]);

        let row_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        f.render_widget(Paragraph::new(line), row_area);
    }
}

pub(super) fn render_search_popup(f: &mut Frame, app: &App, area: Rect) {
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
    let field_w = iw.saturating_sub(prefix_len);

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
