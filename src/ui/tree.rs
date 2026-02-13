use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::App;
use crate::icons::file_icon;

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
