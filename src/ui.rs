use chrono::{DateTime, Local, Utc};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use std::time::SystemTime;

use crate::app::{App, Mode, PanelSide};
use crate::find::FindState;
use crate::panel::{Panel, SortMode};
use crate::preview::Preview;

pub fn render(f: &mut Frame, app: &mut App) {
    let full_area = f.area();

    let has_tabs = app.tabs.len() > 1;

    let chunks = if has_tabs {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Min(3),    // panels
                Constraint::Length(1), // status bar
            ])
            .split(full_area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),    // panels
                Constraint::Length(1), // status bar
            ])
            .split(full_area)
    };

    let (tab_bar_area, panel_chunk, status_area) = if has_tabs {
        (Some(chunks[0]), chunks[1], chunks[2])
    } else {
        (None, chunks[0], chunks[1])
    };

    if let Some(area) = tab_bar_area {
        render_tab_bar(f, app, area);
    }

    let panel_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(panel_chunk);

    let vis_h = panel_area[0].height.saturating_sub(2) as usize;
    app.visible_height = vis_h;

    let tab = app.tab_mut();
    tab.left.adjust_scroll(vis_h);
    tab.right.adjust_scroll(vis_h);

    // Adjust find scroll before rendering
    if let Some(ref mut fs) = app.find_state {
        let popup = centered_rect(60, 70, full_area);
        let inner_h = popup.height.saturating_sub(4) as usize;
        fs.adjust_scroll(inner_h);
    }

    let tab = app.tab();
    if app.preview_mode {
        match tab.active {
            PanelSide::Left => {
                render_panel(f, &tab.left, panel_area[0], true);
                render_preview(f, &app.preview, panel_area[1]);
            }
            PanelSide::Right => {
                render_preview(f, &app.preview, panel_area[0]);
                render_panel(f, &tab.right, panel_area[1], true);
            }
        }
    } else {
        render_panel(f, &tab.left, panel_area[0], tab.active == PanelSide::Left);
        render_panel(
            f,
            &tab.right,
            panel_area[1],
            tab.active == PanelSide::Right,
        );
    }
    render_status(f, app, status_area);

    // Overlays on top of everything
    if app.mode == Mode::Help {
        render_help(f, full_area);
    }

    if let Some(ref fs) = app.find_state {
        render_find(f, fs, full_area);
    }
}

fn render_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    for (i, tab) in app.tabs.iter().enumerate() {
        let dir_name = tab
            .active_panel()
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".into());
        let label = format!(" {}: {dir_name} ", i + 1);
        let style = if i == app.active_tab {
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(label, style));
        if i < app.tabs.len() - 1 {
            spans.push(Span::styled(
                "\u{2502}",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}

fn render_panel(f: &mut Frame, panel: &Panel, area: Rect, is_active: bool) {
    let border_color = if is_active {
        Color::Blue
    } else {
        Color::DarkGray
    };

    let path_str = panel.path.to_string_lossy();
    let max_title = area.width.saturating_sub(4) as usize;
    let title = if path_str.len() > max_title {
        format!("\u{2026}{}", &path_str[path_str.len() - max_title + 1..])
    } else {
        path_str.into_owned()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" {title} "));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_height = inner.height as usize;
    let inner_width = inner.width as usize;

    let meta_width = 16;
    let name_width = inner_width.saturating_sub(meta_width);

    let visual_range = panel.visual_range();

    let items: Vec<ListItem> = panel
        .entries
        .iter()
        .enumerate()
        .skip(panel.offset)
        .take(visible_height)
        .map(|(i, entry)| {
            let display_name = if entry.is_dir && entry.name != ".." {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            let name_col = if display_name.len() > name_width {
                format!(
                    "{}\u{2026}",
                    &display_name[..name_width.saturating_sub(1)]
                )
            } else {
                format!("{:<w$}", display_name, w = name_width)
            };

            let size_col = if entry.is_dir {
                "  <DIR>".into()
            } else {
                format!("{:>7}", format_size(entry.size))
            };

            let date_col = entry
                .modified
                .map(format_time)
                .unwrap_or_else(|| "      ".into());

            let line_text = format!("{name_col} {size_col} {date_col}");

            let in_visual = visual_range
                .map(|(lo, hi)| i >= lo && i <= hi)
                .unwrap_or(false);

            let style = if i == panel.selected && is_active {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if in_visual && is_active {
                Style::default().bg(Color::Magenta).fg(Color::White)
            } else if i == panel.selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if entry.is_dir {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_symlink {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(Span::styled(line_text, style)))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn render_preview(f: &mut Frame, preview: &Option<Preview>, area: Rect) {
    let (title, info) = match preview {
        Some(p) => (p.title.as_str(), p.info.as_str()),
        None => ("Preview", ""),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(format!(" {title} [{info}] "));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(p) = preview else { return };

    let visible = inner.height as usize;
    let width = inner.width as usize;

    let items: Vec<ListItem> = p
        .lines
        .iter()
        .skip(p.scroll)
        .take(visible)
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + p.scroll + 1;
            let max_content = width.saturating_sub(6);
            let content = if line.len() > max_content {
                &line[..max_content]
            } else {
                line.as_str()
            };
            let text = format!("{line_num:>4} {content}");
            ListItem::new(Line::from(Span::styled(
                text,
                Style::default().fg(Color::White),
            )))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    // Search mode
    if app.mode == Mode::Search {
        let input = format!("/{}", app.search_query);
        let p = Paragraph::new(Line::from(Span::styled(
            input,
            Style::default().fg(Color::Yellow).bg(Color::DarkGray),
        )));
        f.render_widget(p, area);
        return;
    }

    // Command mode
    if app.mode == Mode::Command {
        let input = format!(":{}", app.command_input);
        let p = Paragraph::new(Line::from(Span::styled(
            input,
            Style::default().fg(Color::White).bg(Color::DarkGray),
        )));
        f.render_widget(p, area);
        return;
    }

    // Confirm mode
    if app.mode == Mode::Confirm {
        let n = app.confirm_paths.len();
        let msg = if n == 1 {
            let name = app.confirm_paths[0]
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!(" Delete \"{name}\"? [y/N]")
        } else {
            format!(" Delete {n} items? [y/N]")
        };
        let p = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default()
                .fg(Color::White)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )));
        f.render_widget(p, area);
        return;
    }

    let panel = app.tab().active_panel();

    let mode_str = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Visual => "VISUAL",
        Mode::Find => "FIND",
        Mode::Help => "HELP",
        _ => "",
    };

    let mode_style = match app.mode {
        Mode::Visual => Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        Mode::Find => Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD),
        Mode::Help => Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::DarkGray),
    };

    let left_text = if !app.status_message.is_empty() {
        app.status_message.clone()
    } else if app.mode == Mode::Visual {
        let count = panel.targeted_count();
        format!(" {count} selected")
    } else {
        let file_count = panel.entries.iter().filter(|e| !e.is_dir).count();
        let dir_count = panel
            .entries
            .iter()
            .filter(|e| e.is_dir && e.name != "..")
            .count();
        let selected_name = panel
            .selected_entry()
            .map(|e| e.name.as_str())
            .unwrap_or("");
        format!(" {selected_name} | {file_count} files, {dir_count} dirs")
    };

    // Register indicator
    let reg = match &app.register {
        Some(r) => {
            let op = match r.op {
                crate::ops::RegisterOp::Yank => "y",
                crate::ops::RegisterOp::Cut => "d",
            };
            format!("[{op}:{}] ", r.paths.len())
        }
        None => String::new(),
    };

    // Search pattern indicator
    let search = if !app.search_query.is_empty() && app.mode == Mode::Normal {
        format!("/{} ", app.search_query)
    } else {
        String::new()
    };

    let pending = match app.pending_key {
        Some(c) => format!("{c}"),
        None => String::new(),
    };

    let sort_str = if panel.sort_mode != SortMode::Name || panel.sort_reverse {
        let arrow = if panel.sort_reverse {
            "\u{2191}"
        } else {
            "\u{2193}"
        };
        format!("[{}{}] ", panel.sort_mode.label(), arrow)
    } else {
        String::new()
    };

    let preview_str = if app.preview_mode { "[P] " } else { "" };

    let right_text = format!(
        " {search}{reg}{sort_str}{preview_str}{pending}{pos}/{total} ",
        pos = panel.selected + 1,
        total = panel.entries.len(),
    );

    let left = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {mode_str} "), mode_style),
        Span::styled(left_text, Style::default().fg(Color::DarkGray)),
    ]));
    let right = Paragraph::new(Line::from(Span::styled(
        right_text,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Right);

    f.render_widget(left, area);
    f.render_widget(right, area);
}

// --- Help overlay ---

fn render_help(f: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 80, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help \u{2014} press any key to close ");

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let help: &[(&str, &[(&str, &str)])] = &[
        (
            "Navigation",
            &[
                ("j/k", "Move down/up"),
                ("h/l", "Parent / Enter dir"),
                ("gg/G", "Top / Bottom"),
                ("Ctrl-d/u", "Half page down/up"),
                ("Tab", "Switch panel"),
                ("~", "Go home"),
            ],
        ),
        (
            "Files",
            &[
                ("yy", "Yank (copy)"),
                ("dd", "Cut (move)"),
                ("dD", "Delete (trash)"),
                ("p / P", "Paste / Paste to other"),
                ("yp", "Copy path to clipboard"),
                ("u", "Undo"),
            ],
        ),
        (
            "Modes",
            &[
                ("v", "Visual select"),
                ("/", "Search"),
                (":", "Command mode"),
                ("Ctrl-p", "Fuzzy finder"),
                ("?", "This help"),
            ],
        ),
        (
            "Preview & Sort",
            &[
                ("w", "Toggle preview"),
                ("J/K", "Scroll preview"),
                (".", "Toggle hidden files"),
                ("sn/ss/sd/se", "Sort name/size/date/ext"),
                ("sr", "Reverse sort"),
            ],
        ),
        (
            "Tabs",
            &[
                ("gt / gT", "Next / Prev tab"),
                (":tabnew", "New tab"),
                (":tabclose", "Close tab"),
            ],
        ),
        (
            "Other",
            &[
                ("m{a-z}", "Set mark"),
                ("'{a-z}", "Go to mark"),
                ("S", "Open shell"),
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
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))));
        for (key, desc) in *keys {
            let text = format!("   {key:<16}{desc}");
            lines.push(ListItem::new(Line::from(Span::styled(
                text,
                Style::default().fg(Color::White),
            ))));
        }
    }

    let visible = inner.height as usize;
    let items: Vec<ListItem> = lines.into_iter().take(visible).collect();
    f.render_widget(List::new(items), inner);
}

// --- Find overlay ---

fn render_find(f: &mut Frame, fs: &FindState, area: Rect) {
    let popup = centered_rect(60, 70, area);
    f.render_widget(Clear, popup);

    let title = format!(
        " Find ({}/{}) ",
        fs.filtered_count(),
        fs.total_count()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(title);

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 2 {
        return;
    }

    // Input line
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let input_text = format!("> {}", fs.query);
    let input = Paragraph::new(Line::from(Span::styled(
        input_text,
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )));
    f.render_widget(input, input_area);

    // Separator
    let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    let sep = Paragraph::new(Line::from(Span::styled(
        "\u{2500}".repeat(inner.width as usize),
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(sep, sep_area);

    // Results
    let results_height = inner.height.saturating_sub(2) as usize;
    let results_area = Rect::new(inner.x, inner.y + 2, inner.width, results_height as u16);

    let width = results_area.width as usize;
    let items: Vec<ListItem> = (fs.scroll..fs.scroll + results_height)
        .filter_map(|idx| {
            let (rel_path, is_dir) = fs.get_item(idx)?;
            let is_selected = idx == fs.selected;

            let display = if is_dir {
                format!("{rel_path}/")
            } else {
                rel_path.to_string()
            };

            let truncated = if display.len() > width.saturating_sub(2) {
                format!(
                    "\u{2026}{}",
                    &display[display.len() - width.saturating_sub(3)..]
                )
            } else {
                display
            };

            let prefix = if is_selected { "> " } else { "  " };
            let line = format!("{prefix}{truncated}");

            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else if is_dir {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            Some(ListItem::new(Line::from(Span::styled(line, style))))
        })
        .collect();

    f.render_widget(List::new(items), results_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = (area.width * percent_x / 100).max(30).min(area.width);
    let h = (area.height * percent_y / 100).max(8).min(area.height);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect::new(area.x + x, area.y + y, w, h)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn format_time(time: SystemTime) -> String {
    let dt: DateTime<Local> = DateTime::<Utc>::from(time).into();
    let now = Local::now();
    let six_months_ago = now - chrono::Duration::days(180);

    if dt < six_months_ago {
        dt.format("%b %y").to_string()
    } else {
        dt.format("%b %d").to_string()
    }
}
