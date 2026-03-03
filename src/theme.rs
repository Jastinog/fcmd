use ratatui::style::Color;
use serde::Deserialize;

#[derive(Clone)]
pub struct ThemeGroup {
    pub name: &'static str,
    /// Dark theme names in display order.
    pub dark_themes: Vec<String>,
    /// Light theme names in display order.
    pub light_themes: Vec<String>,
}

/// Static group definitions: (group name, dark themes, light themes).
static THEME_GROUP_DEFS: &[(&str, &[&str], &[&str])] = &[
    ("Classic",
     &["dracula", "gruvbox-dark", "monokai", "nord", "solarized-dark"],
     &["gruvbox-light", "nord-light", "one-light", "solarized-light", "papercolor-light"]),
    ("Night City",
     &["tokyo-night", "tokyonight-storm", "tokyonight-moon", "nightfox", "nightfly"],
     &["tokyo-night-day", "dayfox", "vitesse-light", "bluloco-light", "night-owl-light"]),
    ("Pastel",
     &["catppuccin-mocha", "catppuccin-macchiato", "catppuccin-frappe", "carbonfox", "terafox"],
     &["catppuccin-latte", "serendipity-light", "summerfruit-light", "fleet-light", "soft-era"]),
    ("Forest",
     &["everforest-dark", "bamboo", "kanagawa", "forest", "melange"],
     &["everforest-light", "kanagawa-lotus", "melange-light", "modus-operandi", "ayu-light"]),
    ("Minimal",
     &["min-dark", "mellow", "noir", "lackluster", "monochrome"],
     &["alabaster", "quiet-light", "flatwhite", "min-light", "pencil-light"]),
    ("Neon",
     &["synthwave84", "cyberdream", "radical", "retrowave", "poimandres"],
     &["flexoki-light", "noctis-lux", "lucius-light", "zenbones-light", "xcode-light"]),
    ("Retro",
     &["phosphor", "amber", "c64", "bbs", "lcd"],
     &["paper", "sepia", "parchment", "blueprint", "newsprint"]),
    ("Desert",
     &["sahara", "canyon", "dune", "mesa", "nomad"],
     &["sandstone", "adobe", "camel", "mirage", "oasis"]),
    ("Warm",
     &["gruvbox-material", "earthsong", "tender", "palefire", "kanagawa-dragon"],
     &["leuven", "cosmic-latte", "spacemacs-light", "solarized-osaka", "base16-default-light"]),
    ("Ocean",
     &["duskfox", "iceberg", "ayu-dark", "challenger-deep", "midnight"],
     &["oxocarbon-light", "vs-light", "material-light", "winter-is-coming-light", "intellij-light"]),
    ("Dusk",
     &["rose-pine", "rose-pine-moon", "doom-one", "embark", "horizon-dark"],
     &["rose-pine-dawn", "dawnfox", "horizon-light", "edge-light", "github-light"]),
    ("Studio",
     &["github-dark-default", "vscode-dark", "one-dark", "darcula", "material-ocean"],
     &["github-light-default", "github-light-high-contrast", "vscode-light", "sublime-light", "idea-light"]),
];

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
    pub bg_text: Color,
    pub status_bg_orig: Color,

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
        self.resolve_color_depth(value, 0)
    }

    fn resolve_color_depth(&self, value: &str, depth: u8) -> Color {
        if depth > 4 {
            return Color::White;
        }
        if let Some(c) = parse_hex(value) {
            return c;
        }
        match value {
            "bg" => self.resolve_color_depth(&self.bg, depth + 1),
            "bg_light" => self.resolve_color_depth(&self.bg_light, depth + 1),
            "fg" => self.resolve_color_depth(&self.fg, depth + 1),
            "fg_dim" => self.resolve_color_depth(&self.fg_dim, depth + 1),
            "red" => self.resolve_color_depth(&self.red, depth + 1),
            "green" => self.resolve_color_depth(&self.green, depth + 1),
            "yellow" => self.resolve_color_depth(&self.yellow, depth + 1),
            "blue" => self.resolve_color_depth(&self.blue, depth + 1),
            "magenta" => self.resolve_color_depth(&self.magenta, depth + 1),
            "cyan" => self.resolve_color_depth(&self.cyan, depth + 1),
            "orange" => self.resolve_color_depth(&self.orange, depth + 1),
            _ => Color::White,
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
            bg_text: bg,
            status_bg_orig: status_bg,
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
            bg_text: Color::Rgb(11, 14, 20),
            status_bg_orig: Color::Rgb(17, 21, 28),
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

    /// Returns themes organized into named groups with separate dark/light lists.
    /// Any themes not in a static group go into an "Other" group at the end.
    pub fn list_grouped() -> Vec<ThemeGroup> {
        let themes_dir = match crate::util::config_dir() {
            Some(d) => d.join("themes"),
            None => return Vec::new(),
        };

        // Collect names of available .toml files.
        let available: std::collections::HashSet<String> = std::fs::read_dir(&themes_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "toml") {
                    p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();

        let mut groups: Vec<ThemeGroup> = Vec::new();
        let mut assigned: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (group_name, dark_defs, light_defs) in THEME_GROUP_DEFS {
            let dark_themes: Vec<String> = dark_defs
                .iter()
                .filter(|&&n| available.contains(n))
                .map(|&n| { assigned.insert(n.to_string()); n.to_string() })
                .collect();
            let light_themes: Vec<String> = light_defs
                .iter()
                .filter(|&&n| available.contains(n))
                .map(|&n| { assigned.insert(n.to_string()); n.to_string() })
                .collect();
            if !dark_themes.is_empty() || !light_themes.is_empty() {
                groups.push(ThemeGroup { name: group_name, dark_themes, light_themes });
            }
        }

        // Any unassigned themes go to "Other", classified by bg luminance.
        let unassigned: Vec<String> = {
            let mut v: Vec<String> = available
                .into_iter()
                .filter(|n| !assigned.contains(n.as_str()))
                .collect();
            v.sort();
            v
        };
        if !unassigned.is_empty() {
            let mut other_dark = Vec::new();
            let mut other_light = Vec::new();
            for name in &unassigned {
                let path = themes_dir.join(format!("{name}.toml"));
                let is_light = std::fs::read_to_string(&path)
                    .ok()
                    .map(|c| is_light_theme_content(&c))
                    .unwrap_or(false);
                if is_light { other_light.push(name.clone()); } else { other_dark.push(name.clone()); }
            }
            if !other_dark.is_empty() || !other_light.is_empty() {
                groups.push(ThemeGroup { name: "Other", dark_themes: other_dark, light_themes: other_light });
            }
        }

        groups
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

        // Build the canonical set of builtin theme filenames.
        let builtin_names: std::collections::HashSet<&str> =
            BUILTIN_THEMES.iter().map(|(n, _)| *n).collect();

        // Remove any .toml files that are no longer part of the curated set.
        if let Ok(entries) = std::fs::read_dir(&themes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "toml") {
                    if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                        if !builtin_names.contains(fname) {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        // Write any missing builtin themes.
        for (name, content) in BUILTIN_THEMES {
            let path = themes_dir.join(name);
            if !path.exists() {
                let _ = std::fs::write(&path, content);
            }
        }
    }
}

/// Check if a theme's bg color is light (luminance > 128).
fn is_light_theme_content(content: &str) -> bool {
    if let Ok(table) = content.parse::<toml::Table>() {
        if let Some(bg_str) = table.get("bg").and_then(|v| v.as_str()) {
            if let Some(Color::Rgb(r, g, b)) = parse_hex(bg_str) {
                let lum = 0.299 * (r as f64) + 0.587 * (g as f64) + 0.114 * (b as f64);
                return lum > 128.0;
            }
        }
    }
    false
}

const BUILTIN_THEMES: &[(&str, &str)] = &[
    // ── Classic ──
    ("dracula.toml",        include_str!("../themes/dracula.toml")),
    ("gruvbox-dark.toml",   include_str!("../themes/gruvbox-dark.toml")),
    ("monokai.toml",        include_str!("../themes/monokai.toml")),
    ("nord.toml",           include_str!("../themes/nord.toml")),
    ("solarized-dark.toml", include_str!("../themes/solarized-dark.toml")),
    ("gruvbox-light.toml",  include_str!("../themes/gruvbox-light.toml")),
    ("nord-light.toml",     include_str!("../themes/nord-light.toml")),
    ("one-light.toml",      include_str!("../themes/one-light.toml")),
    ("solarized-light.toml",include_str!("../themes/solarized-light.toml")),
    ("papercolor-light.toml",include_str!("../themes/papercolor-light.toml")),
    // ── Tokyo ──
    ("tokyo-night.toml",     include_str!("../themes/tokyo-night.toml")),
    ("tokyonight-storm.toml",include_str!("../themes/tokyonight-storm.toml")),
    ("tokyonight-moon.toml", include_str!("../themes/tokyonight-moon.toml")),
    ("nightfox.toml",        include_str!("../themes/nightfox.toml")),
    ("nightfly.toml",        include_str!("../themes/nightfly.toml")),
    ("tokyo-night-day.toml", include_str!("../themes/tokyo-night-day.toml")),
    ("dayfox.toml",          include_str!("../themes/dayfox.toml")),
    ("vitesse-light.toml",   include_str!("../themes/vitesse-light.toml")),
    ("bluloco-light.toml",   include_str!("../themes/bluloco-light.toml")),
    ("night-owl-light.toml", include_str!("../themes/night-owl-light.toml")),
    // ── Catppuccin ──
    ("catppuccin-mocha.toml",    include_str!("../themes/catppuccin-mocha.toml")),
    ("catppuccin-macchiato.toml",include_str!("../themes/catppuccin-macchiato.toml")),
    ("catppuccin-frappe.toml",   include_str!("../themes/catppuccin-frappe.toml")),
    ("carbonfox.toml",           include_str!("../themes/carbonfox.toml")),
    ("terafox.toml",             include_str!("../themes/terafox.toml")),
    ("catppuccin-latte.toml",    include_str!("../themes/catppuccin-latte.toml")),
    ("serendipity-light.toml",   include_str!("../themes/serendipity-light.toml")),
    ("summerfruit-light.toml",   include_str!("../themes/summerfruit-light.toml")),
    ("fleet-light.toml",         include_str!("../themes/fleet-light.toml")),
    ("soft-era.toml",            include_str!("../themes/soft-era.toml")),
    // ── Forest ──
    ("everforest-dark.toml",include_str!("../themes/everforest-dark.toml")),
    ("bamboo.toml",         include_str!("../themes/bamboo.toml")),
    ("kanagawa.toml",       include_str!("../themes/kanagawa.toml")),
    ("forest.toml",         include_str!("../themes/forest.toml")),
    ("melange.toml",        include_str!("../themes/melange.toml")),
    ("everforest-light.toml",include_str!("../themes/everforest-light.toml")),
    ("kanagawa-lotus.toml", include_str!("../themes/kanagawa-lotus.toml")),
    ("melange-light.toml",  include_str!("../themes/melange-light.toml")),
    ("modus-operandi.toml", include_str!("../themes/modus-operandi.toml")),
    ("ayu-light.toml",      include_str!("../themes/ayu-light.toml")),
    // ── Minimal ──
    ("min-dark.toml",    include_str!("../themes/min-dark.toml")),
    ("mellow.toml",      include_str!("../themes/mellow.toml")),
    ("noir.toml",        include_str!("../themes/noir.toml")),
    ("lackluster.toml",  include_str!("../themes/lackluster.toml")),
    ("monochrome.toml",  include_str!("../themes/monochrome.toml")),
    ("alabaster.toml",   include_str!("../themes/alabaster.toml")),
    ("quiet-light.toml", include_str!("../themes/quiet-light.toml")),
    ("flatwhite.toml",   include_str!("../themes/flatwhite.toml")),
    ("min-light.toml",   include_str!("../themes/min-light.toml")),
    ("pencil-light.toml",include_str!("../themes/pencil-light.toml")),
    // ── Neon ──
    ("synthwave84.toml",   include_str!("../themes/synthwave84.toml")),
    ("cyberdream.toml",    include_str!("../themes/cyberdream.toml")),
    ("radical.toml",       include_str!("../themes/radical.toml")),
    ("retrowave.toml",     include_str!("../themes/retrowave.toml")),
    ("poimandres.toml",    include_str!("../themes/poimandres.toml")),
    ("flexoki-light.toml", include_str!("../themes/flexoki-light.toml")),
    ("noctis-lux.toml",    include_str!("../themes/noctis-lux.toml")),
    ("lucius-light.toml",  include_str!("../themes/lucius-light.toml")),
    ("zenbones-light.toml",include_str!("../themes/zenbones-light.toml")),
    ("xcode-light.toml",   include_str!("../themes/xcode-light.toml")),
    // ── Retro ──
    ("phosphor.toml",  include_str!("../themes/phosphor.toml")),
    ("amber.toml",     include_str!("../themes/amber.toml")),
    ("c64.toml",       include_str!("../themes/c64.toml")),
    ("bbs.toml",       include_str!("../themes/bbs.toml")),
    ("lcd.toml",       include_str!("../themes/lcd.toml")),
    ("paper.toml",     include_str!("../themes/paper.toml")),
    ("sepia.toml",     include_str!("../themes/sepia.toml")),
    ("parchment.toml", include_str!("../themes/parchment.toml")),
    ("blueprint.toml", include_str!("../themes/blueprint.toml")),
    ("newsprint.toml", include_str!("../themes/newsprint.toml")),
    // ── Desert ──
    ("sahara.toml",    include_str!("../themes/sahara.toml")),
    ("canyon.toml",    include_str!("../themes/canyon.toml")),
    ("dune.toml",      include_str!("../themes/dune.toml")),
    ("mesa.toml",      include_str!("../themes/mesa.toml")),
    ("nomad.toml",     include_str!("../themes/nomad.toml")),
    ("sandstone.toml", include_str!("../themes/sandstone.toml")),
    ("adobe.toml",     include_str!("../themes/adobe.toml")),
    ("camel.toml",     include_str!("../themes/camel.toml")),
    ("mirage.toml",    include_str!("../themes/mirage.toml")),
    ("oasis.toml",     include_str!("../themes/oasis.toml")),
    // ── Warm ──
    ("gruvbox-material.toml",include_str!("../themes/gruvbox-material.toml")),
    ("earthsong.toml",       include_str!("../themes/earthsong.toml")),
    ("tender.toml",          include_str!("../themes/tender.toml")),
    ("palefire.toml",        include_str!("../themes/palefire.toml")),
    ("kanagawa-dragon.toml", include_str!("../themes/kanagawa-dragon.toml")),
    ("leuven.toml",          include_str!("../themes/leuven.toml")),
    ("cosmic-latte.toml",    include_str!("../themes/cosmic-latte.toml")),
    ("spacemacs-light.toml", include_str!("../themes/spacemacs-light.toml")),
    ("solarized-osaka.toml", include_str!("../themes/solarized-osaka.toml")),
    ("base16-default-light.toml",include_str!("../themes/base16-default-light.toml")),
    // ── Ocean ──
    ("duskfox.toml",          include_str!("../themes/duskfox.toml")),
    ("iceberg.toml",          include_str!("../themes/iceberg.toml")),
    ("ayu-dark.toml",         include_str!("../themes/ayu-dark.toml")),
    ("challenger-deep.toml",  include_str!("../themes/challenger-deep.toml")),
    ("midnight.toml",         include_str!("../themes/midnight.toml")),
    ("oxocarbon-light.toml",  include_str!("../themes/oxocarbon-light.toml")),
    ("vs-light.toml",         include_str!("../themes/vs-light.toml")),
    ("material-light.toml",   include_str!("../themes/material-light.toml")),
    ("winter-is-coming-light.toml",include_str!("../themes/winter-is-coming-light.toml")),
    ("intellij-light.toml",   include_str!("../themes/intellij-light.toml")),
    // ── Dusk ──
    ("rose-pine.toml",      include_str!("../themes/rose-pine.toml")),
    ("rose-pine-moon.toml", include_str!("../themes/rose-pine-moon.toml")),
    ("doom-one.toml",       include_str!("../themes/doom-one.toml")),
    ("embark.toml",         include_str!("../themes/embark.toml")),
    ("horizon-dark.toml",   include_str!("../themes/horizon-dark.toml")),
    ("rose-pine-dawn.toml", include_str!("../themes/rose-pine-dawn.toml")),
    ("dawnfox.toml",        include_str!("../themes/dawnfox.toml")),
    ("horizon-light.toml",  include_str!("../themes/horizon-light.toml")),
    ("edge-light.toml",     include_str!("../themes/edge-light.toml")),
    ("github-light.toml",   include_str!("../themes/github-light.toml")),
    // ── Pro ──
    ("github-dark-default.toml",       include_str!("../themes/github-dark-default.toml")),
    ("vscode-dark.toml",               include_str!("../themes/vscode-dark.toml")),
    ("one-dark.toml",                  include_str!("../themes/one-dark.toml")),
    ("darcula.toml",                   include_str!("../themes/darcula.toml")),
    ("material-ocean.toml",            include_str!("../themes/material-ocean.toml")),
    ("github-light-default.toml",      include_str!("../themes/github-light-default.toml")),
    ("github-light-high-contrast.toml",include_str!("../themes/github-light-high-contrast.toml")),
    ("vscode-light.toml",              include_str!("../themes/vscode-light.toml")),
    ("sublime-light.toml",             include_str!("../themes/sublime-light.toml")),
    ("idea-light.toml",                include_str!("../themes/idea-light.toml")),
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
    fn is_light_theme_white_bg() {
        let content = r##"
            bg = "#ffffff"
            bg_light = "#eeeeee"
            fg = "#000000"
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
        assert!(is_light_theme_content(content));
    }

    #[test]
    fn is_light_theme_dark_bg() {
        let content = r##"
            bg = "#0b0e14"
            bg_light = "#111111"
            fg = "#bfbdb6"
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
        assert!(!is_light_theme_content(content));
    }

    #[test]
    fn is_light_theme_near_threshold() {
        // Luminance exactly at boundary: R=128, G=128, B=128
        // lum = 0.299*128 + 0.587*128 + 0.114*128 = 128.0 → not > 128
        let content = r##"
            bg = "#808080"
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
        assert!(!is_light_theme_content(content));
    }

    #[test]
    fn is_light_theme_invalid_toml() {
        assert!(!is_light_theme_content("not valid toml {{{{"));
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
