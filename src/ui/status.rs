use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::{App, Mode};
use crate::panel::SortMode;

use super::{SEP_LEFT, SEP_RIGHT};

pub(super) fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;

    // Fill background
    f.render_widget(
        Block::default().style(Style::default().bg(t.status_bg)),
        area,
    );


    // Confirm mode — overlay handles the popup, status bar shows mode
    if app.mode == Mode::Confirm {
        let mut spans = vec![
            Span::styled(format!(" \u{f05e8} CONFIRM "), Style::default().fg(t.bg_text).bg(t.red)),
            Span::styled(SEP_RIGHT, Style::default().fg(t.red).bg(t.status_bg)),
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
        return;
    }

    let panel = app.tab().active_panel();
    let width = area.width as usize;

    // ── Mode segment ────────────────
    let mode_str: String;
    let (mode_ref, mode_bg) = if app.tree_focused && app.mode == Mode::Normal {
        mode_str = format!("\u{f0645} TREE");    // 󰙅
        (mode_str.as_str(), t.cyan)
    } else {
        match app.mode {
            Mode::Normal => {
                mode_str = format!("\u{f018d} NORMAL");  // 󰆍
                (mode_str.as_str(), t.green)
            }
            Mode::Visual => {
                mode_str = format!("\u{f0489} VISUAL");  // 󰒉
                (mode_str.as_str(), t.magenta)
            }
            Mode::Select => {
                mode_str = format!("\u{f0135} SELECT");  // 󰄵
                (mode_str.as_str(), t.orange)
            }
            Mode::Find => {
                mode_str = format!("\u{f002} FIND");     //
                (mode_str.as_str(), t.cyan)
            }
            Mode::Preview => {
                mode_str = format!("\u{f0208} PREVIEW"); // 󰈈
                (mode_str.as_str(), t.cyan)
            }
            Mode::Help => {
                mode_str = format!("\u{f02d6} HELP");    // 󰋖
                (mode_str.as_str(), t.cyan)
            }
            Mode::ThemePicker => {
                mode_str = format!("\u{f03d8} THEME");   // 󰏘
                (mode_str.as_str(), t.cyan)
            }
            Mode::Bookmarks => {
                mode_str = format!("\u{f02e6} BOOKMARKS"); // 󰋦
                (mode_str.as_str(), t.cyan)
            }
            Mode::Rename => {
                mode_str = format!("\u{f03eb} RENAME");  // 󰏫
                (mode_str.as_str(), t.yellow)
            }
            Mode::Create => {
                mode_str = format!("\u{f0415} CREATE");  // 󰐕
                (mode_str.as_str(), t.green)
            }
            Mode::Chmod => {
                mode_str = format!("\u{f033e} CHMOD");   // 󰌾
                (mode_str.as_str(), t.orange)
            }
            Mode::Chown => {
                mode_str = format!("\u{f0004} CHOWN");   // 󰀄
                (mode_str.as_str(), t.orange)
            }
            Mode::Info => {
                mode_str = format!("\u{f02fd} INFO");    // 󰋽
                (mode_str.as_str(), t.cyan)
            }
            Mode::Search => {
                mode_str = format!("\u{f0349} SEARCH");  // 󰍉
                (mode_str.as_str(), t.cyan)
            }
            Mode::Command => {
                mode_str = format!("\u{f018d} CMD");     // 󰆍
                (mode_str.as_str(), t.cyan)
            }
            _ => {
                mode_str = String::new();
                (mode_str.as_str(), t.fg_dim)
            }
        }
    };
    let mode_str = mode_ref;

    let mode_span = Span::styled(
        format!(" {mode_str} "),
        Style::default().fg(t.bg_text).bg(mode_bg),
    );
    let mode_sep = Span::styled(SEP_RIGHT, Style::default().fg(mode_bg).bg(t.bg_light));

    // ── Right side segments (built first to compute width) ────────────────
    let mut right_parts: Vec<(String, Color, Color)> = Vec::new();

    // Position segment (rightmost)
    let pos_text = format!(" {}/{} ", panel.selected + 1, panel.entries.len());
    right_parts.push((pos_text, t.bg, t.blue));

    // Sort segment (always visible)
    {
        let arrow = if panel.sort_reverse {
            "\u{2191}"
        } else {
            "\u{2193}"
        };
        let sort_fg = if panel.sort_mode != SortMode::Name || panel.sort_reverse {
            t.cyan
        } else {
            t.fg_dim
        };
        right_parts.push((
            format!(" 󰒓 {}{arrow} ", panel.sort_mode.label()),
            sort_fg,
            t.bg_light,
        ));
    }

    // Register segment
    if let Some(ref r) = app.register {
        let op = match r.op {
            crate::ops::RegisterOp::Yank => "y",
            crate::ops::RegisterOp::Cut => "d",
        };
        right_parts.push((format!(" {op}:{} ", r.entries.len()), t.yellow, t.bg_light));
    }

    // Search pattern
    if !app.search_query.is_empty() && app.mode == Mode::Normal {
        right_parts.push((format!(" /{} ", app.search_query), t.yellow, t.bg_light));
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
    } else if app.mode == Mode::Select {
        let count = panel.marked.len();
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
    let mode_width = mode_str.chars().count() + 2 + 1; // " MODE " + SEP_RIGHT
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
    let info_sep = Span::styled(SEP_RIGHT, Style::default().fg(t.bg_light).bg(t.status_bg));

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
