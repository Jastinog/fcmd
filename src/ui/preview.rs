use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::preview::Preview;
use crate::theme::Theme;

pub(super) fn render_preview(f: &mut Frame, preview: &Option<Preview>, area: Rect, t: &Theme) {
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
