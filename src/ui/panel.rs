use std::cmp::Ordering;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::PhantomEntry;
use crate::panel::{Panel, SortMode};
use crate::util::icons::file_icon;
use crate::util::format_bytes;
use crate::util::natsort::natsort;
use crate::ops::RegisterOp;

use super::RenderContext;
use super::util::{display_width, truncate_to_width, pad_to_width};

enum DisplaySlot {
    Real(usize),
    Phantom(usize),
}

pub(super) fn render_panel(
    f: &mut Frame,
    panel: &Panel,
    area: Rect,
    is_active: bool,
    phantoms: &[&PhantomEntry],
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
    let title_w = display_width(&path_str);
    let title = if title_w > max_title {
        // Trim from the left to show the most relevant (rightmost) part
        let target = max_title.saturating_sub(1); // 1 for …
        let mut start_byte = 0;
        let mut col = 0;
        // Walk from the end backwards to find the cutoff
        let chars: Vec<(usize, char)> = path_str.char_indices().collect();
        for &(byte_idx, c) in chars.iter().rev() {
            let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if col + w > target {
                start_byte = byte_idx + c.len_utf8();
                break;
            }
            col += w;
        }
        format!("\u{2026}{}", &path_str[start_byte..])
    } else {
        path_str.into_owned()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" {title} "))
        .title_style(Style::default().fg(if is_active { t.fg } else { t.fg_dim }))
        .style(Style::default().bg(t.bg));

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

    // Build display slots: real entries interleaved with phantoms at sorted positions
    let slots = build_display_slots(
        &panel.entries,
        phantoms,
        panel.offset,
        visible_height,
        sort_mode,
        panel.sort_reverse,
    );

    let mut items: Vec<ListItem> = Vec::with_capacity(slots.len());

    for slot in &slots {
        match slot {
            DisplaySlot::Phantom(pi) => {
                let ph = phantoms[*pi];
                let ghost_style = Style::default().fg(t.fg_dim);
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
                let name_w = display_width(&display);
                let name_col = if name_w > name_width {
                    truncate_to_width(&display, name_width)
                } else {
                    pad_to_width(&display, name_width)
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
            DisplaySlot::Real(idx) => {
                let i = *idx;
                let entry = &panel.entries[i];

                let icon = file_icon(&entry.name, entry.is_dir);
                let display_name = if entry.is_dir && entry.name != ".." {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };
                let name_w = display_width(&display_name);
                let name_col = if name_w > name_width {
                    truncate_to_width(&display_name, name_width)
                } else {
                    pad_to_width(&display_name, name_width)
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

                let date_source = if sort_mode == SortMode::Created {
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

                let is_cursor = i == panel.selected && !panel.loading;
                let is_active_cursor = is_cursor && is_active;
                let in_reg = ctx.register_paths.contains(&entry.path);
                let reg_color = ctx.register.and_then(|r| {
                    if in_reg {
                        Some(match r.op {
                            RegisterOp::Yank => t.cyan,
                            RegisterOp::Cut => t.red,
                        })
                    } else {
                        None
                    }
                });

                // Determine styles per segment
                let (icon_style, name_style, meta_style) = if is_active_cursor {
                    let base = Style::default().bg(t.blue).fg(t.bg_text);
                    (base, base, base)
                } else if in_visual && is_active {
                    let base = Style::default().bg(t.magenta).fg(t.bg_text);
                    (base, base, base)
                } else if is_marked && ctx.is_select_mode && is_active {
                    let base = Style::default().bg(t.orange).fg(t.bg_text);
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
                        Style::default().fg(t.dir_color)
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
                    'M' => ("\u{f03eb}", Some(t.yellow)),     // 󰏫 md-pencil
                    'A' => ("\u{f0415}", Some(t.green)),      // 󰐕 md-plus
                    '?' => ("\u{f0613}", Some(t.cyan)),       // 󰘓 md-file_hidden
                    'D' => ("\u{f0374}", Some(t.red)),        // 󰍴 md-minus
                    'R' => ("\u{f0455}", Some(t.magenta)),    // 󰑕 md-rename_box
                    _ => (" ", None),
                };
                let git_style = match (git_color, row_bg) {
                    (Some(c), Some(_)) => Style::default().fg(t.bg_text).bg(c),
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
                        Style::default().fg(t.bg_text).bg(vm_color)
                    } else {
                        Style::default().fg(vm_color)
                    };
                    ("\u{f024}", s)
                } else {
                    let mut s = meta_style;
                    if let Some(bg) = row_bg {
                        s = s.bg(bg);
                    }
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
                items.push(ListItem::new(line));
            }
        }
    }

    f.render_widget(List::new(items), inner);
}

/// Compute display slots merging real entries with phantoms at their sorted positions.
///
/// For Name sort, phantoms are inserted at the position determined by natsort.
/// For other sort modes (Size, Modified, etc.), phantoms are appended to the end
/// of their section (dirs or files) since we don't have metadata for them.
///
/// The scroll offset is preserved: `panel.offset` refers to the real entry that
/// should be the first visible real entry. Phantoms whose insertion position falls
/// within the visible window are interleaved at the correct position.
fn build_display_slots(
    entries: &[crate::panel::FileEntry],
    phantoms: &[&PhantomEntry],
    offset: usize,
    visible_height: usize,
    sort_mode: SortMode,
    sort_reverse: bool,
) -> Vec<DisplaySlot> {
    if phantoms.is_empty() {
        // Fast path: no phantoms
        return (offset..entries.len().min(offset + visible_height))
            .map(DisplaySlot::Real)
            .collect();
    }

    // Find section boundaries: [.., dirs..., files...]
    let has_dotdot = entries.first().is_some_and(|e| e.name == "..");
    let dir_start = if has_dotdot { 1 } else { 0 };
    let file_start = entries[dir_start..]
        .iter()
        .position(|e| !e.is_dir)
        .map(|p| p + dir_start)
        .unwrap_or(entries.len());

    // Compute insertion position for each phantom
    let mut insertions: Vec<(usize, usize)> = Vec::with_capacity(phantoms.len());
    for (pi, ph) in phantoms.iter().enumerate() {
        let (sec_start, sec_end) = if ph.is_dir {
            (dir_start, file_start)
        } else {
            (file_start, entries.len())
        };

        let insert_pos = if sort_mode == SortMode::Name {
            let section = &entries[sec_start..sec_end];
            let pos = if sort_reverse {
                section.partition_point(|e| {
                    natsort(e.name.as_bytes(), ph.name.as_bytes()) == Ordering::Greater
                })
            } else {
                section.partition_point(|e| {
                    natsort(e.name.as_bytes(), ph.name.as_bytes()) == Ordering::Less
                })
            };
            sec_start + pos
        } else {
            // For non-name sort modes, append to end of section
            sec_end
        };

        insertions.push((insert_pos, pi));
    }

    // Stable sort so phantoms at the same position keep their original order
    insertions.sort_by_key(|(pos, _)| *pos);

    // Filter to only phantoms in the visible range
    let visible_phantoms: Vec<(usize, usize)> = insertions
        .into_iter()
        .filter(|(pos, _)| *pos >= offset)
        .collect();

    // Build merged slot list for the visible window
    let mut slots = Vec::with_capacity(visible_height);
    let mut ph_cursor = 0;
    let mut real_idx = offset;

    while slots.len() < visible_height {
        // Insert phantoms whose position is at or before the current real entry
        while ph_cursor < visible_phantoms.len()
            && visible_phantoms[ph_cursor].0 <= real_idx
            && slots.len() < visible_height
        {
            slots.push(DisplaySlot::Phantom(visible_phantoms[ph_cursor].1));
            ph_cursor += 1;
        }

        if slots.len() >= visible_height {
            break;
        }

        if real_idx < entries.len() {
            slots.push(DisplaySlot::Real(real_idx));
            real_idx += 1;
        } else {
            // No more real entries — emit remaining phantoms
            while ph_cursor < visible_phantoms.len() && slots.len() < visible_height {
                slots.push(DisplaySlot::Phantom(visible_phantoms[ph_cursor].1));
                ph_cursor += 1;
            }
            break;
        }
    }

    slots
}
