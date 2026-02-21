use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::theme::Theme;
use crate::ui::util::centered_rect;

pub(in crate::ui) fn render_help(f: &mut Frame, t: &Theme, area: Rect) {
    let popup = centered_rect(60, 80, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(" 󰋖 Help ")
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
                ("dd", "Move to trash"),
                ("dD", "Permanent delete"),
                ("p / P", "Paste / Paste to other"),
                ("yp", "Copy path to clipboard"),
                ("r", "Rename"),
                ("a", "Create file/dir (/ suffix)"),
                ("cp", "Permissions (chmod)"),
                ("co", "Owner (chown)"),
                ("i", "File info"),
                ("u", "Undo"),
            ],
        ),
        (
            " Modes",
            &[
                ("v", "Visual select"),
                ("/", "Search"),
                ("f / F", "Find local / global"),
                (":", "Command mode"),
                ("Space+?", "This help"),
            ],
        ),
        (
            "󱁐 Space leader",
            &[
                ("Space+p", "Toggle preview"),
                ("Space+t", "Toggle tree sidebar"),
                ("Space+h", "Toggle hidden files"),
                ("Space+d", "Directory sizes"),
                ("Space+s", "Sort popup"),
                ("Space+,/.", "Find local/global"),
            ],
        ),
        (
            "󰒓 Sort",
            &[
                ("sn/ss", "Sort by name/size"),
                ("sm/sc", "Sort by modified/created"),
                ("se", "Sort by extension"),
                ("sr", "Reverse sort"),
                ("J/K", "Scroll preview"),
            ],
        ),
        (
            " Tabs",
            &[
                ("gt / gT", "Next / Prev tab"),
                ("Ctrl+t", "New tab"),
                ("Ctrl+w", "Close tab"),
            ],
        ),
        (
            " Other",
            &[
                ("m{a-z}", "Set mark"),
                ("'{a-z}", "Go to mark"),
                ("b", "Add bookmark"),
                ("B", "Bookmarks"),
                ("T", "Theme picker"),
                ("Ctrl-r", "Refresh"),
            ],
        ),
    ];

    let iw = inner.width as usize;
    let mut lines: Vec<ListItem> = Vec::new();
    for (section, keys) in help {
        if !lines.is_empty() {
            lines.push(ListItem::new(Line::from("")));
        }
        lines.push(ListItem::new(Line::from(Span::styled(
            format!(" {section}"),
            Style::default().fg(t.cyan),
        ))));
        for (key, desc) in *keys {
            let line = Line::from(vec![
                Span::styled(format!("   {key:<16}"), Style::default().fg(t.yellow)),
                Span::styled(*desc, Style::default().fg(t.fg)),
            ]);
            lines.push(ListItem::new(line));
        }
    }

    let list_height = inner.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = lines.into_iter().take(list_height).collect();
    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(List::new(items), list_area);

    // Separator
    let sep_y = inner.y + list_height as u16;
    let sep_area = Rect::new(inner.x, sep_y, inner.width, 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(iw),
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );

    // Hint line
    let hint_line = Line::from(vec![
        Span::styled(" esc", Style::default().fg(t.cyan)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
