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
        'c' => ("󰌑 ", "Change"),
        '\'' => (" ", "Mark"),
        _ => return,
    };

    // Parse hints into groups: entries with empty key are section headers
    let mut groups: Vec<Vec<(&str, &str)>> = Vec::new();
    for &(key, desc) in hints {
        if key.is_empty() {
            groups.push(Vec::new());
        } else {
            if groups.is_empty() {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push((key, desc));
        }
    }
    // Remove empty groups
    groups.retain(|g| !g.is_empty());
    let has_sections = groups.len() > 1;

    // Layout
    let col_width = 18usize;
    let usable_width = area.width.saturating_sub(2) as usize;
    let num_cols = (usable_width / col_width).max(1);

    // Calculate total content rows (items + dashed separators between groups)
    let total_rows: usize = if has_sections {
        let item_rows: usize = groups
            .iter()
            .map(|items| items.len().div_ceil(num_cols))
            .sum();
        item_rows + groups.len() - 1
    } else {
        let n: usize = groups.iter().map(|items| items.len()).sum();
        n.div_ceil(num_cols)
    };

    // Popup dimensions
    let popup_h = (total_rows as u16 + 4).min(area.height);
    let popup_w = area
        .width
        .min((num_cols * col_width + 2) as u16)
        .max(20);
    let popup_x = (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + area.height.saturating_sub(popup_h + 1);

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

    // Build render lines
    let mut lines: Vec<ListItem> = Vec::new();

    if has_sections {
        // Grouped layout with dashed separators between groups
        for (gi, items) in groups.iter().enumerate() {
            let group_rows = items.len().div_ceil(num_cols);
            for r in 0..group_rows {
                let mut spans: Vec<Span> = Vec::new();
                for c in 0..num_cols {
                    let idx = r * num_cols + c;
                    if idx < items.len() {
                        let (key, desc) = items[idx];
                        spans.push(Span::styled(
                            format!(" {key} "),
                            Style::default().fg(t.bg).bg(t.orange),
                        ));
                        let desc_text = format!(" {desc}");
                        let entry_chars = key.chars().count() + 2 + desc_text.chars().count();
                        let pad = col_width.saturating_sub(entry_chars);
                        spans.push(Span::styled(
                            desc_text,
                            Style::default().fg(t.fg),
                        ));
                        spans.push(Span::raw(" ".repeat(pad)));
                    } else {
                        spans.push(Span::raw(" ".repeat(col_width)));
                    }
                }
                let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                if used < iw {
                    spans.push(Span::raw(" ".repeat(iw - used)));
                }
                lines.push(ListItem::new(Line::from(spans)));
            }
            // Dashed separator between groups (not after last)
            if gi < groups.len() - 1 {
                lines.push(ListItem::new(Line::from(Span::styled(
                    "\u{254c}".repeat(iw),
                    Style::default().fg(t.border_inactive),
                ))));
            }
        }
    } else {
        // Flat column-major layout (for small leaders without sections)
        let all_items: Vec<(&str, &str)> =
            groups.into_iter().flatten().collect();
        let num_rows = all_items.len().div_ceil(num_cols);

        for row in 0..num_rows {
            let mut spans: Vec<Span> = Vec::new();
            for col in 0..num_cols {
                let idx = col * num_rows + row;
                if idx < all_items.len() {
                    let (key, desc) = all_items[idx];
                    spans.push(Span::styled(
                        format!(" {key} "),
                        Style::default().fg(t.bg).bg(t.orange),
                    ));
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
                    spans.push(Span::styled(
                        " ".repeat(col_width),
                        Style::default().bg(t.bg_light),
                    ));
                }
            }
            let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            if used < iw {
                spans.push(Span::styled(
                    " ".repeat(iw - used),
                    Style::default().bg(t.bg_light),
                ));
            }
            lines.push(ListItem::new(Line::from(spans)));
        }
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
                ("dd", "Move to trash"),
                ("dD", "Permanent delete"),
                ("p / P", "Paste / Paste to other"),
                ("yp", "Copy path to clipboard"),
                ("r", "Rename"),
                ("a", "Create file/dir (/ suffix)"),
                ("cp", "Permissions (chmod)"),
                ("co", "Owner (chown)"),
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
                ("b", "Add bookmark"),
                ("B", "Bookmarks"),
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

pub(super) fn render_confirm_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let permanent = app.confirm_permanent;
    let accent = if permanent { t.red } else { t.yellow };
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

    let title = if permanent {
        format!(" 󰗨 Permanently Delete ({n}) ")
    } else {
        format!("  Move to Trash ({n}) ")
    };
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

pub(super) fn render_bookmarks(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.yellow;
    let bm = &app.bookmarks;
    let len = bm.len();
    if len == 0 {
        return;
    }

    let max_list = 12usize;
    let list_h = len.min(max_list);
    let h = (list_h as u16 + 4).min(area.height);
    let w = 50u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let title = format!(" \u{f02e6} Bookmarks ({len}) ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;
    let scroll = app.bookmark_scroll.min(len.saturating_sub(list_height.max(1)));

    let home = dirs::home_dir().unwrap_or_default();

    let mut items: Vec<ListItem> = Vec::new();
    for (i, (name, path)) in bm.iter().enumerate().skip(scroll).take(list_height) {
        let is_cursor = i == app.bookmark_cursor;

        let marker = if is_cursor { "\u{25b8} " } else { "  " };

        // Shorten path with ~
        let path_str = path.to_string_lossy();
        let home_str = home.to_string_lossy();
        let short_path = if !home_str.is_empty() && path_str.starts_with(home_str.as_ref()) {
            format!("~{}", &path_str[home_str.len()..])
        } else {
            path_str.into_owned()
        };

        let marker_w = marker.chars().count();
        let name_col = format!("{name}  ");
        let name_w = name_col.chars().count();
        let path_max = iw.saturating_sub(marker_w + name_w);
        let path_display = if short_path.chars().count() > path_max {
            let start = short_path.chars().count() - path_max.saturating_sub(1);
            let tail: String = short_path.chars().skip(start).collect();
            format!("\u{2026}{tail}")
        } else {
            short_path.clone()
        };
        let pad = iw.saturating_sub(marker_w + name_w + path_display.chars().count());

        if is_cursor {
            let cursor_style = Style::default().fg(t.bg).bg(t.blue);
            let line = Line::from(vec![
                Span::styled(marker, cursor_style),
                Span::styled(name_col, cursor_style),
                Span::styled(path_display, cursor_style),
                Span::styled(" ".repeat(pad), cursor_style),
            ]);
            items.push(ListItem::new(line));
        } else {
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_col, Style::default().fg(accent)),
                Span::styled(path_display, Style::default().fg(t.fg_dim)),
                Span::styled(" ".repeat(pad), Style::default()),
            ]);
            items.push(ListItem::new(line));
        }
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
        Span::styled(" \u{23ce}", Style::default().fg(accent)),
        Span::styled(" go  ", Style::default().fg(t.fg_dim)),
        Span::styled("a", Style::default().fg(accent)),
        Span::styled(" add  ", Style::default().fg(t.fg_dim)),
        Span::styled("d", Style::default().fg(accent)),
        Span::styled(" del  ", Style::default().fg(t.fg_dim)),
        Span::styled("e", Style::default().fg(accent)),
        Span::styled(" rename  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
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

pub(super) fn render_chmod_popup(f: &mut Frame, app: &App, area: Rect) {
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

    // ── File context line ───────────────────────────────────────
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

    // ── Input field ─────────────────────────────────────────────
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

    // ── Thin separator ──────────────────────────────────────────
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

    // ── Permission breakdown: owner / group / other ─────────────
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

    // ── Separator ───────────────────────────────────────────────
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

    // ── Hint line ───────────────────────────────────────────────
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

pub(super) fn render_chown_picker(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let Some(ref picker) = app.chown_picker else {
        return;
    };

    let accent = t.cyan;

    // Popup size
    let w = 60u16.min(area.width.saturating_sub(4)).max(36);
    let h = 20u16.min(area.height.saturating_sub(2)).max(10);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);
    f.render_widget(Clear, popup);

    // Context info
    let n_paths = picker.paths.len();
    let title = if n_paths > 1 {
        format!("  Owner ({n_paths} items) ")
    } else {
        "  Owner ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 4 || inner.width < 12 {
        return;
    }

    let iw = inner.width as usize;
    let col_w = iw / 2;
    let list_height = inner.height.saturating_sub(2) as usize;

    // Current selection display at top is built into the columns via highlighting

    // --- Users column (left) ---
    let user_area = Rect::new(inner.x, inner.y, col_w as u16, list_height as u16);
    let is_user_active = picker.column == 0;

    let user_scroll = picker.user_scroll;

    let mut user_items: Vec<ListItem> = Vec::new();
    // Column header
    let header_style = if is_user_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(t.fg_dim)
    };
    let user_header_pad = col_w.saturating_sub(6);
    user_items.push(ListItem::new(Line::from(vec![
        Span::styled(" User", header_style),
        Span::styled(" ".repeat(user_header_pad.max(1)), Style::default()),
    ])));

    let user_list_h = list_height.saturating_sub(1); // minus header
    for (i, (name, uid)) in picker
        .users
        .iter()
        .enumerate()
        .skip(user_scroll)
        .take(user_list_h)
    {
        let is_cursor = i == picker.user_cursor;
        let is_current = picker.current_uid == Some(*uid);

        let marker = if is_cursor { "\u{25b8} " } else { "  " };
        let uid_str = format!("{uid}");
        let max_name = col_w.saturating_sub(marker.chars().count() + uid_str.len() + 2);
        let name_display = if name.chars().count() > max_name {
            let trunc: String = name.chars().take(max_name.saturating_sub(1)).collect();
            format!("{trunc}\u{2026}")
        } else {
            name.clone()
        };
        let pad =
            col_w.saturating_sub(marker.chars().count() + name_display.chars().count() + uid_str.len() + 1);

        if is_cursor && is_user_active {
            let s = Style::default().fg(t.bg).bg(t.blue);
            user_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, s),
                Span::styled(name_display, s),
                Span::styled(" ".repeat(pad), s),
                Span::styled(uid_str, s),
                Span::styled(" ", s),
            ])));
        } else if is_current {
            user_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.green)),
                Span::styled(name_display, Style::default().fg(t.green)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(uid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        } else {
            user_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_display, Style::default().fg(t.fg)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(uid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        }
    }

    user_items.truncate(list_height);
    f.render_widget(List::new(user_items), user_area);

    // --- Groups column (right) ---
    let group_x = inner.x + col_w as u16;
    let group_col_w = iw.saturating_sub(col_w);
    let group_area = Rect::new(group_x, inner.y, group_col_w as u16, list_height as u16);
    let is_group_active = picker.column == 1;

    let group_scroll = picker.group_scroll;

    let mut group_items: Vec<ListItem> = Vec::new();
    // Column header
    let header_style = if is_group_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(t.fg_dim)
    };
    let group_header_pad = group_col_w.saturating_sub(7);
    group_items.push(ListItem::new(Line::from(vec![
        Span::styled(" Group", header_style),
        Span::styled(" ".repeat(group_header_pad.max(1)), Style::default()),
    ])));

    let group_list_h = list_height.saturating_sub(1);
    for (i, (name, gid)) in picker
        .groups
        .iter()
        .enumerate()
        .skip(group_scroll)
        .take(group_list_h)
    {
        let is_cursor = i == picker.group_cursor;
        let is_current = picker.current_gid == Some(*gid);

        let marker = if is_cursor { "\u{25b8} " } else { "  " };
        let gid_str = format!("{gid}");
        let max_name = group_col_w.saturating_sub(marker.chars().count() + gid_str.len() + 2);
        let name_display = if name.chars().count() > max_name {
            let trunc: String = name.chars().take(max_name.saturating_sub(1)).collect();
            format!("{trunc}\u{2026}")
        } else {
            name.clone()
        };
        let pad = group_col_w
            .saturating_sub(marker.chars().count() + name_display.chars().count() + gid_str.len() + 1);

        if is_cursor && is_group_active {
            let s = Style::default().fg(t.bg).bg(t.blue);
            group_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, s),
                Span::styled(name_display, s),
                Span::styled(" ".repeat(pad), s),
                Span::styled(gid_str, s),
                Span::styled(" ", s),
            ])));
        } else if is_current {
            group_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.green)),
                Span::styled(name_display, Style::default().fg(t.green)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(gid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        } else {
            group_items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_display, Style::default().fg(t.fg)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(gid_str, Style::default().fg(t.fg_dim)),
                Span::styled(" ", Style::default()),
            ])));
        }
    }

    group_items.truncate(list_height);
    f.render_widget(List::new(group_items), group_area);

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
        Span::styled(" \u{23ce}", Style::default().fg(accent)),
        Span::styled(" apply  ", Style::default().fg(t.fg_dim)),
        Span::styled("tab", Style::default().fg(accent)),
        Span::styled(" switch  ", Style::default().fg(t.fg_dim)),
        Span::styled("esc", Style::default().fg(accent)),
        Span::styled(" cancel", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}


