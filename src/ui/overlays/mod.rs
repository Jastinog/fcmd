mod which_key;
mod sort;
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

pub(super) use which_key::render_which_key;
pub(super) use sort::render_sort;
pub(super) use help::render_help;
pub(super) use input::render_input_popup;
pub(super) use confirm::render_confirm_popup;
pub(super) use preview::render_preview_popup;
pub(super) use theme_picker::render_theme_picker;
pub(super) use bookmarks::render_bookmarks;
pub(super) use search::render_search_popup;
pub(super) use chmod::render_chmod_popup;
pub(super) use info::render_info_popup;
pub(super) use chown::render_chown_picker;

pub(super) fn format_binary_size(bytes: usize) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
