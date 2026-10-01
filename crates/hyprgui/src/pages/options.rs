//! Pages generated from the option schema (Appearance, Input).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use hyprgui_core::LiveStatus;
use hyprschema::{parse_bool, Color, Gradient, Kind, Opt};

use crate::i18n::lang;
use crate::state::AppState;
use crate::ui::{self, trim_float};

type Refresh = Rc<dyn Fn()>;

pub fn build(state: &Rc<AppState>, page_id: &str) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let schema = state.session.borrow().schema.clone();
    let deps: Rc<RefCell<Vec<(gtk::Widget, String)>>> = Rc::default();

    let st = state.clone();
    let d = deps.clone();
    let refresh: Refresh = Rc::new(move || {
        let s = st.session.borrow();
        for (w, key) in d.borrow().iter() {
            let on = s
                .schema
                .option(key)
                .map(|o| parse_bool(&s.value_or_default(o)).unwrap_or(false))
                .unwrap_or(true);
            w.set_sensitive(on);
        }
    });

    for g in schema.groups_of(page_id) {
        let group = adw::PreferencesGroup::builder()
            .title(g.title.get(lang()))
            .build();
        for o in schema.options_of(page_id, &g.id) {
            let row = option_row(state, o, &refresh);
            if let Some(dep) = &o.depends {
                deps.borrow_mut().push((row.clone(), dep.clone()));
            }
            group.add(&row);
        }
        page.add(&group);
    }
    refresh();
    page.upcast()
}

/// Writes the value and previews it; toasts if Hyprland refuses the preview.
fn commit(state: &Rc<AppState>, key: &str, value: &str) {
    let res = state.session.borrow_mut().set_option(key, value);
    if let Ok(LiveStatus::Failed(msg)) = res {
        state.toast(&format!("{key}: {msg}"));
    }
    state.notify_changed();
}

fn subtitle(state: &Rc<AppState>, o: &Opt) -> String {
    let mut s = o
        .desc
        .as_ref()
        .map(|d| d.get(lang()).to_string())
        .unwrap_or_default();
    if let Some(p) = state.session.borrow().defined_elsewhere(&o.key) {
        if !s.is_empty() {
            s.push('\n');
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        s.push_str(&format!("{} {}", crate::i18n::t("Defined in"), name));
    }
    s
}

fn option_row(state: &Rc<AppState>, o: &Opt, refresh: &Refresh) -> gtk::Widget {
    let value = state.session.borrow().value_or_default(o);
    let title = o.label.get(lang());
    let sub = subtitle(state, o);
    let key = o.key.clone();

    match &o.kind {
        Kind::Bool => {
            let row = adw::SwitchRow::builder()
                .title(title)
                .subtitle(&sub)
                .build();
            row.set_active(parse_bool(&value).unwrap_or(false));
            let (st, r) = (state.clone(), refresh.clone());
            row.connect_active_notify(move |r2| {
                commit(&st, &key, if r2.is_active() { "true" } else { "false" });
                r();
            });
            row.upcast()
        }
        Kind::Int { min, max } => {
            let row = adw::SpinRow::with_range(*min as f64, *max as f64, 1.0);
            row.set_title(title);
            row.set_subtitle(&sub);
            row.set_value(value.trim().parse::<f64>().unwrap_or(0.0));
            let st = state.clone();
            row.connect_value_notify(move |r| {
                commit(&st, &key, &(r.value().round() as i64).to_string())
            });
            row.upcast()
        }
        Kind::Float { min, max, step } => {
            let row = adw::SpinRow::with_range(*min, *max, *step);
            row.set_title(title);
            row.set_subtitle(&sub);
            row.set_digits(if *step < 0.01 { 4 } else { 2 });
            row.set_value(value.trim().parse::<f64>().unwrap_or(0.0));
            let st = state.clone();
            row.connect_value_notify(move |r| commit(&st, &key, &trim_float(r.value())));
            row.upcast()
        }
        Kind::Choice { choices } => {
            let row = adw::ComboRow::builder().title(title).subtitle(&sub).build();
            let strs: Vec<&str> = choices.iter().map(String::as_str).collect();
            row.set_model(Some(&gtk::StringList::new(&strs)));
            row.set_selected(choices.iter().position(|c| *c == value.trim()).unwrap_or(0) as u32);
            let (st, ch) = (state.clone(), choices.clone());
            row.connect_selected_notify(move |r| {
                if let Some(c) = ch.get(r.selected() as usize) {
                    commit(&st, &key, c);
                }
            });
            row.upcast()
        }
        Kind::Text => {
            let row = adw::EntryRow::builder().title(title).build();
            row.set_text(&value);
            if !sub.is_empty() {
                row.set_tooltip_text(Some(&sub));
            }
            let st = state.clone();
            row.connect_changed(move |r| commit(&st, &key, r.text().as_str()));
            row.upcast()
        }
        Kind::Color => {
            let row = adw::ActionRow::builder()
                .title(title)
                .subtitle(&sub)
                .build();
            let c = Color::parse(&value).unwrap_or(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            });
            let btn = color_button(c);
            let st = state.clone();
            btn.connect_rgba_notify(move |b| commit(&st, &key, &from_rgba(&b.rgba()).to_hypr()));
            row.add_suffix(&btn);
            row.set_activatable_widget(Some(&btn));
            row.upcast()
        }
        Kind::Gradient => gradient_row(state, title, &sub, &key, &value),
    }
}

fn color_button(c: Color) -> gtk::ColorDialogButton {
    let dialog = gtk::ColorDialog::new();
    dialog.set_with_alpha(true);
    let btn = gtk::ColorDialogButton::new(Some(dialog));
    btn.set_valign(gtk::Align::Center);
    btn.set_rgba(&to_rgba(c));
    btn
}

fn to_rgba(c: Color) -> gdk::RGBA {
    gdk::RGBA::new(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    )
}

fn from_rgba(c: &gdk::RGBA) -> Color {
    let ch = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    Color {
        r: ch(c.red()),
        g: ch(c.green()),
        b: ch(c.blue()),
        a: ch(c.alpha()),
    }
}

/// Row with up to four colour buttons, add/remove buttons and an angle.
fn gradient_row(
    state: &Rc<AppState>,
    title: &str,
    sub: &str,
    key: &str,
    value: &str,
) -> gtk::Widget {
    const MAX_COLORS: usize = 4;
    let grad = Rc::new(RefCell::new(Gradient::parse(value).unwrap_or(Gradient {
        colors: vec![Color {
            r: 128,
            g: 128,
            b: 128,
            a: 255,
        }],
        angle: None,
    })));
    let row = adw::ActionRow::builder().title(title).subtitle(sub).build();
    let holder = gtk::Box::builder()
        .spacing(6)
        .valign(gtk::Align::Center)
        .build();
    row.add_suffix(&holder);

    let rebuilding = Rc::new(Cell::new(false));
    let rebuild: ui::Hook = Rc::default();

    let (st, g, h, key_s, rb, slot) = (
        state.clone(),
        grad.clone(),
        holder.clone(),
        key.to_string(),
        rebuilding.clone(),
        rebuild.clone(),
    );
    let build: Rc<dyn Fn()> = Rc::new(move || {
        rb.set(true);
        while let Some(child) = h.first_child() {
            h.remove(&child);
        }
        let colors = g.borrow().colors.clone();
        for (i, c) in colors.iter().enumerate() {
            let btn = color_button(*c);
            let (st2, g2, k2, rb2) = (st.clone(), g.clone(), key_s.clone(), rb.clone());
            btn.connect_rgba_notify(move |b| {
                if rb2.get() {
                    return;
                }
                g2.borrow_mut().colors[i] = from_rgba(&b.rgba());
                commit(&st2, &k2, &g2.borrow().to_hypr());
            });
            h.append(&btn);
        }
        let mk = |icon: &str| {
            let b = gtk::Button::from_icon_name(icon);
            b.add_css_class("flat");
            b.set_valign(gtk::Align::Center);
            b
        };
        let (add, del) = (mk("list-add-symbolic"), mk("list-remove-symbolic"));
        add.set_sensitive(colors.len() < MAX_COLORS);
        del.set_sensitive(colors.len() > 1);
        for (btn, adding) in [(&add, true), (&del, false)] {
            let (st2, g2, k2, slot2) = (st.clone(), g.clone(), key_s.clone(), slot.clone());
            btn.connect_clicked(move |_| {
                {
                    let mut gr = g2.borrow_mut();
                    if adding {
                        let last = *gr.colors.last().expect("at least one colour");
                        gr.colors.push(last);
                    } else {
                        gr.colors.pop();
                    }
                }
                commit(&st2, &k2, &g2.borrow().to_hypr());
                let f = slot2.borrow().clone();
                if let Some(f) = f {
                    crate::ui::later(move || f());
                }
            });
        }
        h.append(&del);
        h.append(&add);

        let angle = gtk::SpinButton::with_range(0.0, 360.0, 5.0);
        angle.set_valign(gtk::Align::Center);
        angle.set_tooltip_text(Some("°"));
        angle.set_value(g.borrow().angle.unwrap_or(0) as f64);
        let (st2, g2, k2, rb2) = (st.clone(), g.clone(), key_s.clone(), rb.clone());
        angle.connect_value_changed(move |s| {
            if rb2.get() {
                return;
            }
            g2.borrow_mut().angle = Some(s.value() as u32);
            commit(&st2, &k2, &g2.borrow().to_hypr());
        });
        // An angle only means something for a real gradient.
        let multi = colors.len() > 1 || g.borrow().angle.is_some();
        angle.set_visible(multi);
        h.append(&angle);
        let deg = gtk::Label::new(Some("°"));
        deg.set_visible(multi);
        h.append(&deg);
        rb.set(false);
    });
    *rebuild.borrow_mut() = Some(build.clone());
    build();
    row.upcast()
}
