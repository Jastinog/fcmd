//! Byte-level search for the hex viewer.
//!
//! The query is interpreted as a hex byte pattern when it is all hex digits and
//! whitespace with an even nibble count (e.g. `ff d8 ff` or `ffd8ff`); a leading
//! `"` forces a literal ASCII search (so words like `cafe` that also look like
//! hex can still be matched as text); anything else is matched as ASCII. Matches
//! are byte offsets into the loaded hex window.

/// Cap on collected matches so a degenerate query can't blow up memory.
const MAX_MATCHES: usize = 10_000;

#[derive(Default)]
pub struct HexSearch {
    /// Raw query as typed by the user.
    pub query: String,
    /// Byte offsets of every match, in order.
    pub matches: Vec<usize>,
    /// Index into `matches` of the focused match.
    pub current: usize,
    /// Length of the parsed needle (so the renderer can size hit ranges).
    pub needle_len: usize,
}

/// If `q` is a hex byte pattern (no leading `"`, only hex digits + whitespace,
/// even nibble count), return its whitespace-stripped form.
fn hex_compact(q: &str) -> Option<String> {
    if q.starts_with('"') {
        return None;
    }
    let compact: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    (!compact.is_empty()
        && compact.len().is_multiple_of(2)
        && compact.bytes().all(|b| b.is_ascii_hexdigit()))
    .then_some(compact)
}

/// Parse a query into the byte sequence to search for, or `None` when it is
/// empty / not yet meaningful.
pub fn parse_needle(q: &str) -> Option<Vec<u8>> {
    if let Some(rest) = q.strip_prefix('"') {
        // Explicit literal ASCII search.
        return (!rest.is_empty()).then(|| rest.as_bytes().to_vec());
    }
    if let Some(compact) = hex_compact(q) {
        let bytes = (0..compact.len())
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(&compact[i..i + 2], 16).ok())
            .collect();
        return Some(bytes);
    }
    // Fall back to a literal ASCII search of the raw query.
    (!q.is_empty()).then(|| q.as_bytes().to_vec())
}

/// Whether `q` is currently interpreted as a hex byte pattern (for UI labels).
pub fn is_hex_query(q: &str) -> bool {
    hex_compact(q).is_some()
}

impl HexSearch {
    pub fn new() -> Self {
        HexSearch::default()
    }

    pub fn is_active(&self) -> bool {
        !self.matches.is_empty()
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current = 0;
        self.needle_len = 0;
    }

    /// Byte offset of the focused match, if any.
    pub fn current_offset(&self) -> Option<usize> {
        self.matches.get(self.current).copied()
    }

    /// Recompute all matches for the current query against `bytes`. Non-overlapping
    /// (after a hit, scanning resumes past it), matching the text search.
    pub fn recompute(&mut self, bytes: &[u8]) {
        self.matches.clear();
        self.current = 0;
        let Some(needle) = parse_needle(&self.query) else {
            self.needle_len = 0;
            return;
        };
        self.needle_len = needle.len();
        let n = needle.len();
        if n == 0 || n > bytes.len() {
            return;
        }
        let mut i = 0;
        while i + n <= bytes.len() {
            if &bytes[i..i + n] == needle.as_slice() {
                self.matches.push(i);
                if self.matches.len() >= MAX_MATCHES {
                    break;
                }
                i += n;
            } else {
                i += 1;
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

    /// Visible hit ranges as `(start, end, is_current)` in absolute byte offsets,
    /// limited to matches that intersect `[from, to)`.
    pub fn hits_in(&self, from: usize, to: usize) -> Vec<(usize, usize, bool)> {
        if self.needle_len == 0 {
            return Vec::new();
        }
        self.matches
            .iter()
            .enumerate()
            .filter_map(|(idx, &m)| {
                let end = m + self.needle_len;
                (m < to && end > from).then_some((m, end, idx == self.current))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_pattern() {
        assert_eq!(parse_needle("ff d8 ff"), Some(vec![0xff, 0xd8, 0xff]));
        assert_eq!(parse_needle("ffd8ff"), Some(vec![0xff, 0xd8, 0xff]));
    }

    #[test]
    fn odd_nibbles_fall_back_to_ascii() {
        // "f" is a single nibble — treated as the ASCII byte 'f'.
        assert_eq!(parse_needle("f"), Some(vec![b'f']));
    }

    #[test]
    fn quote_forces_ascii() {
        assert_eq!(parse_needle("\"cafe"), Some(b"cafe".to_vec()));
        // Without the quote, "cafe" is a hex pattern.
        assert_eq!(parse_needle("cafe"), Some(vec![0xca, 0xfe]));
    }

    #[test]
    fn non_hex_text_is_ascii() {
        assert_eq!(parse_needle("hello"), Some(b"hello".to_vec()));
    }

    #[test]
    fn recompute_finds_non_overlapping() {
        let mut s = HexSearch::new();
        s.query = "ab ab".into(); // bytes ab ab
        s.recompute(&[0xab, 0xab, 0xab, 0xab]);
        // Non-overlapping: matches at 0 and 2.
        assert_eq!(s.matches, vec![0, 2]);
    }

    #[test]
    fn recompute_ascii_match() {
        let mut s = HexSearch::new();
        s.query = "\"PK".into();
        s.recompute(b"xxPKzz");
        assert_eq!(s.matches, vec![2]);
        assert_eq!(s.needle_len, 2);
    }

    #[test]
    fn advance_retreat_wrap() {
        let mut s = HexSearch::new();
        s.query = "00".into();
        s.recompute(&[0, 1, 0, 1, 0]);
        assert_eq!(s.matches, vec![0, 2, 4]);
        assert!(s.advance());
        assert_eq!(s.current, 1);
        s.current = 2;
        assert!(s.advance());
        assert_eq!(s.current, 0);
        assert!(s.retreat());
        assert_eq!(s.current, 2);
    }

    #[test]
    fn hits_in_window() {
        let mut s = HexSearch::new();
        s.query = "00".into();
        s.recompute(&[0, 0, 1, 0]); // matches at 0, 3 (non-overlapping: 0, then skip to 2, byte 2 is 1, byte 3 is 0 -> need n=1)
        // needle is single byte 0x00; matches at 0,1,3
        assert_eq!(s.matches, vec![0, 1, 3]);
        let hits = s.hits_in(1, 3);
        // offsets 1 (current=false) intersect [1,3); offset 0 ends at 1 -> excluded; 3 starts at 3 -> excluded
        assert_eq!(hits, vec![(1, 2, false)]);
    }
}
