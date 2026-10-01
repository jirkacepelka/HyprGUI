//! Declarative schema of Hyprland options. Pages in the GUI are generated from
//! `schema.toml`; adding an option there is all that is needed to expose it.

mod color;

pub use color::{Color, Gradient};

use serde::Deserialize;

/// Text with an English original and optional translations.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct Text {
    pub en: String,
    pub cs: Option<String>,
}

impl Text {
    pub fn get(&self, lang: &str) -> &str {
        match lang {
            "cs" => self.cs.as_deref().unwrap_or(&self.en),
            _ => &self.en,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64, step: f64 },
    Color,
    Gradient,
    Text,
    Choice { choices: Vec<String> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    pub id: String,
    pub icon: String,
    pub title: Text,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Group {
    pub id: String,
    pub page: String,
    pub title: Text,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Opt {
    /// Config path, e.g. `decoration:blur:size`.
    pub key: String,
    pub page: String,
    pub group: String,
    #[serde(flatten)]
    pub kind: Kind,
    pub default: String,
    pub label: Text,
    pub desc: Option<Text>,
    /// Another boolean option that must be on for this one to matter.
    pub depends: Option<String>,
    /// `false` when `hyprctl keyword` cannot apply the option live.
    #[serde(default = "yes")]
    pub live: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
struct Raw {
    page: Vec<Page>,
    group: Vec<Group>,
    option: Vec<Opt>,
}

#[derive(Debug, Clone)]
pub struct Schema {
    pub pages: Vec<Page>,
    pub groups: Vec<Group>,
    pub options: Vec<Opt>,
}

impl Schema {
    pub fn builtin() -> Schema {
        Schema::parse(include_str!("../schema.toml")).expect("built-in schema is valid")
    }

    pub fn parse(src: &str) -> Result<Schema, toml::de::Error> {
        let r: Raw = toml::from_str(src)?;
        Ok(Schema {
            pages: r.page,
            groups: r.group,
            options: r.option,
        })
    }

    pub fn option(&self, key: &str) -> Option<&Opt> {
        self.options.iter().find(|o| o.key == key)
    }

    pub fn groups_of<'a>(&'a self, page: &'a str) -> impl Iterator<Item = &'a Group> {
        self.groups.iter().filter(move |g| g.page == page)
    }

    pub fn options_of<'a>(
        &'a self,
        page: &'a str,
        group: &'a str,
    ) -> impl Iterator<Item = &'a Opt> {
        self.options
            .iter()
            .filter(move |o| o.page == page && o.group == group)
    }
}

impl Opt {
    /// Whether `value` is acceptable for this option's kind.
    pub fn validate(&self, value: &str) -> bool {
        let v = value.trim();
        match &self.kind {
            Kind::Bool => matches!(
                v,
                "true" | "false" | "yes" | "no" | "on" | "off" | "1" | "0"
            ),
            Kind::Int { min, max } => v.parse::<i64>().is_ok_and(|n| n >= *min && n <= *max),
            Kind::Float { min, max, .. } => v.parse::<f64>().is_ok_and(|n| n >= *min && n <= *max),
            Kind::Color => Color::parse(v).is_some(),
            Kind::Gradient => Gradient::parse(v).is_some(),
            Kind::Text => true,
            Kind::Choice { choices } => choices.iter().any(|c| c == v),
        }
    }
}

/// Interprets Hyprland's boolean spellings.
pub fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builtin_schema_is_consistent() {
        let s = Schema::builtin();
        let pages: HashSet<_> = s.pages.iter().map(|p| p.id.as_str()).collect();
        let mut keys = HashSet::new();
        for g in &s.groups {
            assert!(
                pages.contains(g.page.as_str()),
                "group {} has unknown page",
                g.id
            );
        }
        for o in &s.options {
            assert!(keys.insert(o.key.as_str()), "duplicate option {}", o.key);
            assert!(pages.contains(o.page.as_str()), "{} unknown page", o.key);
            assert!(
                s.groups.iter().any(|g| g.id == o.group && g.page == o.page),
                "{} unknown group {}",
                o.key,
                o.group
            );
            assert!(
                o.validate(&o.default),
                "{}: default `{}` invalid",
                o.key,
                o.default
            );
            if let Some(d) = &o.depends {
                let dep = s
                    .option(d)
                    .unwrap_or_else(|| panic!("{} depends on unknown {d}", o.key));
                assert_eq!(dep.kind, Kind::Bool, "{} depends on non-bool", o.key);
            }
            assert!(o.label.cs.is_some(), "{} lacks Czech label", o.key);
        }
        assert!(s.options.len() >= 40);
    }

    #[test]
    fn validation_by_kind() {
        let s = Schema::builtin();
        let gaps = s.option("general:gaps_in").unwrap();
        assert!(gaps.validate("5") && !gaps.validate("-1") && !gaps.validate("x"));
        let layout = s.option("general:layout").unwrap();
        assert!(layout.validate("dwindle") && !layout.validate("nope"));
        assert!(s.option("decoration:blur:enabled").unwrap().validate("yes"));
        assert!(s
            .option("general:col.active_border")
            .unwrap()
            .validate("rgba(33ccffee) rgba(00ff99ee) 45deg"));
    }

    #[test]
    fn bool_spellings() {
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }
}
