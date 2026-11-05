#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod engine;
mod formats;
mod integration;
mod ipc;
mod metadata;
#[cfg(target_os = "macos")]
mod macos;
mod platform;
mod prefs;
mod resident;
mod update;
mod theme;
mod toast;

use eframe::egui;
use std::path::PathBuf;

const USAGE: &str = "\
Retype: change a file to a neighbouring format.

  retype                               open the window
  retype open <files…>                 open the window with files loaded
  retype convert --to <ext> <files…>   convert without the window (right-click menu)
  retype menu install|uninstall        add or remove the right-click menu
  retype check-update                  ask GitHub whether a newer Retype exists
  retype update-from <dmg|setup.exe>   install a downloaded update
";

fn icon() -> egui::IconData {
    let png = include_bytes!("../assets/icon-256.png");
    eframe::icon_data::from_png_bytes(png).unwrap_or_default()
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("convert") => {
            let mut to = None;
            let mut files = Vec::new();
            let mut it = args[1..].iter();
            while let Some(a) = it.next() {
                if a == "--to" {
                    to = it.next().map(|s| s.trim_start_matches('.').to_ascii_lowercase());
                } else {
                    files.push(absolute(a));
                }
            }
            let Some(to) = to.filter(|_| !files.is_empty()) else {
                eprint!("{USAGE}");
                std::process::exit(2);
            };
            // A right-click launch while another is converting joins its panel.
            if ipc::forward_or_serve("toast", &ipc::Msg { files: files.clone(), target: Some(to.clone()) }) {
                return Ok(());
            }
            std::thread::spawn(engine::parts::sweep);
            run_toast(files, to)
        }
        Some("menu") => {
            let res = match args.get(1).map(String::as_str) {
                Some("install") => integration::install(),
                Some("uninstall") => integration::uninstall(),
                _ => Err(USAGE.to_string()),
            };
            match res {
                Ok(()) => Ok(()),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1)
                }
            }
        }
        Some("utis") => {
            // Used by scripts/bundle-macos.sh to declare document types.
            let mut seen = Vec::new();
            for f in formats::FORMATS {
                for u in f.utis {
                    if !seen.contains(u) {
                        seen.push(u);
                        println!("{u}");
                    }
                }
            }
            Ok(())
        }
        Some("check-update") => {
            match update::check() {
                Ok(Some(r)) => println!("Retype {} is available ({})", r.version, r.asset.map(|a| a.0).unwrap_or_else(|| "no installer for this computer".into())),
                Ok(None) => println!("Retype {} is up to date", update::VERSION),
                Err(e) => {
                    eprintln!("Couldn't check for updates: {e}");
                    std::process::exit(1)
                }
            }
            Ok(())
        }
        Some("update-from") => {
            // Install a downloaded DMG / setup.exe (offline updates, and testing the updater).
            let Some(file) = args.get(1) else {
                eprint!("{USAGE}");
                std::process::exit(2);
            };
            match update::apply(&absolute(file)) {
                Ok(()) => Ok(()),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1)
                }
            }
        }
        Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some("open") => window(args[1..].iter().map(|a| absolute(a)).collect()),
        // Finder and Explorer also launch apps with bare file paths.
        _ => window(args.iter().filter(|a| !a.starts_with("-psn")).map(|a| absolute(a)).collect()),
    }
}

/// One window: a second launch sends its files to the open one and exits.
fn window(files: Vec<PathBuf>) -> eframe::Result<()> {
    if ipc::forward_or_serve("window", &ipc::Msg { files: files.clone(), target: None }) {
        return Ok(());
    }
    housekeeping();
    #[cfg(target_os = "macos")]
    macos::install_open_handler_early();
    run_window(files)
}

fn absolute(a: &str) -> PathBuf {
    let p = PathBuf::from(a);
    std::fs::canonicalize(&p).unwrap_or(p)
}

/// Startup chores, off the UI thread: remove temp files a crash left behind, and
/// re-point the right-click menu if the app has moved since it was installed.
fn housekeeping() {
    std::thread::spawn(|| {
        engine::parts::sweep();
        integration::repair();
    });
}

fn run_window(files: Vec<PathBuf>) -> eframe::Result<()> {
    let prefs = prefs::Prefs::load();
    // A floating card: no title bar, transparent corners, on top unless turned off.
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Retype")
        .with_inner_size(egui::vec2(app::WIDTH, app::START_HEIGHT))
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_drag_and_drop(true)
        .with_icon(icon());
    if prefs.settings.keep_on_top {
        viewport = viewport.with_always_on_top();
    }
    // Back where the user left it, unless that is implausibly far off any screen.
    let saved = prefs.window_pos.filter(|p| p[0].abs() < 20000.0 && p[1].abs() < 20000.0);
    if let Some(p) = saved {
        viewport = viewport.with_position(egui::pos2(p[0], p[1]));
    }
    let options = eframe::NativeOptions { viewport, centered: saved.is_none(), ..Default::default() };
    eframe::run_native("Retype", options, Box::new(move |cc| Ok(Box::new(app::RetypeApp::new(cc, files)))))
}

fn run_toast(files: Vec<PathBuf>, to: String) -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Retype")
        .with_inner_size(toast::SIZE)
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_taskbar(false)
        .with_visible(false)
        .with_icon(icon());
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Retype convert", options, Box::new(move |cc| Ok(Box::new(toast::Toast::new(cc, files, to)))))
}
