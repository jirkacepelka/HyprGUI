//! Application settings (theme, colour scheme), separate from Hyprland's config.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub theme: Option<String>,
    #[serde(default)]
    pub scheme: Scheme,
}

fn user_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("hyprgui/settings.toml"))
}

impl Settings {
    /// User file first, then the distro default in `/etc/hyprgui/settings.toml`.
    pub fn load() -> Settings {
        let candidates = [
            user_path(),
            Some(PathBuf::from("/etc/hyprgui/settings.toml")),
        ];
        for p in candidates.into_iter().flatten() {
            if let Ok(text) = fs::read_to_string(&p) {
                if let Ok(s) = toml::from_str(&text) {
                    return s;
                }
            }
        }
        Settings::default()
    }

    pub fn save(&self) {
        let Some(path) = user_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = toml::to_string(self) {
            let _ = fs::write(path, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = Settings {
            theme: Some("caelestia".into()),
            scheme: Scheme::Dark,
        };
        let text = toml::to_string(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.theme.as_deref(), Some("caelestia"));
        assert_eq!(back.scheme, Scheme::Dark);
        let empty: Settings = toml::from_str("").unwrap();
        assert_eq!(empty.scheme, Scheme::System);
    }
}
