use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// Geometry captured during each render so mouse events can be hit-tested back to
/// the UI element under the pointer. Recomputed every frame in `ui::render`.
#[derive(Default)]
pub struct MouseRegions {
    /// Screen row of the tab bar (clickable tab spans live here).
    pub tab_bar_row: u16,
    /// Clickable tab spans: (x_start, x_end_exclusive, tab_index).
    pub tabs: Vec<(u16, u16, usize)>,
    /// Visible file panels of the active tab.
    pub panels: Vec<PanelRegion>,
}

/// A file panel's clickable content area. Visible row `inner.y + r` maps to the
/// entry at `panel.offset + r`.
pub struct PanelRegion {
    /// Panel index within the active tab.
    pub index: usize,
    /// Inner content area (excludes the border).
    pub inner: Rect,
}

/// Max gap (ms) between two left-clicks at the same cell to count as a double-click.
const DOUBLE_CLICK_MS: u128 = 400;
/// Rows moved per scroll-wheel notch.
const SCROLL_STEP: usize = 3;

impl App {
    pub fn handle_mouse(&mut self, m: MouseEvent) {
        match m.kind {
            MouseEventKind::ScrollDown => self.mouse_scroll(m.column, m.row, true),
            MouseEventKind::ScrollUp => self.mouse_scroll(m.column, m.row, false),
            MouseEventKind::Down(MouseButton::Left) => self.mouse_left_down(m.column, m.row),
            _ => {}
        }
    }

    /// Mouse navigation (clicks, panel scroll) is only active in the panel-facing
    /// modes; overlays and prompts own their own input.
    fn mouse_nav_enabled(&self) -> bool {
        matches!(self.mode, Mode::Normal | Mode::Visual | Mode::Select)
    }

    /// Panel index whose content area contains the given cell, if any.
    fn panel_at(&self, col: u16, row: u16) -> Option<usize> {
        self.mouse_regions
            .panels
            .iter()
            .find(|p| {
                col >= p.inner.x
                    && col < p.inner.x + p.inner.width
                    && row >= p.inner.y
                    && row < p.inner.y + p.inner.height
            })
            .map(|p| p.index)
    }

    fn mouse_scroll(&mut self, col: u16, row: u16, down: bool) {
        // The viewer owns the whole screen: the wheel scrolls its content.
        if matches!(self.mode, Mode::Viewer | Mode::ViewerSearch | Mode::ViewerGoto) {
            let visible = self.viewer_visible();
            if let Some(v) = self.viewer.as_mut() {
                if down {
                    v.move_down(SCROLL_STEP, visible);
                } else {
                    v.move_up(SCROLL_STEP, visible);
                }
            }
            return;
        }

        if !self.mouse_nav_enabled() {
            return;
        }

        // Scroll the panel under the pointer without stealing focus from the active one.
        if let Some(idx) = self.panel_at(col, row) {
            let panel = &mut self.tab_mut().panels[idx];
            for _ in 0..SCROLL_STEP {
                if down {
                    panel.move_down();
                } else {
                    panel.move_up();
                }
            }
        }
    }

    fn mouse_left_down(&mut self, col: u16, row: u16) {
        // Tab bar: click a tab to switch to it.
        if row == self.mouse_regions.tab_bar_row {
            if let Some(&(_, _, ti)) = self
                .mouse_regions
                .tabs
                .iter()
                .find(|&&(x0, x1, _)| col >= x0 && col < x1)
            {
                self.goto_tab(ti);
            }
            return;
        }

        if !self.mouse_nav_enabled() {
            return;
        }

        // Locate the clicked panel and the visible row within it.
        let hit = self
            .mouse_regions
            .panels
            .iter()
            .find(|p| {
                col >= p.inner.x
                    && col < p.inner.x + p.inner.width
                    && row >= p.inner.y
                    && row < p.inner.y + p.inner.height
            })
            .map(|p| (p.index, (row - p.inner.y) as usize));
        let (pidx, vis_row) = match hit {
            Some(h) => h,
            None => return,
        };

        // Focus the clicked panel.
        self.tree_focused = false;
        self.tab_mut().active = pidx;

        let panel = self.active_panel_mut();
        let target = panel.offset + vis_row;
        if target >= panel.entries.len() {
            // Clicked past the last entry (empty space): don't move the cursor and
            // drop any pending double-click so empty clicks never activate anything.
            self.last_click = None;
            return;
        }
        panel.selected = target;

        // Double-click on the same cell activates the entry (Normal mode only).
        let now = Instant::now();
        let is_double = self.last_click.is_some_and(|(t, c, r)| {
            c == col && r == row && now.duration_since(t).as_millis() <= DOUBLE_CLICK_MS
        });
        if is_double && self.mode == Mode::Normal {
            self.last_click = None;
            self.activate_selected();
        } else {
            self.last_click = Some((now, col, row));
        }
    }

    /// Open / enter the currently selected entry, mirroring the `Enter` key:
    /// directories are entered, `..` goes to the parent, archives open the archive
    /// overlay, and other files open in the viewer.
    fn activate_selected(&mut self) {
        let info = self
            .active_panel()
            .selected_entry()
            .map(|e| (e.is_dir, e.name == "..", e.path.clone()));
        match info {
            Some((false, false, path)) => {
                if crate::archive::is_archive(&path) {
                    self.open_archive();
                } else {
                    self.open_viewer(path);
                }
            }
            Some((_, true, _)) => self.go_parent_async(),
            Some((true, false, _)) => self.enter_dir_async(),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn ev(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// A single content panel occupying most of the screen, content starting at row 1.
    fn one_panel_region(app: &mut App) {
        app.mouse_regions.panels = vec![PanelRegion {
            index: 0,
            inner: Rect {
                x: 0,
                y: 1,
                width: 40,
                height: 20,
            },
        }];
    }

    #[tokio::test]
    async fn click_selects_entry_under_pointer() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        one_panel_region(&mut app);

        // Row 1 is ".." (index 0); row 3 is the third entry (index 2 = "b.txt").
        app.handle_mouse(ev(MouseEventKind::Down(MouseButton::Left), 5, 3));
        assert_eq!(app.active_panel().selected, 2);
    }

    #[tokio::test]
    async fn click_past_last_entry_keeps_cursor() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        one_panel_region(&mut app);
        app.active_panel_mut().selected = 1;

        // Row 10 is well past the 2 entries (.., a.txt): cursor must not move.
        app.handle_mouse(ev(MouseEventKind::Down(MouseButton::Left), 5, 10));
        assert_eq!(app.active_panel().selected, 1);
        assert!(app.last_click.is_none());
    }

    #[tokio::test]
    async fn click_focuses_the_clicked_panel() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.mouse_regions.panels = vec![
            PanelRegion {
                index: 0,
                inner: Rect {
                    x: 0,
                    y: 1,
                    width: 20,
                    height: 20,
                },
            },
            PanelRegion {
                index: 1,
                inner: Rect {
                    x: 20,
                    y: 1,
                    width: 20,
                    height: 20,
                },
            },
        ];
        assert_eq!(app.tab().active, 0);
        app.handle_mouse(ev(MouseEventKind::Down(MouseButton::Left), 25, 2));
        assert_eq!(app.tab().active, 1);
    }

    #[tokio::test]
    async fn scroll_down_moves_cursor_of_panel_under_pointer() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt", "c.txt"]);
        let mut app = App::new_for_test(entries);
        one_panel_region(&mut app);
        app.active_panel_mut().selected = 0;

        app.handle_mouse(ev(MouseEventKind::ScrollDown, 5, 5));
        // SCROLL_STEP is 3, clamped to the last entry (index 3 of 4 entries).
        assert_eq!(app.active_panel().selected, 3);

        app.handle_mouse(ev(MouseEventKind::ScrollUp, 5, 5));
        assert_eq!(app.active_panel().selected, 0);
    }

    #[tokio::test]
    async fn click_outside_any_region_is_ignored() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        // No regions registered at all.
        app.active_panel_mut().selected = 0;
        app.handle_mouse(ev(MouseEventKind::Down(MouseButton::Left), 100, 100));
        assert_eq!(app.active_panel().selected, 0);
    }

    #[tokio::test]
    async fn tab_bar_click_switches_tab() {
        let entries = crate::app::make_test_entries(&["a.txt"]);
        let mut app = App::new_for_test(entries);
        app.tabs.push(crate::app::Tab::new(PathBuf::from("/test")));
        app.mouse_regions.tab_bar_row = 0;
        app.mouse_regions.tabs = vec![(0, 10, 0), (10, 20, 1)];
        assert_eq!(app.active_tab, 0);

        app.handle_mouse(ev(MouseEventKind::Down(MouseButton::Left), 12, 0));
        assert_eq!(app.active_tab, 1);
    }

    #[tokio::test]
    async fn mouse_ignored_in_overlay_modes() {
        let entries = crate::app::make_test_entries(&["a.txt", "b.txt"]);
        let mut app = App::new_for_test(entries);
        one_panel_region(&mut app);
        app.mode = Mode::Command;
        app.active_panel_mut().selected = 0;

        app.handle_mouse(ev(MouseEventKind::Down(MouseButton::Left), 5, 2));
        assert_eq!(app.active_panel().selected, 0);
    }
}
