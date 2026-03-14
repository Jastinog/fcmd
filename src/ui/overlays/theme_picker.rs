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

    let popup = crate::ui::util::centered_rect(85, 85, area);
    f.render_widget(Clear, popup);

    // Split vertically: selection (top) | preview (bottom)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(popup);

    // Top: groups | themes side by side
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Percentage(65),
        ])
        .split(rows[0]);

    render_groups_column(f, t, cols[0], app);
    render_themes_column(f, t, cols[1], app);

    if let Some(ref pt) = app.theme_preview {
        render_preview_panel(f, pt, rows[1]);
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

    if area.height < 7 || area.width < 24 {
        return;
    }

    let sep_r = "\u{e0b0}";
    let sep_l = "\u{e0b2}";

    // Layout: tab bar (1) | panels (h-3) | modes (1) | status bar (1)
    let tab_area = Rect::new(area.x, area.y, area.width, 1);
    let modes_area = Rect::new(area.x, area.y + area.height - 2, area.width, 1);
    let status_area = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    let panels_area = Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(3));

    // ── Tab bar with TASKS badge ─────────────────────────────────
    {
        let tw = area.width as usize;
        let tab1 = "  1: Projects ";
        let tab2 = " 2: Documents ";

        let task_info = " \u{f0c5} Copying 3 items 67% ";
        let task_badge = " \u{f0ae} TASKS 1 ";

        let task_info_w = task_info.chars().count() + 1;  // +sep
        let task_badge_w = task_badge.chars().count() + 1; // +sep
        let tabs_used = tab1.chars().count() + 1 + tab2.chars().count();
        let fill = tw.saturating_sub(tabs_used + task_info_w + task_badge_w);

        let tab_spans = vec![
            Span::styled(tab1, Style::default().fg(pt.bg_text).bg(pt.blue)),
            Span::styled(sep_r, Style::default().fg(pt.blue).bg(pt.status_bg)),
            Span::styled(tab2, Style::default().fg(pt.fg_dim).bg(pt.status_bg)),
            Span::styled(" ".repeat(fill), Style::default().bg(pt.status_bg)),
            Span::styled(sep_l, Style::default().fg(pt.bg_light).bg(pt.status_bg)),
            Span::styled(task_info, Style::default().fg(pt.cyan).bg(pt.bg_light)),
            Span::styled(sep_l, Style::default().fg(pt.green).bg(pt.bg_light)),
            Span::styled(task_badge, Style::default().fg(pt.bg_text).bg(pt.green)),
        ];
        f.render_widget(Paragraph::new(Line::from(tab_spans)), tab_area);
    }

    // ── Horizontal split: tree | file panel | code preview ──────
    let show_tree = panels_area.width >= 50;
    let show_code = panels_area.width >= 40;

    let (tree_rect, panel_rect, code_rect) = if show_tree && show_code {
        let tree_w = (panels_area.width * 20 / 100).max(10).min(16);
        let code_w = (panels_area.width * 35 / 100).max(14);
        let panel_w = panels_area.width.saturating_sub(tree_w + code_w);
        (
            Some(Rect::new(panels_area.x, panels_area.y, tree_w, panels_area.height)),
            Rect::new(panels_area.x + tree_w, panels_area.y, panel_w, panels_area.height),
            Some(Rect::new(panels_area.x + tree_w + panel_w, panels_area.y, code_w, panels_area.height)),
        )
    } else if show_code {
        let code_w = (panels_area.width * 35 / 100).max(14);
        let panel_w = panels_area.width.saturating_sub(code_w);
        (
            None,
            Rect::new(panels_area.x, panels_area.y, panel_w, panels_area.height),
            Some(Rect::new(panels_area.x + panel_w, panels_area.y, code_w, panels_area.height)),
        )
    } else {
        (None, panels_area, None)
    };

    // ── Tree sidebar ─────────────────────────────────────────────
    if let Some(tree_area) = tree_rect {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pt.border_inactive))
            .title(" \u{f0645} ")
            .title_style(Style::default().fg(pt.cyan))
            .style(Style::default().bg(pt.bg));
        let inner = block.inner(tree_area);
        f.render_widget(block, tree_area);

        let tree_entries: &[(&str, bool, u8, bool)] = &[
            // (name, is_dir, depth, expanded)
            ("Projects",    true,  0, true),
            ("\u{251c}\u{2500}\u{f07b} src/",      true,  1, true),
            ("\u{2502} \u{251c}\u{2500}\u{f0214} main.rs",  false, 2, false),
            ("\u{2502} \u{2514}\u{2500}\u{f0214} lib.rs",   false, 2, false),
            ("\u{251c}\u{2500}\u{f07b} docs/",     true,  1, false),
            ("\u{251c}\u{2500}\u{f07b} tests/",    true,  1, false),
            ("\u{2514}\u{2500}\u{f0219} Cargo.toml", false, 1, false),
        ];

        let tiw = inner.width as usize;
        for (i, &(name, is_dir, _depth, _expanded)) in tree_entries.iter().enumerate() {
            if i as u16 >= inner.height { break; }
            let is_cur = i == 0;
            let fg = if is_cur { pt.bg_text } else if is_dir { pt.dir_color } else { pt.fg_dim };
            let bg = if is_cur { pt.blue } else { pt.bg };
            let display: String = if name.chars().count() > tiw {
                name.chars().take(tiw.saturating_sub(1)).chain(std::iter::once('\u{2026}')).collect()
            } else {
                let pad = tiw.saturating_sub(name.chars().count());
                format!("{name}{}", " ".repeat(pad))
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(display, Style::default().fg(fg).bg(bg)))),
                Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
            );
        }
    }

    // ── File panel ───────────────────────────────────────────────
    {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pt.border_active))
            .title(" ~/Projects ")
            .title_style(Style::default().fg(pt.fg))
            .style(Style::default().bg(pt.bg));
        let inner = block.inner(panel_rect);
        f.render_widget(block, panel_rect);

        if inner.height >= 2 && inner.width >= 10 {
            struct MockEntry {
                name: &'static str,
                is_dir: bool,
                meta: &'static str,
                git: char,
                mark: u8,
                selected: bool,
            }

            let entries: &[MockEntry] = &[
                MockEntry { name: "..",         is_dir: true,  meta: "",      git: ' ', mark: 0, selected: false },
                MockEntry { name: "src",        is_dir: true,  meta: "<DIR>", git: 'M', mark: 0, selected: false },
                MockEntry { name: "docs",       is_dir: true,  meta: "<DIR>", git: ' ', mark: 1, selected: false },
                MockEntry { name: "tests",      is_dir: true,  meta: "<DIR>", git: ' ', mark: 0, selected: true },
                MockEntry { name: "Cargo.toml", is_dir: false, meta: "1.2K",  git: 'M', mark: 0, selected: true },
                MockEntry { name: "README.md",  is_dir: false, meta: "3.4K",  git: ' ', mark: 0, selected: false },
                MockEntry { name: "main.rs",    is_dir: false, meta: "840",   git: 'A', mark: 2, selected: false },
                MockEntry { name: ".gitignore", is_dir: false, meta: "120",   git: ' ', mark: 0, selected: false },
                MockEntry { name: "config.rs",  is_dir: false, meta: "2.1K",  git: '?', mark: 0, selected: false },
                MockEntry { name: "build.rs",   is_dir: false, meta: "560",   git: ' ', mark: 3, selected: false },
            ];

            let cursor_row = 1usize;
            let iw = inner.width as usize;

            for (i, entry) in entries.iter().enumerate() {
                if i as u16 >= inner.height { break; }
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

                let icon_w = icon.chars().count();
                let meta_str = if entry.name == ".." { String::new() } else { format!("{:>5}", entry.meta) };
                let meta_w = if entry.name == ".." { 0 } else { meta_str.chars().count() + 1 };
                let name_w = iw.saturating_sub(2 + icon_w + meta_w + 1);

                let name_display: String = if display_name.chars().count() > name_w {
                    display_name.chars().take(name_w.saturating_sub(1)).chain(std::iter::once('\u{2026}')).collect()
                } else {
                    let pad = name_w.saturating_sub(display_name.chars().count());
                    format!("{display_name}{}", " ".repeat(pad))
                };

                let row_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);

                if is_cursor {
                    let cs = Style::default().fg(pt.bg_text).bg(pt.blue);
                    let gs = git_color.map(|c| Style::default().fg(pt.bg_text).bg(c)).unwrap_or(cs);
                    let vs = vm_color.map(|c| Style::default().fg(pt.bg_text).bg(c)).unwrap_or(cs);
                    f.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled(git_icon, gs),
                        Span::styled(" ", cs),
                        Span::styled(icon, cs),
                        Span::styled(name_display, cs),
                        Span::styled(format!("{meta_str} "), cs),
                        Span::styled(vm_text, vs),
                    ])), row_area);
                } else {
                    let bg = if entry.selected { pt.cursor_line } else { pt.bg };
                    let name_fg = if entry.selected {
                        pt.orange
                    } else if entry.is_dir {
                        pt.dir_color
                    } else {
                        pt.file_color
                    };
                    let icon_fg = if entry.is_dir { pt.dir_color } else { pt.fg_dim };
                    let gs = git_color.map(|c| Style::default().fg(c).bg(bg)).unwrap_or(Style::default().fg(pt.fg_dim).bg(bg));
                    let vs = vm_color.map(|c| Style::default().fg(c).bg(bg)).unwrap_or(Style::default().bg(bg));
                    f.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled(git_icon, gs),
                        Span::styled(" ", Style::default().bg(bg)),
                        Span::styled(icon, Style::default().fg(icon_fg).bg(bg)),
                        Span::styled(name_display, Style::default().fg(name_fg).bg(bg)),
                        Span::styled(format!("{meta_str} "), Style::default().fg(pt.fg_dim).bg(bg)),
                        Span::styled(vm_text, vs),
                    ])), row_area);
                }
            }
        }
    }

    // ── Code preview ─────────────────────────────────────────────
    if let Some(code_area) = code_rect {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pt.border_inactive))
            .title(" \u{f0214} main.rs ")
            .title_style(Style::default().fg(pt.fg_dim))
            .style(Style::default().bg(pt.bg));
        let inner = block.inner(code_area);
        f.render_widget(block, code_area);

        if inner.height >= 2 && inner.width >= 10 {
            // Mock syntax-highlighted Rust code showing all palette colors
            let code_lines: Vec<Vec<(&str, ratatui::style::Color)>> = vec![
                vec![("use ", pt.magenta), ("std::io", pt.cyan), (";", pt.fg_dim)],
                vec![("", pt.fg)],
                vec![("fn ", pt.magenta), ("main", pt.blue), ("() {", pt.fg)],
                vec![("  let ", pt.magenta), ("name", pt.fg), (" = ", pt.fg_dim), ("\"fcmd\"", pt.green), (";", pt.fg_dim)],
                vec![("  let ", pt.magenta), ("count", pt.fg), (": ", pt.fg_dim), ("u32", pt.cyan), (" = ", pt.fg_dim), ("42", pt.orange), (";", pt.fg_dim)],
                vec![("  if ", pt.magenta), ("count", pt.fg), (" > ", pt.fg_dim), ("0", pt.orange), (" {", pt.fg)],
                vec![("    println!", pt.yellow), ("(", pt.fg), ("\"{}\"", pt.green), (", ", pt.fg_dim), ("name", pt.fg), (")", pt.fg), (";", pt.fg_dim)],
                vec![("  }", pt.fg)],
                vec![("  // TODO: ", pt.fg_dim), ("refactor", pt.fg_dim)],
                vec![("  ", pt.fg), ("Err", pt.red), ("(", pt.fg), ("\"fail\"", pt.green), (")", pt.fg)],
                vec![("}", pt.fg)],
            ];

            let ciw = inner.width as usize;
            for (i, spans_data) in code_lines.iter().enumerate() {
                if i as u16 >= inner.height { break; }
                let line_num = format!("{:>2} ", i + 1);
                let mut spans = vec![Span::styled(line_num, Style::default().fg(pt.fg_dim).bg(pt.bg))];
                let mut used = 3usize;
                for &(text, color) in spans_data {
                    if used >= ciw { break; }
                    let remaining = ciw - used;
                    let display: String = text.chars().take(remaining).collect();
                    used += display.chars().count();
                    spans.push(Span::styled(display, Style::default().fg(color).bg(pt.bg)));
                }
                if used < ciw {
                    spans.push(Span::styled(" ".repeat(ciw - used), Style::default().bg(pt.bg)));
                }
                f.render_widget(
                    Paragraph::new(Line::from(spans)),
                    Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
                );
            }

            // Fill empty lines with tilde
            for i in code_lines.len()..inner.height as usize {
                let tilde = format!(" \u{7e}{}", " ".repeat(ciw.saturating_sub(2)));
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(tilde, Style::default().fg(pt.fg_dim).bg(pt.bg)))),
                    Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
                );
            }
        }
    }

    // ── Mock command modal (floating over the panels) ──────────────
    if panels_area.width >= 30 && panels_area.height >= 7 {
        let modal_w = 40u16.min(panels_area.width.saturating_sub(8));
        let modal_h = 5u16;
        let mx = panels_area.x + (panels_area.width.saturating_sub(modal_w)) / 2;
        let my = panels_area.y + (panels_area.height.saturating_sub(modal_h)) / 2;
        let modal_area = Rect::new(mx, my, modal_w, modal_h);

        f.render_widget(Clear, modal_area);

        let modal_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(pt.cyan))
            .title(" \u{f018d} Command ")
            .title_style(Style::default().fg(pt.cyan))
            .style(Style::default().bg(pt.bg));
        let modal_inner = modal_block.inner(modal_area);
        f.render_widget(modal_block, modal_area);

        if modal_inner.width >= 10 {
            let miw = modal_inner.width as usize;

            // Input line with mock command text
            let prefix = " : ";
            let cmd_text = "theme dracula";
            let cursor_char = "\u{2588}";
            let used = prefix.chars().count() + cmd_text.chars().count() + 1;
            let pad = miw.saturating_sub(used);
            let input_line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(pt.cyan)),
                Span::styled(cmd_text, Style::default().fg(pt.fg).bg(pt.bg_light)),
                Span::styled(cursor_char, Style::default().fg(pt.cyan).bg(pt.bg_light)),
                Span::styled(" ".repeat(pad), Style::default().bg(pt.bg_light)),
            ]);
            f.render_widget(
                Paragraph::new(input_line),
                Rect::new(modal_inner.x, modal_inner.y, modal_inner.width, 1),
            );

            // Separator
            if modal_inner.height >= 2 {
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "\u{2500}".repeat(miw),
                        Style::default().fg(pt.border_inactive),
                    ))),
                    Rect::new(modal_inner.x, modal_inner.y + 1, modal_inner.width, 1),
                );
            }

            // Hints
            if modal_inner.height >= 3 {
                let hints = Line::from(vec![
                    Span::styled(" \u{23ce}", Style::default().fg(pt.cyan)),
                    Span::styled(" run  ", Style::default().fg(pt.fg_dim)),
                    Span::styled("esc", Style::default().fg(pt.cyan)),
                    Span::styled(" cancel", Style::default().fg(pt.fg_dim)),
                ]);
                f.render_widget(
                    Paragraph::new(hints),
                    Rect::new(modal_inner.x, modal_inner.y + 2, modal_inner.width, 1),
                );
            }
        }
    }

    // ── Modes showcase row ─────────────────────────────────────────
    {
        let mw = modes_area.width as usize;
        let modes: &[(&str, ratatui::style::Color)] = &[
            (" \u{f018d} NORMAL ", pt.green),
            (" \u{f0489} VISUAL ", pt.magenta),
            (" \u{f0135} SELECT ", pt.orange),
            (" \u{f0349} SEARCH ", pt.cyan),
            (" \u{f03eb} RENAME ", pt.yellow),
            (" \u{f05e8} CONFIRM ", pt.red),
        ];
        let mut spans: Vec<Span> = Vec::new();
        let mut used = 0usize;
        for (i, &(label, color)) in modes.iter().enumerate() {
            let lw = label.chars().count() + 1; // +1 for sep
            if used + lw > mw { break; }
            spans.push(Span::styled(label, Style::default().fg(pt.bg_text).bg(color)));
            let next_bg = if i + 1 < modes.len() { modes[i + 1].1 } else { pt.bg };
            spans.push(Span::styled(sep_r, Style::default().fg(color).bg(next_bg)));
            used += lw;
        }
        if used < mw {
            spans.push(Span::styled(" ".repeat(mw - used), Style::default().bg(pt.bg)));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), modes_area);
    }

    // ── Status bar ───────────────────────────────────────────────
    {
        let sw = status_area.width as usize;

        let mode_text = " \u{f018d} NORMAL ";
        let mode_span = Span::styled(mode_text, Style::default().fg(pt.bg_text).bg(pt.green));
        let mode_sep = Span::styled(sep_r, Style::default().fg(pt.green).bg(pt.bg_light));

        let info_text = " src/ \u{2502} 5 files, 3 dirs ";

        let pos_text = " 2/10 ";
        let pos_span = Span::styled(pos_text, Style::default().fg(pt.bg_text).bg(pt.blue));
        let pos_sep = Span::styled(sep_l, Style::default().fg(pt.blue).bg(pt.status_bg));

        let sort_text = " Name \u{2193} ";
        let sort_span = Span::styled(sort_text, Style::default().fg(pt.fg_dim).bg(pt.bg_light));
        let sort_sep = Span::styled(sep_l, Style::default().fg(pt.bg_light).bg(pt.status_bg));

        let reg_text = " y:2 ";
        let reg_span = Span::styled(reg_text, Style::default().fg(pt.yellow).bg(pt.bg_light));
        let reg_sep = Span::styled(sep_l, Style::default().fg(pt.bg_light).bg(pt.status_bg));

        let mode_w = mode_text.chars().count() + 1;
        let right_w = pos_text.chars().count() + 1 + sort_text.chars().count() + 1 + reg_text.chars().count() + 1;

        let info_max = sw.saturating_sub(mode_w + 1 + right_w);
        let info_display: String = if info_text.chars().count() > info_max {
            info_text.chars().take(info_max.saturating_sub(1)).chain(std::iter::once('\u{2026}')).collect()
        } else {
            info_text.to_string()
        };
        let info_span = Span::styled(&info_display, Style::default().fg(pt.fg).bg(pt.bg_light));
        let info_sep = Span::styled(sep_r, Style::default().fg(pt.bg_light).bg(pt.status_bg));

        let left_used = mode_w + info_display.chars().count() + 1;
        let fill = sw.saturating_sub(left_used + right_w);

        let mut spans = vec![mode_span, mode_sep, info_span, info_sep];
        spans.push(Span::styled(" ".repeat(fill), Style::default().bg(pt.status_bg)));
        spans.extend([reg_sep, reg_span, sort_sep, sort_span, pos_sep, pos_span]);

        f.render_widget(Paragraph::new(Line::from(spans)), status_area);
    }
}
