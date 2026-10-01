//! Visual monitor layout editor with a "keep these settings?" countdown.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use hyprconf::Located;
use hyprgui_core::monitor::{logical_size, snap, Mode, MonitorCfg, Rect};
use hyprgui_core::LiveStatus;

use crate::i18n::t;
use crate::pages::lists;
use crate::state::AppState;
use crate::ui::{self, trim_float};

const TRANSFORMS: [&str; 8] = [
    "Normal",
    "90°",
    "180°",
    "270°",
    "Flipped",
    "Flipped 90°",
    "Flipped 180°",
    "Flipped 270°",
];
const REVERT_SECONDS: u32 = 15;

struct Mon {
    cfg: MonitorCfg,
    /// Value of the monitor line when the page was built, to detect edits and revert.
    baseline: String,
    /// Existing `monitor =` line, if the config already has one.
    loc: Option<Located>,
    /// Pixel size of the active mode.
    px: (u32, u32),
    /// Where to draw the monitor while its configured position is `auto`.
    draw_pos: (i32, i32),
    modes: Vec<Mode>,
    /// Mode to restore when a disabled monitor is switched back on.
    last_mode: String,
}

impl Mon {
    fn rect(&self) -> Rect {
        let (x, y) = self.cfg.position().unwrap_or(self.draw_pos);
        let (w, h) = logical_size(
            self.px.0,
            self.px.1,
            self.cfg.scale_f64(),
            self.cfg.transform(),
        );
        Rect {
            x: x as f64,
            y: y as f64,
            w,
            h,
        }
    }

    fn changed(&self) -> bool {
        self.cfg.to_value() != self.baseline
    }
}

struct Model {
    mons: Vec<Mon>,
    selected: usize,
}

fn load(state: &Rc<AppState>) -> Model {
    let s = state.session.borrow();
    let lines: Vec<(Located, MonitorCfg)> = s
        .list("monitor")
        .into_iter()
        .filter_map(|l| MonitorCfg::parse(&l.value).map(|m| (l, m)))
        .collect();
    let live = s.ipc().and_then(|c| c.monitors().ok()).unwrap_or_default();

    let mut mons = Vec::new();
    for m in &live {
        let hz = (m.refresh_rate * 100.0).round() / 100.0;
        let found = lines.iter().find(|(_, c)| c.name == m.name);
        // No config line yet: describe the monitor as it currently runs.
        let cfg = found.map(|(_, c)| c.clone()).unwrap_or_else(|| {
            let mut c = MonitorCfg {
                name: m.name.clone(),
                mode: if m.disabled {
                    "disable".into()
                } else {
                    Mode {
                        width: m.width,
                        height: m.height,
                        hz,
                    }
                    .to_config()
                },
                pos: format!("{}x{}", m.x, m.y),
                scale: trim_float(m.scale),
                extra: vec![],
            };
            c.set_transform(m.transform);
            c
        });
        mons.push(Mon {
            last_mode: if cfg.disabled() {
                "preferred".into()
            } else {
                cfg.mode.clone()
            },
            baseline: cfg.to_value(),
            cfg,
            loc: found.map(|(l, _)| l.clone()),
            px: (m.width, m.height),
            draw_pos: (m.x, m.y),
            modes: m
                .available_modes
                .iter()
                .filter_map(|s| Mode::parse(s))
                .collect(),
        });
    }
    // Without a live connection fall back to what the config says.
    if live.is_empty() {
        let mut next_x = 0;
        for (loc, c) in &lines {
            if c.name.is_empty() {
                continue;
            }
            let mode = Mode::parse(&c.mode);
            let px = mode.map_or((1920, 1080), |m| (m.width, m.height));
            let (w, _) = logical_size(px.0, px.1, c.scale_f64(), c.transform());
            let draw_pos = c.position().unwrap_or((next_x, 0));
            next_x = draw_pos.0 + w as i32;
            mons.push(Mon {
                last_mode: if c.disabled() {
                    "preferred".into()
                } else {
                    c.mode.clone()
                },
                baseline: c.to_value(),
                cfg: c.clone(),
                loc: Some(loc.clone()),
                px,
                draw_pos,
                modes: mode.into_iter().collect(),
            });
        }
    }
    Model { mons, selected: 0 }
}

pub fn build(state: &Rc<AppState>) -> gtk::Widget {
    let model = Rc::new(RefCell::new(load(state)));
    let page = adw::PreferencesPage::new();

    if model.borrow().mons.is_empty() {
        let status = adw::StatusPage::builder()
            .icon_name("video-display-symbolic")
            .title(t("No monitors detected"))
            .description(t(
                "Start HyprGUI inside Hyprland, or add monitor lines to your config.",
            ))
            .build();
        let g = adw::PreferencesGroup::new();
        g.add(&status);
        page.add(&g);
        page.add(&lists::list_group(
            state,
            "monitor",
            "monitor",
            "name, resolution@Hz, position, scale",
        ));
        page.add(&lists::list_group(
            state,
            "workspace",
            t("Workspace rules"),
            "id, rule:value",
        ));
        return page.upcast();
    }

    // ---- layout canvas ----------------------------------------------------
    let layout_group = adw::PreferencesGroup::builder()
        .title(t("Layout"))
        .description(t("Drag monitors to arrange them."))
        .build();
    let area = gtk::DrawingArea::builder()
        .content_height(260)
        .hexpand(true)
        .build();
    area.add_css_class("card");
    layout_group.add(&area);
    let apply_btn = gtk::Button::with_label(t("Preview"));
    apply_btn.add_css_class("suggested-action");
    apply_btn.set_sensitive(false);
    layout_group.set_header_suffix(Some(&apply_btn));
    page.add(&layout_group);

    // ---- selected monitor ---------------------------------------------------
    let sel_group = adw::PreferencesGroup::builder()
        .title(t("Selected monitor"))
        .build();
    let enabled = adw::SwitchRow::builder().title(t("Enabled")).build();
    let mode_row = adw::ComboRow::builder()
        .title(t("Resolution and refresh rate"))
        .build();
    let scale_row = adw::SpinRow::with_range(0.5, 4.0, 0.25);
    scale_row.set_title(t("Scale"));
    scale_row.set_digits(2);
    let rot_row = adw::ComboRow::builder().title(t("Rotation")).build();
    rot_row.set_model(Some(&gtk::StringList::new(&TRANSFORMS.map(|s| {
        if s == "Normal" {
            t("Normal")
        } else {
            s
        }
    }))));
    for r in [
        enabled.upcast_ref::<gtk::Widget>(),
        mode_row.upcast_ref(),
        scale_row.upcast_ref(),
        rot_row.upcast_ref(),
    ] {
        sel_group.add(r);
    }
    page.add(&sel_group);

    // Mode choices per selected monitor: "Preferred" + the monitor's modes.
    let mode_values: Rc<RefCell<Vec<String>>> = Rc::default();
    let updating = Rc::new(Cell::new(false));

    let refresh_controls: Rc<dyn Fn()> = {
        let (model, enabled, mode_row, scale_row, rot_row, mode_values, updating) = (
            model.clone(),
            enabled.clone(),
            mode_row.clone(),
            scale_row.clone(),
            rot_row.clone(),
            mode_values.clone(),
            updating.clone(),
        );
        Rc::new(move || {
            let m = model.borrow();
            let Some(mon) = m.mons.get(m.selected) else {
                return;
            };
            updating.set(true);
            sel_title(&enabled, &mon.cfg.name);
            enabled.set_active(!mon.cfg.disabled());
            let mut labels = vec![t("Preferred").to_string()];
            let mut values = vec!["preferred".to_string()];
            for md in &mon.modes {
                let v = md.to_config();
                if !values.contains(&v) {
                    labels.push(md.label());
                    values.push(v);
                }
            }
            let current = if mon.cfg.disabled() {
                mon.last_mode.clone()
            } else {
                mon.cfg.mode.clone()
            };
            // `@144` in the config should select the monitor's `@143.97` mode.
            let selected = values.iter().position(|v| *v == current).or_else(|| {
                let want = Mode::parse(&current)?;
                mon.modes
                    .iter()
                    .find(|m| {
                        m.width == want.width
                            && m.height == want.height
                            && (m.hz - want.hz).abs() < 0.5
                    })
                    .and_then(|m| values.iter().position(|v| *v == m.to_config()))
            });
            let selected = selected.unwrap_or_else(|| {
                labels.push(current.clone());
                values.push(current.clone());
                values.len() - 1
            });
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            mode_row.set_model(Some(&gtk::StringList::new(&refs)));
            mode_row.set_selected(selected as u32);
            *mode_values.borrow_mut() = values;
            scale_row.set_value(mon.cfg.scale_f64());
            rot_row.set_selected(mon.cfg.transform().min(7));
            let on = !mon.cfg.disabled();
            mode_row.set_sensitive(on);
            scale_row.set_sensitive(on);
            rot_row.set_sensitive(on);
            updating.set(false);
        })
    };

    let after_edit: Rc<dyn Fn()> = {
        let (model, area, apply_btn) = (model.clone(), area.clone(), apply_btn.clone());
        Rc::new(move || {
            area.queue_draw();
            apply_btn.set_sensitive(model.borrow().mons.iter().any(Mon::changed));
        })
    };

    // Control handlers write into the selected monitor.
    {
        let (model, up, ae, rc) = (
            model.clone(),
            updating.clone(),
            after_edit.clone(),
            refresh_controls.clone(),
        );
        enabled.connect_active_notify(move |r| {
            if up.get() {
                return;
            }
            {
                let mut m = model.borrow_mut();
                let sel = m.selected;
                let mon = &mut m.mons[sel];
                if r.is_active() {
                    mon.cfg.mode = mon.last_mode.clone();
                } else {
                    if !mon.cfg.disabled() {
                        mon.last_mode = mon.cfg.mode.clone();
                    }
                    mon.cfg.mode = "disable".into();
                }
            }
            rc();
            ae();
        });
    }
    {
        let (model, up, ae, mv) = (
            model.clone(),
            updating.clone(),
            after_edit.clone(),
            mode_values.clone(),
        );
        mode_row.connect_selected_notify(move |r| {
            if up.get() {
                return;
            }
            let Some(v) = mv.borrow().get(r.selected() as usize).cloned() else {
                return;
            };
            {
                let mut m = model.borrow_mut();
                let sel = m.selected;
                let mon = &mut m.mons[sel];
                if let Some(md) = Mode::parse(&v) {
                    mon.px = (md.width, md.height);
                } else if let Some(best) = mon.modes.first() {
                    mon.px = (best.width, best.height);
                }
                mon.cfg.mode = v.clone();
                mon.last_mode = v;
            }
            ae();
        });
    }
    {
        let (model, up, ae) = (model.clone(), updating.clone(), after_edit.clone());
        scale_row.connect_value_notify(move |r| {
            if up.get() {
                return;
            }
            {
                let mut m = model.borrow_mut();
                let sel = m.selected;
                m.mons[sel].cfg.scale = trim_float(r.value());
            }
            ae();
        });
    }
    {
        let (model, up, ae) = (model.clone(), updating.clone(), after_edit.clone());
        rot_row.connect_selected_notify(move |r| {
            if up.get() {
                return;
            }
            {
                let mut m = model.borrow_mut();
                let sel = m.selected;
                m.mons[sel].cfg.set_transform(r.selected());
            }
            ae();
        });
    }

    // ---- drawing ---------------------------------------------------------------
    let geometry = {
        let model = model.clone();
        move |w: f64, h: f64| -> (f64, f64, f64) {
            let m = model.borrow();
            let rects: Vec<Rect> = m.mons.iter().map(Mon::rect).collect();
            let (minx, miny) = rects
                .iter()
                .fold((f64::MAX, f64::MAX), |a, r| (a.0.min(r.x), a.1.min(r.y)));
            let (maxx, maxy) = rects.iter().fold((f64::MIN, f64::MIN), |a, r| {
                (a.0.max(r.x + r.w), a.1.max(r.y + r.h))
            });
            // Leave room around the monitors so there is space to drag them.
            let (bw, bh) = ((maxx - minx).max(1.0) * 1.5, (maxy - miny).max(1.0) * 1.5);
            let s = ((w - 32.0) / bw).min((h - 32.0) / bh).max(0.01);
            let ox = (w - (maxx - minx) * s) / 2.0 - minx * s;
            let oy = (h - (maxy - miny) * s) / 2.0 - miny * s;
            (s, ox, oy)
        }
    };
    let geometry = Rc::new(geometry);
    {
        let (model, geometry) = (model.clone(), geometry.clone());
        area.set_draw_func(move |a, cr, w, h| {
            let (s, ox, oy) = geometry(w as f64, h as f64);
            let m = model.borrow();
            #[allow(deprecated)]
            let ctx = a.style_context();
            #[allow(deprecated)]
            let accent = ctx
                .lookup_color("accent_bg_color")
                .unwrap_or(gtk::gdk::RGBA::new(0.4, 0.4, 0.9, 1.0));
            #[allow(deprecated)]
            let card = ctx
                .lookup_color("headerbar_bg_color")
                .unwrap_or(gtk::gdk::RGBA::new(0.2, 0.2, 0.2, 1.0));
            let fg = a.color();
            for (i, mon) in m.mons.iter().enumerate() {
                let r = mon.rect();
                let (x, y, rw, rh) = (ox + r.x * s, oy + r.y * s, r.w * s, r.h * s);
                let radius = 8.0;
                cr.new_sub_path();
                cr.arc(
                    x + rw - radius,
                    y + radius,
                    radius,
                    -std::f64::consts::FRAC_PI_2,
                    0.0,
                );
                cr.arc(
                    x + rw - radius,
                    y + rh - radius,
                    radius,
                    0.0,
                    std::f64::consts::FRAC_PI_2,
                );
                cr.arc(
                    x + radius,
                    y + rh - radius,
                    radius,
                    std::f64::consts::FRAC_PI_2,
                    std::f64::consts::PI,
                );
                cr.arc(
                    x + radius,
                    y + radius,
                    radius,
                    std::f64::consts::PI,
                    1.5 * std::f64::consts::PI,
                );
                cr.close_path();
                let selected = i == m.selected;
                let alpha = if mon.cfg.disabled() { 0.35 } else { 1.0 };
                cr.set_source_rgba(
                    card.red() as f64,
                    card.green() as f64,
                    card.blue() as f64,
                    alpha,
                );
                let _ = cr.fill_preserve();
                if selected {
                    cr.set_source_rgba(
                        accent.red() as f64,
                        accent.green() as f64,
                        accent.blue() as f64,
                        1.0,
                    );
                    cr.set_line_width(3.0);
                } else {
                    cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.35);
                    cr.set_line_width(1.5);
                }
                let _ = cr.stroke();
                cr.set_source_rgba(
                    fg.red() as f64,
                    fg.green() as f64,
                    fg.blue() as f64,
                    alpha.max(0.6),
                );
                cr.select_font_face(
                    "sans",
                    gtk::cairo::FontSlant::Normal,
                    gtk::cairo::FontWeight::Bold,
                );
                cr.set_font_size(13.0);
                let name = if mon.cfg.name.is_empty() {
                    "*".to_string()
                } else {
                    mon.cfg.name.clone()
                };
                if let Ok(e) = cr.text_extents(&name) {
                    cr.move_to(
                        x + (rw - e.width()) / 2.0 - e.x_bearing(),
                        y + rh / 2.0 - 2.0,
                    );
                    let _ = cr.show_text(&name);
                }
                cr.select_font_face(
                    "sans",
                    gtk::cairo::FontSlant::Normal,
                    gtk::cairo::FontWeight::Normal,
                );
                cr.set_font_size(11.0);
                let info = format!("{}×{}", mon.px.0, mon.px.1);
                if let Ok(e) = cr.text_extents(&info) {
                    cr.move_to(
                        x + (rw - e.width()) / 2.0 - e.x_bearing(),
                        y + rh / 2.0 + 14.0,
                    );
                    let _ = cr.show_text(&info);
                }
            }
        });
    }

    // Drag to move, click to select.
    let drag = gtk::GestureDrag::new();
    let start: Rc<Cell<Option<(usize, i32, i32)>>> = Rc::new(Cell::new(None));
    {
        let (model, geometry, start, area2, rc) = (
            model.clone(),
            geometry.clone(),
            start.clone(),
            area.clone(),
            refresh_controls.clone(),
        );
        drag.connect_drag_begin(move |_, px, py| {
            let (s, ox, oy) = geometry(area2.width() as f64, area2.height() as f64);
            let hit = {
                let m = model.borrow();
                // Topmost (last drawn) monitor under the pointer wins.
                m.mons
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, mon)| {
                        let r = mon.rect();
                        px >= ox + r.x * s
                            && px <= ox + (r.x + r.w) * s
                            && py >= oy + r.y * s
                            && py <= oy + (r.y + r.h) * s
                    })
                    .map(|(i, mon)| (i, mon.cfg.position().unwrap_or(mon.draw_pos)))
            };
            match hit {
                Some((i, (x, y))) => {
                    model.borrow_mut().selected = i;
                    start.set(Some((i, x, y)));
                    rc();
                    area2.queue_draw();
                }
                None => start.set(None),
            }
        });
    }
    {
        let (model, geometry, start, area2, ae) = (
            model.clone(),
            geometry.clone(),
            start.clone(),
            area.clone(),
            after_edit.clone(),
        );
        drag.connect_drag_update(move |_, dx, dy| {
            let Some((i, x0, y0)) = start.get() else {
                return;
            };
            let (s, _, _) = geometry(area2.width() as f64, area2.height() as f64);
            {
                let mut m = model.borrow_mut();
                let mut r = m.mons[i].rect();
                r.x = x0 as f64 + dx / s;
                r.y = y0 as f64 + dy / s;
                let others: Vec<Rect> = m
                    .mons
                    .iter()
                    .enumerate()
                    .filter(|(j, mm)| *j != i && !mm.cfg.disabled())
                    .map(|(_, mm)| mm.rect())
                    .collect();
                let (nx, ny) = snap(r, &others, 24.0 / s);
                m.mons[i]
                    .cfg
                    .set_position(nx.round() as i32, ny.round() as i32);
            }
            ae();
        });
    }
    area.add_controller(drag);

    // ---- preview + keep/revert --------------------------------------------------
    {
        let (st, model) = (state.clone(), model.clone());
        apply_btn.connect_clicked(move |b| preview(b, &st, &model));
    }

    refresh_controls();
    page.add(&lists::list_group(
        state,
        "workspace",
        t("Workspace rules"),
        "id, rule:value",
    ));
    page.upcast()
}

fn sel_title(row: &adw::SwitchRow, name: &str) {
    row.set_title(&format!(
        "{}  ·  {}",
        name_or_default(name),
        crate::i18n::t("Enabled")
    ));
}

fn name_or_default(n: &str) -> &str {
    if n.is_empty() {
        "*"
    } else {
        n
    }
}

/// Sends the edited monitor lines live, then asks to keep them. Without an
/// answer within [`REVERT_SECONDS`] everything is restored.
fn preview(parent: &impl IsA<gtk::Widget>, state: &Rc<AppState>, model: &Rc<RefCell<Model>>) {
    let changed: Vec<(MonitorCfg, String, Option<Located>)> = model
        .borrow()
        .mons
        .iter()
        .filter(|m| m.changed())
        .map(|m| (m.cfg.clone(), m.baseline.clone(), m.loc.clone()))
        .collect();
    if changed.is_empty() {
        return;
    }
    let write = {
        let (st, changed) = (state.clone(), changed.clone());
        move || {
            {
                let mut s = st.session.borrow_mut();
                for (cfg, _, loc) in &changed {
                    match loc {
                        Some(l) => {
                            s.list_replace(l, &cfg.to_value());
                        }
                        None => s.list_push("monitor", &cfg.to_value()),
                    }
                }
            }
            st.notify_changed();
            st.request_rebuild();
        }
    };

    let (live, mut errors) = (state.session.borrow().ipc().is_some(), Vec::new());
    if !live {
        write();
        state.toast(t(
            "Previewing is not available outside Hyprland. Changes were written to the config.",
        ));
        return;
    }
    for (cfg, _, _) in &changed {
        if let LiveStatus::Failed(e) = state
            .session
            .borrow()
            .preview_raw("monitor", &cfg.to_value())
        {
            errors.push(e);
        }
    }
    if !errors.is_empty() {
        for (_, base, _) in &changed {
            state.session.borrow().preview_raw("monitor", base);
        }
        ui::message_dialog(
            parent,
            t("Could not save"),
            &t("Could not apply: {}").replace("{}", &errors.join("\n")),
        );
        return;
    }

    let dialog = adw::AlertDialog::new(
        Some(t("Keep these display settings?")),
        Some(&t("Reverting in {} s…").replace("{}", &REVERT_SECONDS.to_string())),
    );
    dialog.add_response("revert", t("Revert"));
    dialog.add_response("keep", t("Keep"));
    dialog.set_response_appearance("keep", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("revert"));
    dialog.set_close_response("revert");

    let done = Rc::new(Cell::new(false));
    let left = Rc::new(Cell::new(REVERT_SECONDS));
    {
        let (d, done, left) = (dialog.clone(), done.clone(), left.clone());
        glib::timeout_add_seconds_local(1, move || {
            if done.get() {
                return glib::ControlFlow::Break;
            }
            left.set(left.get().saturating_sub(1));
            if left.get() == 0 {
                d.close();
                return glib::ControlFlow::Break;
            }
            d.set_body(&t("Reverting in {} s…").replace("{}", &left.get().to_string()));
            glib::ControlFlow::Continue
        });
    }
    let (st, done2) = (state.clone(), done.clone());
    dialog.choose(parent, gtk::gio::Cancellable::NONE, move |r| {
        done2.set(true);
        if r == "keep" {
            write();
        } else {
            for (_, base, _) in &changed {
                st.session.borrow().preview_raw("monitor", base);
            }
            st.request_rebuild();
        }
    });
}
