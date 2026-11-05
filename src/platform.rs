//! Small OS bridges: open, reveal, copy-as-file, Quick Look, Reduce Motion,
//! and the native drag session that makes a converted file a real file.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn file_manager_name() -> &'static str {
    if cfg!(target_os = "macos") { "Finder" } else if cfg!(windows) { "Explorer" } else { "Files" }
}

/// Where Retype's icon lives while the window is closed.
pub fn tray_name() -> &'static str {
    if cfg!(target_os = "macos") { "menu bar" } else { "system tray" }
}

pub fn open(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(path).spawn();
    #[cfg(windows)]
    let _ = Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(path).spawn();
}

pub fn reveal(paths: &[PathBuf]) {
    let Some(first) = paths.first() else { return };
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg("-R").args(paths).spawn();
    #[cfg(windows)]
    let _ = Command::new("explorer").arg(format!("/select,{}", first.display())).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(first.parent().unwrap_or(first)).spawn();
    let _ = first;
}

/// Put the files themselves on the clipboard, so pasting into a chat app or a
/// folder pastes the file rather than its path.
pub fn copy_files(paths: &[PathBuf]) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set().file_list(paths);
    }
}

pub fn quick_look(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("qlmanage").arg("-p").arg(path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    let _ = path;
}

/// Reduce Motion, read once per process.
pub fn reduce_motion_cached() -> bool {
    static RM: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *RM.get_or_init(reduce_motion)
}

pub fn reduce_motion() -> bool {
    #[cfg(target_os = "macos")]
    {
        return Command::new("defaults")
            .args(["read", "com.apple.universalaccess", "reduceMotion"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
            .unwrap_or(false);
    }
    #[allow(unreachable_code)]
    false
}

/// Start an OS drag session carrying real files. Drops into any app that
/// accepts files from Finder or Explorer.
pub fn start_file_drag(frame: &eframe::Frame, files: Vec<PathBuf>, preview_png: Vec<u8>) {
    #[cfg(any(target_os = "macos", windows))]
    {
        let item = drag::DragItem::Files(files);
        let _ = drag::start_drag(frame, item, drag::Image::Raw(preview_png), |_, _| {}, drag::Options::default());
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = (frame, files, preview_png);
}

/// Move a file to the Trash / Recycle Bin. On macOS this uses NSFileManager, so
/// Retype never needs permission to control Finder.
pub fn trash(path: &Path) -> Result<(), String> {
    #[allow(unused_mut)]
    let mut ctx = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        ctx.set_delete_method(DeleteMethod::NsFileManager);
    }
    ctx.delete(path).map_err(|e| e.to_string())
}

static PASTE_KEY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn note_paste_key() {
    PASTE_KEY.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(c) = crate::ipc::waker() {
        c.request_repaint();
    }
}

/// Whether ⌘V / Ctrl+V was pressed since the last call, whatever is on the clipboard.
pub fn take_paste_key(focused: bool) -> bool {
    #[cfg(windows)]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
        static WAS_DOWN: AtomicBool = AtomicBool::new(false);
        let down = focused && unsafe { GetAsyncKeyState(VK_CONTROL as i32) < 0 && GetAsyncKeyState(b'V' as i32) < 0 };
        if down && !WAS_DOWN.swap(true, Ordering::Relaxed) {
            PASTE_KEY.store(true, Ordering::Relaxed);
        } else if !down {
            WAS_DOWN.store(false, Ordering::Relaxed);
        }
    }
    let _ = focused;
    PASTE_KEY.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Whether Shift is held right now. Read from the OS because a window that is
/// only the target of a file drag gets no key events.
pub fn shift_down() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
        return unsafe { GetAsyncKeyState(VK_SHIFT as i32) < 0 };
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSEvent, NSEventModifierFlags};
        return NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Shift);
    }
    #[allow(unreachable_code)]
    false
}

/// Whether the window may be transparent around the card's rounded corners.
/// Windows doesn't rely on it: some GPU drivers ignore the alpha and the corners
/// show white. There the OS rounds the window instead (see `round_corners`).
pub const TRANSPARENT_WINDOW: bool = !cfg!(windows);

static OS_ROUNDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Windows 11: ask DWM to round the window's corners (antialiased, with the
/// system border and shadow). Windows 10 can't; its window stays square.
pub fn round_corners(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
        let Ok(h) = cc.window_handle() else { return };
        let RawWindowHandle::Win32(h) = h.as_raw() else { return };
        let pref = DWMWCP_ROUND;
        let hr = unsafe { DwmSetWindowAttribute(h.hwnd.get() as _, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&raw const pref).cast(), size_of_val(&pref) as u32) };
        OS_ROUNDED.store(hr >= 0, std::sync::atomic::Ordering::Relaxed);
    }
    let _ = cc;
}

/// The OS draws the window's rounded corners and border (Windows 11).
pub fn os_rounds_corners() -> bool {
    OS_ROUNDED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Corner radius for a card that fills its window: square on Windows, where the
/// window itself is opaque and the OS rounds it.
pub fn card_radius(radius: u8) -> u8 {
    if TRANSPARENT_WINDOW { radius } else { 0 }
}

/// Windows has no transparent corners to fill: the clear colour is the card's.
pub fn clear_color() -> [f32; 4] {
    if TRANSPARENT_WINDOW { [0.0; 4] } else { crate::theme::paper().to_normalized_gamma_f32() }
}

/// Haptic tick (macOS trackpads only).
pub fn haptic(strong: bool) {
    #[cfg(target_os = "macos")]
    crate::macos::haptic(strong);
    let _ = strong;
}

/// A system notification (used when a conversion finishes after the window closed).
pub fn notify(title: &str, body: &str) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    #[cfg(target_os = "macos")]
    let _ = Command::new("osascript").arg("-e").arg(format!("display notification \"{}\" with title \"{}\"", esc(body), esc(title))).output();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent(1); $x = $t.GetElementsByTagName('text'); $x.Item(0).AppendChild($t.CreateTextNode('{}')) > $null; $x.Item(1).AppendChild($t.CreateTextNode('{}')) > $null; [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Retype').Show([Windows.UI.Notifications.ToastNotification]::new($t))",
            title.replace('\'', "''"),
            body.replace('\'', "''")
        );
        let _ = Command::new("powershell").args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]).creation_flags(0x0800_0000).output();
    }
    let _ = (title, body, esc);
}

/// Files selected in the frontmost Finder window (macOS asks once for
/// permission to control Finder), or in open Explorer windows (Windows).
pub fn file_manager_selection() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let script = "set out to \"\"\ntell application \"Finder\"\nrepeat with f in (get selection as alias list)\nset out to out & POSIX path of f & linefeed\nend repeat\nend tell\nreturn out";
        return Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).map(|l| PathBuf::from(l.trim_end_matches('/'))).collect())
            .unwrap_or_default();
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = "(New-Object -ComObject Shell.Application).Windows() | ForEach-Object { $_.Document.SelectedItems() } | ForEach-Object { $_.Path }";
        return Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()).map(|l| PathBuf::from(l.trim())).collect())
            .unwrap_or_default();
    }
    #[allow(unreachable_code)]
    Vec::new()
}

/// macOS: no Dock icon while Retype lives only in the menu bar.
pub fn set_dock_icon(visible: bool) {
    #[cfg(target_os = "macos")]
    crate::macos::set_dock_icon(visible);
    let _ = visible;
}

pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
    #[cfg(windows)]
    let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    let _ = url;
}
