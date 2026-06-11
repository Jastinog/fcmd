//! Syntax highlighting for the viewer.
//!
//! Highlighting is *stateful* across lines, so it's computed for the whole file
//! at once (off the UI thread) and cached as per-line colored char ranges. The
//! renderer overlays these colors; search-match highlighting takes precedence on
//! lines that contain matches.

use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::Color;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// Skip highlighting for files larger than this (line count) to keep loads snappy.
const MAX_HL_LINES: usize = 20_000;

/// A colored char range `[start, end)` within a single logical line.
#[derive(Clone, Copy, Debug)]
pub struct HlSpan {
    pub start: usize,
    pub end: usize,
    pub color: Color,
}

/// Per-line syntax colors. `lines[i]` covers logical line `i` contiguously.
pub struct HlCache {
    pub lines: Vec<Vec<HlSpan>>,
}

fn syntax_set() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_nonewlines)
}

fn theme_set() -> &'static ThemeSet {
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    TS.get_or_init(ThemeSet::load_defaults)
}

/// Compute syntax highlighting for `lines` (sanitized, newline-free). Returns
/// `None` when the file is too large or highlighting fails — callers fall back
/// to plain rendering. `dark` selects a light/dark syntect theme to match the UI.
pub fn highlight(lines: &[String], path: &Path, dark: bool) -> Option<HlCache> {
    if lines.is_empty() || lines.len() > MAX_HL_LINES {
        return None;
    }

    let ps = syntax_set();
    let ts = theme_set();

    let syntax = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(|e| ps.find_syntax_by_extension(e))
        .or_else(|| ps.find_syntax_by_first_line(&lines[0]))
        .unwrap_or_else(|| ps.find_syntax_plain_text());

    // Plain text won't produce useful colors — skip the work entirely.
    if syntax.name == ps.find_syntax_plain_text().name {
        return None;
    }

    let theme_name = if dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    };
    let theme = ts.themes.get(theme_name)?;

    let mut h = HighlightLines::new(syntax, theme);
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let regions = h.highlight_line(line, ps).ok()?;
        let mut spans = Vec::with_capacity(regions.len());
        let mut char_idx = 0;
        for (style, text) in regions {
            let n = text.chars().count();
            if n > 0 {
                let c = style.foreground;
                spans.push(HlSpan {
                    start: char_idx,
                    end: char_idx + n,
                    color: Color::Rgb(c.r, c.g, c.b),
                });
                char_idx += n;
            }
        }
        out.push(spans);
    }

    Some(HlCache { lines: out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn highlights_rust_source() {
        let lines: Vec<String> = "fn main() {\n    let x = 5;\n}"
            .lines()
            .map(|s| s.to_string())
            .collect();
        let cache = highlight(&lines, Path::new("test.rs"), true).expect("rust highlights");
        assert_eq!(cache.lines.len(), 3);
        // The first line should be split into multiple colored regions.
        assert!(cache.lines[0].len() > 1);
        // Spans cover the line contiguously from offset 0.
        assert_eq!(cache.lines[0][0].start, 0);
    }

    #[test]
    fn unknown_extension_returns_none() {
        let lines = vec!["just some plain text".to_string()];
        assert!(highlight(&lines, Path::new("notes.xyzzy"), true).is_none());
    }

    #[test]
    fn empty_returns_none() {
        assert!(highlight(&[], Path::new("a.rs"), true).is_none());
    }
}
