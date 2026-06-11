//! Display-row layout: maps logical content lines onto the rows actually drawn on
//! screen, so soft-wrap, scrolling, search, and highlighting all share one
//! coordinate system.
//!
//! With wrap off there is exactly one display row per logical line (1:1, so a
//! display-row index equals a logical-line index). With wrap on a long logical
//! line spans several display rows, each covering a `[start, end)` char slice.

use unicode_width::UnicodeWidthChar;

/// One drawn row: a `[start, end)` char slice of logical line `logical`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayRow {
    pub logical: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Default)]
pub struct Layout {
    /// Content width (in cells) this layout was built for. Excludes the gutter.
    pub width: usize,
    pub wrap: bool,
    pub rows: Vec<DisplayRow>,
    /// Maps a logical line index to the index of its first display row.
    pub line_to_row: Vec<usize>,
}

impl Layout {
    pub fn empty() -> Self {
        Layout::default()
    }

    /// Build a layout for `lines` at the given content `width`.
    pub fn build(lines: &[String], width: usize, wrap: bool) -> Self {
        let width = width.max(1);
        let mut rows = Vec::with_capacity(lines.len());
        let mut line_to_row = Vec::with_capacity(lines.len());

        for (logical, line) in lines.iter().enumerate() {
            line_to_row.push(rows.len());

            if !wrap {
                rows.push(DisplayRow {
                    logical,
                    start: 0,
                    end: line.chars().count(),
                });
                continue;
            }

            let chars: Vec<char> = line.chars().collect();
            if chars.is_empty() {
                rows.push(DisplayRow { logical, start: 0, end: 0 });
                continue;
            }

            let mut start = 0;
            let mut w = 0;
            let mut idx = 0;
            while idx < chars.len() {
                let cw = UnicodeWidthChar::width(chars[idx]).unwrap_or(0);
                // Break before a char that would overflow the row — unless the row
                // is still empty (a single too-wide char must occupy its own row).
                if w + cw > width && idx > start {
                    rows.push(DisplayRow { logical, start, end: idx });
                    start = idx;
                    w = 0;
                    continue;
                }
                w += cw;
                idx += 1;
            }
            rows.push(DisplayRow { logical, start, end: chars.len() });
        }

        Layout { width, wrap, rows, line_to_row }
    }

    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }

    /// First display row of a logical line (clamped to a valid row index).
    pub fn row_of_line(&self, logical: usize) -> usize {
        self.line_to_row.get(logical).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_wrap_is_one_row_per_line() {
        let l = Layout::build(&lines(&["abc", "defgh", ""]), 3, false);
        assert_eq!(l.rows.len(), 3);
        assert_eq!(l.rows[1], DisplayRow { logical: 1, start: 0, end: 5 });
        assert_eq!(l.line_to_row, vec![0, 1, 2]);
    }

    #[test]
    fn wrap_splits_long_line() {
        // width 3: "abcdefg" -> "abc","def","g"
        let l = Layout::build(&lines(&["abcdefg"]), 3, true);
        assert_eq!(l.rows.len(), 3);
        assert_eq!(l.rows[0], DisplayRow { logical: 0, start: 0, end: 3 });
        assert_eq!(l.rows[1], DisplayRow { logical: 0, start: 3, end: 6 });
        assert_eq!(l.rows[2], DisplayRow { logical: 0, start: 6, end: 7 });
        assert_eq!(l.line_to_row, vec![0]);
    }

    #[test]
    fn wrap_empty_line_keeps_one_row() {
        let l = Layout::build(&lines(&["", "x"]), 5, true);
        assert_eq!(l.rows.len(), 2);
        assert_eq!(l.rows[0], DisplayRow { logical: 0, start: 0, end: 0 });
        assert_eq!(l.rows[1], DisplayRow { logical: 1, start: 0, end: 1 });
    }

    #[test]
    fn wrap_multiple_lines_maps_correctly() {
        // "aaaa" width 2 -> 2 rows; "b" -> 1 row
        let l = Layout::build(&lines(&["aaaa", "b"]), 2, true);
        assert_eq!(l.rows.len(), 3);
        assert_eq!(l.line_to_row, vec![0, 2]);
        assert_eq!(l.row_of_line(1), 2);
    }

    #[test]
    fn wrap_wide_char_gets_own_row() {
        // width 1, CJK char has width 2 -> still one char per row
        let l = Layout::build(&lines(&["\u{4e16}\u{754c}"]), 1, true);
        assert_eq!(l.rows.len(), 2);
        assert_eq!(l.rows[0], DisplayRow { logical: 0, start: 0, end: 1 });
        assert_eq!(l.rows[1], DisplayRow { logical: 0, start: 1, end: 2 });
    }
}
