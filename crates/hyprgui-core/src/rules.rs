//! Helpers for window rules and autostart lines.

/// Legacy `windowrulev2` line from the pieces shown in the "add rule" dialog.
pub fn compose_windowrule(rule: &str, class: &str, title: &str) -> Option<String> {
    let rule = rule.trim();
    if rule.is_empty() || (class.trim().is_empty() && title.trim().is_empty()) {
        return None;
    }
    let mut matchers = Vec::new();
    if !class.trim().is_empty() {
        matchers.push(format!("class:^({})$", regex_escape(class.trim())));
    }
    if !title.trim().is_empty() {
        matchers.push(format!("title:^({})$", regex_escape(title.trim())));
    }
    Some(format!("{rule}, {}", matchers.join(", ")))
}

fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\.^$|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Rules commonly wanted; offered as suggestions in the dialog.
pub const COMMON_RULES: &[&str] = &[
    "float",
    "tile",
    "fullscreen",
    "maximize",
    "center",
    "pin",
    "noblur",
    "noshadow",
    "noborder",
    "noanim",
    "opaque",
    "nofocus",
    "workspace 2",
    "size 800 600",
    "opacity 0.9",
    "idleinhibit focus",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_rules() {
        assert_eq!(
            compose_windowrule("float", "pavucontrol", "").unwrap(),
            "float, class:^(pavucontrol)$"
        );
        assert_eq!(
            compose_windowrule("size 800 600", "org.gnome.Calc", "Calc (1)").unwrap(),
            "size 800 600, class:^(org\\.gnome\\.Calc)$, title:^(Calc \\(1\\))$"
        );
        assert!(compose_windowrule("", "x", "").is_none());
        assert!(compose_windowrule("float", "", " ").is_none());
    }
}
