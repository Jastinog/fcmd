use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;
use crate::theme::Theme;
use crate::ui::util::centered_rect;

#[derive(Clone, Copy)]
enum Cell {
    Blank,
    Separator,
    Header(&'static str),
    Binding(&'static str, &'static str),
}

fn push_section(col: &mut Vec<Cell>, header: &'static str, keys: &[(&'static str, &'static str)]) {
    col.push(Cell::Header(header));
    for &(k, d) in keys {
        col.push(Cell::Binding(k, d));
    }
}

fn align(left: &mut Vec<Cell>, right: &mut Vec<Cell>) {
    let target = left.len().max(right.len());
    left.resize(target, Cell::Blank);
    right.resize(target, Cell::Blank);
}

fn build_help_rows() -> (Vec<Cell>, Vec<Cell>) {
    let mut l: Vec<Cell> = Vec::new();
    let mut r: Vec<Cell> = Vec::new();

    // ── Block 1: Navigation | Visual + Select ────────
    push_section(&mut l, " Navigation", &[
        ("j k / \u{2191}\u{2193}", "Move down / up"),
        ("h l / \u{2190}\u{2192}", "Parent / Enter dir"),
        ("Enter", "Open dir or preview"),
        ("gg G", "Top / Bottom"),
        ("Ctrl-d/u", "Half page \u{2195}"),
        ("Ctrl-l/h", "Focus panel \u{2192}/\u{2190}"),
        ("Tab", "Cycle panels"),
        ("~ / -", "Home / Parent"),
    ]);
    push_section(&mut r, "\u{f0ad0} Visual  (v)", &[
        ("j k", "Extend selection"),
        ("G / gg", "Bottom / Top"),
        ("Ctrl-d/u", "Half page \u{2195}"),
        ("y d D p", "Yank/Del/Paste"),
        ("cp co", "Chmod / Chown"),
        ("v Esc", "Exit \u{2192} Normal"),
    ]);
    r.push(Cell::Blank);
    push_section(&mut r, "\u{f0ad0} Select  (Shift-\u{2191}\u{2193})", &[
        ("Shift-\u{2191}/\u{2193}", "Toggle mark & move"),
        ("j k", "Move (keep marks)"),
        ("y d D p", "Yank/Del/Paste"),
        ("cp co", "Chmod / Chown"),
        ("Esc", "Clear \u{2192} Normal"),
    ]);
    align(&mut l, &mut r);
    l.push(Cell::Separator); r.push(Cell::Separator);

    // ── Block 2: Files | Preview + Search ────────────
    push_section(&mut l, "\u{f0214} Files", &[
        ("yy", "Yank (copy to register)"),
        ("dd", "Move to trash"),
        ("dD", "Permanent delete"),
        ("p / P", "Paste here / other"),
        ("yp / yn", "Copy path / name"),
        ("r F2", "Rename"),
        ("a F7", "Create (/ = dir)"),
        ("cp / co", "Chmod / Chown"),
        ("i", "File info"),
        ("o F4", "Open in editor"),
        ("u", "Undo last operation"),
        ("F3", "Preview popup"),
        ("F5 / F6", "Copy / Move to other"),
    ]);
    push_section(&mut r, "\u{f06e} Preview  (Enter / F3)", &[
        ("j k", "Scroll \u{2193}/\u{2191}"),
        ("Ctrl-d/u", "Half page \u{2195}"),
        ("Ctrl-f/b", "Full page \u{2195}"),
        ("G / g", "Bottom / Top"),
        ("/ n N", "Search / Next / Prev"),
        ("o", "Open in editor"),
        ("q Esc", "Close preview"),
    ]);
    r.push(Cell::Blank);
    push_section(&mut r, "\u{f002} Search  (/)", &[
        ("type", "Filter incrementally"),
        ("Enter", "Accept match"),
        ("Esc", "Cancel, restore cursor"),
        ("n / N", "Next / Prev (Normal)"),
    ]);
    align(&mut l, &mut r);
    l.push(Cell::Separator); r.push(Cell::Separator);

    // ── Block 3: Space Leader | Find + Tree ──────────
    push_section(&mut l, "\u{f1720} Space Leader", &[
        ("Sp+p", "Toggle preview panel"),
        ("Sp+t", "Toggle tree sidebar"),
        ("Sp+h", "Toggle hidden files"),
        ("Sp+d", "Calculate dir sizes"),
        ("Sp+, / .", "Find local / global"),
        ("Sp+a / n", "Select all / Unselect"),
        ("Sp+b", "Bookmarks list"),
        ("Sp+?", "This help"),
        ("Sp+s..", "Sort sub-menu"),
        ("Sp+ut", "Toggle transparent"),
        ("Sp+w1/2/3", "Layout 1/2/3 panels"),
    ]);
    push_section(&mut r, "\u{f0b0} Find  (f / F)", &[
        ("type", "Fuzzy filter files"),
        ("\u{2191}/\u{2193}", "Navigate results"),
        ("Tab", "Local \u{2194} Global"),
        ("Enter", "Open selected"),
        ("Esc", "Cancel"),
    ]);
    r.push(Cell::Blank);
    push_section(&mut r, "\u{f1bb} Tree  (Sp+t)", &[
        ("j k", "Move cursor"),
        ("l / h", "Expand / Collapse"),
        ("Enter", "Navigate to entry"),
        ("G / gg", "Bottom / Top"),
        ("Tab", "Return to panel"),
        ("t", "Close tree"),
    ]);
    align(&mut l, &mut r);
    l.push(Cell::Separator); r.push(Cell::Separator);

    // ── Block 4: Sort & Tabs | Bookmarks ─────────────
    push_section(&mut l, "\u{f0493} Sort & Tabs", &[
        ("sn ss", "Name / Size"),
        ("sm sc", "Modified / Created"),
        ("se sr", "Extension / Reverse"),
        ("gt gT", "Next / Prev tab"),
        ("Ctrl-t/w", "New / Close tab"),
        ("J K", "Scroll preview \u{2193}/\u{2191}"),
    ]);
    push_section(&mut r, "\u{f02e} Bookmarks  (B)", &[
        ("j k", "Move cursor"),
        ("Enter", "Go to bookmark"),
        ("a", "Add bookmark"),
        ("e", "Rename bookmark"),
        ("d", "Delete bookmark"),
        ("q Esc", "Close"),
    ]);
    align(&mut l, &mut r);
    l.push(Cell::Separator); r.push(Cell::Separator);

    // ── Block 5: Marks & Selection | Command ─────────
    push_section(&mut l, "\u{f02b} Marks & Selection", &[
        ("m", "Toggle visual mark"),
        ("M", "Jump to next marked"),
        ("'{a-z}", "Go to named mark"),
        ("v V", "Enter visual mode"),
        ("A", "Select all \u{2192} Select"),
        ("+ / -", "Sel / Unsel by pattern"),
        ("*", "Invert selection"),
        ("Shift-\u{2191}/\u{2193}", "Mark entry & move"),
        ("b / B", "Add / List bookmarks"),
        ("T", "Theme picker"),
        ("Ctrl-r", "Refresh panel"),
    ]);
    push_section(&mut r, "\u{f120} Command  (:)", &[
        (":q :quit", "Quit application"),
        (":cd <path>", "Change directory"),
        (":sort ..", "Sort name/size/mod.."),
        (":find ..", "Open fuzzy finder"),
        (":sel ..", "Select by glob"),
        (":unsel ..", "Unselect by glob"),
        (":theme ..", "Load / list themes"),
        (":mark a-z", "Set named mark"),
        (":du", "Directory sizes"),
        (":hidden", "Toggle hidden files"),
        (":tabnew", "New tab"),
    ]);
    align(&mut l, &mut r);

    (l, r)
}

fn render_cell(
    spans: &mut Vec<Span<'static>>,
    cell: Cell,
    col_w: usize,
    key_width: usize,
    t: &Theme,
) {
    match cell {
        Cell::Blank | Cell::Separator => {
            spans.push(Span::raw(" ".repeat(col_w)));
        }
        Cell::Header(header) => {
            let text = format!("  {header}");
            let used = text.chars().count();
            let pad = col_w.saturating_sub(used);
            spans.push(Span::styled(text, Style::default().fg(t.cyan)));
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
        }
        Cell::Binding(key, desc) => {
            let key_text = format!("  {key:<width$}", width = key_width);
            let key_used = key_text.chars().count();
            let desc_space = col_w.saturating_sub(key_used);
            let desc_text: String = if desc.chars().count() > desc_space {
                desc.chars().take(desc_space).collect()
            } else {
                let pad = desc_space.saturating_sub(desc.chars().count());
                format!("{desc}{}", " ".repeat(pad))
            };
            spans.push(Span::styled(key_text, Style::default().fg(t.yellow)));
            spans.push(Span::styled(desc_text, Style::default().fg(t.fg)));
        }
    }
}

pub(in crate::ui) fn render_help(f: &mut Frame, app: &mut App, area: Rect) {
    let t = &app.theme;
    let popup = centered_rect(72, 88, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.cyan))
        .title(" \u{f02d6} Help ")
        .title_style(Style::default().fg(t.cyan))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let (left_cells, right_cells) = build_help_rows();
    let total_rows = left_cells.len();

    let inner_w = inner.width as usize;
    let col_w = inner_w / 2;
    let right_w = inner_w.saturating_sub(col_w);
    let key_width = 14;

    let mut rows: Vec<Line> = Vec::with_capacity(total_rows);
    for (lc, rc) in left_cells.into_iter().zip(right_cells.into_iter()) {
        let line = match (lc, rc) {
            (Cell::Separator, Cell::Separator) => {
                // Full-width dim separator between blocks
                let dashes = "\u{2500}".repeat(inner_w.saturating_sub(2));
                Line::from(Span::styled(
                    format!(" {dashes} "),
                    Style::default().fg(t.border_inactive),
                ))
            }
            _ => {
                let mut spans = Vec::new();
                render_cell(&mut spans, lc, col_w, key_width, t);
                render_cell(&mut spans, rc, right_w, key_width, t);
                Line::from(spans)
            }
        };
        rows.push(line);
    }

    // Reserve 2 lines at bottom: separator + hint
    let list_height = inner.height.saturating_sub(2) as usize;
    let max_scroll = total_rows.saturating_sub(list_height);
    app.help_scroll = app.help_scroll.min(max_scroll);
    let scroll = app.help_scroll;

    let visible: Vec<Line> = rows
        .into_iter()
        .skip(scroll)
        .take(list_height)
        .collect();

    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height as u16);
    f.render_widget(Paragraph::new(visible), list_area);

    // Bottom separator with scroll indicator
    let sep_y = inner.y + list_height as u16;
    let sep_area = Rect::new(inner.x, sep_y, inner.width, 1);
    let sep_text = if total_rows > list_height {
        let pct = if max_scroll == 0 { 100 } else { (scroll * 100) / max_scroll };
        let indicator = format!(" {pct}%");
        let dash_len = inner_w.saturating_sub(indicator.chars().count());
        format!("{}{indicator}", "\u{2500}".repeat(dash_len))
    } else {
        "\u{2500}".repeat(inner_w)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            sep_text,
            Style::default().fg(t.border_inactive),
        ))),
        sep_area,
    );

    // Hint line
    let hint_line = Line::from(vec![
        Span::styled(" j/k", Style::default().fg(t.yellow)),
        Span::styled(" scroll  ", Style::default().fg(t.fg_dim)),
        Span::styled("G/g", Style::default().fg(t.yellow)),
        Span::styled(" bottom/top  ", Style::default().fg(t.fg_dim)),
        Span::styled("q", Style::default().fg(t.yellow)),
        Span::styled(" close", Style::default().fg(t.fg_dim)),
    ]);
    let hint_y = inner.y + inner.height.saturating_sub(1);
    let hint_area = Rect::new(inner.x, hint_y, inner.width, 1);
    f.render_widget(Paragraph::new(hint_line), hint_area);
}
