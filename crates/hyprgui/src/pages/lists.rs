//! Editors for repeatable keywords (`exec-once`, `windowrulev2`, `bezier`, …).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use hyprgui_core::rules;

use crate::i18n::t;
use crate::state::AppState;
use crate::ui;

/// A group listing every `key = value` line with inline editing, deletion and
/// a "new entry" row.
pub fn list_group(
    state: &Rc<AppState>,
    key: &'static str,
    title: &str,
    description: &str,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(title)
        .description(description)
        .build();
    let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::default();
    let slot: ui::Hook = Rc::default();

    let new_row = adw::EntryRow::builder()
        .title(t("New entry"))
        .show_apply_button(true)
        .build();
    group.add(&new_row);

    let (st, g, rs, sl, nr) = (
        state.clone(),
        group.clone(),
        rows.clone(),
        slot.clone(),
        new_row.clone(),
    );
    let rebuild: Rc<dyn Fn()> = Rc::new(move || {
        for r in rs.borrow_mut().drain(..) {
            g.remove(&r);
        }
        let items = st.session.borrow().list(key);
        // Keep the "new entry" row last: remove and re-add after the items.
        g.remove(&nr);
        for item in items {
            let row = adw::EntryRow::builder().title(key).build();
            row.set_text(&item.value);
            let (st2, it) = (st.clone(), item.clone());
            row.connect_changed(move |r| {
                st2.session
                    .borrow_mut()
                    .list_replace(&it, r.text().as_str());
                st2.notify_changed();
            });
            let del = gtk::Button::from_icon_name("user-trash-symbolic");
            del.add_css_class("flat");
            del.set_valign(gtk::Align::Center);
            del.set_tooltip_text(Some(t("Delete")));
            let (st2, it, sl2) = (st.clone(), item.clone(), sl.clone());
            del.connect_clicked(move |_| {
                st2.session.borrow_mut().list_remove(&it);
                st2.notify_changed();
                let f = sl2.borrow().clone();
                if let Some(f) = f {
                    ui::later(move || f());
                }
            });
            row.add_suffix(&del);
            g.add(&row);
            rs.borrow_mut().push(row.upcast());
        }
        g.add(&nr);
    });
    *slot.borrow_mut() = Some(rebuild.clone());

    let (st, sl) = (state.clone(), slot.clone());
    new_row.connect_apply(move |r| {
        let text = r.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        st.session.borrow_mut().list_push(key, &text);
        r.set_text("");
        st.notify_changed();
        let f = sl.borrow().clone();
        if let Some(f) = f {
            ui::later(move || f());
        }
    });
    rebuild();
    group
}

/// Menu button whose popover lists `(label, value)` pairs.
pub fn picker_button(
    items: Vec<(String, String)>,
    tooltip: &str,
    on_pick: impl Fn(&str) + 'static,
) -> gtk::MenuButton {
    let btn = gtk::MenuButton::builder()
        .icon_name("pan-down-symbolic")
        .valign(gtk::Align::Center)
        .build();
    btn.add_css_class("flat");
    btn.set_tooltip_text(Some(tooltip));
    let pop = gtk::Popover::new();
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    for (label, _) in &items {
        let l = gtk::Label::builder().label(label).xalign(0.0).build();
        list.append(&l);
    }
    let pop2 = pop.clone();
    list.connect_row_activated(move |_, row| {
        if let Some((_, v)) = items.get(row.index() as usize) {
            on_pick(v);
        }
        pop2.popdown();
    });
    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .propagate_natural_height(true)
        .max_content_height(320)
        .min_content_width(260)
        .build();
    pop.set_child(Some(&scroll));
    btn.set_popover(Some(&pop));
    btn
}

/// Header button + dialog that composes a `windowrulev2` line.
fn add_rule_button(state: &Rc<AppState>, rebuild_target: &adw::PreferencesGroup) -> gtk::Button {
    let btn = gtk::Button::with_label(t("Add a rule"));
    btn.add_css_class("flat");
    let st = state.clone();
    let _ = rebuild_target;
    btn.connect_clicked(move |b| {
        let content = adw::PreferencesGroup::new();
        let rule = adw::EntryRow::builder().title(t("Rule")).build();
        let class = adw::EntryRow::builder().title(t("Window class")).build();
        let title = adw::EntryRow::builder().title(t("Window title")).build();

        let common: Vec<(String, String)> = rules::COMMON_RULES
            .iter()
            .map(|r| (r.to_string(), r.to_string()))
            .collect();
        let r2 = rule.clone();
        rule.add_suffix(&picker_button(common, t("Rule"), move |v| r2.set_text(v)));

        let windows: Vec<(String, String)> = st
            .session
            .borrow()
            .ipc()
            .and_then(|c| c.windows().ok())
            .map(|ws| {
                ws.into_iter()
                    .filter(|w| !w.class.is_empty())
                    .map(|w| (format!("{}  ·  {}", w.class, w.title), w.class))
                    .collect()
            })
            .unwrap_or_default();
        if !windows.is_empty() {
            let c2 = class.clone();
            class.add_suffix(&picker_button(
                windows,
                t("Pick from open windows"),
                move |v| c2.set_text(v),
            ));
        }
        content.add(&rule);
        content.add(&class);
        content.add(&title);
        let wrapper = gtk::Box::builder()
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        wrapper.append(&content);
        content.set_hexpand(true);

        let st2 = st.clone();
        let (rule, class, title) = (rule.clone(), class.clone(), title.clone());
        ui::form_dialog(b, t("Add a rule"), &wrapper, t("Add"), move || {
            match rules::compose_windowrule(
                rule.text().as_str(),
                class.text().as_str(),
                title.text().as_str(),
            ) {
                Some(line) => {
                    st2.session.borrow_mut().list_push("windowrulev2", &line);
                    st2.notify_changed();
                    st2.request_rebuild();
                    true
                }
                None => false,
            }
        });
    });
    btn
}

pub fn animations_page(state: &Rc<AppState>) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    page.add(&list_group(
        state,
        "bezier",
        t("Bezier curves"),
        "name, x0, y0, x1, y1",
    ));
    page.add(&list_group(
        state,
        "animation",
        t("Animation rules"),
        "name, onoff, speed, curve [, style]",
    ));
    page.upcast()
}

pub fn rules_page(state: &Rc<AppState>) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let g = list_group(
        state,
        "windowrulev2",
        t("Window rules"),
        t("Add a rule for a window class or title."),
    );
    g.set_header_suffix(Some(&add_rule_button(state, &g)));
    page.add(&g);
    page.add(&list_group(state, "windowrule", "windowrule", ""));
    page.upcast()
}

pub fn autostart_page(state: &Rc<AppState>) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    page.add(&list_group(
        state,
        "exec-once",
        t("Run on start"),
        t("Commands run once when Hyprland starts"),
    ));
    page.add(&list_group(state, "exec", t("Run on every reload"), ""));
    page.add(&list_group(
        state,
        "env",
        t("Environment variables"),
        "NAME,value",
    ));
    page.upcast()
}
