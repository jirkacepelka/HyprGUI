//! `bind*` lines: parsing, formatting, collision detection.

/// Flag letters Hyprland allows after `bind`.
const FLAGS: &str = "lrenmtisdpoc";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bind {
    /// Flag letters, e.g. `el` for `bindel`.
    pub flags: String,
    pub mods: String,
    pub key: String,
    /// Only with the `d` flag.
    pub description: Option<String>,
    pub dispatcher: String,
    pub arg: String,
}

/// Whether a config keyword (`bind`, `bindm`, …) is a bind.
pub fn is_bind_keyword(keyword: &str) -> bool {
    keyword
        .strip_prefix("bind")
        .is_some_and(|f| f.chars().all(|c| FLAGS.contains(c)))
}

impl Bind {
    pub fn parse(keyword: &str, value: &str) -> Option<Bind> {
        if !is_bind_keyword(keyword) {
            return None;
        }
        let flags = keyword["bind".len()..].to_string();
        let has_desc = flags.contains('d');
        let parts: Vec<&str> = value
            .splitn(if has_desc { 5 } else { 4 }, ',')
            .map(str::trim)
            .collect();
        let need = if has_desc { 4 } else { 3 };
        if parts.len() < need {
            return None;
        }
        let (description, rest) = if has_desc {
            (Some(parts[2].to_string()), &parts[3..])
        } else {
            (None, &parts[2..])
        };
        Some(Bind {
            flags,
            mods: parts[0].to_string(),
            key: parts[1].to_string(),
            description,
            dispatcher: rest[0].to_string(),
            arg: rest.get(1).map_or(String::new(), |s| s.to_string()),
        })
    }

    pub fn keyword(&self) -> String {
        format!("bind{}", self.flags)
    }

    pub fn to_value(&self) -> String {
        let mut v = vec![self.mods.clone(), self.key.clone()];
        if self.flags.contains('d') {
            v.push(self.description.clone().unwrap_or_default());
        }
        v.push(self.dispatcher.clone());
        if !self.arg.is_empty() {
            v.push(self.arg.clone());
        }
        v.join(", ")
    }

    /// Human readable shortcut, with variables resolved: `SUPER + SHIFT + Q`.
    pub fn shortcut(&self, vars: &[(String, String)]) -> String {
        let mut parts = normalized_mods(&self.mods, vars);
        parts.push(self.key.to_uppercase());
        parts.join(" + ")
    }
}

/// Replaces `$name` by its value (longest names first so `$mod2` wins over `$mod`).
pub fn resolve_vars(s: &str, vars: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = vars.iter().collect();
    sorted.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    let mut out = s.to_string();
    for (k, v) in sorted {
        out = out.replace(k.as_str(), v);
    }
    out
}

/// Modifier set, upper-cased, deduplicated and in a fixed order.
pub fn normalized_mods(mods: &str, vars: &[(String, String)]) -> Vec<String> {
    const ORDER: [&str; 5] = ["SUPER", "CTRL", "ALT", "SHIFT", "MOD"];
    let resolved = resolve_vars(mods, vars).to_uppercase();
    let mut found: Vec<String> = resolved
        .split(|c: char| c.is_whitespace() || c == '+' || c == '_')
        .filter(|s| !s.is_empty())
        .map(|s| match s {
            "WIN" | "LOGO" | "META" | "MOD4" => "SUPER".to_string(),
            "CONTROL" => "CTRL".to_string(),
            "MOD1" => "ALT".to_string(),
            other => other.to_string(),
        })
        .collect();
    found.sort_by_key(|m| ORDER.iter().position(|o| o == m).unwrap_or(ORDER.len()));
    found.dedup();
    found
}

/// Pairs of indices that trigger on the same key combination. Mouse binds
/// and release binds live in their own namespace.
pub fn collisions(binds: &[Bind], vars: &[(String, String)]) -> Vec<(usize, usize)> {
    let sig = |b: &Bind| {
        (
            normalized_mods(&b.mods, vars),
            b.key.to_lowercase(),
            b.flags.contains('m'),
            b.flags.contains('r'),
        )
    };
    let sigs: Vec<_> = binds.iter().map(sig).collect();
    let mut out = Vec::new();
    for i in 0..binds.len() {
        for j in i + 1..binds.len() {
            if sigs[i] == sigs[j] {
                out.push((i, j));
            }
        }
    }
    out
}

/// Common dispatchers offered in the editor (free text is still allowed).
pub const DISPATCHERS: &[&str] = &[
    "exec",
    "execr",
    "killactive",
    "closewindow",
    "workspace",
    "movetoworkspace",
    "movetoworkspacesilent",
    "togglefloating",
    "fullscreen",
    "fullscreenstate",
    "pseudo",
    "togglesplit",
    "movefocus",
    "movewindow",
    "swapwindow",
    "resizeactive",
    "moveactive",
    "togglegroup",
    "changegroupactive",
    "togglespecialworkspace",
    "focusmonitor",
    "movecurrentworkspacetomonitor",
    "cyclenext",
    "centerwindow",
    "pin",
    "exit",
    "forcerendererreload",
    "dpms",
    "submap",
    "global",
    "layoutmsg",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vec<(String, String)> {
        vec![
            ("$mod".into(), "SUPER".into()),
            ("$mod2".into(), "ALT".into()),
        ]
    }

    #[test]
    fn parses_simple_and_flagged_binds() {
        let b = Bind::parse("bind", "$mod SHIFT, Q, exec, kitty --title a,b").unwrap();
        assert_eq!(
            (b.mods.as_str(), b.key.as_str(), b.dispatcher.as_str()),
            ("$mod SHIFT", "Q", "exec")
        );
        assert_eq!(b.arg, "kitty --title a,b");
        assert_eq!(b.to_value(), "$mod SHIFT, Q, exec, kitty --title a,b");
        let k = Bind::parse(
            "bindel",
            ", XF86AudioRaiseVolume, exec, wpctl set-volume +5%",
        )
        .unwrap();
        assert_eq!(k.flags, "el");
        assert_eq!(k.keyword(), "bindel");
        let noarg = Bind::parse("bind", "$mod, Q, killactive").unwrap();
        assert_eq!(noarg.arg, "");
        assert_eq!(noarg.to_value(), "$mod, Q, killactive");
    }

    #[test]
    fn description_flag_adds_a_field() {
        let b = Bind::parse("bindd", "$mod, T, Open terminal, exec, kitty").unwrap();
        assert_eq!(b.description.as_deref(), Some("Open terminal"));
        assert_eq!(b.dispatcher, "exec");
        assert_eq!(b.arg, "kitty");
        assert_eq!(b.to_value(), "$mod, T, Open terminal, exec, kitty");
    }

    #[test]
    fn rejects_non_binds_and_short_lines() {
        // `binds` is Hyprland's section name, but as a keyword prefix `s` is a valid flag.
        assert!(is_bind_keyword("binds"));
        assert!(!is_bind_keyword("bindx"));
        assert!(!is_bind_keyword("windowrule"));
        assert!(Bind::parse("bind", "$mod, Q").is_none());
        assert!(Bind::parse("exec", "a, b, c").is_none());
    }

    #[test]
    fn shortcut_resolves_variables() {
        let b = Bind::parse("bind", "$mod SHIFT, q, killactive").unwrap();
        assert_eq!(b.shortcut(&vars()), "SUPER + SHIFT + Q");
        let alt = Bind::parse("bind", "$mod2, a, killactive").unwrap();
        assert_eq!(alt.shortcut(&vars()), "ALT + A");
    }

    #[test]
    fn collisions_ignore_order_case_and_aliases() {
        let binds: Vec<Bind> = [
            ("bind", "$mod SHIFT, Q, killactive"),
            ("bind", "SHIFT SUPER, q, exit"),
            ("bind", "$mod, Q, exec, x"),
            ("bindm", "$mod SHIFT, Q, movewindow"),
            ("bindr", "$mod SHIFT, Q, exec, y"),
        ]
        .iter()
        .map(|(k, v)| Bind::parse(k, v).unwrap())
        .collect();
        assert_eq!(collisions(&binds, &vars()), vec![(0, 1)]);
    }
}
