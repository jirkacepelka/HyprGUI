//! HyprGUI theme engine.
//!
//! A theme is a directory with a `theme.toml` (metadata + tokens) and an
//! optional `style.css` that is appended after the generated CSS. Missing
//! tokens fall back to the built-in default theme, so a minimal theme can
//! override just a few colours.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}: {1}")]
    Parse(PathBuf, toml::de::Error),
    #[error("theme `{0}` not found")]
    NotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Meta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    /// Variant used when the system does not express a preference.
    #[serde(default)]
    pub base: Variant,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Fonts {
    pub ui: Option<String>,
    pub mono: Option<String>,
    pub icons: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Shape {
    #[serde(default = "d_small")]
    pub radius_small: u32,
    #[serde(default = "d_medium")]
    pub radius_medium: u32,
    #[serde(default = "d_large")]
    pub radius_large: u32,
}
fn d_small() -> u32 {
    6
}
fn d_medium() -> u32 {
    12
}
fn d_large() -> u32 {
    24
}
impl Default for Shape {
    fn default() -> Self {
        Shape {
            radius_small: 6,
            radius_medium: 12,
            radius_large: 24,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Colors {
    #[serde(default)]
    pub dark: BTreeMap<String, String>,
    #[serde(default)]
    pub light: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Dynamic {
    /// Flat JSON object `{ "primary": "#aabbcc", ... }` written by a shell.
    /// When it exists it overrides the static colours of the active variant.
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawTheme {
    theme: Meta,
    #[serde(default)]
    fonts: Fonts,
    #[serde(default)]
    shape: Shape,
    #[serde(default)]
    colors: Colors,
    #[serde(default)]
    dynamic: Dynamic,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub meta: Meta,
    pub fonts: Fonts,
    pub shape: Shape,
    pub colors: Colors,
    pub dynamic: Dynamic,
    pub css: String,
    pub dir: Option<PathBuf>,
}

/// Tokens every complete theme variant should define (Material 3 names).
pub const REQUIRED_TOKENS: &[&str] = &[
    "primary",
    "on_primary",
    "primary_container",
    "on_primary_container",
    "secondary",
    "on_secondary",
    "tertiary",
    "error",
    "on_error",
    "background",
    "on_background",
    "surface",
    "on_surface",
    "on_surface_variant",
    "surface_container_low",
    "surface_container",
    "surface_container_high",
    "outline",
    "outline_variant",
];

/// (foreground, background) pairs that must stay readable.
pub const CONTRAST_PAIRS: &[(&str, &str)] = &[
    ("on_primary", "primary"),
    ("on_primary_container", "primary_container"),
    ("on_error", "error"),
    ("on_background", "background"),
    ("on_surface", "surface"),
    ("on_surface_variant", "surface"),
];

const DEFAULT_TOML: &str = include_str!("../../../themes/default/theme.toml");
const DEFAULT_CSS: &str = include_str!("../../../themes/default/style.css");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub message: String,
}

impl Theme {
    pub fn builtin_default() -> Theme {
        let mut t = Theme::parse(DEFAULT_TOML, DEFAULT_CSS.to_string(), None)
            .expect("built-in default theme is valid");
        t.dir = None;
        t
    }

    fn parse(toml_src: &str, css: String, dir: Option<PathBuf>) -> Result<Theme, toml::de::Error> {
        let raw: RawTheme = toml::from_str(toml_src)?;
        Ok(Theme {
            meta: raw.theme,
            fonts: raw.fonts,
            shape: raw.shape,
            colors: raw.colors,
            dynamic: raw.dynamic,
            css,
            dir,
        })
    }

    pub fn load(dir: impl AsRef<Path>) -> Result<Theme, Error> {
        let dir = dir.as_ref();
        let toml_path = dir.join("theme.toml");
        let src = fs::read_to_string(&toml_path).map_err(|e| Error::Io(toml_path.clone(), e))?;
        let css = fs::read_to_string(dir.join("style.css")).unwrap_or_default();
        Theme::parse(&src, css, Some(dir.to_path_buf())).map_err(|e| Error::Parse(toml_path, e))
    }

    /// Colours of `variant` with fallbacks from the built-in default and, if
    /// configured and readable, the dynamic source on top.
    pub fn resolved_colors(&self, variant: Variant) -> BTreeMap<String, String> {
        let pick = |c: &Colors| match variant {
            Variant::Dark => c.dark.clone(),
            Variant::Light => c.light.clone(),
        };
        let mut out = if self.meta.id == "default" {
            BTreeMap::new()
        } else {
            pick(&Theme::builtin_default().colors)
        };
        out.extend(pick(&self.colors));
        if let Some(src) = &self.dynamic.source {
            if let Some(map) = read_dynamic(src, self.dir.as_deref()) {
                out.extend(map);
            }
        }
        out
    }

    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut push = |severity, message: String| issues.push(Issue { severity, message });
        if self.meta.id.is_empty()
            || !self
                .meta
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            push(
                Severity::Error,
                format!("invalid theme id `{}`", self.meta.id),
            );
        }
        for (variant, own) in [("dark", &self.colors.dark), ("light", &self.colors.light)] {
            if own.is_empty() {
                continue;
            }
            for (k, v) in own {
                if parse_hex(v).is_none() {
                    push(
                        Severity::Error,
                        format!("colors.{variant}.{k}: `{v}` is not a #rrggbb colour"),
                    );
                }
            }
            for t in REQUIRED_TOKENS {
                if !own.contains_key(*t) {
                    push(
                        Severity::Warning,
                        format!("colors.{variant}.{t} missing, falling back to default"),
                    );
                }
            }
        }
        if self.colors.dark.is_empty()
            && self.colors.light.is_empty()
            && self.dynamic.source.is_none()
        {
            push(
                Severity::Warning,
                "theme defines no colours; default colours are used".into(),
            );
        }
        for (name, variant) in [("dark", Variant::Dark), ("light", Variant::Light)] {
            let own_empty = match variant {
                Variant::Dark => self.colors.dark.is_empty(),
                Variant::Light => self.colors.light.is_empty(),
            };
            if own_empty {
                continue;
            }
            let c = self.resolved_colors(variant);
            for (fg, bg) in CONTRAST_PAIRS {
                if let (Some(f), Some(b)) = (
                    c.get(*fg).and_then(|s| parse_hex(s)),
                    c.get(*bg).and_then(|s| parse_hex(s)),
                ) {
                    let ratio = contrast(f, b);
                    if ratio < 4.5 {
                        push(
                            Severity::Warning,
                            format!("{name}: contrast {fg}/{bg} is {ratio:.2}:1 (< 4.5:1)"),
                        );
                    }
                }
            }
        }
        issues
    }

    /// GTK CSS for `variant`: colour definitions, shape/font rules, then the
    /// theme's own `style.css`.
    pub fn to_css(&self, variant: Variant) -> String {
        let colors = self.resolved_colors(variant);
        let mut css = String::new();
        let _ = writeln!(css, "/* HyprGUI theme: {} ({:?}) */", self.meta.id, variant);
        for (k, v) in &colors {
            let _ = writeln!(css, "@define-color m3_{k} {v};");
        }
        // libadwaita named colours derived from Material 3 tokens.
        let map: &[(&str, &str)] = &[
            ("accent_bg_color", "primary"),
            ("accent_color", "primary"),
            ("accent_fg_color", "on_primary"),
            ("destructive_bg_color", "error"),
            ("destructive_fg_color", "on_error"),
            ("window_bg_color", "background"),
            ("window_fg_color", "on_background"),
            ("view_bg_color", "surface"),
            ("view_fg_color", "on_surface"),
            ("headerbar_bg_color", "surface_container_low"),
            ("headerbar_fg_color", "on_surface"),
            ("sidebar_bg_color", "surface_container_low"),
            ("sidebar_fg_color", "on_surface"),
            ("card_bg_color", "surface_container"),
            ("card_fg_color", "on_surface"),
            ("popover_bg_color", "surface_container_high"),
            ("popover_fg_color", "on_surface"),
            ("dialog_bg_color", "surface_container_high"),
            ("dialog_fg_color", "on_surface"),
        ];
        for (adw, m3) in map {
            if let Some(v) = colors.get(*m3) {
                let _ = writeln!(css, "@define-color {adw} {v};");
            }
        }
        let s = &self.shape;
        let _ = writeln!(
            css,
            "button, entry, spinbutton, dropdown {{ border-radius: {}px; }}\n\
             .card, .boxed-list, popover > contents {{ border-radius: {}px; }}\n\
             window.hyprgui .hero {{ border-radius: {}px; }}",
            s.radius_small, s.radius_medium, s.radius_large
        );
        if let Some(ui) = &self.fonts.ui {
            let _ = writeln!(
                css,
                "window.hyprgui {{ font-family: \"{ui}\", sans-serif; }}"
            );
        }
        if let Some(mono) = &self.fonts.mono {
            let _ = writeln!(
                css,
                ".monospace, .hyprgui-mono {{ font-family: \"{mono}\", monospace; }}"
            );
        }
        css.push_str(&self.css);
        css
    }
}

fn read_dynamic(source: &str, dir: Option<&Path>) -> Option<BTreeMap<String, String>> {
    let expanded = if let Some(rest) = source.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME")?).join(rest)
    } else {
        PathBuf::from(source)
    };
    let path = if expanded.is_relative() {
        dir?.join(expanded)
    } else {
        expanded
    };
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json_lite::Map = serde_json_lite::parse_flat(&text)?;
    Some(
        value
            .into_iter()
            .filter_map(|(k, v)| {
                let v = if v.starts_with('#') {
                    v
                } else {
                    format!("#{v}")
                };
                parse_hex(&v).map(|_| (k, v))
            })
            .collect(),
    )
}

/// Tiny flat `{"k":"v"}` JSON reader so the crate needs no JSON dependency.
mod serde_json_lite {
    pub type Map = Vec<(String, String)>;
    pub fn parse_flat(s: &str) -> Option<Map> {
        let s = s.trim().strip_prefix('{')?.strip_suffix('}')?;
        let mut out = Vec::new();
        let mut rest = s.trim();
        while !rest.is_empty() {
            let (k, r) = string(rest)?;
            let r = r.trim_start().strip_prefix(':')?.trim_start();
            // Values that are not strings (nested objects, numbers) are skipped.
            if r.starts_with('"') {
                let (v, r2) = string(r)?;
                out.push((k, v));
                rest = r2;
            } else {
                let end = r.find(',').unwrap_or(r.len());
                rest = &r[end..];
            }
            rest = rest.trim_start().trim_start_matches(',').trim_start();
        }
        Some(out)
    }
    fn string(s: &str) -> Option<(String, &str)> {
        let s = s.strip_prefix('"')?;
        let end = s.find('"')?;
        Some((s[..end].to_string(), &s[end + 1..]))
    }
}

pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let p = |i| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((p(0)?, p(2)?, p(4)?))
}

fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let f = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
}

/// WCAG contrast ratio, 1.0 – 21.0.
pub fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Directories searched for themes, highest priority first.
pub fn search_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = std::env::var_os("HYPRGUI_THEME_PATH") {
        v.extend(std::env::split_paths(&p));
    }
    let home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    if let Some(h) = home {
        v.push(h.join("hyprgui/themes"));
    }
    let data =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in data.split(':').filter(|d| !d.is_empty()) {
        v.push(Path::new(d).join("hyprgui/themes"));
    }
    v
}

/// Finds all themes in `paths`; earlier paths shadow later ones by id.
pub fn discover(paths: &[PathBuf]) -> Vec<Theme> {
    let mut seen = BTreeMap::new();
    for base in paths {
        let Ok(rd) = fs::read_dir(base) else { continue };
        let mut dirs: Vec<_> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.join("theme.toml").is_file())
            .collect();
        dirs.sort();
        for d in dirs {
            if let Ok(t) = Theme::load(&d) {
                seen.entry(t.meta.id.clone()).or_insert(t);
            }
        }
    }
    seen.into_values().collect()
}

pub fn find(id: &str, paths: &[PathBuf]) -> Result<Theme, Error> {
    if id == "default" {
        if let Some(t) = discover(paths).into_iter().find(|t| t.meta.id == id) {
            return Ok(t);
        }
        return Ok(Theme::builtin_default());
    }
    discover(paths)
        .into_iter()
        .find(|t| t.meta.id == id)
        .ok_or_else(|| Error::NotFound(id.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hyprgui-theme-{name}-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn builtin_default_is_valid_and_complete() {
        let t = Theme::builtin_default();
        assert!(
            t.validate().iter().all(|i| i.severity != Severity::Error),
            "{:?}",
            t.validate()
        );
        for v in [Variant::Dark, Variant::Light] {
            let c = t.resolved_colors(v);
            for k in REQUIRED_TOKENS {
                assert!(c.contains_key(*k), "default {v:?} lacks {k}");
            }
        }
    }

    #[test]
    fn default_passes_contrast() {
        let issues = Theme::builtin_default().validate();
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn partial_theme_falls_back_to_default() {
        let d = tmp("partial");
        fs::write(
            d.join("theme.toml"),
            "[theme]\nid=\"mini\"\nname=\"Mini\"\n[colors.dark]\nprimary=\"#ff0000\"\n",
        )
        .unwrap();
        let t = Theme::load(&d).unwrap();
        let c = t.resolved_colors(Variant::Dark);
        assert_eq!(c["primary"], "#ff0000");
        assert!(c.contains_key("surface"));
        let css = t.to_css(Variant::Dark);
        assert!(css.contains("@define-color accent_bg_color #ff0000;"));
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn invalid_colour_is_error_and_low_contrast_warns() {
        let d = tmp("bad");
        fs::write(
            d.join("theme.toml"),
            "[theme]\nid=\"bad\"\nname=\"Bad\"\n[colors.dark]\nprimary=\"red\"\non_surface=\"#101010\"\nsurface=\"#111111\"\n",
        )
        .unwrap();
        let issues = Theme::load(&d).unwrap().validate();
        assert!(issues
            .iter()
            .any(|i| i.severity == Severity::Error && i.message.contains("primary")));
        assert!(issues
            .iter()
            .any(|i| i.message.contains("contrast on_surface/surface")));
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn dynamic_source_overrides_colours() {
        let d = tmp("dyn");
        fs::write(
            d.join("scheme.json"),
            "{\"name\": \"x\", \"primary\": \"00ff00\", \"surface\": \"#010203\", \"n\": 3}",
        )
        .unwrap();
        fs::write(
            d.join("theme.toml"),
            "[theme]\nid=\"dyn\"\nname=\"Dyn\"\n[colors.dark]\nprimary=\"#ff0000\"\n[dynamic]\nsource=\"scheme.json\"\n",
        )
        .unwrap();
        let c = Theme::load(&d).unwrap().resolved_colors(Variant::Dark);
        assert_eq!(c["primary"], "#00ff00");
        assert_eq!(c["surface"], "#010203");
        assert!(!c.contains_key("name"));
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn style_css_is_appended_last() {
        let d = tmp("css");
        fs::write(d.join("theme.toml"), "[theme]\nid=\"c\"\nname=\"C\"\n").unwrap();
        fs::write(d.join("style.css"), ".x { color: red; }").unwrap();
        assert!(Theme::load(&d)
            .unwrap()
            .to_css(Variant::Dark)
            .ends_with(".x { color: red; }"));
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn discover_prefers_earlier_paths() {
        let a = tmp("da");
        let b = tmp("db");
        for (dir, name) in [(&a, "A"), (&b, "B")] {
            fs::create_dir_all(dir.join("t")).unwrap();
            fs::write(
                dir.join("t/theme.toml"),
                format!("[theme]\nid=\"t\"\nname=\"{name}\"\n"),
            )
            .unwrap();
        }
        let found = discover(&[a.clone(), b.clone()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.name, "A");
        fs::remove_dir_all(a).ok();
        fs::remove_dir_all(b).ok();
    }

    #[test]
    fn shipped_themes_are_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let themes = discover(&[root]);
        let ids: Vec<_> = themes.iter().map(|t| t.meta.id.as_str()).collect();
        assert!(
            ids.contains(&"default") && ids.contains(&"caelestia"),
            "{ids:?}"
        );
        for t in &themes {
            let issues = t.validate();
            assert!(issues.is_empty(), "{}: {issues:?}", t.meta.id);
            assert!(t
                .to_css(Variant::Dark)
                .contains("@define-color window_bg_color"));
            assert!(t
                .to_css(Variant::Light)
                .contains("@define-color window_bg_color"));
        }
    }

    #[test]
    fn contrast_black_white_is_21() {
        assert!((contrast((0, 0, 0), (255, 255, 255)) - 21.0).abs() < 0.01);
    }
}
