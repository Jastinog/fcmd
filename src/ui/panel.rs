use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::PhantomEntry;
use crate::icons::file_icon;
use crate::ops::RegisterOp;
use crate::panel::Panel;
use crate::util::format_bytes;

use super::RenderContext;

pub(super) fn render_panel(
    f: &mut Frame,
    panel: &Panel,
    area: Rect,
    is_active: bool,
    phantoms: &[PhantomEntry],
    ctx: &RenderContext,
) {
    let t = ctx.theme;
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
    let sign_width = 2; // 1 git + 1 reg
    let meta_width = 16;
    let vm_width = 1; // visual mark on the right
    let name_width = inner_width.saturating_sub(meta_width + icon_width + sign_width + vm_width);

    let visual_range = panel.visual_range();

    let sort_mode = panel.sort_mode;

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
                if let Some(&sz) = ctx.dir_sizes.get(&entry.path) {
                    format!("{:>7}", format_bytes(sz))
                } else {
                    "  <DIR>".into()
                }
            } else {
                format!("{:>7}", format_bytes(entry.size))
            };

            let date_source = if sort_mode == crate::panel::SortMode::Created {
                entry.created
            } else {
                entry.modified
            };
            let date_str = date_source
                .map(crate::ui::util::format_time)
                .unwrap_or_else(|| "      ".into());

            let in_visual = visual_range
                .map(|(lo, hi)| i >= lo && i <= hi)
                .unwrap_or(false);
            let is_marked = panel.marked.contains(&entry.path);

            let is_cursor = i == panel.selected;
            let is_active_cursor = is_cursor && is_active;
            let in_reg = ctx.register.map_or(false, |r| r.paths.contains(&entry.path));
            let reg_color = ctx.register.and_then(|r| if in_reg {
                Some(match r.op {
                    RegisterOp::Yank => t.cyan,
                    RegisterOp::Cut => t.red,
                })
            } else {
                None
            });

            // Determine styles per segment
            let (icon_style, name_style, meta_style) = if is_active_cursor {
                let base = Style::default()
                    .bg(t.blue)
                    .fg(t.bg);
                (base, base, base)
            } else if in_visual && is_active {
                let base = Style::default().bg(t.magenta).fg(t.bg);
                (base, base, base)
            } else if is_marked && ctx.is_select_mode && is_active {
                let base = Style::default().bg(t.orange).fg(t.bg);
                (base, base, base)
            } else if is_marked {
                let base = Style::default().fg(t.green);
                (base, base, base)
            } else if let Some(c) = reg_color {
                let base = Style::default().fg(c);
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

            let vm_level = ctx.visual_marks.get(&entry.path).copied().unwrap_or(0);
            let row_bg = if is_active_cursor {
                Some(t.blue)
            } else if in_visual && is_active {
                Some(t.magenta)
            } else if is_marked && ctx.is_select_mode && is_active {
                Some(t.orange)
            } else if is_cursor {
                Some(t.cursor_line)
            } else {
                None
            };
            let sign_text = " ";
            let mut sign_style = Style::default();
            if let Some(bg) = row_bg {
                sign_style = sign_style.bg(bg);
            }

            let git_raw = ctx.git_statuses.get(&entry.path).copied().unwrap_or(' ');
            let (git_icon, git_color) = match git_raw {
                'M' => ("●", Some(t.yellow)),
                'A' => ("●", Some(t.green)),
                '?' => ("●", Some(t.cyan)),
                'D' => ("●", Some(t.red)),
                'R' => ("●", Some(t.magenta)),
                _ => (" ", None),
            };
            let git_style = match (git_color, row_bg) {
                (Some(c), Some(_)) => Style::default().fg(t.bg).bg(c),
                (Some(c), None) => Style::default().fg(c),
                (None, Some(bg)) => Style::default().fg(t.fg_dim).bg(bg),
                (None, None) => Style::default().fg(t.fg_dim),
            };

            let meta_text = format!(" {size_str} {date_str} ");

            let (vm_text, vm_style) = if vm_level > 0 {
                let vm_color = match vm_level {
                    1 => t.green,
                    2 => t.yellow,
                    _ => t.red,
                };
                let s = if row_bg.is_some() {
                    Style::default().fg(t.bg).bg(vm_color)
                } else {
                    Style::default().fg(vm_color)
                };
                ("\u{258a}", s)
            } else {
                let mut s = meta_style;
                if let Some(bg) = row_bg { s = s.bg(bg); }
                (" ", s)
            };

            let line = Line::from(vec![
                Span::styled(git_icon, git_style),
                Span::styled(sign_text, sign_style),
                Span::styled(icon, icon_style),
                Span::styled(name_col, name_style),
                Span::styled(meta_text, meta_style),
                Span::styled(vm_text, vm_style),
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
                Span::styled("\u{25cc}", ghost_style),
                Span::styled(" ", ghost_style),
                Span::styled(icon, ghost_style),
                Span::styled(name_col, ghost_style),
                Span::styled(" ".repeat(meta_width + vm_width), ghost_style),
            ]);
            items.push(ListItem::new(line));
        }
    }

    f.render_widget(List::new(items), inner);
}
