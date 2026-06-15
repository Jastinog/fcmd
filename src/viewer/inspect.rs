//! Data inspector for the hex viewer.
//!
//! Interprets the bytes at the byte cursor as integers (signed/unsigned, both
//! endiannesses), floats, and timestamps (Unix epoch and Windows FILETIME). Pure
//! and allocation-light; the viewer's side panel renders the returned rows. This
//! is the manual-reverse-engineering aid for picking apart on-disk structures
//! without leaving the dump.

use chrono::{TimeZone, Utc};

/// A single inspector row: a short fixed label and its formatted value.
pub struct Field {
    pub label: &'static str,
    pub value: String,
}

fn field(label: &'static str, value: String) -> Field {
    Field { label, value }
}

/// Printable representation of a single byte for the `chr` row.
fn char_repr(b: u8) -> String {
    match b {
        0x20..=0x7e => format!("'{}'", b as char),
        b'\n' => "\\n".into(),
        b'\r' => "\\r".into(),
        b'\t' => "\\t".into(),
        0 => "\\0".into(),
        _ => "·".into(),
    }
}

/// Format `secs` since the Unix epoch as UTC, or `--` when out of range.
fn unix_secs(secs: i64) -> String {
    match Utc.timestamp_opt(secs, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%SZ").to_string(),
        None => "--".into(),
    }
}

/// Format a Windows `FILETIME` (100-ns ticks since 1601-01-01 UTC) as UTC, or
/// `--` when zero / out of range.
fn filetime(ticks: u64) -> String {
    if ticks == 0 {
        return "--".into();
    }
    // Ticks since 1601 → seconds since 1970. 11_644_473_600 s between the epochs.
    let secs = (ticks / 10_000_000) as i64 - 11_644_473_600;
    unix_secs(secs)
}

/// Interpret up to 8 bytes starting at `off`. Interpretations needing more bytes
/// than remain in `bytes` (near EOF) are simply omitted. Returns label/value rows
/// in the order the side panel renders them top-to-bottom.
pub fn describe(bytes: &[u8], off: usize) -> Vec<Field> {
    let avail = bytes.len().saturating_sub(off);
    if avail == 0 {
        return vec![field("off", format!("0x{off:x} ({off})"))];
    }
    let mut buf = [0u8; 8];
    let n = avail.min(8);
    buf[..n].copy_from_slice(&bytes[off..off + n]);
    let have = |k: usize| avail >= k;

    let u8v = buf[0];
    let le16 = u16::from_le_bytes([buf[0], buf[1]]);
    let be16 = u16::from_be_bytes([buf[0], buf[1]]);
    let le32 = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let be32 = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let le64 = u64::from_le_bytes(buf);
    let be64 = u64::from_be_bytes(buf);
    let f32le = f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let f64le = f64::from_le_bytes(buf);

    let mut rows = vec![
        field("off", format!("0x{off:x} ({off})")),
        field("u8/i8", format!("{u8v} / {}", u8v as i8)),
        field("hex", format!("{u8v:02x}")),
        field("bin", format!("{u8v:08b}")),
        field("chr", char_repr(u8v)),
    ];
    if have(2) {
        rows.push(field("u16 le", format!("{le16}")));
        rows.push(field("u16 be", format!("{be16}")));
        rows.push(field("i16 le", format!("{}", le16 as i16)));
    }
    if have(4) {
        rows.push(field("u32 le", format!("{le32}")));
        rows.push(field("u32 be", format!("{be32}")));
        rows.push(field("i32 le", format!("{}", le32 as i32)));
        rows.push(field("f32 le", format!("{f32le:.6}")));
    }
    if have(8) {
        rows.push(field("u64 le", format!("{le64}")));
        rows.push(field("u64 be", format!("{be64}")));
        rows.push(field("i64 le", format!("{}", le64 as i64)));
        rows.push(field("f64 le", format!("{f64le:.6}")));
    }
    if have(4) {
        rows.push(field("unix32", unix_secs(le32 as i64)));
    }
    if have(8) {
        rows.push(field("unix64", unix_secs(le64 as i64)));
        rows.push(field("ftime", filetime(le64)));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val<'a>(rows: &'a [Field], label: &str) -> Option<&'a str> {
        rows.iter()
            .find(|f| f.label == label)
            .map(|f| f.value.as_str())
    }

    #[test]
    fn little_and_big_endian_integers() {
        let bytes = [0x0e, 0x1f, 0xba, 0x0e, 0x00, 0x00, 0x00, 0x00];
        let rows = describe(&bytes, 0);
        assert_eq!(val(&rows, "u8/i8"), Some("14 / 14"));
        assert_eq!(val(&rows, "u16 le"), Some("7950"));
        assert_eq!(val(&rows, "u16 be"), Some("3615"));
        assert_eq!(val(&rows, "u32 le"), Some(&*format!("{}", 0x0eba1f0eu32)));
        assert_eq!(val(&rows, "u32 be"), Some(&*format!("{}", 0x0e1fba0eu32)));
    }

    #[test]
    fn truncated_near_eof_omits_wide_fields() {
        let bytes = [0xffu8, 0x00]; // only 2 bytes from offset 0
        let rows = describe(&bytes, 0);
        assert!(val(&rows, "u16 le").is_some());
        assert!(val(&rows, "u32 le").is_none());
        assert!(val(&rows, "u64 le").is_none());
        // At the very last byte only the 1-byte views remain.
        let rows = describe(&bytes, 1);
        assert!(val(&rows, "u8/i8").is_some());
        assert!(val(&rows, "u16 le").is_none());
    }

    #[test]
    fn past_end_offset_yields_only_offset() {
        let bytes = [0u8; 4];
        let rows = describe(&bytes, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "off");
    }

    #[test]
    fn unix_and_filetime_format() {
        // 2009-02-13 23:31:30 UTC.
        assert_eq!(unix_secs(1_234_567_890), "2009-02-13 23:31:30Z");
        // FILETIME for the Unix epoch start.
        assert_eq!(filetime(116_444_736_000_000_000), "1970-01-01 00:00:00Z");
        assert_eq!(filetime(0), "--");
    }
}
