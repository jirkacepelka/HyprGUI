//! Lossless parser/editor for `hyprland.conf`.
//!
//! The file is kept as a list of lines. Untouched lines are emitted byte for
//! byte, so comments, blank lines and formatting survive an edit. Only lines
//! that are changed through the API are re-rendered.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("unbalanced braces: {0}")]
    Unbalanced(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Blank(String),
    Comment(String),
    /// `key = value`, optionally with a trailing `# comment`.
    Assign {
        indent: String,
        key: String,
        value: String,
        trailing: String,
        /// Original text, used while the line is unmodified.
        raw: Option<String>,
    },
    SectionStart {
        indent: String,
        name: String,
        raw: String,
    },
    SectionEnd {
        raw: String,
    },
}

impl Line {
    fn render(&self) -> String {
        match self {
            Line::Blank(s) | Line::Comment(s) => s.clone(),
            Line::SectionStart { raw, .. } | Line::SectionEnd { raw } => raw.clone(),
            Line::Assign { raw: Some(raw), .. } => raw.clone(),
            Line::Assign {
                indent,
                key,
                value,
                trailing,
                ..
            } => {
                format!("{indent}{key} = {value}{trailing}")
            }
        }
    }
}

/// One `key = value` entry as seen by callers (e.g. a `bind`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Full path, e.g. `general:gaps_in` or just `bind` for top-level keywords.
    pub path: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    lines: Vec<Line>,
    trailing_newline: bool,
}

/// Splits a value from a trailing `# comment`. `##` is an escaped `#` in Hyprland.
fn split_trailing_comment(rest: &str) -> (String, String) {
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            if bytes.get(i + 1) == Some(&b'#') {
                i += 2;
                continue;
            }
            let value = rest[..i].trim_end();
            let trailing = &rest[value.len()..];
            return (value.to_string(), trailing.to_string());
        }
        i += 1;
    }
    (rest.trim_end().to_string(), String::new())
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let trailing_newline = text.ends_with('\n');
        let mut lines = Vec::new();
        let mut depth = 0usize;
        for raw in text.lines() {
            let trimmed = raw.trim();
            let indent: String = raw.chars().take_while(|c| c.is_whitespace()).collect();
            let line = if trimmed.is_empty() {
                Line::Blank(raw.to_string())
            } else if trimmed.starts_with('#') {
                Line::Comment(raw.to_string())
            } else if trimmed == "}" {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| Error::Unbalanced("unexpected `}`".into()))?;
                Line::SectionEnd {
                    raw: raw.to_string(),
                }
            } else if let Some(name) = trimmed.strip_suffix('{') {
                depth += 1;
                Line::SectionStart {
                    indent,
                    name: name.trim().to_string(),
                    raw: raw.to_string(),
                }
            } else if let Some((key, rest)) = trimmed.split_once('=') {
                let (value, trailing) = split_trailing_comment(rest.trim_start());
                Line::Assign {
                    indent,
                    key: key.trim().to_string(),
                    value,
                    trailing,
                    raw: Some(raw.to_string()),
                }
            } else {
                // Unknown syntax: keep verbatim so we never lose user data.
                Line::Comment(raw.to_string())
            };
            lines.push(line);
        }
        if depth != 0 {
            return Err(Error::Unbalanced("missing `}`".into()));
        }
        Ok(Config {
            lines,
            trailing_newline,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::parse(&fs::read_to_string(path)?)
    }

    /// All assignments with their section path, in file order.
    pub fn entries(&self) -> Vec<Entry> {
        let mut stack: Vec<&str> = Vec::new();
        let mut out = Vec::new();
        for line in &self.lines {
            match line {
                Line::SectionStart { name, .. } => stack.push(name),
                Line::SectionEnd { .. } => {
                    stack.pop();
                }
                Line::Assign { key, value, .. } => {
                    let mut path = stack.join(":");
                    if !path.is_empty() {
                        path.push(':');
                    }
                    path.push_str(key);
                    out.push(Entry {
                        path,
                        value: value.clone(),
                    });
                }
                _ => {}
            }
        }
        out
    }

    /// Value of the last assignment of `path` (Hyprland: later wins).
    pub fn get(&self, path: &str) -> Option<String> {
        self.entries()
            .into_iter()
            .rev()
            .find(|e| e.path == path)
            .map(|e| e.value)
    }

    /// Every value of a repeatable keyword such as `bind` or `exec-once`.
    pub fn get_all(&self, path: &str) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|e| e.path == path)
            .map(|e| e.value)
            .collect()
    }

    /// Sets `path` (e.g. `decoration:blur:enabled`). Updates the last existing
    /// assignment in place, otherwise inserts it, creating sections as needed.
    pub fn set(&mut self, path: &str, value: &str) {
        let parts: Vec<&str> = path.split(':').collect();
        let (key, sections) = parts.split_last().expect("path is never empty");

        if let Some(idx) = self.find_assign(sections, key).last().copied() {
            if let Line::Assign { value: v, raw, .. } = &mut self.lines[idx] {
                if v != value {
                    *v = value.to_string();
                    *raw = None;
                }
            }
            return;
        }
        self.insert_new(sections, key, value);
    }

    /// Removes every assignment of `path`. Returns how many were removed.
    pub fn remove(&mut self, path: &str) -> usize {
        let parts: Vec<&str> = path.split(':').collect();
        let (key, sections) = parts.split_last().expect("path is never empty");
        let idxs = self.find_assign(sections, key);
        for idx in idxs.iter().rev() {
            self.lines.remove(*idx);
        }
        idxs.len()
    }

    /// Replaces all values of a repeatable keyword (`bind`, `windowrule`, …)
    /// with `values`, keeping the position of the first existing entry.
    pub fn set_all(&mut self, path: &str, values: &[String]) {
        let parts: Vec<&str> = path.split(':').collect();
        let (key, sections) = parts.split_last().expect("path is never empty");
        let idxs = self.find_assign(sections, key);
        let at = idxs.first().copied();
        for idx in idxs.iter().rev() {
            self.lines.remove(*idx);
        }
        let new_lines: Vec<Line> = values
            .iter()
            .map(|v| Line::Assign {
                indent: String::new(),
                key: (*key).to_string(),
                value: v.clone(),
                trailing: String::new(),
                raw: None,
            })
            .collect();
        match at {
            Some(i) => {
                for (n, l) in new_lines.into_iter().enumerate() {
                    self.lines.insert(i + n, l);
                }
            }
            None => {
                for v in values {
                    self.insert_new(sections, key, v);
                }
            }
        }
    }

    /// Replaces the `n`-th assignment of `path` (file order). Keeps comments.
    pub fn replace_nth(&mut self, path: &str, n: usize, value: &str) -> bool {
        let Some(idx) = self.nth_index(path, n) else {
            return false;
        };
        if let Line::Assign { value: v, raw, .. } = &mut self.lines[idx] {
            if v != value {
                *v = value.to_string();
                *raw = None;
            }
        }
        true
    }

    /// Removes the `n`-th assignment of `path`.
    pub fn remove_nth(&mut self, path: &str, n: usize) -> bool {
        let Some(idx) = self.nth_index(path, n) else {
            return false;
        };
        self.lines.remove(idx);
        true
    }

    /// Appends a new assignment of a repeatable keyword right after the last
    /// existing one, or at the end of the matching section/file.
    pub fn push(&mut self, path: &str, value: &str) {
        let parts: Vec<&str> = path.split(':').collect();
        let (key, sections) = parts.split_last().expect("path is never empty");
        if let Some(last) = self.find_assign(sections, key).last().copied() {
            let indent = match &self.lines[last] {
                Line::Assign { indent, .. } => indent.clone(),
                _ => String::new(),
            };
            self.lines.insert(
                last + 1,
                Line::Assign {
                    indent,
                    key: (*key).to_string(),
                    value: value.to_string(),
                    trailing: String::new(),
                    raw: None,
                },
            );
            self.trailing_newline = true;
        } else {
            self.insert_new(sections, key, value);
        }
    }

    fn nth_index(&self, path: &str, n: usize) -> Option<usize> {
        let parts: Vec<&str> = path.split(':').collect();
        let (key, sections) = parts.split_last()?;
        self.find_assign(sections, key).get(n).copied()
    }

    /// Indices of assignments `key` located exactly in `sections`.
    fn find_assign(&self, sections: &[&str], key: &str) -> Vec<usize> {
        let mut stack: Vec<&str> = Vec::new();
        let mut found = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            match line {
                Line::SectionStart { name, .. } => stack.push(name),
                Line::SectionEnd { .. } => {
                    stack.pop();
                }
                Line::Assign { key: k, .. } if k == key && stack == sections => found.push(i),
                _ => {}
            }
        }
        found
    }

    fn insert_new(&mut self, sections: &[&str], key: &str, value: &str) {
        // Walk down as far as existing sections match, remembering the closing
        // brace of the deepest match; missing sections are appended after it.
        let mut stack: Vec<&str> = Vec::new();
        let mut best: Option<(usize, usize)> = None; // (matched depth, end-line index)
        let mut open: Vec<(usize, bool)> = Vec::new(); // (start index, on target path)
        for (i, line) in self.lines.iter().enumerate() {
            match line {
                Line::SectionStart { name, .. } => {
                    stack.push(name);
                    let on_path =
                        stack.len() <= sections.len() && stack[..] == sections[..stack.len()];
                    open.push((i, on_path));
                }
                Line::SectionEnd { .. } => {
                    let depth = stack.len();
                    let (_, on_path) = open.pop().unwrap_or((0, false));
                    stack.pop();
                    // Keep the deepest matching block; among equals, the last one.
                    if on_path && best.is_none_or(|(d, _)| depth >= d) {
                        best = Some((depth, i));
                    }
                }
                _ => {}
            }
        }

        let matched = best.map_or(0, |(d, _)| d);
        let indent = "    ".repeat(sections.len());
        let mut new_lines: Vec<Line> = Vec::new();
        // Open the missing sections.
        for (n, name) in sections.iter().enumerate().skip(matched) {
            let ind = "    ".repeat(n);
            new_lines.push(Line::SectionStart {
                indent: ind.clone(),
                name: (*name).to_string(),
                raw: format!("{ind}{name} {{"),
            });
        }
        new_lines.push(Line::Assign {
            indent,
            key: key.to_string(),
            value: value.to_string(),
            trailing: String::new(),
            raw: None,
        });
        for n in (matched..sections.len()).rev() {
            new_lines.push(Line::SectionEnd {
                raw: format!("{}}}", "    ".repeat(n)),
            });
        }

        let at = match best {
            // Inside the deepest matching block, right before its `}`.
            Some((_, end_idx)) => end_idx,
            None => {
                if !self.lines.is_empty() && !matches!(self.lines.last(), Some(Line::Blank(_))) {
                    self.lines.push(Line::Blank(String::new()));
                }
                self.lines.len()
            }
        };
        for (n, l) in new_lines.into_iter().enumerate() {
            self.lines.insert(at + n, l);
        }
        self.trailing_newline = true;
    }

    /// Writes atomically (temp file + rename) after saving `<path>.bak`.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        if path.exists() {
            let mut bak: PathBuf = path.into();
            bak.as_mut_os_string().push(".bak");
            fs::copy(path, bak)?;
        }
        let mut tmp: PathBuf = path.into();
        tmp.as_mut_os_string().push(".tmp");
        fs::write(&tmp, self.to_string())?;
        fs::rename(tmp, path)?;
        Ok(())
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for l in &self.lines {
            if !first {
                f.write_str("\n")?;
            }
            first = false;
            f.write_str(&l.render())?;
        }
        if self.trailing_newline && !self.lines.is_empty() {
            f.write_str("\n")?;
        }
        Ok(())
    }
}

mod set;
pub use set::{ConfigSet, Located, Saved};

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# my config
$mod = SUPER

monitor = eDP-1, 1920x1080@60, 0x0, 1   # laptop

general {
    gaps_in = 5
    gaps_out = 10
    col.active_border = rgba(33ccffee) rgba(00ff99ee) 45deg
}

decoration {
    rounding = 10
    blur {
        enabled = true
        size = 3
    }
}

bind = $mod, Q, killactive
bind = $mod, T, exec, kitty
";

    #[test]
    fn roundtrip_is_exact() {
        let c = Config::parse(SAMPLE).unwrap();
        assert_eq!(c.to_string(), SAMPLE);
    }

    #[test]
    fn roundtrip_without_trailing_newline() {
        let s = SAMPLE.trim_end();
        assert_eq!(Config::parse(s).unwrap().to_string(), s);
    }

    #[test]
    fn reads_nested_values_and_keywords() {
        let c = Config::parse(SAMPLE).unwrap();
        assert_eq!(c.get("general:gaps_in").as_deref(), Some("5"));
        assert_eq!(c.get("decoration:blur:size").as_deref(), Some("3"));
        assert_eq!(
            c.get("monitor").as_deref(),
            Some("eDP-1, 1920x1080@60, 0x0, 1")
        );
        assert_eq!(c.get_all("bind").len(), 2);
    }

    #[test]
    fn edit_touches_only_one_line() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.set("general:gaps_in", "8");
        assert_eq!(c.to_string(), SAMPLE.replace("gaps_in = 5", "gaps_in = 8"));
    }

    #[test]
    fn edit_keeps_trailing_comment() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.set("monitor", "eDP-1, 2560x1440@144, 0x0, 1");
        assert!(c
            .to_string()
            .contains("monitor = eDP-1, 2560x1440@144, 0x0, 1   # laptop"));
    }

    #[test]
    fn inserts_into_existing_section() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.set("general:border_size", "2");
        let out = c.to_string();
        assert!(out.contains("    gaps_out = 10\n    col.active_border"));
        assert_eq!(
            Config::parse(&out)
                .unwrap()
                .get("general:border_size")
                .as_deref(),
            Some("2")
        );
        // Insert lands inside `general`, before its closing brace.
        let g_end = out.find("}\n\ndecoration").unwrap();
        assert!(out.find("border_size").unwrap() < g_end);
    }

    #[test]
    fn inserts_into_existing_nested_section() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.set("decoration:blur:passes", "2");
        let out = c.to_string();
        let r = Config::parse(&out).unwrap();
        assert_eq!(r.get("decoration:blur:passes").as_deref(), Some("2"));
        assert!(out.contains("        size = 3\n        passes = 2\n    }"));
    }

    #[test]
    fn creates_missing_sections() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.set("input:touchpad:natural_scroll", "true");
        let out = c.to_string();
        assert_eq!(
            Config::parse(&out)
                .unwrap()
                .get("input:touchpad:natural_scroll")
                .as_deref(),
            Some("true")
        );
        assert!(out.contains("input {\n    touchpad {\n        natural_scroll = true\n    }\n}"));
    }

    #[test]
    fn creates_nested_section_inside_existing_parent() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.set("decoration:shadow:enabled", "false");
        let out = c.to_string();
        assert_eq!(
            Config::parse(&out)
                .unwrap()
                .get("decoration:shadow:enabled")
                .as_deref(),
            Some("false")
        );
        // New block must be inside `decoration`, i.e. before the `bind` lines and balanced.
        assert!(out.find("shadow {").unwrap() < out.find("bind =").unwrap());
    }

    #[test]
    fn set_all_replaces_binds_in_place() {
        let mut c = Config::parse(SAMPLE).unwrap();
        c.set_all("bind", &["$mod, E, exec, thunar".into()]);
        assert_eq!(c.get_all("bind"), vec!["$mod, E, exec, thunar"]);
    }

    #[test]
    fn nth_helpers_edit_lists() {
        let mut c = Config::parse(SAMPLE).unwrap();
        assert!(c.replace_nth("bind", 1, "$mod, B, exec, firefox"));
        assert!(c.remove_nth("bind", 0));
        c.push("bind", "$mod, X, exit");
        assert_eq!(
            c.get_all("bind"),
            vec!["$mod, B, exec, firefox", "$mod, X, exit"]
        );
        assert!(!c.replace_nth("bind", 9, "x"));
        // pushing a new keyword lands at the end and re-parses cleanly
        c.push("exec-once", "waybar");
        assert_eq!(
            Config::parse(&c.to_string()).unwrap().get_all("exec-once"),
            vec!["waybar"]
        );
    }

    #[test]
    fn remove_deletes_assignments() {
        let mut c = Config::parse(SAMPLE).unwrap();
        assert_eq!(c.remove("general:gaps_out"), 1);
        assert_eq!(c.get("general:gaps_out"), None);
    }

    #[test]
    fn hash_escape_is_not_a_comment() {
        let c = Config::parse("a = b ## c # real\n").unwrap();
        assert_eq!(c.get("a").as_deref(), Some("b ## c"));
    }

    #[test]
    fn unbalanced_is_error() {
        assert!(Config::parse("general {\n a = 1\n").is_err());
        assert!(Config::parse("}\n").is_err());
    }

    #[test]
    fn save_makes_backup_and_is_atomic() {
        let dir = std::env::temp_dir().join(format!("hyprconf-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("hyprland.conf");
        fs::write(&p, SAMPLE).unwrap();
        let mut c = Config::load(&p).unwrap();
        c.set("general:gaps_in", "1");
        c.save(&p).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("hyprland.conf.bak")).unwrap(),
            SAMPLE
        );
        assert!(fs::read_to_string(&p).unwrap().contains("gaps_in = 1"));
        fs::remove_dir_all(dir).ok();
    }
}
