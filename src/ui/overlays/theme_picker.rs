use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::App;
use crate::theme::Theme;

pub(in crate::ui) fn render_theme_picker(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    if app.theme_groups.is_empty() {
        return;
    }

    let popup = crate::ui::util::centered_rect(85, 80, area);
    f.render_widget(Clear, popup);

    // Split into: groups | themes | preview
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(55),
        ])
        .split(popup);

    render_groups_column(f, t, panes[0], app);
    render_themes_column(f, t, panes[1], app);

    if let Some(ref pt) = app.theme_preview {
        render_preview_panel(f, pt, panes[2]);
    }
}

fn render_groups_column(f: &mut Frame, t: &Theme, area: Rect, app: &App) {
    let is_focused = app.theme_active_col == 0;
    let border_color = if is_focused { t.cyan } else { t.border_inactive };
    let title_color = if is_focused { t.cyan } else { t.fg_dim };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Groups ")
        .title_style(Style::default().fg(title_color))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 6 {
        return;
    }

    let groups = &app.theme_groups;
    let cursor = app.theme_group_cursor;
    let scroll = app.theme_group_scroll;
    let liw = inner.width as usize;
    let list_height = inner.height.saturating_sub(2) as usize;

    let mut items: Vec<ListItem> = Vec::new();
    for (i, group) in groups
        .iter()
        .enumerate()
        .take(groups.len().min(scroll + list_height))
        .skip(scroll)
    {
        let is_cursor = is_focused && i == cursor;
        let total = group.dark_themes.len() + group.light_themes.len();
        let count_str = format!(" ({total})");
        let name_max = liw.saturating_sub(2 + count_str.len());
        let name_display: String = if group.name.chars().count() > name_max {
            group.name
                .chars()
                .take(name_max.saturating_sub(1))
                .chain(std::iter::once('\u{2026}'))
                .collect()
        } else {
            group.name.to_string()
        };
        let marker = if is_cursor { "\u{25b8} " } else { "  " };
        let pad = liw.saturating_sub(
            marker.chars().count() + name_display.chars().count() + count_str.chars().count(),
        );

        if is_cursor {
            let cs = Style::default().fg(t.bg_text).bg(t.blue);
            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, cs),
                Span::styled(name_display, cs),
                Span::styled(" ".repeat(pad), cs),
                Span::styled(count_str, cs),
            ])));
        } else if !is_focused && i == cursor {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.cyan)),
                Span::styled(name_display, Style::default().fg(t.cyan)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(count_str, Style::default().fg(t.fg_dim)),
            ])));
        } else {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_display, Style::default().fg(t.fg)),
                Span::styled(" ".repeat(pad), Style::default()),
                Span::styled(count_str, Style::default().fg(t.fg_dim)),
            ])));
        }
    }

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

    render_column_footer(f, t, inner, list_height, is_focused, "l items  g/G top/bot");
}

fn render_themes_column(f: &mut Frame, t: &Theme, area: Rect, app: &App) {
    let is_focused = app.theme_active_col == 1;
    let border_color = if is_focused { t.cyan } else { t.border_inactive };
    let title_color = if is_focused { t.cyan } else { t.fg_dim };

    let group = app.theme_groups.get(app.theme_group_cursor);
    let group_title = group.map(|g| g.name).unwrap_or("Themes");
    let title = format!(" {group_title} ");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title.as_str())
        .title_style(Style::default().fg(title_color))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 6 {
        return;
    }

    // Dark/Light toggle indicator line
    let toggle_y = inner.y;
    let liw = inner.width as usize;
    if inner.height >= 3 {
        let dark_label = "\u{f0594} Dark";   // moon icon
        let light_label = "\u{f0595} Light"; // sun icon
        if app.theme_show_light {
            let spans = vec![
                Span::styled(format!(" {dark_label}  "), Style::default().fg(t.fg_dim)),
                Span::styled(format!("{light_label} "), Style::default().fg(t.yellow).bg(t.cursor_line)),
                Span::styled(
                    " ".repeat(liw.saturating_sub(dark_label.chars().count() + light_label.chars().count() + 5)),
                    Style::default(),
                ),
            ];
            f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, toggle_y, inner.width, 1));
        } else {
            let spans = vec![
                Span::styled(format!(" {dark_label} "), Style::default().fg(t.blue).bg(t.cursor_line)),
                Span::styled(format!("  {light_label} "), Style::default().fg(t.fg_dim)),
                Span::styled(
                    " ".repeat(liw.saturating_sub(dark_label.chars().count() + light_label.chars().count() + 5)),
                    Style::default(),
                ),
            ];
            f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, toggle_y, inner.width, 1));
        }
    }

    let themes: &[String] = match group {
        Some(g) => {
            if app.theme_show_light { &g.light_themes } else { &g.dark_themes }
        }
        None => return,
    };

    let cursor = app.theme_item_cursor;
    let scroll = app.theme_item_scroll;
    let active_name = app.theme_active_name.as_deref();
    // list area starts one row below (toggle indicator) and leaves 2 rows for footer
    let list_start_y = inner.y + 1;
    let list_height = inner.height.saturating_sub(3) as usize;

    let mut items: Vec<ListItem> = Vec::new();
    for (i, name) in themes
        .iter()
        .enumerate()
        .take(themes.len().min(scroll + list_height))
        .skip(scroll)
    {
        let is_cursor = is_focused && i == cursor;
        let is_active = active_name.is_some_and(|an| an == name);
        let marker = if is_cursor { "\u{25b8} " } else { "  " };
        let name_max = liw.saturating_sub(marker.chars().count());
        let name_display: String = if name.chars().count() > name_max {
            name.chars()
                .take(name_max.saturating_sub(1))
                .chain(std::iter::once('\u{2026}'))
                .collect()
        } else {
            let pad = name_max - name.chars().count();
            format!("{name}{}", " ".repeat(pad))
        };

        if is_cursor {
            let cs = Style::default().fg(t.bg_text).bg(t.blue);
            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, cs),
                Span::styled(name_display, cs),
            ])));
        } else if is_active {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.green)),
                Span::styled(name_display, Style::default().fg(t.green)),
            ])));
        } else {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(t.fg_dim)),
                Span::styled(name_display, Style::default().fg(t.fg)),
            ])));
        }
    }

    let list_area = Rect::new(inner.x, list_start_y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

    // Footer: separator + hints
    let sep_y = list_start_y + list_height as u16;
    if sep_y < inner.y + inner.height {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2500}".repeat(liw),
                Style::default().fg(t.border_inactive),
            ))),
            Rect::new(inner.x, sep_y, inner.width, 1),
        );
    }
    let hint_y = inner.y + inner.height.saturating_sub(1);
    if is_focused && hint_y < inner.y + inner.height {
        let spans = vec![
            Span::styled("h", Style::default().fg(t.cyan)),
            Span::styled(" groups  ", Style::default().fg(t.fg_dim)),
            Span::styled("t", Style::default().fg(t.cyan)),
            Span::styled(" toggle  ", Style::default().fg(t.fg_dim)),
            Span::styled("\u{23ce}", Style::default().fg(t.cyan)),
            Span::styled(" apply", Style::default().fg(t.fg_dim)),
        ];
        f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, hint_y, inner.width, 1));
    }
}

/// Renders a separator line + hint line at the bottom of a column's inner area.
fn render_column_footer(
    f: &mut Frame,
    t: &Theme,
    inner: Rect,
    list_height: usize,
    is_focused: bool,
    hint: &str,
) {
    let liw = inner.width as usize;
    let sep_y = inner.y + list_height as u16;
    if sep_y < inner.y + inner.height {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2500}".repeat(liw),
                Style::default().fg(t.border_inactive),
            ))),
            Rect::new(inner.x, sep_y, inner.width, 1),
        );
    }

    let hint_y = inner.y + inner.height.saturating_sub(1);
    if is_focused && hint_y < inner.y + inner.height {
        let parts: Vec<&str> = hint.splitn(2, "  ").collect();
        let mut spans = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            let mut sub = part.splitn(2, ' ');
            if let Some(key) = sub.next() {
                spans.push(Span::styled(key.to_string(), Style::default().fg(t.cyan)));
            }
            if let Some(desc) = sub.next() {
                spans.push(Span::styled(
                    format!(" {desc}"),
                    Style::default().fg(t.fg_dim),
                ));
            }
            if i + 1 < parts.len() {
                spans.push(Span::styled("  ", Style::default()));
            }
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, hint_y, inner.width, 1),
        );
    }
}

fn render_preview_panel(f: &mut Frame, pt: &Theme, area: Rect) {
    use crate::util::icons::file_icon;

    if area.height < 5 || area.width < 16 {
        return;
    }

    let panel_area = Rect::new(area.x, area.y, area.width, area.height - 1);
    let status_area = Rect::new(area.x, area.y + area.height - 1, area.width, 1);

    let title = " ~/Projects ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pt.border_active))
        .title(title)
        .title_style(Style::default().fg(pt.fg))
        .style(Style::default().bg(pt.bg));

    let inner = block.inner(panel_area);
    f.render_widget(block, panel_area);

    if inner.height < 2 || inner.width < 12 {
        return;
    }

    struct MockEntry {
        name: &'static str,
        is_dir: bool,
        meta: &'static str,
        date: &'static str,
        git: char,
        mark: u8,
    }

    let entries: &[MockEntry] = &[
        MockEntry { name: "..",         is_dir: true,  meta: "",      date: "",       git: ' ', mark: 0 },
        MockEntry { name: "src",        is_dir: true,  meta: "<DIR>", date: "Feb 14", git: 'M', mark: 0 },
        MockEntry { name: "docs",       is_dir: true,  meta: "<DIR>", date: "Feb 10", git: ' ', mark: 1 },
        MockEntry { name: "tests",      is_dir: true,  meta: "<DIR>", date: "Jan 28", git: ' ', mark: 0 },
        MockEntry { name: "Cargo.toml", is_dir: false, meta: "1.2K",  date: "Feb 14", git: 'M', mark: 0 },
        MockEntry { name: "README.md",  is_dir: false, meta: "3.4K",  date: "Feb 12", git: ' ', mark: 0 },
        MockEntry { name: "main.rs",    is_dir: false, meta: "840",   date: "Feb 01", git: 'A', mark: 2 },
        MockEntry { name: ".gitignore", is_dir: false, meta: "120",   date: "Jan 15", git: ' ', mark: 0 },
    ];

    let cursor_row = 1usize;
    let iw = inner.width as usize;

    for (i, entry) in entries.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }

        let is_cursor = i == cursor_row;
        let icon = if entry.name == ".." { " \u{f07c} " } else { file_icon(entry.name, entry.is_dir) };
        let display_name = if entry.is_dir && entry.name != ".." {
            format!("{}/", entry.name)
        } else {
            entry.name.to_string()
        };

        let (git_icon, git_color) = match entry.git {
            'M' => ("\u{f03eb}", Some(pt.yellow)),
            'A' => ("\u{f0415}", Some(pt.green)),
            '?' => ("\u{f0613}", Some(pt.cyan)),
            _ => (" ", None),
        };

        let (vm_text, vm_color) = match entry.mark {
            1 => ("\u{258a}", Some(pt.green)),
            2 => ("\u{258a}", Some(pt.yellow)),
            3 => ("\u{258a}", Some(pt.red)),
            _ => (" ", None),
        };

        let git_w = 1;
        let sign_w = 1;
        let icon_w = icon.chars().count();
        let vm_w = 1;
        let meta_date = if entry.name == ".." {
            String::new()
        } else {
            format!("{:>5}  {}", entry.meta, entry.date)
        };
        let meta_w = meta_date.chars().count() + 1;
        let name_w = iw.saturating_sub(git_w + sign_w + icon_w + meta_w + vm_w);

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

        if is_cursor {
            let cursor_style = Style::default().fg(pt.bg_text).bg(pt.blue);
            let git_style = match git_color {
                Some(c) => Style::default().fg(pt.bg_text).bg(c),
                None => cursor_style,
            };
            let vm_style = match vm_color {
                Some(c) => Style::default().fg(pt.bg_text).bg(c),
                None => cursor_style,
            };
            let line = Line::from(vec![
                Span::styled(git_icon, git_style),
                Span::styled(" ", cursor_style),
                Span::styled(icon, cursor_style),
                Span::styled(name_display, cursor_style),
                Span::styled(format!("{meta_date} "), cursor_style),
                Span::styled(vm_text, vm_style),
            ]);
            f.render_widget(Paragraph::new(line), Rect::new(inner.x, inner.y + i as u16, inner.width, 1));
        } else {
            let name_color = if entry.is_dir { pt.dir_color } else { pt.file_color };
            let icon_color = if entry.is_dir { pt.dir_color } else { pt.fg_dim };
            let git_style = match git_color {
                Some(c) => Style::default().fg(c),
                None => Style::default().fg(pt.fg_dim),
            };
            let vm_style = match vm_color {
                Some(c) => Style::default().fg(c),
                None => Style::default(),
            };
            let line = Line::from(vec![
                Span::styled(git_icon, git_style),
                Span::styled(" ", Style::default()),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::styled(name_display, Style::default().fg(name_color)),
                Span::styled(format!("{meta_date} "), Style::default().fg(pt.fg_dim)),
                Span::styled(vm_text, vm_style),
            ]);
            f.render_widget(Paragraph::new(line), Rect::new(inner.x, inner.y + i as u16, inner.width, 1));
        }
    }

    // Mini status bar
    let sw = status_area.width as usize;
    let sep_r = "\u{e0b0}";
    let sep_l = "\u{e0b2}";

    let mode_text = " \u{f018d} NORMAL ";
    let mode_span = Span::styled(mode_text, Style::default().fg(pt.bg_text).bg(pt.green));
    let mode_sep = Span::styled(sep_r, Style::default().fg(pt.green).bg(pt.bg_light));

    let info_text = " src/ \u{2502} 4 files, 3 dirs ";

    let pos_text = " 2/8 ";
    let pos_span = Span::styled(pos_text, Style::default().fg(pt.bg_text).bg(pt.blue));
    let pos_sep = Span::styled(sep_l, Style::default().fg(pt.blue).bg(pt.status_bg));

    let sort_text = " Name \u{25bc} ";
    let sort_span = Span::styled(sort_text, Style::default().fg(pt.fg_dim).bg(pt.bg_light));
    let sort_sep = Span::styled(sep_l, Style::default().fg(pt.bg_light).bg(pt.status_bg));

    let mode_w = mode_text.chars().count() + 1;
    let right_w = pos_text.chars().count() + 1 + sort_text.chars().count() + 1;

    let info_max = sw.saturating_sub(mode_w + 1 + right_w);
    let info_chars: Vec<char> = info_text.chars().collect();
    let info_display: String = if info_chars.len() > info_max {
        info_chars[..info_max.saturating_sub(1)]
            .iter()
            .chain(std::iter::once(&'\u{2026}'))
            .collect()
    } else {
        info_text.to_string()
    };
    let info_span = Span::styled(&info_display, Style::default().fg(pt.fg).bg(pt.bg_light));
    let info_sep = Span::styled(sep_r, Style::default().fg(pt.bg_light).bg(pt.status_bg));

    let left_used = mode_w + info_display.chars().count() + 1;
    let fill = sw.saturating_sub(left_used + right_w);

    let mut spans = vec![mode_span, mode_sep, info_span, info_sep];
    spans.push(Span::styled(" ".repeat(fill), Style::default().bg(pt.status_bg)));
    spans.extend([sort_sep, sort_span, pos_sep, pos_span]);

    f.render_widget(Paragraph::new(Line::from(spans)), status_area);
}
