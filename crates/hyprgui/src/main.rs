mod i18n;
mod pages;
mod settings;
mod state;
mod theme;
mod ui;
mod window;

use std::path::PathBuf;

use adw::prelude::*;
use hyprgui_core::Session;

const APP_ID: &str = "io.github.jirkacepelka.HyprGUI";

struct Args {
    config: Option<PathBuf>,
    theme: Option<String>,
    page: Option<String>,
    no_ipc: bool,
}

fn usage() -> ! {
    eprintln!(
        "HyprGUI {}\n\n\
         Usage: hyprgui [--config FILE] [--theme ID] [--page ID] [--no-ipc]\n\
         \x20      hyprgui theme list\n\
         \x20      hyprgui theme check <DIR|ID>\n",
        env!("CARGO_PKG_VERSION")
    );
    std::process::exit(2)
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Args {
    let mut a = Args {
        config: None,
        theme: None,
        page: None,
        no_ipc: false,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => a.config = it.next().map(PathBuf::from),
            "--theme" => a.theme = it.next(),
            "--page" => a.page = it.next(),
            "--no-ipc" => a.no_ipc = true,
            "--version" | "-V" => {
                println!("hyprgui {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0)
            }
            _ => usage(),
        }
    }
    a
}

/// `hyprgui theme list|check`: no GTK needed.
fn theme_cli(args: &[String]) -> i32 {
    let paths = theme::search_paths();
    match args.first().map(String::as_str) {
        Some("list") => {
            for t in hyprgui_theme::discover(&paths) {
                println!(
                    "{:<16} {}  ({})",
                    t.meta.id,
                    t.meta.name,
                    t.dir.map(|d| d.display().to_string()).unwrap_or_default()
                );
            }
            0
        }
        Some("check") => {
            let Some(target) = args.get(1) else { usage() };
            let loaded = if std::path::Path::new(target).join("theme.toml").is_file() {
                hyprgui_theme::Theme::load(target)
            } else {
                hyprgui_theme::find(target, &paths)
            };
            let theme = match loaded {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 1;
                }
            };
            let issues = theme.validate();
            for i in &issues {
                let tag = match i.severity {
                    hyprgui_theme::Severity::Error => "error",
                    hyprgui_theme::Severity::Warning => "warning",
                };
                println!("{tag}: {}", i.message);
            }
            let errors = issues
                .iter()
                .filter(|i| i.severity == hyprgui_theme::Severity::Error)
                .count();
            println!(
                "{}: {} issue(s), {errors} error(s)",
                theme.meta.id,
                issues.len()
            );
            i32::from(errors > 0)
        }
        _ => usage(),
    }
}

fn main() -> gtk::glib::ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("theme") {
        std::process::exit(theme_cli(&argv[1..]));
    }
    let args = parse_args(argv.into_iter());

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| {
        if let Some(win) = app.active_window() {
            win.present();
            return;
        }
        let path = args
            .config
            .clone()
            .unwrap_or_else(Session::default_config_path);
        let ipc = if args.no_ipc {
            None
        } else {
            hypripc::Client::from_env()
        };
        let session = match Session::open(&path, ipc) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot open {}: {e}", path.display());
                app.quit();
                return;
            }
        };
        let themes = theme::ThemeManager::new(settings::Settings::load(), args.theme.clone());
        let state = state::AppState::new(session);
        window::build(app, state, themes, args.page.as_deref()).present();
    });
    app.run_with_args::<&str>(&[])
}
