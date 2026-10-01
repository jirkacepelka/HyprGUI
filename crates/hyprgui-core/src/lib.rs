//! GUI-independent logic of HyprGUI.

pub mod binds;
pub mod monitor;
pub mod rules;
mod session;

pub use session::{Error, LiveStatus, SaveReport, Session};

/// UI language, from the environment (`cs*` → Czech, everything else English).
pub fn language() -> &'static str {
    for var in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return if v.starts_with("cs") { "cs" } else { "en" };
            }
        }
    }
    "en"
}
