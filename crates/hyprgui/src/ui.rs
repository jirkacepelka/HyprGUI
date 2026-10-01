//! Small shared widgets and dialogs.

use adw::prelude::*;
use gtk::glib;

use crate::i18n::t;

/// Modal form with Cancel / Save. `on_save` returns `true` to close.
pub fn form_dialog(
    parent: &impl IsA<gtk::Widget>,
    title: &str,
    content: &impl IsA<gtk::Widget>,
    save_label: &str,
    on_save: impl Fn() -> bool + 'static,
) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(460)
        .build();
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .show_start_title_buttons(false)
        .build();
    let cancel = gtk::Button::with_label(t("Cancel"));
    let save = gtk::Button::with_label(save_label);
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);
    let scroll = gtk::ScrolledWindow::builder()
        .child(content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(560)
        .build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&scroll));
    dialog.set_child(Some(&view));
    dialog.set_default_widget(Some(&save));
    let d = dialog.clone();
    cancel.connect_clicked(move |_| {
        d.close();
    });
    let d = dialog.clone();
    save.connect_clicked(move |_| {
        if on_save() {
            d.close();
        }
    });
    dialog.present(Some(parent));
    dialog
}

/// Message with a single OK button.
pub fn message_dialog(parent: &impl IsA<gtk::Widget>, heading: &str, body: &str) {
    let d = adw::AlertDialog::new(Some(heading), Some(body));
    d.add_response("ok", "OK");
    d.present(Some(parent));
}

/// A callback slot filled after construction, so a widget's own handlers can
/// ask for the list they live in to be rebuilt.
pub type Hook = std::rc::Rc<std::cell::RefCell<Option<std::rc::Rc<dyn Fn()>>>>;

/// Runs `f` once the current signal emission has finished (safe place to
/// rebuild widgets that are in the middle of handling an event).
pub fn later(f: impl FnOnce() + 'static) {
    glib::idle_add_local_once(f);
}

pub fn trim_float(v: f64) -> String {
    format!("{}", (v * 10_000.0).round() / 10_000.0)
}
