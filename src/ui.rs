use chrono::{DateTime, Local, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use std::time::SystemTime;

use crate::app::{App, Mode, PanelSide};
use crate::find::FindState;
use crate::icons::file_icon;
use crate::panel::{Panel, SortMode};
use crate::preview::Preview;
use crate::theme::Theme;

// Powerline separators
const SEP_RIGHT: &str = "\u{e0b0}"; //
const SEP_LEFT: &str = "\u{e0b2}"; //

// ── Main render ─────────────────────────────────────────────────────

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

    // Build horizontal layout: optional tree + panels (or panel + preview)
    let (tree_area, panel_areas) = if app.show_tree {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(40),
                Constraint::Percentage(40),
            ])
            .split(panel_chunk);
        (Some(cols[0]), vec![cols[1], cols[2]])
    } else {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(panel_chunk);
        (None, vec![cols[0], cols[1]])
    };

    let vis_h = panel_areas[0].height.saturating_sub(2) as usize;
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

    // Rebuild tree data only when needed
    if app.show_tree {
        let current_path = app.tab().active_panel().path.clone();
        let current_hidden = app.tab().active_panel().show_hidden;
        let needs_rebuild = app.tree_dirty
            || app.tree_last_path.as_ref() != Some(&current_path)
            || app.tree_last_hidden != current_hidden;
        if needs_rebuild {
            app.rebuild_tree();
            app.tree_last_path = Some(current_path);
            app.tree_last_hidden = current_hidden;
            app.tree_dirty = false;
        }
        // If not focused, auto-position cursor on current dir
        if !app.tree_focused {
            if let Some(idx) = app.tree_data.iter().position(|l| l.is_current) {
                app.tree_selected = idx;
            }
        }
    }

    if let Some(area) = tree_area {
        // Adjust tree scroll
        let tree_vis_h = area.height.saturating_sub(2) as usize;
        if tree_vis_h > 0 {
            let focus_idx = app.tree_selected;
            if focus_idx < app.tree_scroll {
                app.tree_scroll = focus_idx;
            } else if focus_idx >= app.tree_scroll + tree_vis_h {
                app.tree_scroll = focus_idx - tree_vis_h + 1;
            }
        }
        render_tree(f, app, area);
    }

    let panels_active = !app.tree_focused;
    let tab = app.tab();
    let t = &app.theme;
    let vm = &app.visual_marks;
    let left_phantoms = app.phantoms_for(&tab.left.path);
    let right_phantoms = app.phantoms_for(&tab.right.path);
    if app.preview_mode {
        match tab.active {
            PanelSide::Left => {
                render_panel(f, &tab.left, panel_areas[0], panels_active, vm, left_phantoms, t);
                render_preview(f, &app.preview, panel_areas[1], t);
            }
            PanelSide::Right => {
                render_preview(f, &app.preview, panel_areas[0], t);
                render_panel(f, &tab.right, panel_areas[1], panels_active, vm, right_phantoms, t);
            }
        }
    } else {
        render_panel(
            f,
            &tab.left,
            panel_areas[0],
            panels_active && tab.active == PanelSide::Left,
            vm,
            left_phantoms,
            t,
        );
        render_panel(
            f,
            &tab.right,
            panel_areas[1],
            panels_active && tab.active == PanelSide::Right,
            vm,
            right_phantoms,
            t,
        );
    }
    render_status(f, app, status_area);

    // Overlays on top of everything
    if app.mode == Mode::Help {
        render_help(f, &app.theme, full_area);
    }

    if let Some(ref fs) = app.find_state {
        render_find(f, fs, &app.theme, full_area);
    }
}

// ── Tab bar ─────────────────────────────────────────────────────────

fn render_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let mut spans = Vec::new();

    for (i, tab) in app.tabs.iter().enumerate() {
        let dir_name = tab
            .active_panel()
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".into());

        let is_active = i == app.active_tab;

        if is_active {
            spans.push(Span::styled(
                format!("  {}: {dir_name} ", i + 1),
                Style::default()
                    .fg(t.bg)
                    .bg(t.blue),
            ));
            spans.push(Span::styled(
                SEP_RIGHT,
                Style::default().fg(t.blue).bg(t.status_bg),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {}: {dir_name} ", i + 1),
                Style::default().fg(t.fg_dim).bg(t.status_bg),
            ));
            if i < app.tabs.len() - 1 {
                spans.push(Span::styled(
                    "\u{2502}",
                    Style::default().fg(t.border_inactive).bg(t.status_bg),
                ));
            }
        }
    }

    // Fill rest with status bg
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let remaining = (area.width as usize).saturating_sub(used);
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining),
            Style::default().bg(t.status_bg),
        ));
    }

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}

// ── Panel ───────────────────────────────────────────────────────────

fn render_panel(
    f: &mut Frame,
    panel: &Panel,
    area: Rect,
    is_active: bool,
    visual_marks: &std::collections::HashSet<std::path::PathBuf>,
    phantoms: &[crate::app::PhantomEntry],
    t: &Theme,
) {
    let border_color = if is_active {
        t.border_active
    } else {
        t.border_inactive
    };

    let path_str = panel.path.to_string_lossy();
    let max_title = area.width.saturating_sub(4) as usize;
    let path_chars: Vec<char> = path_str.chars().collect();
    let title = if path_chars.len() > max_title {
        let start = path_chars.len() - max_title + 1;
        let tail: String = path_chars[start..].iter().collect();
        format!("\u{2026}{tail}")
    } else {
        path_str.into_owned()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" {title} "))
        .title_style(Style::default().fg(if is_active { t.fg } else { t.fg_dim }));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_height = inner.height as usize;
    let inner_width = inner.width as usize;

    let icon_width = 2;
    let sign_width = 2;
    let meta_width = 16;
    let name_width = inner_width.saturating_sub(meta_width + icon_width + sign_width);

    let visual_range = panel.visual_range();

    let mut items: Vec<ListItem> = panel
        .entries
        .iter()
        .enumerate()
        .skip(panel.offset)
        .take(visible_height)
        .map(|(i, entry)| {
            let icon = file_icon(&entry.name, entry.is_dir);
            let display_name = if entry.is_dir && entry.name != ".." {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            let name_chars = display_name.chars().count();
            let name_col = if name_chars > name_width {
                let truncated: String = display_name.chars().take(name_width.saturating_sub(1)).collect();
                format!("{truncated}\u{2026}")
            } else {
                let pad = name_width.saturating_sub(name_chars);
                format!("{display_name}{}", " ".repeat(pad))
            };

            let size_str = if entry.is_dir {
                "  <DIR>".into()
            } else {
                format!("{:>7}", format_size(entry.size))
            };

            let date_str = entry
                .modified
                .map(format_time)
                .unwrap_or_else(|| "      ".into());

            let in_visual = visual_range
                .map(|(lo, hi)| i >= lo && i <= hi)
                .unwrap_or(false);
            let is_marked = panel.marked.contains(&entry.path);

            let is_cursor = i == panel.selected;
            let is_active_cursor = is_cursor && is_active;

            // Determine styles per segment
            let (icon_style, name_style, meta_style) = if is_active_cursor {
                let base = Style::default()
                    .bg(t.blue)
                    .fg(t.bg);
                (base, base, base)
            } else if in_visual && is_active {
                let base = Style::default().bg(t.magenta).fg(t.bg);
                (base, base, base)
            } else if is_marked {
                let base = Style::default().fg(t.green);
                (base, base, base)
            } else if is_cursor {
                // Inactive panel cursor
                let base = Style::default().bg(t.cursor_line).fg(t.fg);
                (base, base, base)
            } else {
                // Normal entry
                let ic = if entry.is_dir {
                    Style::default().fg(t.dir_color)
                } else {
                    Style::default().fg(t.fg_dim)
                };
                let nc = if entry.is_dir {
                    Style::default()
                        .fg(t.dir_color)
                                        } else if entry.is_symlink {
                    Style::default().fg(t.symlink_color)
                } else {
                    Style::default().fg(t.file_color)
                };
                let mc = Style::default().fg(t.fg_dim);
                (ic, nc, mc)
            };

            let is_vm = visual_marks.contains(&entry.path);
            let row_bg = if is_active_cursor {
                Some(t.blue)
            } else if in_visual && is_active {
                Some(t.magenta)
            } else if is_cursor {
                Some(t.cursor_line)
            } else {
                None
            };
            let sign_text = if is_vm { "● " } else { "  " };
            let mut sign_style = if is_vm {
                Style::default().fg(t.yellow)
            } else {
                Style::default()
            };
            if let Some(bg) = row_bg {
                sign_style = sign_style.bg(bg);
            }

            let meta_text = format!(" {size_str} {date_str}");
            let line = Line::from(vec![
                Span::styled(sign_text, sign_style),
                Span::styled(icon, icon_style),
                Span::styled(name_col, name_style),
                Span::styled(meta_text, meta_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    // Phantom entries for in-progress paste
    let real_count = items.len();
    let remaining_slots = visible_height.saturating_sub(real_count);
    if remaining_slots > 0 && !phantoms.is_empty() {
        let ghost_style = Style::default().fg(t.fg_dim);
        for ph in phantoms.iter().take(remaining_slots) {
            let icon = if ph.is_dir {
                "\u{f07b} "
            } else {
                file_icon(&ph.name, false)
            };
            let display = if ph.is_dir {
                format!("{}/", ph.name)
            } else {
                ph.name.clone()
            };
            let name_chars = display.chars().count();
            let name_col = if name_chars > name_width {
                let truncated: String = display.chars().take(name_width.saturating_sub(1)).collect();
                format!("{truncated}\u{2026}")
            } else {
                let pad = name_width.saturating_sub(name_chars);
                format!("{display}{}", " ".repeat(pad))
            };
            let line = Line::from(vec![
                Span::styled("\u{25cc} ", ghost_style),
                Span::styled(icon, ghost_style),
                Span::styled(name_col, ghost_style),
                Span::styled(" ".repeat(meta_width), ghost_style),
            ]);
            items.push(ListItem::new(line));
        }
    }

    f.render_widget(List::new(items), inner);
}

// ── Tree sidebar ────────────────────────────────────────────────────

fn render_tree(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let is_focused = app.tree_focused;
    let border_color = if is_focused {
        t.border_active
    } else {
        t.border_inactive
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" 󰙅 Tree ")
        .title_style(Style::default().fg(if is_focused { t.fg } else { t.cyan }));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible = inner.height as usize;
    let width = inner.width as usize;

    if visible == 0 || width == 0 || app.tree_data.is_empty() {
        return;
    }

    let items: Vec<ListItem> = app
        .tree_data
        .iter()
        .enumerate()
        .skip(app.tree_scroll)
        .take(visible)
        .map(|(i, line)| {
            let icon = if line.depth == 0 {
                " "
            } else if line.is_dir {
                if line.is_current || line.is_on_path {
                    "󰝰 "
                } else {
                    "\u{f07b} "
                }
            } else {
                file_icon(&line.name, false)
            };

            let is_cursor = i == app.tree_selected;

            // Cursor row: uniform style for the whole line
            if is_cursor && is_focused {
                let full = format!("{}{}{}", line.prefix, icon, line.name);
                let chars: Vec<char> = full.chars().collect();
                let text = if chars.len() > width {
                    let mut s: String = chars[..width.saturating_sub(1)].iter().collect();
                    s.push('\u{2026}');
                    s
                } else {
                    full
                };
                return ListItem::new(Line::from(Span::styled(
                    text,
                    Style::default().fg(t.bg).bg(t.blue),
                )));
            }

            // Colors matching panels: dirs=dir_color, file icons=fg_dim, file names=file_color
            let (icon_style, name_style) = if is_cursor || line.is_current {
                let s = Style::default().fg(t.yellow);
                (s, s)
            } else if line.is_on_path {
                let s = Style::default().fg(t.dir_color);
                (s, s)
            } else if line.is_dir {
                let s = Style::default().fg(t.dir_color);
                (s, s)
            } else {
                (
                    Style::default().fg(t.fg_dim),
                    Style::default().fg(t.file_color),
                )
            };

            // Truncate name if needed
            let prefix_chars: usize = line.prefix.chars().count();
            let icon_chars: usize = icon.chars().count();
            let name_chars: usize = line.name.chars().count();
            let total = prefix_chars + icon_chars + name_chars;

            let name_display = if total > width && width > prefix_chars + icon_chars {
                let avail = width - prefix_chars - icon_chars;
                let chars: Vec<char> = line.name.chars().collect();
                let mut s: String = chars[..avail.saturating_sub(1)].iter().collect();
                s.push('\u{2026}');
                s
            } else {
                line.name.clone()
            };

            ListItem::new(Line::from(vec![
                Span::styled(&line.prefix, Style::default().fg(t.border_inactive)),
                Span::styled(icon.to_string(), icon_style),
                Span::styled(name_display, name_style),
            ]))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

// ── Preview ─────────────────────────────────────────────────────────

fn render_preview(f: &mut Frame, preview: &Option<Preview>, area: Rect, t: &Theme) {
    let (title, info) = match preview {
        Some(p) => (p.title.as_str(), p.info.as_str()),
        None => ("Preview", ""),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(format!(" {title} [{info}] "))
        .title_style(Style::default().fg(t.cyan));

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
            let num_width = 4;
            let max_content = width.saturating_sub(num_width + 2);
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
                Span::styled(content.to_string(), Style::default().fg(t.fg)),
            ])
        })
        .map(ListItem::new)
        .collect();

    f.render_widget(List::new(items), inner);
}

// ── Status bar (lualine style) ──────────────────────────────────────

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;

    // Fill background
    f.render_widget(
        Block::default().style(Style::default().bg(t.status_bg)),
        area,
    );

    // Input modes (Search / Command) get special treatment
    if app.mode == Mode::Search {
        render_status_input(f, area, "/", &app.search_query, t.blue, t);
        return;
    }
    if app.mode == Mode::Command {
        render_status_input(f, area, ":", &app.command_input, t.green, t);
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
            format!(" 󰗨 Delete \"{name}\"? [y/N] ")
        } else {
            format!(" 󰗨 Delete {n} items? [y/N] ")
        };

        let mut spans = vec![
            Span::styled(
                " CONFIRM ",
                Style::default()
                    .fg(t.bg)
                    .bg(t.red),
            ),
            Span::styled(SEP_RIGHT, Style::default().fg(t.red).bg(t.bg_light)),
            Span::styled(msg, Style::default().fg(t.red).bg(t.bg_light)),
        ];

        // Fill remaining
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let remaining = (area.width as usize).saturating_sub(used);
        if remaining > 0 {
            spans.push(Span::styled(
                " ".repeat(remaining),
                Style::default().bg(t.status_bg),
            ));
        }

        f.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    let panel = app.tab().active_panel();
    let width = area.width as usize;

    // ── Mode segment ────────────────
    let (mode_str, mode_bg) = if app.tree_focused && app.mode == Mode::Normal {
        ("TREE", t.cyan)
    } else {
        match app.mode {
            Mode::Normal => ("NORMAL", t.green),
            Mode::Visual => ("VISUAL", t.magenta),
            Mode::Find => ("FIND", t.cyan),
            Mode::Help => ("HELP", t.cyan),
            _ => ("", t.fg_dim),
        }
    };

    let mode_span = Span::styled(
        format!(" {mode_str} "),
        Style::default()
            .fg(t.bg)
            .bg(mode_bg),
    );
    let mode_sep = Span::styled(
        SEP_RIGHT,
        Style::default().fg(mode_bg).bg(t.bg_light),
    );

    // ── Right side segments (built first to compute width) ────────────────
    let mut right_parts: Vec<(String, Color, Color)> = Vec::new();

    // Position segment (rightmost)
    let pos_text = format!(
        " {}/{} ",
        panel.selected + 1,
        panel.entries.len()
    );
    right_parts.push((pos_text, t.bg, t.blue));

    // Sort segment
    if panel.sort_mode != SortMode::Name || panel.sort_reverse {
        let arrow = if panel.sort_reverse { "\u{2191}" } else { "\u{2193}" };
        right_parts.push((
            format!(" {}{arrow} ", panel.sort_mode.label()),
            t.fg,
            t.bg_light,
        ));
    }

    // Register segment
    if let Some(ref r) = app.register {
        let op = match r.op {
            crate::ops::RegisterOp::Yank => "y",
            crate::ops::RegisterOp::Cut => "d",
        };
        right_parts.push((format!(" {op}:{} ", r.paths.len()), t.yellow, t.bg_light));
    }

    // Search pattern
    if !app.search_query.is_empty() && app.mode == Mode::Normal {
        right_parts.push((
            format!(" /{} ", app.search_query),
            t.yellow,
            t.bg_light,
        ));
    }

    // Preview indicator
    if app.preview_mode {
        right_parts.push((" 󰈈 ".to_string(), t.cyan, t.bg_light));
    }

    // Pending key
    if let Some(c) = app.pending_key {
        right_parts.push((format!(" {c} "), t.orange, t.bg_light));
    }

    // Build right spans (reverse order so rightmost is last)
    let mut right_spans: Vec<Span> = Vec::new();
    for (idx, (text, fg, seg_bg)) in right_parts.iter().enumerate().rev() {
        let prev_bg = if idx == right_parts.len() - 1 {
            t.status_bg
        } else {
            right_parts[idx + 1].2
        };
        right_spans.push(Span::styled(
            SEP_LEFT,
            Style::default().fg(*seg_bg).bg(prev_bg),
        ));
        right_spans.push(Span::styled(
            text.clone(),
            Style::default().fg(*fg).bg(*seg_bg),
        ));
    }

    let right_used: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();

    // ── Info segment (capped so right segments stay fixed) ────────────────
    let info_text = if !app.status_message.is_empty() {
        format!(" {} ", app.status_message)
    } else if app.mode == Mode::Visual {
        let count = panel.targeted_count();
        format!("  {count} selected ")
    } else if !panel.marked.is_empty() {
        let count = panel.marked.len();
        format!("  {count} marked ")
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
        format!(" {selected_name} \u{2502} {file_count} files, {dir_count} dirs ")
    };

    // Cap info width so right segments always stay at the right edge
    let mode_width = mode_str.len() + 2 + 1; // " MODE " + SEP_RIGHT
    let info_sep_width = 1;
    let max_info = width.saturating_sub(mode_width + info_sep_width + right_used);

    let info_chars: Vec<char> = info_text.chars().collect();
    let info_display = if info_chars.len() > max_info {
        if max_info > 1 {
            let truncated: String = info_chars[..max_info - 1].iter().collect();
            format!("{truncated}\u{2026}")
        } else {
            String::new()
        }
    } else {
        info_text
    };

    let info_span = Span::styled(
        info_display.clone(),
        Style::default().fg(t.fg).bg(t.bg_light),
    );
    let info_sep = Span::styled(
        SEP_RIGHT,
        Style::default().fg(t.bg_light).bg(t.status_bg),
    );

    // Calculate fill to push right segments to the edge
    let left_used: usize = mode_width + info_display.chars().count() + info_sep_width;
    let fill = width.saturating_sub(left_used + right_used);

    let mut all_spans = vec![mode_span, mode_sep, info_span, info_sep];
    all_spans.push(Span::styled(
        " ".repeat(fill),
        Style::default().bg(t.status_bg),
    ));
    all_spans.extend(right_spans);

    f.render_widget(Paragraph::new(Line::from(all_spans)), area);
}

fn render_status_input(f: &mut Frame, area: Rect, prefix: &str, input: &str, accent: Color, t: &Theme) {
    let label = if prefix == "/" { " SEARCH " } else { " CMD " };

    let mut spans = vec![
        Span::styled(
            label,
            Style::default()
                .fg(t.bg)
                .bg(accent),
        ),
        Span::styled(SEP_RIGHT, Style::default().fg(accent).bg(t.bg_light)),
        Span::styled(
            format!(" {prefix}{input} "),
            Style::default().fg(t.fg).bg(t.bg_light),
        ),
        Span::styled(SEP_RIGHT, Style::default().fg(t.bg_light).bg(t.status_bg)),
    ];

    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let remaining = (area.width as usize).saturating_sub(used);
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining),
            Style::default().bg(t.status_bg),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── Help overlay ────────────────────────────────────────────────────

fn render_help(f: &mut Frame, t: &Theme, area: Rect) {
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
                ("Ctrl-p", "Fuzzy finder"),
                ("?", "This help"),
            ],
        ),
        (
            "󰒓 Preview & Sort",
            &[
                ("w", "Toggle preview"),
                ("t", "Toggle tree sidebar"),
                ("J/K", "Scroll preview"),
                (".", "Toggle hidden files"),
                ("sn/ss/sd/se", "Sort name/size/date/ext"),
                ("sr", "Reverse sort"),
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

// ── Find overlay ────────────────────────────────────────────────────

fn render_find(f: &mut Frame, fs: &FindState, t: &Theme, area: Rect) {
    let popup = centered_rect(60, 70, area);
    f.render_widget(Clear, popup);

    let walk_indicator = if fs.walking { "\u{2026}" } else { "" };
    let title = format!(
        "  Find ({}/{}{walk_indicator}) ",
        fs.filtered_count(),
        fs.total_count()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(title)
        .title_style(Style::default().fg(t.cyan));

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
            .fg(t.green),
    )));
    f.render_widget(input, input_area);

    // Separator
    let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    let sep = Paragraph::new(Line::from(Span::styled(
        "\u{2500}".repeat(inner.width as usize),
        Style::default().fg(t.border_inactive),
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

            let icon = file_icon(
                rel_path.rsplit('/').next().unwrap_or(rel_path),
                is_dir,
            );

            let display = if is_dir {
                format!("{rel_path}/")
            } else {
                rel_path.to_string()
            };

            let max_display = width.saturating_sub(4 + icon.chars().count());
            let display_chars: Vec<char> = display.chars().collect();
            let truncated = if display_chars.len() > max_display {
                let start = display_chars.len() - max_display.saturating_sub(1);
                let tail: String = display_chars[start..].iter().collect();
                format!("\u{2026}{tail}")
            } else {
                display
            };

            let prefix = if is_selected { "> " } else { "  " };

            let style = if is_selected {
                Style::default()
                    .fg(t.fg)
                    .bg(t.cyan)
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

// ── Utilities ───────────────────────────────────────────────────────

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
