//! Settings of HyprGUI itself: theme and colour scheme.

use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;
use crate::settings::Scheme;
use crate::theme::{self, ThemeManager};

pub fn build(themes: &Rc<ThemeManager>) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::builder()
        .title(t("Theme"))
        .description(t(
            "Look of HyprGUI itself. Themes are folders with a theme.toml, see docs/THEMING.md.",
        ))
        .build();

    let list = themes.themes();
    let names: Vec<String> = list.iter().map(|th| th.meta.name.clone()).collect();
    let ids: Vec<String> = list.iter().map(|th| th.meta.id.clone()).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();

    let row = adw::ComboRow::builder().title(t("Theme")).build();
    row.set_model(Some(&gtk::StringList::new(&name_refs)));
    let current = themes.current_id();
    let sel = ids.iter().position(|i| *i == current).unwrap_or(0);
    row.set_selected(sel as u32);
    let describe = {
        let list = list.clone();
        move |row: &adw::ComboRow| {
            if let Some(th) = list.get(row.selected() as usize) {
                let by = if th.meta.author.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", th.meta.author)
                };
                row.set_subtitle(&format!("{}{}", th.meta.description, by));
            }
        }
    };
    describe(&row);
    let (tm, ids2) = (themes.clone(), ids.clone());
    row.connect_selected_notify(move |r| {
        describe(r);
        if let Some(id) = ids2.get(r.selected() as usize) {
            tm.set_theme(id);
        }
    });
    group.add(&row);

    let scheme = adw::ComboRow::builder().title(t("Color scheme")).build();
    scheme.set_model(Some(&gtk::StringList::new(&[
        t("Follow system"),
        t("Light"),
        t("Dark"),
    ])));
    scheme.set_selected(match themes.scheme() {
        Scheme::System => 0,
        Scheme::Light => 1,
        Scheme::Dark => 2,
    });
    let tm = themes.clone();
    scheme.connect_selected_notify(move |r| {
        tm.set_scheme(match r.selected() {
            1 => Scheme::Light,
            2 => Scheme::Dark,
            _ => Scheme::System,
        });
    });
    group.add(&scheme);

    let open = adw::ActionRow::builder()
        .title(t("Open themes folder"))
        .activatable(true)
        .build();
    open.add_suffix(&gtk::Image::from_icon_name("folder-open-symbolic"));
    open.connect_activated(|_| {
        if let Some(dir) = theme::search_paths().into_iter().find(|p| {
            p.ends_with("hyprgui/themes")
                && p.starts_with(std::env::var("HOME").unwrap_or_default())
        }) {
            let _ = std::fs::create_dir_all(&dir);
            let _ = gtk::gio::AppInfo::launch_default_for_uri(
                &format!("file://{}", dir.display()),
                gtk::gio::AppLaunchContext::NONE,
            );
        }
    });
    group.add(&open);
    page.add(&group);
    page.upcast()
}
