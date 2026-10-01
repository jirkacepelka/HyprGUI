//! Hyprland colour and gradient values.

/// RGBA, 0–255 per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Accepts `rgba(rrggbbaa)`, `rgb(rrggbb)`, `0xaarrggbb` and
    /// `rgba(r, g, b, a)` / `rgb(r, g, b)` with 0–255 channels (a: 0–1 float).
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim();
        if let Some(h) = s.strip_prefix("0x") {
            if h.len() == 8 {
                let n = u32::from_str_radix(h, 16).ok()?;
                return Some(Color {
                    a: (n >> 24) as u8,
                    r: (n >> 16) as u8,
                    g: (n >> 8) as u8,
                    b: n as u8,
                });
            }
            return None;
        }
        let (inner, has_alpha) = if let Some(i) = s.strip_prefix("rgba(") {
            (i.strip_suffix(')')?, true)
        } else {
            (s.strip_prefix("rgb(")?.strip_suffix(')')?, false)
        };
        let inner = inner.trim();
        if !inner.contains(',') {
            let want = if has_alpha { 8 } else { 6 };
            if inner.len() != want || !inner.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let p = |i: usize| u8::from_str_radix(&inner[i..i + 2], 16).ok();
            return Some(Color {
                r: p(0)?,
                g: p(2)?,
                b: p(4)?,
                a: if has_alpha { p(6)? } else { 255 },
            });
        }
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() != if has_alpha { 4 } else { 3 } {
            return None;
        }
        let ch = |i: usize| parts[i].parse::<u8>().ok();
        let a = if has_alpha {
            (parts[3].parse::<f64>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u8
        } else {
            255
        };
        Some(Color {
            r: ch(0)?,
            g: ch(1)?,
            b: ch(2)?,
            a,
        })
    }

    /// Canonical Hyprland form: `rgba(rrggbbaa)`.
    pub fn to_hypr(self) -> String {
        format!(
            "rgba({:02x}{:02x}{:02x}{:02x})",
            self.r, self.g, self.b, self.a
        )
    }
}

/// One or more colours and an optional angle: `rgba(..) rgba(..) 45deg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gradient {
    pub colors: Vec<Color>,
    pub angle: Option<u32>,
}

impl Gradient {
    pub fn parse(s: &str) -> Option<Gradient> {
        let mut colors = Vec::new();
        let mut angle = None;
        for tok in tokens(s) {
            if let Some(n) = tok.strip_suffix("deg") {
                // The angle must be last and appear once.
                if angle.is_some() {
                    return None;
                }
                angle = Some(n.parse().ok()?);
            } else if angle.is_some() {
                return None;
            } else {
                colors.push(Color::parse(tok)?);
            }
        }
        if colors.is_empty() {
            return None;
        }
        Some(Gradient { colors, angle })
    }

    pub fn to_hypr(&self) -> String {
        let mut parts: Vec<String> = self.colors.iter().map(|c| c.to_hypr()).collect();
        if let Some(a) = self.angle {
            parts.push(format!("{a}deg"));
        }
        parts.join(" ")
    }
}

/// Whitespace separated tokens, keeping `rgba(1, 2, 3, 1)` together.
fn tokens(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut depth) = (None, 0usize);
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if c.is_whitespace() && depth == 0 {
            if let Some(st) = start.take() {
                out.push(&s[st..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(st) = start {
        out.push(&s[st..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_color_forms() {
        let c = Color {
            r: 0x33,
            g: 0xcc,
            b: 0xff,
            a: 0xee,
        };
        assert_eq!(Color::parse("rgba(33ccffee)"), Some(c));
        assert_eq!(Color::parse("0xee33ccff"), Some(c));
        assert_eq!(Color::parse("rgb(33ccff)"), Some(Color { a: 255, ..c }));
        assert_eq!(
            Color::parse("rgba(51, 204, 255, 1)"),
            Some(Color { a: 255, ..c })
        );
        assert_eq!(Color::parse("rgb(zzzzzz)"), None);
        assert_eq!(Color::parse("red"), None);
        assert_eq!(c.to_hypr(), "rgba(33ccffee)");
    }

    #[test]
    fn parses_gradient() {
        let g = Gradient::parse("rgba(33ccffee) rgba(00ff99ee) 45deg").unwrap();
        assert_eq!(g.colors.len(), 2);
        assert_eq!(g.angle, Some(45));
        assert_eq!(g.to_hypr(), "rgba(33ccffee) rgba(00ff99ee) 45deg");
        let single = Gradient::parse("rgba(595959aa)").unwrap();
        assert_eq!((single.colors.len(), single.angle), (1, None));
        let hex = Gradient::parse("0xff112233 90deg").unwrap();
        assert_eq!((hex.colors.len(), hex.angle), (1, Some(90)));
        assert!(Gradient::parse("").is_none());
        assert!(Gradient::parse("45deg").is_none());
        assert!(Gradient::parse("blue").is_none());
        let spaced = Gradient::parse("rgba(1, 2, 3, 1) 0xff112233").unwrap();
        assert_eq!(spaced.colors.len(), 2);
        assert!(Gradient::parse("45deg rgba(33ccffee)").is_none());
    }
}
