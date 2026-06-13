use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;
use crate::theme::Theme;

/// The variant-specific content of an input prompt; the surrounding scaffold is
/// shared between the search and filter popups.
struct InputPrompt<'a> {
    accent: Color,
    title: String,
    input: &'a str,
    prefix: &'a str,
    /// Hint line entries as (accented key, dimmed label) pairs.
    hints: &'a [(&'a str, &'a str)],
}

/// A centered single-line input prompt: title bar, input field, separator, and a
/// hint line. Shared scaffold for the search and filter popups.
fn render_input_prompt(f: &mut Frame, area: Rect, t: &Theme, p: InputPrompt) {
    let InputPrompt {
        accent,
        title,
        input,
        prefix,
        hints,
    } = p;
    // Height: border(2) + input(1) + separator(1) + hints(1) = 5
    let h = 5u16;
    let w = 50u16.min(area.width.saturating_sub(4)).max(30);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(title)
        .title_style(Style::default().fg(accent))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;

    // Input field (cursor stays at the end; text scrolls to show the tail).
    let input_line = super::input_field_line(input, prefix, iw, accent, t);
    f.render_widget(
        Paragraph::new(input_line),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    // Separator.
    let sep_line = Line::from(Span::styled(
        "\u{2500}".repeat(iw),
        Style::default().fg(t.border_inactive),
    ));
    f.render_widget(
        Paragraph::new(sep_line),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    // Hint line: each (key, label) pair shown as accented key + dimmed label.
    let mut hint_spans = Vec::with_capacity(hints.len() * 2);
    for (key, label) in hints {
        hint_spans.push(Span::styled(*key, Style::default().fg(accent)));
        hint_spans.push(Span::styled(*label, Style::default().fg(t.fg_dim)));
    }
    f.render_widget(
        Paragraph::new(Line::from(hint_spans)),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
}

pub(in crate::ui) fn render_search_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.cyan;

    let query_lower = app.search_query.to_lowercase();
    let match_count = if query_lower.is_empty() {
        0
    } else {
        app.tab()
            .active_panel()
            .entries
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&query_lower))
            .count()
    };

    let title = if app.search_query.is_empty() {
        " \u{f0349} Search ".to_string()
    } else {
        format!(" \u{f0349} Search ({match_count}) ")
    };

    render_input_prompt(
        f,
        area,
        t,
        InputPrompt {
            accent,
            title,
            input: &app.search_query,
            prefix: " / ",
            hints: &[
                (" \u{23ce}", " confirm  "),
                ("esc", " cancel  "),
                ("n/N", " next/prev"),
            ],
        },
    );
}

pub(in crate::ui) fn render_filter_popup(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let accent = t.green;

    // Visible matches (the panel is already narrowed live; exclude "..").
    let match_count = app
        .tab()
        .active_panel()
        .entries
        .iter()
        .filter(|e| e.name != "..")
        .count();

    let title = if app.filter_input.is_empty() {
        " \u{f0233} Filter ".to_string()
    } else {
        format!(" \u{f0233} Filter ({match_count}) ")
    };

    render_input_prompt(
        f,
        area,
        t,
        InputPrompt {
            accent,
            title,
            input: &app.filter_input,
            prefix: " \u{f0233} ",
            hints: &[(" \u{23ce}", " keep  "), ("esc", " cancel")],
        },
    );
}
