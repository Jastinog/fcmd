use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::App;
use crate::util::icons::file_icon;

use super::util::{display_width, pad_to_width, truncate_to_width};

pub(super) fn render_tree(f: &mut Frame, app: &App, area: Rect) {
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
        .title_style(Style::default().fg(if is_focused { t.fg } else { t.cyan }))
        .style(Style::default().bg(t.bg));

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
                if line.is_expanded {
                    "󰝰 "
                } else {
                    "\u{f07b} "
                }
            } else {
                file_icon(&line.name, false)
            };

            let is_cursor = i == app.tree_selected;

            // Cursor row: uniform style for the whole line, padded so the
            // highlight bar spans the full panel width.
            if is_cursor && is_focused {
                let full = format!("{}{}{}", line.prefix, icon, line.name);
                let text = pad_to_width(&truncate_to_width(&full, width), width);
                return ListItem::new(Line::from(Span::styled(
                    text,
                    Style::default().fg(t.bg_text).bg(t.blue),
                )));
            }

            // Colors matching panels: dirs=dir_color, file icons=fg_dim, file names=file_color
            let (icon_style, name_style) = if is_cursor || line.is_current {
                let s = Style::default().fg(t.yellow);
                (s, s)
            } else if line.is_on_path || line.is_dir {
                let s = Style::default().fg(t.dir_color);
                (s, s)
            } else {
                (
                    Style::default().fg(t.fg_dim),
                    Style::default().fg(t.file_color),
                )
            };

            // Truncate the name by display width so wide chars don't overflow.
            let fixed_w = display_width(&line.prefix) + display_width(icon);
            let avail = width.saturating_sub(fixed_w);
            let name_display = if avail == 0 {
                String::new()
            } else {
                truncate_to_width(&line.name, avail)
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
