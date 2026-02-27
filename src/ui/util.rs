use std::time::SystemTime;

use chrono::{DateTime, Local, Utc};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthChar;

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = (area.width * percent_x / 100).max(30).min(area.width);
    let h = (area.height * percent_y / 100).max(8).min(area.height);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect::new(area.x + x, area.y + y, w, h)
}

/// Display width in terminal columns (CJK = 2, combining = 0, etc.)
pub(crate) fn display_width(s: &str) -> usize {
    s.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum()
}

/// Truncate a string to fit within `max_cols` terminal columns.
/// Appends `…` if truncated.
pub(crate) fn truncate_to_width(s: &str, max_cols: usize) -> String {
    let width = display_width(s);
    if width <= max_cols {
        return s.to_string();
    }
    if max_cols <= 1 {
        return "\u{2026}".to_string();
    }
    let target = max_cols - 1; // reserve 1 col for …
    let mut out = String::new();
    let mut col = 0;
    for c in s.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if col + w > target {
            break;
        }
        out.push(c);
        col += w;
    }
    out.push('\u{2026}');
    out
}

/// Pad a string with spaces to fill exactly `target_cols` terminal columns.
/// If the string is already wider, returns it as-is.
pub(crate) fn pad_to_width(s: &str, target_cols: usize) -> String {
    let w = display_width(s);
    if w >= target_cols {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(target_cols - w))
    }
}

pub(crate) fn format_time(time: SystemTime) -> String {
    let dt: DateTime<Local> = DateTime::<Utc>::from(time).into();
    let now = Local::now();
    let six_months_ago = now - chrono::Duration::days(180);

    if dt < six_months_ago {
        dt.format("%b %y").to_string()
    } else {
        dt.format("%b %d").to_string()
    }
}
