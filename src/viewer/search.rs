//! In-file incremental search for the viewer.
//!
//! Case-insensitive substring search over the loaded lines, producing a list of
//! match positions that the renderer highlights and `n`/`N` step through.

/// Cap on collected matches so a degenerate query (e.g. a single space in a huge
/// file) can't blow up memory or stall the UI.
const MAX_MATCHES: usize = 10_000;

/// A single match: `(line index, byte offset within the line)`.
pub type Match = (usize, usize);

#[derive(Default)]
pub struct Search {
    /// Raw query as typed by the user.
    pub query: String,
    /// All match positions, in document order.
    pub matches: Vec<Match>,
    /// Index into `matches` of the currently-focused match.
    pub current: usize,
}

impl Search {
    pub fn new() -> Self {
        Search::default()
    }

    pub fn is_active(&self) -> bool {
        !self.matches.is_empty()
    }

    pub fn current_match(&self) -> Option<Match> {
        self.matches.get(self.current).copied()
    }

    /// Smart-case: case-sensitive only when the query contains an uppercase char.
    pub fn case_sensitive(&self) -> bool {
        self.query.chars().any(|c| c.is_uppercase())
    }

    /// Fold a string according to smart-case (identity when case-sensitive).
    pub fn fold(&self, s: &str) -> String {
        if self.case_sensitive() {
            s.to_string()
        } else {
            s.to_lowercase()
        }
    }

    /// Clear query and matches.
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current = 0;
    }

    /// Recompute all matches for the current query against `lines`.
    /// Smart-case (see [`Search::case_sensitive`]). Resets the current match.
    pub fn recompute(&mut self, lines: &[String]) {
        self.matches.clear();
        self.current = 0;
        let query = self.fold(&self.query);
        if query.is_empty() {
            return;
        }
        'outer: for (line_idx, line) in lines.iter().enumerate() {
            let line_folded = self.fold(line);
            let mut start = 0;
            while let Some(pos) = line_folded[start..].find(&query) {
                self.matches.push((line_idx, start + pos));
                if self.matches.len() >= MAX_MATCHES {
                    break 'outer;
                }
                start += pos + query.len();
            }
        }
    }

    /// Move to the next match, wrapping. Returns false if there are none.
    pub fn advance(&mut self) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        self.current = (self.current + 1) % self.matches.len();
        true
    }

    /// Move to the previous match, wrapping. Returns false if there are none.
    pub fn retreat(&mut self) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        let len = self.matches.len();
        self.current = (self.current + len - 1) % len;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recompute_finds_case_insensitive_positions() {
        let mut s = Search::new();
        s.query = "hello".into();
        s.recompute(&lines(&["Hello World", "hello again", "no match here"]));
        assert_eq!(s.matches, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn recompute_multiple_per_line() {
        let mut s = Search::new();
        s.query = "ab".into();
        s.recompute(&lines(&["ababab"]));
        assert_eq!(s.matches, vec![(0, 0), (0, 2), (0, 4)]);
    }

    #[test]
    fn empty_query_clears_matches() {
        let mut s = Search::new();
        s.query.clear();
        s.recompute(&lines(&["some text"]));
        assert!(s.matches.is_empty());
    }

    #[test]
    fn advance_and_retreat_wrap() {
        let mut s = Search::new();
        s.query = "x".into();
        s.recompute(&lines(&["x", "x", "x"]));
        assert_eq!(s.current, 0);
        assert!(s.advance());
        assert_eq!(s.current, 1);
        s.current = 2;
        assert!(s.advance());
        assert_eq!(s.current, 0); // wrapped
        assert!(s.retreat());
        assert_eq!(s.current, 2); // wrapped back
    }

    #[test]
    fn smart_case_lowercase_query_is_insensitive() {
        let mut s = Search::new();
        s.query = "foo".into();
        assert!(!s.case_sensitive());
        s.recompute(&lines(&["FOO", "foo", "Foo"]));
        assert_eq!(s.matches, vec![(0, 0), (1, 0), (2, 0)]);
    }

    #[test]
    fn smart_case_uppercase_query_is_sensitive() {
        let mut s = Search::new();
        s.query = "Foo".into();
        assert!(s.case_sensitive());
        s.recompute(&lines(&["FOO", "foo", "Foo"]));
        assert_eq!(s.matches, vec![(2, 0)]);
    }

    #[test]
    fn advance_no_matches_returns_false() {
        let mut s = Search::new();
        assert!(!s.advance());
        assert!(!s.retreat());
    }
}
