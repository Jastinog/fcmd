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
    let list = &app.theme_list;
    let len = list.len();
    if len == 0 {
        return;
    }

    let popup = crate::ui::util::centered_rect(80, 75, area);
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
    use crate::util::icons::file_icon;

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
