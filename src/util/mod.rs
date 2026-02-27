pub mod icons;
pub mod natsort;

use std::path::PathBuf;

/// Returns `~/.config/fcmd` as the config directory (XDG-style, cross-platform).
pub fn config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("fcmd"))
}

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

pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(|c| c.to_lowercase()).collect();
    let t: Vec<char> = text.chars().flat_map(|c| c.to_lowercase()).collect();
    glob_match_iter(&p, &t)
}

/// Iterative glob matching with O(n*m) worst case (no exponential backtracking).
fn glob_match_iter(pattern: &[char], text: &[char]) -> bool {
    let mut pi = 0; // pattern index
    let mut ti = 0; // text index
    let mut star_pi = usize::MAX; // last '*' position in pattern
    let mut star_ti = 0; // text position when last '*' was hit

    while ti < text.len() {
        if pi < pattern.len() && pattern[pi] == '*' {
            // Record '*' and try matching zero characters
            star_pi = pi;
            star_ti = ti;
            pi += 1;
            // Skip consecutive '*'
            while pi < pattern.len() && pattern[pi] == '*' {
                pi += 1;
            }
        } else if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if star_pi != usize::MAX {
            // Backtrack: advance the text position past the '*' by one more char
            star_ti += 1;
            ti = star_ti;
            pi = star_pi + 1;
            // Skip consecutive '*'
            while pi < pattern.len() && pattern[pi] == '*' {
                pi += 1;
            }
        } else {
            return false;
        }
    }

    // Skip trailing '*'
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }

    pi == pattern.len()
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

    #[test]
    fn glob_multiple_stars() {
        assert!(glob_match("*test*file*", "my_test_data_file_v2"));
        assert!(!glob_match("*test*file*", "my_data_v2"));
    }

    #[test]
    fn glob_consecutive_stars() {
        // Multiple consecutive stars should behave like one star
        assert!(glob_match("**", "anything"));
        assert!(glob_match("***test***", "mytest"));
    }

    #[test]
    fn glob_only_question_marks() {
        assert!(glob_match("???", "abc"));
        assert!(!glob_match("???", "ab"));
        assert!(!glob_match("???", "abcd"));
    }

    #[test]
    fn glob_star_at_end() {
        assert!(glob_match("hello*", "hello"));
        assert!(glob_match("hello*", "hello world"));
    }

    #[test]
    fn glob_star_at_start() {
        assert!(glob_match("*.txt", ".txt"));
        assert!(glob_match("*world", "world"));
    }

    #[test]
    fn glob_pattern_longer_than_text() {
        assert!(!glob_match("abcdef", "abc"));
        assert!(!glob_match("a?c?e", "ace"));
    }

    #[test]
    fn glob_unicode() {
        assert!(glob_match("*.рс", "файл.рс"));
        assert!(glob_match("日*", "日本語"));
    }

    #[test]
    fn glob_no_exponential_backtracking() {
        // This pattern would cause exponential backtracking with naive recursive impl
        let pattern = "*a*a*a*a*a*a*a*a*b";
        let text = "aaaaaaaaaaaaaaaaaaaaaaaaa"; // no 'b' at end
        assert!(!glob_match(pattern, text));
    }
}

pub async fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = if cfg!(target_os = "macos") {
        tokio::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?
    } else if std::env::var("WAYLAND_DISPLAY").is_ok() {
        tokio::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?
    } else {
        tokio::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?
    };
    if let Some(ref mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }
    match tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await {
        Ok(result) => {
            result?;
            Ok(())
        }
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "clipboard command timed out",
        )),
    }
}
