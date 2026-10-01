//! Keybind list and editor.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};
use hyprconf::Located;
use hyprgui_core::binds::{collisions, is_bind_keyword, Bind, DISPATCHERS};

use crate::i18n::t;
use crate::state::AppState;
use crate::ui;

/// (label, keyword) pairs for the type selector.
const KINDS: &[(&str, &str)] = &[
    ("Normal", "bind"),
    ("Repeat on hold", "binde"),
    ("Works when locked", "bindl"),
    ("Locked + repeat", "bindel"),
    ("Mouse", "bindm"),
    ("On release", "bindr"),
];

pub fn build(state: &Rc<AppState>) -> gtk::Widget {
    let page = adw::PreferencesPage::new();

    let search = gtk::SearchEntry::builder()
        .placeholder_text(t("Search"))
        .hexpand(true)
        .build();
    let search_group = adw::PreferencesGroup::new();
    search_group.add(&search);
    page.add(&search_group);

    let group = adw::PreferencesGroup::builder()
        .title(t("Keybinds"))
        .description(t("Keybinds that fire on the same combination are marked."))
        .build();
    let add = gtk::Button::with_label(t("Add keybind"));
    add.add_css_class("flat");
    group.set_header_suffix(Some(&add));

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    list.add_css_class("boxed-list");
    let empty = adw::StatusPage::builder()
        .icon_name("edit-find-symbolic")
        .title(t("No keybinds found"))
        .build();
    empty.add_css_class("compact");
    list.set_placeholder(Some(&empty));
    group.add(&list);
    page.add(&group);

    let query = Rc::new(RefCell::new(String::new()));
    let slot: ui::Hook = Rc::default();

    let (st, l, sl) = (state.clone(), list.clone(), slot.clone());
    let rebuild: Rc<dyn Fn()> = Rc::new(move || {
        while let Some(c) = l.first_child() {
            l.remove(&c);
        }
        let vars = st.session.borrow().variables();
        let items: Vec<(Located, Bind)> = st
            .session
            .borrow()
            .entries_with_prefix("bind")
            .into_iter()
            .filter(|e| is_bind_keyword(&e.path))
            .filter_map(|e| Bind::parse(&e.path, &e.value).map(|b| (e, b)))
            .collect();
        let binds: Vec<Bind> = items.iter().map(|(_, b)| b.clone()).collect();
        let clash: HashSet<usize> = collisions(&binds, &vars)
            .into_iter()
            .flat_map(|(a, b)| [a, b])
            .collect();

        for (i, (loc, bind)) in items.into_iter().enumerate() {
            let sub = if bind.arg.is_empty() {
                bind.dispatcher.clone()
            } else {
                format!("{}  {}", bind.dispatcher, bind.arg)
            };
            let sub = match &bind.description {
                Some(d) if !d.is_empty() => format!("{d} · {sub}"),
                _ => sub,
            };
            let row = adw::ActionRow::builder()
                .title(bind.shortcut(&vars))
                .subtitle(&sub)
                .use_markup(false)
                .activatable(true)
                .build();
            row.set_subtitle_lines(1);
            if clash.contains(&i) {
                let w = gtk::Image::from_icon_name("dialog-warning-symbolic");
                w.add_css_class("warning");
                w.set_tooltip_text(Some(t("Conflicts with another keybind")));
                row.add_prefix(&w);
            }
            let kind = KINDS
                .iter()
                .find(|(_, k)| *k == bind.keyword())
                .map(|(l, _)| t(l))
                .unwrap_or("");
            if !kind.is_empty() && bind.keyword() != "bind" {
                let tag = gtk::Label::new(Some(kind));
                tag.add_css_class("dim-label");
                tag.add_css_class("caption");
                row.add_suffix(&tag);
            }
            let del = gtk::Button::from_icon_name("user-trash-symbolic");
            del.add_css_class("flat");
            del.set_valign(gtk::Align::Center);
            del.set_tooltip_text(Some(t("Delete")));
            let (st2, loc2, sl2) = (st.clone(), loc.clone(), sl.clone());
            del.connect_clicked(move |_| {
                st2.session.borrow_mut().list_remove(&loc2);
                st2.notify_changed();
                let f = sl2.borrow().clone();
                if let Some(f) = f {
                    ui::later(move || f());
                }
            });
            row.add_suffix(&del);
            let haystack = format!(
                "{} {} {} {}",
                bind.shortcut(&vars),
                bind.dispatcher,
                bind.arg,
                bind.description.clone().unwrap_or_default()
            )
            .to_lowercase();
            row.set_widget_name(&haystack);
            let (st2, loc2, sl2) = (st.clone(), loc.clone(), sl.clone());
            row.connect_activated(move |r| {
                edit_dialog(r, &st2, Some((loc2.clone(), bind.clone())), sl2.clone())
            });
            l.append(&row);
        }
        l.invalidate_filter();
    });
    *slot.borrow_mut() = Some(rebuild.clone());

    let q = query.clone();
    list.set_filter_func(move |row| {
        let q = q.borrow();
        q.is_empty() || row.widget_name().contains(q.as_str())
    });
    let (q, l) = (query.clone(), list.clone());
    search.connect_search_changed(move |s| {
        *q.borrow_mut() = s.text().to_lowercase();
        l.invalidate_filter();
    });
    let (st, sl) = (state.clone(), slot.clone());
    add.connect_clicked(move |b| edit_dialog(b, &st, None, sl.clone()));

    rebuild();
    page.upcast()
}

const MODIFIER_KEYS: &[&str] = &[
    "Control",
    "Shift",
    "Alt",
    "Super",
    "Meta",
    "Hyper",
    "ISO_Level",
    "Caps_Lock",
    "Num_Lock",
];

fn mods_from_state(s: gdk::ModifierType) -> String {
    let mut out = Vec::new();
    if s.contains(gdk::ModifierType::SUPER_MASK) {
        out.push("SUPER");
    }
    if s.contains(gdk::ModifierType::CONTROL_MASK) {
        out.push("CTRL");
    }
    if s.contains(gdk::ModifierType::ALT_MASK) {
        out.push("ALT");
    }
    if s.contains(gdk::ModifierType::SHIFT_MASK) {
        out.push("SHIFT");
    }
    out.join(" ")
}

/// Key name in Hyprland's spelling, taken from the unshifted key of the
/// physical key so that Shift+1 is recorded as `1`, not `exclam`.
fn key_name(widget: &impl IsA<gtk::Widget>, keyval: gdk::Key, keycode: u32) -> Option<String> {
    let base = widget
        .display()
        .map_keycode(keycode)
        .and_then(|list| {
            list.into_iter()
                .find(|(k, _)| k.group() == 0 && k.level() == 0)
                .map(|(_, key)| key)
        })
        .unwrap_or(keyval);
    let name = base.to_lower().name()?.to_string();
    Some(if name.chars().count() == 1 {
        name.to_uppercase()
    } else {
        name
    })
}

fn edit_dialog(
    parent: &impl IsA<gtk::Widget>,
    state: &Rc<AppState>,
    existing: Option<(Located, Bind)>,
    rebuild: ui::Hook,
) {
    let group = adw::PreferencesGroup::new();
    let initial = existing.as_ref().map(|(_, b)| b.clone());

    let mut kinds: Vec<(String, String)> = KINDS
        .iter()
        .map(|(l, k)| (t(l).to_string(), k.to_string()))
        .collect();
    let cur_kw = initial.as_ref().map_or("bind".to_string(), |b| b.keyword());
    if !kinds.iter().any(|(_, k)| *k == cur_kw) {
        kinds.push((cur_kw.clone(), cur_kw.clone()));
    }
    let kind = adw::ComboRow::builder().title(t("Type")).build();
    let labels: Vec<&str> = kinds.iter().map(|(l, _)| l.as_str()).collect();
    kind.set_model(Some(&gtk::StringList::new(&labels)));
    kind.set_selected(kinds.iter().position(|(_, k)| *k == cur_kw).unwrap_or(0) as u32);

    let mods = adw::EntryRow::builder().title(t("Modifiers")).build();
    mods.set_text(
        &initial
            .as_ref()
            .map_or("$mod".to_string(), |b| b.mods.clone()),
    );
    let key = adw::EntryRow::builder().title(t("Key")).build();
    key.set_text(&initial.as_ref().map(|b| b.key.clone()).unwrap_or_default());

    let record = gtk::ToggleButton::with_label(t("Record"));
    record.set_valign(gtk::Align::Center);
    key.add_suffix(&record);

    let mut names: Vec<String> = DISPATCHERS.iter().map(|s| s.to_string()).collect();
    let cur_disp = initial
        .as_ref()
        .map_or("exec".to_string(), |b| b.dispatcher.clone());
    if !names.contains(&cur_disp) {
        names.insert(0, cur_disp.clone());
    }
    let action = adw::ComboRow::builder().title(t("Action")).build();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    action.set_model(Some(&gtk::StringList::new(&name_refs)));
    action.set_enable_search(true);
    action.set_selected(names.iter().position(|n| *n == cur_disp).unwrap_or(0) as u32);

    let arg = adw::EntryRow::builder().title(t("Argument")).build();
    arg.set_text(&initial.as_ref().map(|b| b.arg.clone()).unwrap_or_default());

    let desc = adw::EntryRow::builder().title(t("Description")).build();
    desc.set_text(
        &initial
            .as_ref()
            .and_then(|b| b.description.clone())
            .unwrap_or_default(),
    );
    desc.set_visible(initial.as_ref().is_some_and(|b| b.flags.contains('d')));

    for w in [
        kind.upcast_ref::<gtk::Widget>(),
        mods.upcast_ref(),
        key.upcast_ref(),
        action.upcast_ref(),
        arg.upcast_ref(),
        desc.upcast_ref(),
    ] {
        group.add(w);
    }
    let wrapper = gtk::Box::builder()
        .margin_top(12)
        .margin_bottom(18)
        .margin_start(12)
        .margin_end(12)
        .build();
    wrapper.append(&group);
    group.set_hexpand(true);

    let st = state.clone();
    let (kinds2, names2) = (kinds.clone(), names.clone());
    let (kind2, mods2, key2, action2, arg2, desc2) = (
        kind.clone(),
        mods.clone(),
        key.clone(),
        action.clone(),
        arg.clone(),
        desc.clone(),
    );
    let dialog = ui::form_dialog(
        parent,
        if existing.is_some() {
            t("Edit keybind")
        } else {
            t("Add keybind")
        },
        &wrapper,
        t("Save"),
        move || {
            let key_text = key2.text().trim().to_string();
            let disp = names2
                .get(action2.selected() as usize)
                .cloned()
                .unwrap_or_default();
            if key_text.is_empty() || disp.is_empty() {
                key2.add_css_class("error");
                return false;
            }
            let keyword = kinds2
                .get(kind2.selected() as usize)
                .map(|(_, k)| k.clone())
                .unwrap_or_else(|| "bind".into());
            let flags = keyword["bind".len()..].to_string();
            let b = Bind {
                flags,
                mods: mods2.text().trim().to_string(),
                key: key_text,
                description: desc2.is_visible().then(|| desc2.text().to_string()),
                dispatcher: disp,
                arg: arg2.text().trim().to_string(),
            };
            {
                let mut s = st.session.borrow_mut();
                match &existing {
                    Some((loc, old)) if old.keyword() == b.keyword() => {
                        s.list_replace(loc, &b.to_value());
                    }
                    Some((loc, _)) => {
                        s.list_remove(loc);
                        s.list_push(&b.keyword(), &b.to_value());
                    }
                    None => s.list_push(&b.keyword(), &b.to_value()),
                }
            }
            st.notify_changed();
            let f = rebuild.borrow().clone();
            if let Some(f) = f {
                ui::later(move || f());
            }
            true
        },
    );

    // Recording: capture the next key combination anywhere in the dialog.
    let recording = Rc::new(Cell::new(false));
    let ctl = gtk::EventControllerKey::new();
    ctl.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (rec, mods3, key3, record3, d3) = (
        recording.clone(),
        mods.clone(),
        key.clone(),
        record.clone(),
        dialog.clone(),
    );
    ctl.connect_key_pressed(move |_, keyval, keycode, state| {
        if !rec.get() {
            return glib::Propagation::Proceed;
        }
        let name = keyval.name().map(|n| n.to_string()).unwrap_or_default();
        if MODIFIER_KEYS.iter().any(|m| name.starts_with(m)) {
            return glib::Propagation::Stop;
        }
        rec.set(false);
        record3.set_active(false);
        if name == "Escape" {
            return glib::Propagation::Stop;
        }
        if let Some(k) = key_name(&d3, keyval, keycode) {
            key3.set_text(&k);
            mods3.set_text(&mods_from_state(state));
            key3.remove_css_class("error");
        }
        glib::Propagation::Stop
    });
    dialog.add_controller(ctl);
    let rec = recording.clone();
    record.connect_toggled(move |b| {
        rec.set(b.is_active());
        if b.is_active() {
            b.set_label(t("Press a key combination…"));
        } else {
            b.set_label(t("Record"));
        }
    });
}
