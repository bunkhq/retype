//! Remembered choices and settings, in `<config>/Retype/prefs.json`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

fn yes() -> bool {
    true
}

/// User settings. Every field has a default, so old prefs files keep loading.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Settings {
    /// Carry photo metadata (capture date, camera, orientation) into converted photos.
    #[serde(default = "yes")]
    pub keep_metadata: bool,
    /// Carry GPS location too. Off by default: a converted file is often one that gets shared.
    #[serde(default)]
    pub keep_location: bool,
    /// Closing the window mid-conversion finishes the job in the background.
    #[serde(default = "yes")]
    pub finish_after_close: bool,
    /// Use the system's hardware video encoder when there is one (macOS VideoToolbox).
    #[serde(default = "yes")]
    pub hardware_video: bool,
    /// Save into Downloads instead of next to the original.
    #[serde(default)]
    pub save_to_downloads: bool,
    /// Keep the window above other windows.
    #[serde(default = "yes")]
    pub keep_on_top: bool,
    /// Stay in the menu bar (system tray) when the window is closed.
    #[serde(default = "yes")]
    pub menu_bar: bool,
    /// ⌥⌘R (Ctrl+Alt+R on Windows) converts the files selected in Finder / Explorer.
    #[serde(default = "yes")]
    pub global_shortcut: bool,
    /// Ask GitHub once a day whether a newer Retype exists.
    #[serde(default = "yes")]
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { keep_metadata: true, keep_location: false, finish_after_close: true, hardware_video: true, save_to_downloads: false, keep_on_top: true, menu_bar: true, global_shortcut: true, check_updates: true }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub last_target: HashMap<String, String>,
    #[serde(default)]
    pub seen_first_run: bool,
    #[serde(default)]
    pub settings: Settings,
    /// Top-left of the window, in points, where the user last left it.
    #[serde(default)]
    pub window_pos: Option<[f32; 2]>,
}

fn path() -> Option<PathBuf> {
    // A development instance (RETYPE_INSTANCE) keeps its own prefs, so tests never
    // change the settings of the Retype you actually use.
    let name = match std::env::var("RETYPE_INSTANCE") {
        Ok(tag) if !tag.is_empty() => format!("prefs-{tag}.json"),
        _ => "prefs.json".to_string(),
    };
    Some(dirs::config_dir()?.join("Retype").join(name))
}

impl Prefs {
    pub fn load() -> Prefs {
        path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(p, b);
        }
    }
}
