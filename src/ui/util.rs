use std::time::SystemTime;

use chrono::{DateTime, Local, Utc};
use ratatui::layout::Rect;

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = (area.width * percent_x / 100).max(30).min(area.width);
    let h = (area.height * percent_y / 100).max(8).min(area.height);
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect::new(area.x + x, area.y + y, w, h)
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
