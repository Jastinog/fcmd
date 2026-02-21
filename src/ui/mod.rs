use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, Mode, PanelLayout};
use crate::ops::Register;
use crate::theme::Theme;

mod find_overlay;
mod overlays;
mod panel;
mod preview;
mod status;
mod tree;
pub(crate) mod util;

// Powerline separators
pub(super) const SEP_RIGHT: &str = "\u{e0b0}"; //
pub(super) const SEP_LEFT: &str = "\u{e0b2}"; //

pub struct RenderContext<'a> {
    pub visual_marks: &'a HashMap<PathBuf, u8>,
    pub dir_sizes: &'a HashMap<PathBuf, u64>,
    pub register: Option<&'a Register>,
    pub git_statuses: &'a HashMap<PathBuf, char>,
    pub theme: &'a Theme,
    pub is_select_mode: bool,
}

// ── Main render ─────────────────────────────────────────────────────

pub fn render(f: &mut Frame, app: &mut App) {
    let full_area = f.area();

    let has_tabs = app.tabs.len() > 1;

    let chunks = if has_tabs {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Min(3),    // panels
                Constraint::Length(1), // status bar
            ])
            .split(full_area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),    // panels
                Constraint::Length(1), // status bar
            ])
            .split(full_area)
    };

    let (tab_bar_area, panel_chunk, status_area) = if has_tabs {
        (Some(chunks[0]), chunks[1], chunks[2])
    } else {
        (None, chunks[0], chunks[1])
    };

    if let Some(area) = tab_bar_area {
        render_tab_bar(f, app, area);
    }

    // Build horizontal layout based on panel layout + tree
    let layout = app.layout;
    let visible_count = layout.count();
    let (tree_area, panel_areas) = build_panel_layout(app.show_tree, layout, panel_chunk);

    let vis_h = panel_areas[0].height.saturating_sub(2) as usize;
    app.visible_height = vis_h;

    // Adjust scroll for all visible panels
    let tab = app.tab_mut();
    for i in 0..visible_count {
        tab.panels[i].adjust_scroll(vis_h);
    }

    // Adjust find scroll before rendering
    if let Some(ref mut fs) = app.find_state {
        let popup = util::centered_rect(80, 75, full_area);
        let inner_h = popup.height.saturating_sub(4) as usize;
        let results_h = inner_h.saturating_sub(1);
        fs.adjust_scroll(results_h);
    }

    // Trigger async tree rebuild when needed
    if app.show_tree {
        let current_path = app.tab().active_panel().path.clone();
        let current_hidden = app.tab().active_panel().show_hidden;
        let needs_rebuild = app.tree_dirty
            || app.tree_last_path.as_ref() != Some(&current_path)
            || app.tree_last_hidden != current_hidden;
        if needs_rebuild && app.tree_load_rx.is_none() {
            app.spawn_rebuild_tree();
        }
        // If not focused, auto-position cursor on current dir
        if !app.tree_focused
            && let Some(idx) = app.tree_data.iter().position(|l| l.is_current)
        {
            app.tree_selected = idx;
        }
    }

    if let Some(area) = tree_area {
        // Adjust tree scroll
        let tree_vis_h = area.height.saturating_sub(2) as usize;
        if tree_vis_h > 0 {
            let focus_idx = app.tree_selected;
            if focus_idx < app.tree_scroll {
                app.tree_scroll = focus_idx;
            } else if focus_idx >= app.tree_scroll + tree_vis_h {
                app.tree_scroll = focus_idx - tree_vis_h + 1;
            }
        }
        tree::render_tree(f, app, area);
    }

    let ctx = RenderContext {
        visual_marks: &app.visual_marks,
        dir_sizes: &app.dir_sizes,
        register: app.register.as_ref(),
        git_statuses: &app.git_statuses,
        theme: &app.theme,
        is_select_mode: app.mode == Mode::Select,
    };

    let panels_active = !app.tree_focused;
    let tab = app.tab();
    let active_idx = tab.active;

    // Determine how many panels to render as file panels vs preview
    let preview_replaces_last = app.preview_mode && visible_count >= 2;
    let file_panel_count = if preview_replaces_last {
        visible_count - 1
    } else {
        visible_count
    };

    // Render file panels
    for i in 0..file_panel_count {
        let phantoms = app.phantoms_for(&tab.panels[i].path);
        panel::render_panel(
            f,
            &tab.panels[i],
            panel_areas[i],
            panels_active && i == active_idx,
            phantoms,
            &ctx,
        );
    }

    // Render preview in the last slot if preview mode is on
    if preview_replaces_last {
        let last = visible_count - 1;
        preview::render_preview(f, &app.preview, panel_areas[last], ctx.theme);
    }

    status::render_status(f, app, status_area);

    // Overlays on top of everything
    if app.mode == Mode::Help {
        overlays::render_help(f, &app.theme, full_area);
    }

    if matches!(app.mode, Mode::Preview | Mode::PreviewSearch) {
        overlays::render_preview_popup(f, app, full_area);
    }

    if app.mode == Mode::ThemePicker {
        overlays::render_theme_picker(f, app, full_area);
    }

    if app.mode == Mode::Bookmarks {
        overlays::render_bookmarks(f, app, full_area);
    }

    if matches!(app.mode, Mode::Rename | Mode::Create | Mode::BookmarkAdd | Mode::BookmarkRename) {
        overlays::render_input_popup(f, app, full_area);
    }

    if app.mode == Mode::Chmod {
        overlays::render_chmod_popup(f, app, full_area);
    }

    if app.mode == Mode::Chown {
        overlays::render_chown_picker(f, app, full_area);
    }

    if app.mode == Mode::Info {
        overlays::render_info_popup(f, app, full_area);
    }

    if app.mode == Mode::Confirm {
        overlays::render_confirm_popup(f, app, full_area);
    }

    if let Some(ref fs) = app.find_state {
        find_overlay::render_find(f, fs, &app.theme, full_area);
    }

    if let Some(hints) = app.which_key_hints() {
        overlays::render_which_key(
            f,
            &hints,
            app.pending_key.unwrap_or(' '),
            &app.theme,
            full_area,
        );
    }
}

/// Build panel layout areas based on layout mode and tree visibility.
fn build_panel_layout(
    show_tree: bool,
    layout: PanelLayout,
    chunk: Rect,
) -> (Option<Rect>, Vec<Rect>) {
    let constraints: Vec<Constraint> = match (show_tree, layout) {
        (false, PanelLayout::Single) => vec![Constraint::Percentage(100)],
        (false, PanelLayout::Dual) => {
            vec![Constraint::Percentage(50), Constraint::Percentage(50)]
        }
        (false, PanelLayout::Triple) => vec![
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ],
        (true, PanelLayout::Single) => {
            vec![Constraint::Percentage(30), Constraint::Percentage(70)]
        }
        (true, PanelLayout::Dual) => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(38),
            Constraint::Percentage(37),
        ],
        (true, PanelLayout::Triple) => vec![
            Constraint::Percentage(20),
            Constraint::Percentage(27),
            Constraint::Percentage(27),
            Constraint::Percentage(26),
        ],
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(chunk);

    if show_tree {
        let tree_area = cols[0];
        let panel_areas: Vec<Rect> = cols[1..].to_vec();
        (Some(tree_area), panel_areas)
    } else {
        (None, cols.to_vec())
    }
}

// ── Tab bar ─────────────────────────────────────────────────────────

fn render_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let mut spans = Vec::new();

    for (i, tab) in app.tabs.iter().enumerate() {
        let dir_name = tab
            .active_panel()
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".into());

        let is_active = i == app.active_tab;

        if is_active {
            spans.push(Span::styled(
                format!("  {}: {dir_name} ", i + 1),
                Style::default().fg(t.bg).bg(t.blue),
            ));
            spans.push(Span::styled(
                SEP_RIGHT,
                Style::default().fg(t.blue).bg(t.status_bg),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {}: {dir_name} ", i + 1),
                Style::default().fg(t.fg_dim).bg(t.status_bg),
            ));
            if i < app.tabs.len() - 1 {
                spans.push(Span::styled(
                    "\u{2502}",
                    Style::default().fg(t.border_inactive).bg(t.status_bg),
                ));
            }
        }
    }

    // Fill rest with status bg
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let remaining = (area.width as usize).saturating_sub(used);
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining),
            Style::default().bg(t.status_bg),
        ));
    }

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}
