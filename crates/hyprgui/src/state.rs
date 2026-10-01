//! State shared by every page.

use std::cell::RefCell;
use std::rc::Rc;

use hyprgui_core::Session;

pub struct AppState {
    pub session: RefCell<Session>,
    pub toasts: adw::ToastOverlay,
    listeners: RefCell<Vec<Rc<dyn Fn()>>>,
    rebuild: RefCell<Option<Rc<dyn Fn()>>>,
}

impl AppState {
    pub fn new(session: Session) -> Rc<AppState> {
        Rc::new(AppState {
            session: RefCell::new(session),
            toasts: adw::ToastOverlay::new(),
            listeners: RefCell::new(Vec::new()),
            rebuild: RefCell::new(None),
        })
    }

    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    /// Registers a callback run after every edit (used to refresh Apply/Revert).
    pub fn on_changed(&self, f: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Rc::new(f));
    }

    /// The window registers how to rebuild all pages from the session.
    pub fn set_rebuild(&self, f: impl Fn() + 'static) {
        *self.rebuild.borrow_mut() = Some(Rc::new(f));
    }

    /// Asks the window to rebuild all pages (after a list changed elsewhere).
    pub fn request_rebuild(&self) {
        let f = self.rebuild.borrow().clone();
        if let Some(f) = f {
            crate::ui::later(move || f());
        }
    }

    /// Call after any edit of the session.
    pub fn notify_changed(&self) {
        let ls: Vec<_> = self.listeners.borrow().clone();
        for l in ls {
            l();
        }
    }
}
