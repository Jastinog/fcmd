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
        "\u{2503}{}\u{2591}{}\u{2503}",
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
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}
