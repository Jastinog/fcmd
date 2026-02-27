use std::cmp::Ordering;

/// Natural sort comparison on byte slices (case-insensitive, UTF-8 aware).
///
/// Based on Martin Pool's strnatcmp algorithm. Compares strings so that
/// embedded numbers are sorted numerically: "file2" < "file10".
/// Non-ASCII bytes are decoded as UTF-8 codepoints for correct comparison.
pub fn natsort(left: &[u8], right: &[u8]) -> Ordering {
    let mut li = 0;
    let mut ri = 0;

    loop {
        // Skip whitespace
        while li < left.len() && left[li].is_ascii_whitespace() {
            li += 1;
        }
        while ri < right.len() && right[ri].is_ascii_whitespace() {
            ri += 1;
        }

        let lc = left.get(li);
        let rc = right.get(ri);

        match (lc, rc) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&lb), Some(&rb)) => {
                if lb.is_ascii_digit() && rb.is_ascii_digit() {
                    let ord = compare_numbers(left, right, &mut li, &mut ri);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else if lb.is_ascii() && rb.is_ascii() {
                    // Fast path for ASCII
                    let la = lb.to_ascii_lowercase();
                    let ra = rb.to_ascii_lowercase();
                    match la.cmp(&ra) {
                        Ordering::Equal => {}
                        ord => return ord,
                    }
                    li += 1;
                    ri += 1;
                } else {
                    // Decode UTF-8 codepoints for correct multi-byte comparison
                    let (lch, llen) = decode_utf8_char(&left[li..]);
                    let (rch, rlen) = decode_utf8_char(&right[ri..]);
                    let lo: char = lch.to_lowercase().next().unwrap_or(lch);
                    let ro: char = rch.to_lowercase().next().unwrap_or(rch);
                    match lo.cmp(&ro) {
                        Ordering::Equal => {}
                        ord => return ord,
                    }
                    li += llen;
                    ri += rlen;
                }
            }
        }
    }
}

/// Decode one UTF-8 character from a byte slice. Returns (char, byte_length).
/// Falls back to U+FFFD for invalid sequences.
fn decode_utf8_char(bytes: &[u8]) -> (char, usize) {
    match std::str::from_utf8(bytes) {
        Ok(s) => match s.chars().next() {
            Some(c) => (c, c.len_utf8()),
            None => ('\u{FFFD}', 1),
        },
        Err(e) => {
            let valid_len = e.valid_up_to();
            if valid_len > 0 {
                let s = &bytes[..valid_len];
                let s = unsafe { std::str::from_utf8_unchecked(s) };
                match s.chars().next() {
                    Some(c) => (c, c.len_utf8()),
                    None => ('\u{FFFD}', 1),
                }
            } else {
                ('\u{FFFD}', 1)
            }
        }
    }
}

/// Compare numeric subsequences starting at the current positions.
fn compare_numbers(left: &[u8], right: &[u8], li: &mut usize, ri: &mut usize) -> Ordering {
    // Skip leading zeros and count them
    let lz_left = skip_zeros(left, li);
    let lz_right = skip_zeros(right, ri);

    // Compare digit sequences
    let mut result = Ordering::Equal;
    loop {
        let ld = left.get(*li).filter(|b| b.is_ascii_digit());
        let rd = right.get(*ri).filter(|b| b.is_ascii_digit());

        match (ld, rd) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&lb), Some(&rb)) => {
                // Record the first difference, but keep going to compare lengths
                if result == Ordering::Equal {
                    result = lb.cmp(&rb);
                }
                *li += 1;
                *ri += 1;
            }
        }
    }

    // Same number of significant digits — use digit comparison, then leading zeros
    if result != Ordering::Equal {
        return result;
    }

    // Fewer leading zeros comes first (e.g., "7" < "07" < "007")
    lz_left.cmp(&lz_right)
}

/// Skip leading zeros, advancing the index. Returns the count of zeros skipped.
fn skip_zeros(s: &[u8], i: &mut usize) -> usize {
    let start = *i;
    while *i < s.len() && s[*i] == b'0' {
        *i += 1;
    }
    *i - start
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut v: Vec<&str>) -> Vec<&str> {
        v.sort_by(|a, b| natsort(a.as_bytes(), b.as_bytes()));
        v
    }

    #[test]
    fn basic_numeric() {
        assert_eq!(
            sorted(vec!["file10", "file2", "file1", "file20"]),
            vec!["file1", "file2", "file10", "file20"]
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            sorted(vec!["Banana", "apple", "Cherry"]),
            vec!["apple", "Banana", "Cherry"]
        );
    }

    #[test]
    fn leading_zeros() {
        assert_eq!(
            sorted(vec!["item007", "item07", "item7"]),
            vec!["item7", "item07", "item007"]
        );
    }

    #[test]
    fn mixed_alpha_numeric() {
        assert_eq!(
            sorted(vec!["abc10def", "abc2def", "abc1def"]),
            vec!["abc1def", "abc2def", "abc10def"]
        );
    }

    #[test]
    fn pure_numbers() {
        assert_eq!(
            sorted(vec!["100", "20", "3", "1"]),
            vec!["1", "3", "20", "100"]
        );
    }

    #[test]
    fn dates() {
        assert_eq!(
            sorted(vec!["2023-10-01", "2023-2-15", "2023-1-01"]),
            vec!["2023-1-01", "2023-2-15", "2023-10-01"]
        );
    }

    #[test]
    fn empty_and_whitespace() {
        assert_eq!(natsort(b"", b""), Ordering::Equal);
        assert_eq!(natsort(b"  a", b"a"), Ordering::Equal);
    }

    #[test]
    fn no_numbers() {
        assert_eq!(
            sorted(vec!["delta", "alpha", "charlie", "bravo"]),
            vec!["alpha", "bravo", "charlie", "delta"]
        );
    }

    #[test]
    fn dotfiles() {
        assert_eq!(
            sorted(vec![".config", ".bashrc", ".zshrc", ".abc"]),
            vec![".abc", ".bashrc", ".config", ".zshrc"]
        );
    }

    #[test]
    fn version_numbers() {
        assert_eq!(
            sorted(vec!["v1.10.0", "v1.2.0", "v1.9.0", "v1.1.0"]),
            vec!["v1.1.0", "v1.2.0", "v1.9.0", "v1.10.0"]
        );
    }

    #[test]
    fn cyrillic_names() {
        assert_eq!(
            sorted(vec!["файл", "альфа", "бета"]),
            vec!["альфа", "бета", "файл"]
        );
    }

    #[test]
    fn mixed_ascii_and_unicode() {
        assert_eq!(
            sorted(vec!["zzz", "ааа", "aaa"]),
            vec!["aaa", "zzz", "ааа"]
        );
    }
}
