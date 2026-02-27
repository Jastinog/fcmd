use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::App;
use crate::util::format_bytes;

fn format_datetime(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Local, Utc};
    let dt: DateTime<Local> = DateTime::<Utc>::from(time).into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

pub(in crate::ui) fn render_conflict_popup(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.conflict_info.as_ref() {
        Some(i) => i,
        None => return,
    };
    let t = &app.theme;

    let w = 54u16.min(area.width.saturating_sub(4)).max(30);
    let h = 14u16.min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(area.x + x, area.y + y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.yellow))
        .title(" File Exists ")
        .title_style(Style::default().fg(t.yellow))
        .style(Style::default().bg(t.bg));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let iw = inner.width as usize;

    // Build lines
    let mut lines: Vec<Line> = Vec::new();

    let src_name = info
        .src_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dst_name = info
        .dst_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Source info
    let src_size_str = format_bytes(info.src_size);
    let src_date_str = info
        .src_modified
        .map(format_datetime)
        .unwrap_or_else(|| "unknown".into());
    let newer_src = match (info.src_modified, info.dst_modified) {
        (Some(s), Some(d)) => s > d,
        _ => false,
    };
    let newer_dst = match (info.src_modified, info.dst_modified) {
        (Some(s), Some(d)) => d > s,
        _ => false,
    };

    lines.push(Line::from(Span::styled(
        " Source:",
        Style::default().fg(t.fg_dim),
    )));

    let icon = if info.is_dir { " \u{f115} " } else { " \u{f016} " };
    let max_name = iw.saturating_sub(icon.len() + 1);
    let name_disp = if src_name.chars().count() > max_name {
        let truncated: String = src_name.chars().take(max_name.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    } else {
        src_name
    };
    lines.push(Line::from(vec![
        Span::styled(icon, Style::default().fg(t.cyan)),
        Span::styled(name_disp, Style::default().fg(t.cyan)),
    ]));

    let newer_indicator = if newer_src { " newer" } else { "" };
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {src_size_str}  {src_date_str}"),
            Style::default().fg(t.fg_dim),
        ),
        Span::styled(newer_indicator, Style::default().fg(t.green)),
    ]));

    lines.push(Line::from(""));

    // Destination info
    let dst_size_str = format_bytes(info.dst_size);
    let dst_date_str = info
        .dst_modified
        .map(format_datetime)
        .unwrap_or_else(|| "unknown".into());

    lines.push(Line::from(Span::styled(
        " Existing:",
        Style::default().fg(t.fg_dim),
    )));

    let max_name = iw.saturating_sub(icon.len() + 1);
    let dst_disp = if dst_name.chars().count() > max_name {
        let truncated: String = dst_name.chars().take(max_name.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    } else {
        dst_name
    };
    lines.push(Line::from(vec![
        Span::styled(icon, Style::default().fg(t.fg)),
        Span::styled(dst_disp, Style::default().fg(t.fg)),
    ]));

    let newer_indicator = if newer_dst { " newer" } else { "" };
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {dst_size_str}  {dst_date_str}"),
            Style::default().fg(t.fg_dim),
        ),
        Span::styled(newer_indicator, Style::default().fg(t.green)),
    ]));

    // Separator
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(iw),
        Style::default().fg(t.border_inactive),
    )));

    // Button rows: 2 rows x 3 cols
    // Row 1: [O]verwrite [S]kip  [A]ll
    // Row 2: skip al[N]  ne[W]er [Esc]
    let buttons = [
        ("[O]verwrite", 0),
        ("[S]kip", 1),
        ("overwrite [A]ll", 2),
        ("skip al[N]", 3),
        ("ne[W]er", 4),
        ("[Esc] abort", 5),
    ];

    for row in 0..2 {
        let mut spans = Vec::new();
        spans.push(Span::raw(" "));
        for col in 0..3 {
            let idx = row * 3 + col;
            let (label, btn_idx) = buttons[idx];
            let is_selected = app.conflict_selected == btn_idx;

            let style = if is_selected {
                Style::default()
                    .fg(t.bg)
                    .bg(t.yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.yellow)
            };
            spans.push(Span::styled(label, style));
            if col < 2 {
                spans.push(Span::styled("  ", Style::default()));
            }
        }
        lines.push(Line::from(spans));
    }

    // Render all lines
    let content_area = Rect::new(inner.x, inner.y, inner.width, inner.height);
    f.render_widget(Paragraph::new(lines), content_area);
}
