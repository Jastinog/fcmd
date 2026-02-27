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

#[cfg(test)]
mod tests {
    use super::*;

    // ── display_width ──────────────────────────────────────────────

    #[test]
    fn display_width_ascii() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width(" "), 1);
    }

    #[test]
    fn display_width_cjk() {
        // CJK characters take 2 columns
        assert_eq!(display_width("日本語"), 6);
        assert_eq!(display_width("a日b"), 4); // 1 + 2 + 1
    }

    #[test]
    fn display_width_combining() {
        // Combining diacritical marks have zero width
        assert_eq!(display_width("e\u{0301}"), 1); // é (e + combining acute)
    }

    #[test]
    fn display_width_emoji() {
        // Standard emoji are typically 2 columns wide
        assert_eq!(display_width("🔥"), 2);
        assert_eq!(display_width("a🔥b"), 4);
    }

    #[test]
    fn display_width_nerd_font_icons() {
        // Nerd Font icons used in the app
        assert_eq!(display_width("\u{f07b}"), 1); // folder icon
        assert_eq!(display_width("\u{f024}"), 1); // flag icon
    }

    // ── truncate_to_width ──────────────────────────────────────────

    #[test]
    fn truncate_no_truncation_needed() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("hello", 5), "hello");
    }

    #[test]
    fn truncate_basic() {
        let result = truncate_to_width("hello world", 6);
        assert_eq!(result, "hello\u{2026}"); // "hello…"
        assert_eq!(display_width(&result), 6);
    }

    #[test]
    fn truncate_max_cols_zero() {
        assert_eq!(truncate_to_width("hello", 0), "\u{2026}");
        assert_eq!(truncate_to_width("hello", 1), "\u{2026}");
    }

    #[test]
    fn truncate_cjk_boundary() {
        // CJK char is 2 cols, can't fit partial char
        let result = truncate_to_width("日本語abc", 4);
        // "日本" = 4 cols, but need 1 for …, so only "日" (2) + "…" (1) = 3
        // Actually: target = 4-1 = 3, "日" = 2 cols, next "本" = 2 cols would exceed 3
        assert_eq!(display_width(&result), 3); // "日…"
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate_to_width("", 5), "");
    }

    // ── pad_to_width ──────────────────────────────────────────────

    #[test]
    fn pad_basic() {
        let result = pad_to_width("hi", 5);
        assert_eq!(result, "hi   ");
        assert_eq!(display_width(&result), 5);
    }

    #[test]
    fn pad_already_wider() {
        assert_eq!(pad_to_width("hello world", 5), "hello world");
    }

    #[test]
    fn pad_exact_width() {
        assert_eq!(pad_to_width("hello", 5), "hello");
    }

    #[test]
    fn pad_cjk() {
        let result = pad_to_width("日", 5);
        assert_eq!(result, "日   "); // 2 + 3 spaces = 5
        assert_eq!(display_width(&result), 5);
    }

    #[test]
    fn pad_zero_width() {
        assert_eq!(pad_to_width("hello", 0), "hello");
    }

    // ── centered_rect ──────────────────────────────────────────────

    #[test]
    fn centered_rect_basic() {
        let area = Rect::new(0, 0, 100, 50);
        let r = centered_rect(50, 50, area);
        assert_eq!(r.width, 50);
        assert_eq!(r.height, 25);
        assert_eq!(r.x, 25); // (100 - 50) / 2
        assert_eq!(r.y, 12); // (50 - 25) / 2 = 12.5, truncated
    }

    #[test]
    fn centered_rect_min_size() {
        let area = Rect::new(0, 0, 100, 50);
        let r = centered_rect(1, 1, area);
        // min width = 30, min height = 8
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 8);
    }

    #[test]
    fn centered_rect_max_size() {
        let area = Rect::new(0, 0, 40, 10);
        let r = centered_rect(100, 100, area);
        // Can't exceed area
        assert_eq!(r.width, 40);
        assert_eq!(r.height, 10);
    }

    #[test]
    fn centered_rect_small_area() {
        // Area smaller than minimums
        let area = Rect::new(0, 0, 20, 5);
        let r = centered_rect(50, 50, area);
        // min 30 clamped to 20 (area width), min 8 clamped to 5 (area height)
        assert_eq!(r.width, 20);
        assert_eq!(r.height, 5);
    }

    // ── format_time ────────────────────────────────────────────────

    #[test]
    fn format_time_recent() {
        use std::time::Duration;
        let recent = SystemTime::now() - Duration::from_secs(86400); // 1 day ago
        let result = format_time(recent);
        // Should be "Mon DD" format
        assert_eq!(result.len(), 6); // e.g. "Feb 26"
    }

    #[test]
    fn format_time_old() {
        use std::time::Duration;
        let old = SystemTime::now() - Duration::from_secs(365 * 86400); // 1 year ago
        let result = format_time(old);
        // Should be "Mon YY" format
        assert_eq!(result.len(), 6); // e.g. "Feb 25"
    }
}
