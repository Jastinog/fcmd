mod which_key;
mod command;
mod help;
mod input;
mod confirm;
mod preview;
mod theme_picker;
mod bookmarks;
mod search;
mod chmod;
mod info;
mod chown;
mod conflict;
mod archive;
mod bulk_rename;
mod tasks;

pub(super) use which_key::render_which_key;
pub(super) use command::render_command_popup;
pub(super) use help::render_help;
pub(super) use input::render_input_popup;
pub(super) use confirm::render_confirm_popup;
pub(super) use confirm::render_quit_confirm;
pub(super) use preview::render_preview_popup;
pub(super) use theme_picker::render_theme_picker;
pub(super) use bookmarks::render_bookmarks;
pub(super) use search::render_search_popup;
pub(super) use chmod::render_chmod_popup;
pub(super) use info::render_info_popup;
pub(super) use chown::render_chown_picker;
pub(super) use conflict::render_conflict_popup;
pub(super) use archive::render_archive;
pub(super) use bulk_rename::render_bulk_rename;
pub(super) use tasks::render_tasks_overlay;

/// Render a single-line text input field: `<prefix><text>█` with the visible text
/// scrolled to keep the tail (where the cursor sits) in view, padded to `total_cols`.
/// Width-aware so CJK/emoji input doesn't overflow.
pub(in crate::ui) fn input_field_line<'a>(
    input: &str,
    prefix: &'a str,
    total_cols: usize,
    accent: ratatui::style::Color,
    t: &crate::theme::Theme,
) -> ratatui::text::Line<'a> {
    use crate::ui::util::{display_width, visible_input_tail};
    use ratatui::{style::Style, text::{Line, Span}};

    let prefix_w = display_width(prefix);
    // Reserve 1 column for the cursor block at the end.
    let field_w = total_cols.saturating_sub(prefix_w).max(1).saturating_sub(1);
    let visible = visible_input_tail(input, field_w);
    let used = prefix_w + display_width(&visible) + 1;
    let pad = total_cols.saturating_sub(used);

    Line::from(vec![
        Span::styled(prefix, Style::default().fg(accent)),
        Span::styled(visible, Style::default().fg(t.fg).bg(t.bg_light)),
        Span::styled("\u{2588}", Style::default().fg(accent).bg(t.bg_light)),
        Span::styled(" ".repeat(pad), Style::default().bg(t.bg_light)),
    ])
}

/// A horizontal separator line (`width` cells of dashes) with a right-aligned `indicator`
/// overwriting the trailing dashes, so the line stays exactly `width` wide.
pub(in crate::ui) fn separator_with_indicator(width: usize, indicator: &str) -> String {
    let dash_len = width.saturating_sub(crate::ui::util::display_width(indicator));
    format!("{}{indicator}", "\u{2500}".repeat(dash_len))
}

/// A horizontal separator that shows a right-aligned scroll percentage when the content
/// is scrollable (`max_scroll > 0`), matching the help overlay's indicator. Lets the user
/// see there's more above/below in scrollable list popups.
pub(in crate::ui) fn scroll_separator(width: usize, scroll: usize, max_scroll: usize) -> String {
    if max_scroll == 0 {
        return "\u{2500}".repeat(width);
    }
    let pct = (scroll * 100).checked_div(max_scroll).unwrap_or(100);
    separator_with_indicator(width, &format!(" {pct}%"))
}

pub(super) fn format_binary_size(bytes: usize) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
