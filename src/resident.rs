//! Retype when its window is closed: a menu-bar (tray) icon and a global
//! shortcut that converts whatever is selected in Finder or Explorer.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::Mutex;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Show,
    ConvertSelection,
    Settings,
    CheckUpdates,
    Quit,
}

static ACTIONS: Mutex<Vec<Action>> = Mutex::new(Vec::new());

fn push(a: Action) {
    ACTIONS.lock().unwrap().push(a);
    if let Some(ctx) = crate::ipc::waker() {
        ctx.request_repaint();
    }
}

pub fn take() -> Vec<Action> {
    std::mem::take(&mut *ACTIONS.lock().unwrap())
}

pub fn shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") { "⌥⌘R" } else { "Ctrl+Alt+R" }
}

fn hotkey() -> HotKey {
    let mods = if cfg!(target_os = "macos") { Modifiers::META | Modifiers::ALT } else { Modifiers::CONTROL | Modifiers::ALT };
    HotKey::new(Some(mods), Code::KeyR)
}

/// The menu-bar mark: the dot, the selection block, and the caret, drawn as a
/// template image (black on transparent) so the system tints it.
#[cfg(target_os = "macos")]
fn tray_image() -> tray_icon::Icon {
    let (w, h) = (44u32, 44u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut set = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        px[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
    };
    for y in 0..h {
        for x in 0..w {
            let dot = (x as i32 - 9).pow(2) + (y as i32 - 28).pow(2) <= 16;
            let block = (16..33).contains(&x) && (14..31).contains(&y);
            let caret = (34..37).contains(&x) && (10..35).contains(&y);
            if dot || block || caret {
                set(x, y);
            }
        }
    }
    tray_icon::Icon::from_rgba(px, w, h).expect("tray icon")
}

#[cfg(not(target_os = "macos"))]
fn color_icon() -> tray_icon::Icon {
    let img = image::load_from_memory(include_bytes!("../assets/icon-256.png")).expect("icon").thumbnail(32, 32).to_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("tray icon")
}

#[derive(Default)]
pub struct Resident {
    tray: Option<TrayIcon>,
    hotkeys: Option<GlobalHotKeyManager>,
    menu_ids: Vec<(tray_icon::menu::MenuId, Action)>,
    shortcut_id: Option<u32>,
}

impl Resident {
    /// Bring the menu-bar icon and the shortcut in line with the settings.
    /// Must run on the main thread with the event loop running (from `ui`).
    pub fn sync(&mut self, s: crate::prefs::Settings) {
        if s.menu_bar && self.tray.is_none() {
            let fm = crate::platform::file_manager_name();
            let open = MenuItem::new("Open Retype", true, None);
            let convert = MenuItem::new(format!("Convert {fm} selection   {}", shortcut_label()), true, None);
            let settings = MenuItem::new("Settings…", true, None);
            let updates = MenuItem::new("Check for Updates…", true, None);
            let quit = MenuItem::new("Quit Retype", true, None);
            let menu = Menu::new();
            let _ = menu.append_items(&[&open, &convert, &PredefinedMenuItem::separator(), &settings, &updates, &PredefinedMenuItem::separator(), &quit]);
            self.menu_ids = vec![
                (open.id().clone(), Action::Show),
                (convert.id().clone(), Action::ConvertSelection),
                (settings.id().clone(), Action::Settings),
                (updates.id().clone(), Action::CheckUpdates),
                (quit.id().clone(), Action::Quit),
            ];
            let ids = self.menu_ids.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if let Some((_, a)) = ids.iter().find(|(id, _)| *id == e.id) {
                    push(*a);
                }
            }));
            // macOS: a template the system tints for the menu bar. Windows: the
            // full-colour app icon, which reads on light and dark taskbars.
            #[cfg(target_os = "macos")]
            let builder = TrayIconBuilder::new().with_icon_templated(tray_image());
            #[cfg(not(target_os = "macos"))]
            let builder = TrayIconBuilder::new().with_icon(color_icon());
            self.tray = builder
                .with_tooltip("Retype")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(true)
                .build()
                .map_err(|e| eprintln!("tray: {e}"))
                .ok();
        } else if !s.menu_bar && self.tray.is_some() {
            self.tray = None;
        }

        if s.global_shortcut && self.shortcut_id.is_none() {
            if self.hotkeys.is_none() {
                self.hotkeys = GlobalHotKeyManager::new().ok();
            }
            if let Some(m) = &self.hotkeys {
                let hk = hotkey();
                if m.register(hk).is_ok() {
                    let id = hk.id();
                    self.shortcut_id = Some(id);
                    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
                        if e.id == id && e.state == HotKeyState::Pressed {
                            push(Action::ConvertSelection);
                        }
                    }));
                }
            }
        } else if !s.global_shortcut && self.shortcut_id.is_some() {
            if let Some(m) = &self.hotkeys {
                let _ = m.unregister(hotkey());
            }
            self.shortcut_id = None;
        }
    }

    pub fn in_menu_bar(&self) -> bool {
        self.tray.is_some()
    }
}
