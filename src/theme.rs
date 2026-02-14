use ratatui::style::Color;
use serde::Deserialize;

#[derive(Deserialize)]
struct RawTheme {
    bg: String,
    bg_light: String,
    fg: String,
    fg_dim: String,

    red: String,
    green: String,
    yellow: String,
    blue: String,
    magenta: String,
    cyan: String,
    orange: String,

    border_active: String,
    border_inactive: String,
    status_bg: String,
    cursor_line: String,

    dir_color: String,
    symlink_color: String,
    file_color: String,
}

#[derive(Clone)]
pub struct Theme {
    pub bg: Color,
    pub bg_light: Color,
    pub fg: Color,
    pub fg_dim: Color,

    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub orange: Color,

    pub border_active: Color,
    pub border_inactive: Color,
    pub status_bg: Color,
    pub cursor_line: Color,

    pub dir_color: Color,
    pub symlink_color: Color,
    pub file_color: Color,
}

fn parse_hex(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

impl RawTheme {
    fn resolve_color(&self, value: &str) -> Color {
        // Try hex first
        if let Some(c) = parse_hex(value) {
            return c;
        }
        // Otherwise treat as a reference to another field
        match value {
            "bg" => self.resolve_color(&self.bg),
            "bg_light" => self.resolve_color(&self.bg_light),
            "fg" => self.resolve_color(&self.fg),
            "fg_dim" => self.resolve_color(&self.fg_dim),
            "red" => self.resolve_color(&self.red),
            "green" => self.resolve_color(&self.green),
            "yellow" => self.resolve_color(&self.yellow),
            "blue" => self.resolve_color(&self.blue),
            "magenta" => self.resolve_color(&self.magenta),
            "cyan" => self.resolve_color(&self.cyan),
            "orange" => self.resolve_color(&self.orange),
            _ => Color::White, // fallback
        }
    }

    fn into_theme(self) -> Theme {
        let bg = self.resolve_color(&self.bg);
        let bg_light = self.resolve_color(&self.bg_light);
        let fg = self.resolve_color(&self.fg);
        let fg_dim = self.resolve_color(&self.fg_dim);
        let red = self.resolve_color(&self.red);
        let green = self.resolve_color(&self.green);
        let yellow = self.resolve_color(&self.yellow);
        let blue = self.resolve_color(&self.blue);
        let magenta = self.resolve_color(&self.magenta);
        let cyan = self.resolve_color(&self.cyan);
        let orange = self.resolve_color(&self.orange);
        let border_active = self.resolve_color(&self.border_active);
        let border_inactive = self.resolve_color(&self.border_inactive);
        let status_bg = self.resolve_color(&self.status_bg);
        let cursor_line = self.resolve_color(&self.cursor_line);
        let dir_color = self.resolve_color(&self.dir_color);
        let symlink_color = self.resolve_color(&self.symlink_color);
        let file_color = self.resolve_color(&self.file_color);

        Theme {
            bg,
            bg_light,
            fg,
            fg_dim,
            red,
            green,
            yellow,
            blue,
            magenta,
            cyan,
            orange,
            border_active,
            border_inactive,
            status_bg,
            cursor_line,
            dir_color,
            symlink_color,
            file_color,
        }
    }
}

impl Theme {
    pub fn default_theme() -> Self {
        Theme {
            bg: Color::Rgb(11, 14, 20),
            bg_light: Color::Rgb(15, 19, 26),
            fg: Color::Rgb(191, 189, 182),
            fg_dim: Color::Rgb(86, 91, 102),
            red: Color::Rgb(240, 113, 120),
            green: Color::Rgb(170, 217, 76),
            yellow: Color::Rgb(230, 180, 80),
            blue: Color::Rgb(89, 194, 255),
            magenta: Color::Rgb(210, 166, 255),
            cyan: Color::Rgb(57, 186, 230),
            orange: Color::Rgb(255, 143, 64),
            border_active: Color::Rgb(89, 194, 255),
            border_inactive: Color::Rgb(60, 65, 74),
            status_bg: Color::Rgb(17, 21, 28),
            cursor_line: Color::Rgb(27, 58, 91),
            dir_color: Color::Rgb(89, 194, 255),
            symlink_color: Color::Rgb(230, 182, 115),
            file_color: Color::Rgb(191, 189, 182),
        }
    }

    fn load(path: &std::path::Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let raw: RawTheme = toml::from_str(&content).ok()?;
        Some(raw.into_theme())
    }

    pub fn load_by_name(name: &str) -> Option<Self> {
        let themes_dir = crate::util::config_dir()?.join("themes");
        let path = themes_dir.join(format!("{name}.toml"));
        Self::load(&path)
    }

    pub fn list_available() -> Vec<String> {
        let themes_dir = match crate::util::config_dir() {
            Some(d) => d.join("themes"),
            None => return Vec::new(),
        };
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&themes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "toml")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        names
    }

    pub fn from_config() -> Self {
        let config_dir = match crate::util::config_dir() {
            Some(d) => d,
            None => return Self::default_theme(),
        };

        // Always deploy any missing builtin themes
        Self::ensure_default_theme(&config_dir);

        // Try loading config.toml for theme name
        let config_path = config_dir.join("config.toml");
        let theme_name = Self::read_theme_name(&config_path);

        if let Some(name) = &theme_name {
            let theme_path = config_dir.join("themes").join(format!("{name}.toml"));
            if let Some(theme) = Self::load(&theme_path) {
                return theme;
            }
        }

        Self::default_theme()
    }

    fn read_theme_name(path: &std::path::Path) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        let table: toml::Table = toml::from_str(&content).ok()?;
        table.get("theme")?.as_str().map(|s| s.to_string())
    }

    pub fn ensure_builtin_themes() {
        if let Some(config_dir) = crate::util::config_dir() {
            Self::ensure_default_theme(&config_dir);
        }
    }

    fn ensure_default_theme(config_dir: &std::path::Path) {
        let themes_dir = config_dir.join("themes");
        let _ = std::fs::create_dir_all(&themes_dir);
        for (name, content) in BUILTIN_THEMES {
            let path = themes_dir.join(name);
            if !path.exists() {
                let _ = std::fs::write(&path, content);
            }
        }
    }
}

const BUILTIN_THEMES: &[(&str, &str)] = &[
    ("ayu-dark.toml", include_str!("../themes/ayu-dark.toml")),
    (
        "ayu-mirage.toml",
        include_str!("../themes/ayu-mirage.toml"),
    ),
    ("andromeda.toml", include_str!("../themes/andromeda.toml")),
    (
        "aura-dark.toml",
        include_str!("../themes/aura-dark.toml"),
    ),
    (
        "bluloco-dark.toml",
        include_str!("../themes/bluloco-dark.toml"),
    ),
    (
        "carbonfox.toml",
        include_str!("../themes/carbonfox.toml"),
    ),
    (
        "catppuccin-frappe.toml",
        include_str!("../themes/catppuccin-frappe.toml"),
    ),
    (
        "catppuccin-macchiato.toml",
        include_str!("../themes/catppuccin-macchiato.toml"),
    ),
    (
        "catppuccin-mocha.toml",
        include_str!("../themes/catppuccin-mocha.toml"),
    ),
    ("cobalt2.toml", include_str!("../themes/cobalt2.toml")),
    (
        "dark-plus.toml",
        include_str!("../themes/dark-plus.toml"),
    ),
    ("dracula.toml", include_str!("../themes/dracula.toml")),
    ("everblush.toml", include_str!("../themes/everblush.toml")),
    (
        "everforest-dark.toml",
        include_str!("../themes/everforest-dark.toml"),
    ),
    (
        "fleet-dark.toml",
        include_str!("../themes/fleet-dark.toml"),
    ),
    (
        "github-dark.toml",
        include_str!("../themes/github-dark.toml"),
    ),
    (
        "github-dimmed.toml",
        include_str!("../themes/github-dimmed.toml"),
    ),
    (
        "gruvbox-dark.toml",
        include_str!("../themes/gruvbox-dark.toml"),
    ),
    ("horizon.toml", include_str!("../themes/horizon.toml")),
    ("iceberg.toml", include_str!("../themes/iceberg.toml")),
    ("kanagawa.toml", include_str!("../themes/kanagawa.toml")),
    (
        "kanagawa-dragon.toml",
        include_str!("../themes/kanagawa-dragon.toml"),
    ),
    (
        "material-ocean.toml",
        include_str!("../themes/material-ocean.toml"),
    ),
    ("melange.toml", include_str!("../themes/melange.toml")),
    (
        "modus-vivendi.toml",
        include_str!("../themes/modus-vivendi.toml"),
    ),
    (
        "monokai-pro.toml",
        include_str!("../themes/monokai-pro.toml"),
    ),
    ("moonfly.toml", include_str!("../themes/moonfly.toml")),
    ("moonlight.toml", include_str!("../themes/moonlight.toml")),
    (
        "night-owl.toml",
        include_str!("../themes/night-owl.toml"),
    ),
    ("nightfox.toml", include_str!("../themes/nightfox.toml")),
    ("nord.toml", include_str!("../themes/nord.toml")),
    ("one-dark.toml", include_str!("../themes/one-dark.toml")),
    ("oxocarbon.toml", include_str!("../themes/oxocarbon.toml")),
    ("palenight.toml", include_str!("../themes/palenight.toml")),
    ("poimandres.toml", include_str!("../themes/poimandres.toml")),
    (
        "rose-pine.toml",
        include_str!("../themes/rose-pine.toml"),
    ),
    (
        "rose-pine-moon.toml",
        include_str!("../themes/rose-pine-moon.toml"),
    ),
    (
        "shades-of-purple.toml",
        include_str!("../themes/shades-of-purple.toml"),
    ),
    (
        "solarized-dark.toml",
        include_str!("../themes/solarized-dark.toml"),
    ),
    ("sonokai.toml", include_str!("../themes/sonokai.toml")),
    ("spaceduck.toml", include_str!("../themes/spaceduck.toml")),
    (
        "synthwave84.toml",
        include_str!("../themes/synthwave84.toml"),
    ),
    (
        "tokyo-night.toml",
        include_str!("../themes/tokyo-night.toml"),
    ),
    (
        "tokyonight-storm.toml",
        include_str!("../themes/tokyonight-storm.toml"),
    ),
    ("vesper.toml", include_str!("../themes/vesper.toml")),
    (
        "vitesse-dark.toml",
        include_str!("../themes/vitesse-dark.toml"),
    ),
    ("zenburn.toml", include_str!("../themes/zenburn.toml")),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_themes_parse() {
        for (name, content) in BUILTIN_THEMES {
            let raw: RawTheme =
                toml::from_str(content).unwrap_or_else(|e| panic!("{name}: TOML parse error: {e}"));
            let _theme = raw.into_theme();
        }
    }

    #[test]
    fn parse_hex_valid() {
        assert_eq!(parse_hex("#000000"), Some(Color::Rgb(0, 0, 0)));
        assert_eq!(parse_hex("#ffffff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_hex("#ff8040"), Some(Color::Rgb(255, 128, 64)));
    }

    #[test]
    fn parse_hex_invalid() {
        assert_eq!(parse_hex("000000"), None); // no #
        assert_eq!(parse_hex("#fff"), None); // too short
        assert_eq!(parse_hex("#gggggg"), None); // not hex
        assert_eq!(parse_hex(""), None);
    }

    const TEST_THEME_TOML: &str = r##"
            bg = "#111111"
            bg_light = "#222222"
            fg = "#333333"
            fg_dim = "#444444"
            red = "#ff0000"
            green = "#00ff00"
            yellow = "#ffff00"
            blue = "#0000ff"
            magenta = "#ff00ff"
            cyan = "#00ffff"
            orange = "#ff8000"
            border_active = "#aaaaaa"
            border_inactive = "#bbbbbb"
            status_bg = "#cccccc"
            cursor_line = "#dddddd"
            dir_color = "#eeeeee"
            symlink_color = "#999999"
            file_color = "#888888"
    "##;

    #[test]
    fn resolve_color_hex() {
        let raw: RawTheme = toml::from_str(TEST_THEME_TOML).unwrap();
        assert_eq!(raw.resolve_color("#ff0000"), Color::Rgb(255, 0, 0));
    }

    #[test]
    fn resolve_color_reference() {
        let raw: RawTheme = toml::from_str(
            r##"
            bg = "#111111"
            bg_light = "#222222"
            fg = "#333333"
            fg_dim = "#444444"
            red = "#ff0000"
            green = "#00ff00"
            yellow = "#ffff00"
            blue = "#0000ff"
            magenta = "#ff00ff"
            cyan = "#00ffff"
            orange = "#ff8000"
            border_active = "blue"
            border_inactive = "fg_dim"
            status_bg = "bg"
            cursor_line = "#dddddd"
            dir_color = "cyan"
            symlink_color = "orange"
            file_color = "fg"
            "##,
        )
        .unwrap();
        let theme = raw.into_theme();
        assert_eq!(theme.border_active, Color::Rgb(0, 0, 255));
        assert_eq!(theme.border_inactive, Color::Rgb(68, 68, 68));
        assert_eq!(theme.status_bg, Color::Rgb(17, 17, 17));
        assert_eq!(theme.dir_color, Color::Rgb(0, 255, 255));
        assert_eq!(theme.file_color, Color::Rgb(51, 51, 51));
    }

    #[test]
    fn resolve_color_unknown_fallback() {
        let raw: RawTheme = toml::from_str(TEST_THEME_TOML).unwrap();
        // Unknown reference falls back to Color::White
        assert_eq!(raw.resolve_color("nonexistent"), Color::White);
    }

    #[test]
    fn default_theme_has_distinct_colors() {
        let t = Theme::default_theme();
        // Basic sanity: bg and fg should differ
        assert_ne!(t.bg, t.fg);
        assert_ne!(t.red, t.green);
        assert_ne!(t.blue, t.yellow);
    }
}
