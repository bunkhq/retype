//! Right-click menu integration.
//!
//! macOS: one Finder Quick Action per target format, each limited to the source
//! types that can become it, so Finder only lists the formats that apply
//! (Quick Actions › Retype to .jpg). A Finder Sync extension with a true
//! "Retype ▸" submenu is the planned upgrade; it needs a signed app extension.
//!
//! Windows: a classic cascading "Retype ▸" verb per source type under
//! HKCU\Software\Classes\SystemFileAssociations. On Windows 11 it appears under
//! "Show more options" until the IExplorerCommand handler ships.

use crate::formats::FORMATS;
use std::path::PathBuf;

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

pub fn menu_path() -> &'static str {
    if cfg!(target_os = "macos") { "Quick Actions › Retype to …" } else { "Retype" }
}

/// Every target any source offers, in first-seen order.
#[cfg_attr(windows, allow(dead_code))]
fn all_targets() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for f in FORMATS {
        for t in crate::formats::targets_for(f.ext) {
            if !v.contains(&t) {
                v.push(t);
            }
        }
    }
    v
}

// ------------------------------------------------------------------ macOS

#[cfg(target_os = "macos")]
fn services_dir() -> Result<PathBuf, String> {
    Ok(dirs::home_dir().ok_or("No home folder")?.join("Library/Services"))
}

#[cfg(target_os = "macos")]
const PREFIX: &str = "Retype to .";

#[cfg(target_os = "macos")]
const OPEN_NAME: &str = "Open in Retype";

#[cfg(target_os = "macos")]
pub fn installed() -> bool {
    services_dir().map(|d| d.join(format!("{OPEN_NAME}.workflow")).exists()).unwrap_or(false)
}

/// Whether the installed menu runs this copy of Retype.
#[cfg(target_os = "macos")]
fn points_here() -> bool {
    let (Ok(dir), Ok(exe)) = (services_dir(), exe()) else { return true };
    std::fs::read_to_string(dir.join(format!("{OPEN_NAME}.workflow/Contents/document.wflow")))
        .map(|w| w.contains(&xml(&shell_quote(&exe.display().to_string()))))
        .unwrap_or(true)
}

#[cfg(target_os = "macos")]
pub fn install() -> Result<(), String> {
    let dir = services_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = exe()?;
    let q = shell_quote(&exe.display().to_string());
    for t in all_targets() {
        let utis: Vec<&str> = FORMATS.iter().filter(|f| crate::formats::targets_for(f.ext).contains(&t)).flat_map(|f| f.utis.iter().copied()).collect();
        let cmd = format!("{q} convert --to {t} \"$@\" >/dev/null 2>&1 &");
        write_workflow(&dir.join(format!("{PREFIX}{t}.workflow")), &format!("{PREFIX}{t}"), &utis, &cmd)?;
    }
    let cmd = format!("{q} open \"$@\" >/dev/null 2>&1 &");
    let mut all: Vec<&str> = vec!["public.movie", "public.audio", "public.image", "public.folder"];
    all.extend(FORMATS.iter().flat_map(|f| f.utis.iter().copied()));
    write_workflow(&dir.join(format!("{OPEN_NAME}.workflow")), &format!("{OPEN_NAME}…"), &all, &cmd)?;
    refresh_services();
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<(), String> {
    let dir = services_dir()?;
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".workflow") && (name.starts_with(PREFIX) || name.starts_with(OPEN_NAME)) {
            std::fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    refresh_services();
    Ok(())
}

#[cfg(target_os = "macos")]
fn refresh_services() {
    let _ = std::process::Command::new("/System/Library/CoreServices/pbs").arg("-update").output();
}

#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(target_os = "macos")]
fn write_workflow(path: &std::path::Path, title: &str, utis: &[&str], command: &str) -> Result<(), String> {
    let contents = path.join("Contents");
    std::fs::create_dir_all(&contents).map_err(|e| e.to_string())?;
    let types: String = utis.iter().map(|u| format!("<string>{}</string>", xml(u))).collect();
    let info = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>NSServices</key><array><dict>
<key>NSBackgroundColorName</key><string>background</string>
<key>NSIconName</key><string>NSActionTemplate</string>
<key>NSMenuItem</key><dict><key>default</key><string>{title}</string></dict>
<key>NSMessage</key><string>runWorkflowAsService</string>
<key>NSRequiredContext</key><dict><key>NSApplicationIdentifier</key><string>com.apple.finder</string></dict>
<key>NSSendFileTypes</key><array>{types}</array>
</dict></array>
</dict></plist>
"#,
        title = xml(title)
    );
    let uuid = |n: u8| format!("5E7A1B2C-0000-4000-8000-0000000000{n:02X}");
    let wflow = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>AMApplicationBuild</key><string>523</string>
<key>AMApplicationVersion</key><string>2.10</string>
<key>AMDocumentVersion</key><string>2</string>
<key>actions</key><array><dict>
<key>action</key><dict>
<key>AMAccepts</key><dict><key>Container</key><string>List</string><key>Optional</key><true/><key>Types</key><array><string>com.apple.cocoa.string</string></array></dict>
<key>AMActionVersion</key><string>2.0.3</string>
<key>AMApplication</key><array><string>Automator</string></array>
<key>AMParameterProperties</key><dict><key>COMMAND_STRING</key><dict/><key>CheckedForUserDefaultShell</key><dict/><key>inputMethod</key><dict/><key>shell</key><dict/><key>source</key><dict/></dict>
<key>AMProvides</key><dict><key>Container</key><string>List</string><key>Types</key><array><string>com.apple.cocoa.string</string></array></dict>
<key>ActionBundlePath</key><string>/System/Library/Automator/Run Shell Script.action</string>
<key>ActionName</key><string>Run Shell Script</string>
<key>ActionParameters</key><dict>
<key>COMMAND_STRING</key><string>{cmd}</string>
<key>CheckedForUserDefaultShell</key><true/>
<key>inputMethod</key><integer>1</integer>
<key>shell</key><string>/bin/zsh</string>
<key>source</key><string></string>
</dict>
<key>BundleIdentifier</key><string>com.apple.RunShellScript</string>
<key>CFBundleVersion</key><string>2.0.3</string>
<key>CanShowSelectedItemsWhenRun</key><false/>
<key>CanShowWhenRun</key><true/>
<key>Category</key><array><string>AMCategoryUtilities</string></array>
<key>Class Name</key><string>RunShellScriptAction</string>
<key>InputUUID</key><string>{u1}</string>
<key>Keywords</key><array><string>Shell</string><string>Script</string></array>
<key>OutputUUID</key><string>{u2}</string>
<key>UUID</key><string>{u3}</string>
<key>UnlocalizedApplications</key><array><string>Automator</string></array>
<key>arguments</key><dict/>
<key>isViewVisible</key><integer>1</integer>
<key>location</key><string>309.000000:253.000000</string>
<key>nibPath</key><string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
</dict>
<key>isViewVisible</key><integer>1</integer>
</dict></array>
<key>connectors</key><dict/>
<key>workflowMetaData</key><dict>
<key>applicationBundleIDsByPath</key><dict/>
<key>applicationPaths</key><array/>
<key>inputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
<key>outputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
<key>presentationMode</key><integer>15</integer>
<key>processesInput</key><false/>
<key>serviceInputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
<key>serviceOutputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
<key>serviceProcessesInput</key><false/>
<key>systemImageName</key><string>NSActionTemplate</string>
<key>useAutomaticInputType</key><false/>
<key>workflowTypeIdentifier</key><string>com.apple.Automator.servicesMenu</string>
</dict>
</dict></plist>
"#,
        cmd = xml(command),
        u1 = uuid(1),
        u2 = uuid(2),
        u3 = uuid(3),
    );
    std::fs::write(contents.join("Info.plist"), info).map_err(|e| e.to_string())?;
    std::fs::write(contents.join("document.wflow"), wflow).map_err(|e| e.to_string())?;
    Ok(())
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let ok = std::process::Command::new("reg").args(args).creation_flags(0x0800_0000).output().map(|o| o.status.success()).map_err(|e| e.to_string())?;
    if ok { Ok(()) } else { Err("Couldn't update the registry.".into()) }
}

#[cfg(windows)]
fn assoc_key(ext: &str) -> String {
    format!(r"HKCU\Software\Classes\SystemFileAssociations\.{ext}\shell\Retype")
}

#[cfg(windows)]
pub fn installed() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg").args(["query", &assoc_key("png")]).creation_flags(0x0800_0000).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Every spelling Explorer should attach the menu to.
#[cfg(windows)]
fn windows_exts() -> Vec<&'static str> {
    FORMATS
        .iter()
        .filter(|f| crate::formats::lookup(f.ext).is_some())
        .flat_map(|f| std::iter::once(f.ext).chain(crate::formats::aliases(f.ext).iter().copied()))
        .collect()
}

/// Whether the installed menu runs this copy of Retype, and is the current layout
/// (menus from 0.1.1 and earlier have no command on the "Retype" verb itself).
#[cfg(windows)]
fn points_here() -> bool {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = exe() else { return true };
    let runs_here = |key: String| {
        std::process::Command::new("reg")
            .args(["query", &key, "/ve"])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains(&exe.display().to_string()))
            .unwrap_or(true)
    };
    runs_here(format!(r"{}\shell\99open\command", assoc_key("png"))) && runs_here(format!(r"{}\command", assoc_key("png")))
}

#[cfg(windows)]
pub fn install() -> Result<(), String> {
    let exe = exe()?.display().to_string();
    for ext in windows_exts() {
        let norm = crate::formats::ext_of(std::path::Path::new(&format!("x.{ext}"))).unwrap_or_default();
        let base = assoc_key(ext);
        reg(&["add", &base, "/v", "MUIVerb", "/d", "Retype", "/f"])?;
        reg(&["add", &base, "/v", "SubCommands", "/d", "", "/f"])?;
        reg(&["add", &base, "/v", "Icon", "/d", &exe, "/f"])?;
        // Explorer can invoke the cascade's own verb too (Enter on it, the Windows 11
        // menu, or as the default when a type has no "open"). Without a command that
        // shows "This file does not have an app associated with it"; open in Retype instead.
        reg(&["add", &format!(r"{base}\command"), "/ve", "/d", &format!("\"{exe}\" open \"%1\""), "/f"])?;
        for (i, t) in crate::formats::targets_for(&norm).iter().enumerate() {
            let k = format!(r"{base}\shell\{i:02}{t}");
            reg(&["add", &k, "/v", "MUIVerb", "/d", &format!(".{t}"), "/f"])?;
            // One launch per selected file, without Explorer's 15-file cap; the
            // launches merge into one toast.
            reg(&["add", &k, "/v", "MultiSelectModel", "/d", "Player", "/f"])?;
            reg(&["add", &format!(r"{k}\command"), "/ve", "/d", &format!("\"{exe}\" convert --to {t} \"%1\""), "/f"])?;
        }
        let k = format!(r"{base}\shell\99open");
        reg(&["add", &k, "/v", "MUIVerb", "/d", "Open in Retype…", "/f"])?;
        reg(&["add", &k, "/v", "CommandFlags", "/t", "REG_DWORD", "/d", "32", "/f"])?; // separator before
        reg(&["add", &format!(r"{k}\command"), "/ve", "/d", &format!("\"{exe}\" open \"%1\""), "/f"])?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn uninstall() -> Result<(), String> {
    for ext in windows_exts() {
        let _ = reg(&["delete", &assoc_key(ext), "/f"]);
    }
    Ok(())
}

/// If the menu is installed but points at an old copy (the app was moved, e.g.
/// from Downloads to Applications), reinstall it for this copy.
#[cfg(any(target_os = "macos", windows))]
pub fn repair() {
    // Only the installed app repairs; a dev build (`cargo run`) must not take the menu over.
    let installed_copy = exe().map(|e| {
        let e = e.display().to_string();
        if cfg!(target_os = "macos") { e.contains(".app/Contents/MacOS/") } else { !e.contains("\\target\\") }
    });
    if !installed_copy.unwrap_or(false) {
        return;
    }
    if installed() && !points_here() {
        let _ = install();
    }
}

// ------------------------------------------------------------------ other

#[cfg(not(any(target_os = "macos", windows)))]
pub fn repair() {}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn installed() -> bool {
    false
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn install() -> Result<(), String> {
    let _ = (exe(), all_targets());
    Err("The right-click menu is only available on macOS and Windows.".into())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn uninstall() -> Result<(), String> {
    Ok(())
}
