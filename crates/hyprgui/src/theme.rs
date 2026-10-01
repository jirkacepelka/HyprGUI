//! Applies a `hyprgui-theme` theme to GTK and keeps it live.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use gtk::gdk;
use hyprgui_theme::{Theme, Variant};

use crate::settings::{Scheme, Settings};

pub struct ThemeManager {
    provider: gtk::CssProvider,
    inner: RefCell<Inner>,
}

struct Inner {
    settings: Settings,
    theme: Theme,
    watched: Vec<(PathBuf, Option<SystemTime>)>,
}

/// Directories searched for themes. Dev builds also look in the source tree.
pub fn search_paths() -> Vec<PathBuf> {
    let mut v = hyprgui_theme::search_paths();
    #[cfg(debug_assertions)]
    v.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../themes"
    )));
    v
}

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl ThemeManager {
    pub fn new(settings: Settings, forced_theme: Option<String>) -> Rc<ThemeManager> {
        let paths = search_paths();
        let wanted = forced_theme
            .or_else(|| std::env::var("HYPRGUI_THEME").ok())
            .or_else(|| settings.theme.clone())
            .unwrap_or_else(|| "caelestia".into());
        let theme = hyprgui_theme::find(&wanted, &paths)
            .or_else(|_| hyprgui_theme::find("default", &paths))
            .unwrap_or_else(|_| Theme::builtin_default());
        let provider = gtk::CssProvider::new();
        let mgr = Rc::new(ThemeManager {
            provider,
            inner: RefCell::new(Inner {
                settings,
                theme,
                watched: Vec::new(),
            }),
        });
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &mgr.provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        mgr.apply();
        // Re-apply when the system flips between light and dark.
        let weak = Rc::downgrade(&mgr);
        adw::StyleManager::default().connect_dark_notify(move |_| {
            if let Some(m) = weak.upgrade() {
                m.apply();
            }
        });
        // Hot reload: poll the theme's files so theme authors see edits instantly.
        let weak = Rc::downgrade(&mgr);
        gtk::glib::timeout_add_seconds_local(1, move || {
            let Some(m) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            m.reload_if_changed();
            gtk::glib::ControlFlow::Continue
        });
        mgr
    }

    pub fn themes(&self) -> Vec<Theme> {
        let mut t = hyprgui_theme::discover(&search_paths());
        if !t.iter().any(|x| x.meta.id == "default") {
            t.insert(0, Theme::builtin_default());
        }
        t
    }

    pub fn current_id(&self) -> String {
        self.inner.borrow().theme.meta.id.clone()
    }

    pub fn scheme(&self) -> Scheme {
        self.inner.borrow().settings.scheme
    }

    pub fn set_theme(&self, id: &str) {
        if let Ok(t) = hyprgui_theme::find(id, &search_paths()) {
            let mut i = self.inner.borrow_mut();
            i.theme = t;
            i.settings.theme = Some(id.to_string());
            i.settings.save();
            drop(i);
            self.apply();
        }
    }

    pub fn set_scheme(&self, scheme: Scheme) {
        let mut i = self.inner.borrow_mut();
        i.settings.scheme = scheme;
        i.settings.save();
        drop(i);
        self.apply();
    }

    fn variant(&self) -> Variant {
        let i = self.inner.borrow();
        let sm = adw::StyleManager::default();
        match i.settings.scheme {
            Scheme::Light => Variant::Light,
            Scheme::Dark => Variant::Dark,
            Scheme::System if sm.system_supports_color_schemes() => {
                if sm.is_dark() {
                    Variant::Dark
                } else {
                    Variant::Light
                }
            }
            Scheme::System => i.theme.meta.base,
        }
    }

    fn apply(&self) {
        let sm = adw::StyleManager::default();
        let (scheme, base) = {
            let i = self.inner.borrow();
            (i.settings.scheme, i.theme.meta.base)
        };
        // Keep libadwaita's own light/dark in step with the variant we render.
        let want = match scheme {
            Scheme::Light => adw::ColorScheme::ForceLight,
            Scheme::Dark => adw::ColorScheme::ForceDark,
            Scheme::System if sm.system_supports_color_schemes() => adw::ColorScheme::Default,
            Scheme::System => match base {
                Variant::Dark => adw::ColorScheme::ForceDark,
                Variant::Light => adw::ColorScheme::ForceLight,
            },
        };
        if sm.color_scheme() != want {
            sm.set_color_scheme(want);
        }
        let css = self.inner.borrow().theme.to_css(self.variant());
        self.provider.load_from_string(&css);
        self.rewatch();
    }

    fn rewatch(&self) {
        let mut i = self.inner.borrow_mut();
        let mut files = Vec::new();
        if let Some(dir) = &i.theme.dir {
            files.push(dir.join("theme.toml"));
            files.push(dir.join("style.css"));
        }
        if let Some(src) = &i.theme.dynamic.source {
            let p = match src.strip_prefix("~/") {
                Some(rest) => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(rest)),
                None => Some(PathBuf::from(src)),
            };
            files.extend(p);
        }
        i.watched = files.into_iter().map(|f| (f.clone(), mtime(&f))).collect();
    }

    fn reload_if_changed(&self) {
        let changed = self
            .inner
            .borrow()
            .watched
            .iter()
            .any(|(p, old)| mtime(p) != *old);
        if !changed {
            return;
        }
        let dir = self.inner.borrow().theme.dir.clone();
        if let Some(dir) = dir {
            if let Ok(t) = Theme::load(&dir) {
                self.inner.borrow_mut().theme = t;
            }
        }
        self.apply();
    }
}
