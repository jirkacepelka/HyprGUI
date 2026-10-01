//! `monitor = name, mode, position, scale[, extras…]` lines.

/// One parsed `monitor` line. Fields stay as strings to keep Hyprland's
/// special values (`preferred`, `auto`, `highrr`, `disable`) intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorCfg {
    pub name: String,
    pub mode: String,
    pub pos: String,
    pub scale: String,
    /// Remaining fields, e.g. `["transform", "1"]` or `["mirror", "DP-1"]`.
    pub extra: Vec<String>,
}

impl MonitorCfg {
    pub fn parse(value: &str) -> Option<MonitorCfg> {
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        let name = parts
            .first()
            .filter(|n| !n.is_empty() || parts.len() > 1)?
            .to_string();
        let get = |i: usize, d: &str| parts.get(i).map_or(d.to_string(), |s| s.to_string());
        Some(MonitorCfg {
            name,
            mode: get(1, "preferred"),
            pos: get(2, "auto"),
            scale: get(3, "1"),
            extra: parts.iter().skip(4).map(|s| s.to_string()).collect(),
        })
    }

    pub fn to_value(&self) -> String {
        if self.mode == "disable" {
            return format!("{}, disable", self.name);
        }
        let mut v = vec![
            self.name.clone(),
            self.mode.clone(),
            self.pos.clone(),
            self.scale.clone(),
        ];
        v.extend(self.extra.iter().cloned());
        v.join(", ")
    }

    pub fn disabled(&self) -> bool {
        self.mode == "disable"
    }

    pub fn transform(&self) -> u32 {
        self.extra
            .windows(2)
            .find(|w| w[0] == "transform")
            .and_then(|w| w[1].parse().ok())
            .unwrap_or(0)
    }

    pub fn set_transform(&mut self, t: u32) {
        self.extra = {
            let mut out = Vec::new();
            let mut skip = false;
            for (i, e) in self.extra.iter().enumerate() {
                if skip {
                    skip = false;
                } else if e == "transform" && i + 1 < self.extra.len() {
                    skip = true;
                } else {
                    out.push(e.clone());
                }
            }
            out
        };
        if t != 0 {
            self.extra.push("transform".into());
            self.extra.push(t.to_string());
        }
    }

    /// `(x, y)` when the position is explicit.
    pub fn position(&self) -> Option<(i32, i32)> {
        let (x, y) = self.pos.split_once('x')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.pos = format!("{x}x{y}");
    }

    pub fn scale_f64(&self) -> f64 {
        self.scale
            .parse()
            .ok()
            .filter(|s: &f64| *s > 0.0)
            .unwrap_or(1.0)
    }
}

/// A resolution + refresh rate such as `2560x1440@143.97Hz`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub hz: f64,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        let s = s.trim();
        let (res, rate) = match s.split_once('@') {
            Some((r, h)) => (r, h.trim_end_matches("Hz").parse().ok()?),
            None => (s, 0.0),
        };
        let (w, h) = res.split_once('x')?;
        Some(Mode {
            width: w.parse().ok()?,
            height: h.parse().ok()?,
            hz: rate,
        })
    }

    /// Hyprland config form: `2560x1440@144` (rate rounded).
    pub fn to_config(self) -> String {
        if self.hz > 0.0 {
            format!("{}x{}@{}", self.width, self.height, trim_float(self.hz))
        } else {
            format!("{}x{}", self.width, self.height)
        }
    }

    /// Label for lists: `2560×1440 @ 144 Hz`.
    pub fn label(self) -> String {
        format!(
            "{}×{} @ {} Hz",
            self.width,
            self.height,
            trim_float(self.hz)
        )
    }
}

fn trim_float(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 0.005 {
        format!("{}", r.round() as i64)
    } else {
        format!("{r}")
    }
}

/// Axis-aligned rectangle in layout (logical pixel) space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Size on the layout after scaling and rotation (odd transforms swap axes).
pub fn logical_size(width: u32, height: u32, scale: f64, transform: u32) -> (f64, f64) {
    let (w, h) = if transform % 2 == 1 {
        (height, width)
    } else {
        (width, height)
    };
    (w as f64 / scale, h as f64 / scale)
}

/// Snaps `moving` so its edges line up with edges of `others` when within
/// `threshold`. Returns the adjusted top-left corner.
pub fn snap(moving: Rect, others: &[Rect], threshold: f64) -> (f64, f64) {
    let (mut best_dx, mut best_dy) = (None::<f64>, None::<f64>);
    let consider = |best: &mut Option<f64>, d: f64| {
        if d.abs() <= threshold && best.is_none_or(|b| d.abs() < b.abs()) {
            *best = Some(d);
        }
    };
    for o in others {
        // Horizontal: left/right edges against left/right edges.
        for (m, t) in [
            (moving.x, o.x + o.w),
            (moving.x + moving.w, o.x),
            (moving.x, o.x),
            (moving.x + moving.w, o.x + o.w),
        ] {
            consider(&mut best_dx, t - m);
        }
        for (m, t) in [
            (moving.y, o.y + o.h),
            (moving.y + moving.h, o.y),
            (moving.y, o.y),
            (moving.y + moving.h, o.y + o.h),
        ] {
            consider(&mut best_dy, t - m);
        }
    }
    (
        moving.x + best_dx.unwrap_or(0.0),
        moving.y + best_dy.unwrap_or(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_monitor_lines() {
        let m = MonitorCfg::parse("eDP-1, 1920x1080@60, 0x0, 1").unwrap();
        assert_eq!(
            (
                m.name.as_str(),
                m.mode.as_str(),
                m.pos.as_str(),
                m.scale.as_str()
            ),
            ("eDP-1", "1920x1080@60", "0x0", "1")
        );
        assert_eq!(m.to_value(), "eDP-1, 1920x1080@60, 0x0, 1");
        assert_eq!(m.position(), Some((0, 0)));
        let d = MonitorCfg::parse("HDMI-A-1, disable").unwrap();
        assert!(d.disabled());
        assert_eq!(d.to_value(), "HDMI-A-1, disable");
        let fallback = MonitorCfg::parse(", preferred, auto, 1").unwrap();
        assert_eq!(fallback.name, "");
        assert_eq!(MonitorCfg::parse("DP-1").unwrap().mode, "preferred");
    }

    #[test]
    fn transform_roundtrips_and_keeps_other_extras() {
        let mut m =
            MonitorCfg::parse("DP-1, 2560x1440@144, 0x0, 1, bitdepth, 10, transform, 1").unwrap();
        assert_eq!(m.transform(), 1);
        m.set_transform(3);
        assert_eq!(m.transform(), 3);
        assert!(m.to_value().contains("bitdepth, 10"));
        m.set_transform(0);
        assert_eq!(m.transform(), 0);
        assert_eq!(m.to_value(), "DP-1, 2560x1440@144, 0x0, 1, bitdepth, 10");
    }

    #[test]
    fn mode_parsing() {
        let m = Mode::parse("2560x1440@143.97Hz").unwrap();
        assert_eq!((m.width, m.height), (2560, 1440));
        assert_eq!(m.to_config(), "2560x1440@143.97");
        assert_eq!(
            Mode::parse("1920x1080@60.00Hz").unwrap().to_config(),
            "1920x1080@60"
        );
        assert_eq!(Mode::parse("1920x1080").unwrap().to_config(), "1920x1080");
        assert_eq!(
            Mode::parse("1920x1080@60.00Hz").unwrap().label(),
            "1920×1080 @ 60 Hz"
        );
        assert!(Mode::parse("preferred").is_none());
    }

    #[test]
    fn logical_size_applies_scale_and_rotation() {
        assert_eq!(logical_size(2560, 1440, 2.0, 0), (1280.0, 720.0));
        assert_eq!(logical_size(1920, 1080, 1.0, 1), (1080.0, 1920.0));
    }

    #[test]
    fn snapping_aligns_edges_within_threshold() {
        let other = Rect {
            x: 0.0,
            y: 0.0,
            w: 1920.0,
            h: 1080.0,
        };
        let moving = Rect {
            x: 1925.0,
            y: 12.0,
            w: 1280.0,
            h: 720.0,
        };
        assert_eq!(snap(moving, &[other], 20.0), (1920.0, 0.0));
        let far = Rect {
            x: 2100.0,
            y: 400.0,
            ..moving
        };
        assert_eq!(snap(far, &[other], 20.0), (2100.0, 400.0));
    }
}
