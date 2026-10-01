//! Main window: sidebar navigation, header actions, unsaved-changes guard.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::i18n::t;
use crate::pages;
use crate::state::AppState;
use crate::theme::ThemeManager;
use crate::ui;

struct PageDef {
    id: &'static str,
    icon: &'static str,
    title: &'static str,
}

const PAGES: &[PageDef] = &[
    PageDef {
        id: "appearance",
        icon: "preferences-desktop-appearance-symbolic",
        title: "Appearance",
    },
    PageDef {
        id: "animations",
        icon: "media-playback-start-symbolic",
        title: "Animations",
    },
    PageDef {
        id: "input",
        icon: "input-keyboard-symbolic",
        title: "Input",
    },
    PageDef {
        id: "monitors",
        icon: "video-display-symbolic",
        title: "Monitors",
    },
    PageDef {
        id: "binds",
        icon: "preferences-desktop-keyboard-shortcuts-symbolic",
        title: "Keybinds",
    },
    PageDef {
        id: "rules",
        icon: "view-grid-symbolic",
        title: "Window rules",
    },
    PageDef {
        id: "autostart",
        icon: "system-run-symbolic",
        title: "Autostart",
    },
    PageDef {
        id: "app",
        icon: "applications-graphics-symbolic",
        title: "Application",
    },
];

/// `/home/me/.config/hypr/hyprland.conf` → `~/.config/hypr/hyprland.conf`.
fn short_path(p: &std::path::Path) -> String {
    match std::env::var_os("HOME").and_then(|h| p.strip_prefix(h).ok().map(|r| r.to_path_buf())) {
        Some(rest) => format!("~/{}", rest.display()),
        None => p.display().to_string(),
    }
}

fn build_page(id: &str, state: &Rc<AppState>, themes: &Rc<ThemeManager>) -> gtk::Widget {
    match id {
        "appearance" | "input" => pages::options::build(state, id),
        "animations" => pages::lists::animations_page(state),
        "monitors" => pages::monitors::build(state),
        "binds" => pages::binds::build(state),
        "rules" => pages::lists::rules_page(state),
        "autostart" => pages::lists::autostart_page(state),
        _ => pages::app::build(themes),
    }
}

pub fn build(
    app: &adw::Application,
    state: Rc<AppState>,
    themes: Rc<ThemeManager>,
    start_page: Option<&str>,
) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("HyprGUI")
        .default_width(1100)
        .default_height(760)
        .width_request(360)
        .height_request(400)
        .build();
    window.add_css_class("hyprgui");

    // ---- content side -----------------------------------------------------
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .vexpand(true)
        .build();
    let title = adw::WindowTitle::new("", &short_path(state.session.borrow().main_path()));

    let apply = gtk::Button::with_label(t("Apply"));
    apply.add_css_class("suggested-action");
    let revert = gtk::Button::with_label(t("Revert"));
    let menu = gio::Menu::new();
    menu.append(Some("About HyprGUI"), Some("win.about"));
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();

    let content_header = adw::HeaderBar::new();
    content_header.set_title_widget(Some(&title));
    content_header.pack_start(&revert);
    content_header.pack_end(&menu_btn);
    content_header.pack_end(&apply);

    let banner = adw::Banner::new(t(
        "Not running under Hyprland: changes are saved to the config but cannot be previewed live.",
    ));
    banner.set_revealed(state.session.borrow().ipc().is_none());

    let content_view = adw::ToolbarView::new();
    content_view.add_top_bar(&content_header);
    content_view.add_top_bar(&banner);
    content_view.set_content(Some(&stack));
    state.toasts.set_child(Some(&content_view));
    let content_page = adw::NavigationPage::new(&state.toasts, "HyprGUI");

    // ---- sidebar ------------------------------------------------------------
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    list.add_css_class("navigation-sidebar");
    for p in PAGES {
        let row = adw::ActionRow::builder().title(t(p.title)).build();
        row.add_prefix(&gtk::Image::from_icon_name(p.icon));
        row.set_widget_name(p.id);
        list.append(&row);
    }
    let side_scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let side_header = adw::HeaderBar::new();
    side_header.set_title_widget(Some(&adw::WindowTitle::new("HyprGUI", "")));
    let side_view = adw::ToolbarView::new();
    side_view.add_top_bar(&side_header);
    side_view.set_content(Some(&side_scroll));
    let sidebar_page = adw::NavigationPage::new(&side_view, "HyprGUI");

    let split = adw::NavigationSplitView::new();
    split.set_sidebar(Some(&sidebar_page));
    split.set_content(Some(&content_page));
    split.set_min_sidebar_width(220.0);
    split.set_max_sidebar_width(280.0);
    window.set_content(Some(&split));

    let bp = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 700sp").expect("valid condition"),
    );
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp);

    // ---- pages ----------------------------------------------------------------
    let rebuild = {
        let (stack, state, themes) = (stack.clone(), state.clone(), themes.clone());
        move || {
            let current = stack.visible_child_name();
            while let Some(c) = stack.first_child() {
                stack.remove(&c);
            }
            for p in PAGES {
                stack.add_named(&build_page(p.id, &state, &themes), Some(p.id));
            }
            stack.set_visible_child_name(current.as_deref().unwrap_or("appearance"));
        }
    };
    rebuild();
    {
        let rebuild = rebuild.clone();
        state.set_rebuild(rebuild);
    }

    let select = {
        let (stack, title, split) = (stack.clone(), title.clone(), split.clone());
        move |id: &str| {
            stack.set_visible_child_name(id);
            if let Some(p) = PAGES.iter().find(|p| p.id == id) {
                title.set_title(t(p.title));
            }
            split.set_show_content(true);
        }
    };
    let select = Rc::new(select);
    {
        let select = select.clone();
        list.connect_row_selected(move |_, row| {
            if let Some(r) = row {
                select(r.widget_name().as_str());
            }
        });
    }
    let start = start_page
        .filter(|s| PAGES.iter().any(|p| p.id == *s))
        .unwrap_or("appearance");
    let idx = PAGES.iter().position(|p| p.id == start).unwrap_or(0);
    if let Some(row) = list.row_at_index(idx as i32) {
        list.select_row(Some(&row));
    }

    // ---- dirty state / actions ------------------------------------------------
    let update = {
        let (state, apply, revert) = (state.clone(), apply.clone(), revert.clone());
        move || {
            let dirty = state.session.borrow().is_dirty();
            apply.set_sensitive(dirty);
            revert.set_sensitive(dirty);
        }
    };
    update();
    state.on_changed(update.clone());

    let save = {
        let (state, window) = (state.clone(), window.clone());
        Rc::new(move || -> bool {
            let res = state.session.borrow_mut().save();
            match res {
                Ok(report) => {
                    state.notify_changed();
                    state.toast(&match report.reload_error {
                        Some(e) => format!("{}: {e}", t("Saved")),
                        None => t("Saved").to_string(),
                    });
                    true
                }
                Err(e) => {
                    state.notify_changed();
                    ui::message_dialog(&window, t("Could not save"), &e.to_string());
                    false
                }
            }
        })
    };
    {
        let save = save.clone();
        apply.connect_clicked(move |_| {
            save();
        });
    }
    {
        let (state, rebuild) = (state.clone(), rebuild.clone());
        revert.connect_clicked(move |_| {
            let res = state.session.borrow_mut().revert();
            if res.is_ok() {
                rebuild();
                state.notify_changed();
                state.toast(t("Changes reverted"));
            }
        });
    }

    let about = gio::SimpleAction::new("about", None);
    {
        let window = window.clone();
        about.connect_activate(move |_, _| {
            let d = adw::AboutDialog::builder()
                .application_name("HyprGUI")
                .application_icon("preferences-system-symbolic")
                .version(env!("CARGO_PKG_VERSION"))
                .license_type(gtk::License::Gpl30)
                .website("https://github.com/jirkacepelka/HyprGUI")
                .comments("Settings for Hyprland")
                .build();
            d.present(Some(&window));
        });
    }
    window.add_action(&about);

    // Ask before discarding unsaved edits.
    let force = Rc::new(Cell::new(false));
    {
        let (state, save, force) = (state.clone(), save.clone(), force.clone());
        window.connect_close_request(move |w| {
            if force.get() || !state.session.borrow().is_dirty() {
                return glib::Propagation::Proceed;
            }
            let d = adw::AlertDialog::new(
                Some(t("Unsaved changes")),
                Some(t(
                    "Save the changes to your Hyprland config before closing?",
                )),
            );
            d.add_response("cancel", t("Cancel"));
            d.add_response("discard", t("Discard"));
            d.add_response("save", t("Save"));
            d.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
            d.set_response_appearance("save", adw::ResponseAppearance::Suggested);
            d.set_default_response(Some("save"));
            d.set_close_response("cancel");
            let (w2, state2, save2, force2) =
                (w.clone(), state.clone(), save.clone(), force.clone());
            d.choose(w, gio::Cancellable::NONE, move |r| match r.as_str() {
                "save" => {
                    if save2() {
                        force2.set(true);
                        w2.close();
                    }
                }
                "discard" => {
                    let _ = state2.session.borrow_mut().revert();
                    force2.set(true);
                    w2.close();
                }
                _ => {}
            });
            glib::Propagation::Stop
        });
    }
    window
}
