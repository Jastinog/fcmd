pub fn format_bytes(b: u64) -> String {
    if b < 1024 {
        format!("{b}B")
    } else if b < 1024 * 1024 {
        format!("{:.1}K", b as f64 / 1024.0)
    } else if b < 1024 * 1024 * 1024 {
        format!("{:.1}M", b as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", b as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

pub fn progress_bar(pct: u8, width: usize) -> String {
    let filled = (pct as usize * width / 100).min(width);
    let empty = width - filled;
    format!(
        "\u{2503}{}{}\u{2503}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty),
    )
}

pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(|c| c.to_lowercase()).collect();
    let t: Vec<char> = text.chars().flat_map(|c| c.to_lowercase()).collect();
    glob_match_inner(&p, &t)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // '*' matches zero chars, or consume one char of text
            glob_match_inner(&pattern[1..], text)
                || (!text.is_empty() && glob_match_inner(pattern, &text[1..]))
        }
        (Some('?'), Some(_)) => glob_match_inner(&pattern[1..], &text[1..]),
        (Some(a), Some(b)) if *a == *b => glob_match_inner(&pattern[1..], &text[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_ranges() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(1023), "1023B");
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(1536), "1.5K");
        assert_eq!(format_bytes(1024 * 1024), "1.0M");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0G");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0G");
    }

    #[test]
    fn format_duration_seconds_and_minutes() {
        use std::time::Duration;
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(59)), "59s");
        assert_eq!(format_duration(Duration::from_secs(60)), "1m00s");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m30s");
        assert_eq!(format_duration(Duration::from_secs(3661)), "61m01s");
    }

    #[test]
    fn progress_bar_boundaries() {
        let bar = progress_bar(0, 10);
        assert!(bar.contains(&"\u{2591}".repeat(10)));

        let bar = progress_bar(100, 10);
        assert!(bar.contains(&"\u{2588}".repeat(10)));

        let bar = progress_bar(50, 10);
        assert!(bar.contains(&"\u{2588}".repeat(5)));
    }

    #[test]
    fn glob_exact_match() {
        assert!(glob_match("hello", "hello"));
        assert!(glob_match("hello", "HELLO")); // case insensitive
        assert!(!glob_match("hello", "world"));
    }

    #[test]
    fn glob_star_wildcard() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "MOD.RS"));
        assert!(!glob_match("*.rs", "main.py"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("src*", "src/main.rs"));
    }

    #[test]
    fn glob_question_mark() {
        assert!(glob_match("?.rs", "a.rs"));
        assert!(!glob_match("?.rs", "ab.rs"));
        assert!(!glob_match("?.rs", ".rs"));
    }

    #[test]
    fn glob_combined() {
        assert!(glob_match("*test*", "my_test_file"));
        assert!(glob_match("?oo*", "foobar"));
        assert!(!glob_match("?oo*", "oobar"));
    }

    #[test]
    fn glob_empty() {
        assert!(glob_match("", ""));
        assert!(!glob_match("", "a"));
        assert!(glob_match("*", ""));
    }
}

pub fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut child = if cfg!(target_os = "macos") {
        std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?
    } else {
        std::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()?
    };
    if let Some(ref mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    // Poll with timeout to avoid hanging if clipboard tool is stuck
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait()? {
            Some(_) => return Ok(()),
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "clipboard command timed out",
                ));
            }
            None => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
}
