//! Update checks against GitHub Releases, and one-click install.
//!
//! The check is the only time Retype talks to the network: one request to the
//! GitHub API for the latest release, at launch and every 24 hours, and only
//! while "Check for updates" is on. Files never leave the computer.
//!
//! Installing:
//! - macOS: download this Mac's DMG, mount it, replace the running .app in place,
//!   relaunch.
//! - Windows: download setup.exe and run it silently; the installer closes
//!   Retype, upgrades it, and starts it again.

use std::io::Read;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const REPO: &str = "bunkhq/retype";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub page: String,
    /// Installer for this platform: (file name, download URL).
    pub asset: Option<(String, String)>,
}

#[derive(Clone, Debug)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(String),
    Failed(String),
}

pub type Shared = Arc<Mutex<State>>;

fn agent() -> ureq::Agent {
    // The OS's own TLS (Security.framework / SChannel): no bundled crypto library.
    let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build();
    ureq::Agent::config_builder().tls_config(tls).timeout_global(Some(Duration::from_secs(30))).user_agent(format!("Retype/{VERSION}")).build().into()
}

/// "1.10.2" > "1.9.0"; a leading "v" is ignored.
pub fn newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| v.trim_start_matches('v').split(['.', '-']).take(3).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(candidate) > parse(current)
}

/// The installer that fits this machine, by file name.
pub fn pick_asset(names: &[(String, String)]) -> Option<(String, String)> {
    let want = |n: &str| {
        let n = n.to_ascii_lowercase();
        if cfg!(target_os = "macos") {
            let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "intel" };
            n.ends_with(".dmg") && n.contains(arch)
        } else if cfg!(windows) {
            n.ends_with("setup.exe")
        } else {
            false
        }
    };
    names.iter().find(|(n, _)| want(n)).cloned()
}

/// Ask GitHub for the latest release. Ok(None) when this is the newest.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut res = agent().get(&url).header("Accept", "application/vnd.github+json").call().map_err(|e| e.to_string())?;
    let v: serde_json::Value = res.body_mut().read_json().map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("no release found")?;
    if !newer(tag, VERSION) {
        return Ok(None);
    }
    let assets: Vec<(String, String)> = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a["name"].as_str()?.to_string(), a["browser_download_url"].as_str()?.to_string())))
        .collect();
    Ok(Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        page: v["html_url"].as_str().unwrap_or("").to_string(),
        asset: pick_asset(&assets),
    }))
}

/// Run a check on a worker thread, writing the result into `state`.
pub fn check_in_background(state: Shared, on_done: impl Fn() + Send + 'static) {
    *state.lock().unwrap() = State::Checking;
    std::thread::spawn(move || {
        let next = match check() {
            Ok(Some(r)) => State::Available(r),
            Ok(None) => State::UpToDate,
            Err(e) => State::Failed(e),
        };
        *state.lock().unwrap() = next;
        on_done();
    });
}

fn download(url: &str, to: &Path, progress: &dyn Fn(f32)) -> Result<(), String> {
    let mut res = agent().get(url).call().map_err(|e| e.to_string())?;
    let total: Option<u64> = res.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
    let mut reader = res.body_mut().with_config().limit(1 << 31).reader();
    let mut file = std::fs::File::create(to).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 16];
    let mut got = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if let Some(t) = total {
            progress(got as f32 / t as f32);
        }
    }
    Ok(())
}

/// Download and install `release`, reporting progress into `state`. On success
/// this process exits (the new version takes over).
pub fn install_in_background(release: Release, state: Shared, on_change: impl Fn() + Send + Sync + 'static) {
    let on_change = Arc::new(on_change);
    std::thread::spawn(move || {
        let set = |s: State| {
            *state.lock().unwrap() = s;
            on_change();
        };
        let Some((name, url)) = release.asset.clone() else {
            set(State::Failed("No installer for this computer in that release.".into()));
            return;
        };
        let dir = std::env::temp_dir().join(format!("retype-update-{}", release.version));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join(&name);
        set(State::Installing("Downloading…".into()));
        let progress = |f: f32| set(State::Installing(format!("Downloading… {}%", (f * 100.0).round() as i32)));
        if let Err(e) = download(&url, &file, &progress) {
            set(State::Failed(format!("Download failed: {e}")));
            return;
        }
        set(State::Installing("Installing…".into()));
        match apply(&file) {
            Ok(()) => std::process::exit(0),
            Err(e) => set(State::Failed(e)),
        }
    });
}

/// The .app bundle this process runs from, if any.
#[cfg(target_os = "macos")]
fn current_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

#[cfg(target_os = "macos")]
pub fn apply(dmg: &Path) -> Result<(), String> {
    use std::process::Command;
    let bundle = current_bundle().ok_or("Retype isn't running from an installed app, so it can't replace itself. Download the update from the release page.")?;
    let mount = dmg.with_extension("mount");
    let _ = std::fs::create_dir_all(&mount);
    let ok = Command::new("hdiutil").args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"]).arg(&mount).arg(dmg).output().map(|o| o.status.success()).unwrap_or(false);
    if !ok {
        return Err("Couldn't open the downloaded disk image.".into());
    }
    let detach = || {
        let _ = Command::new("hdiutil").args(["detach", "-quiet", "-force"]).arg(&mount).output();
    };
    let src = mount.join("Retype.app");
    if !src.exists() {
        detach();
        return Err("The disk image doesn't contain Retype.app.".into());
    }
    // Swap atomically-ish: copy beside, then move the old one aside and the new one in.
    let staged = bundle.with_file_name(".Retype-update.app");
    let old = bundle.with_file_name(".Retype-old.app");
    let _ = std::fs::remove_dir_all(&staged);
    let _ = std::fs::remove_dir_all(&old);
    let copied = Command::new("ditto").arg(&src).arg(&staged).output().map(|o| o.status.success()).unwrap_or(false);
    detach();
    if !copied {
        return Err("Couldn't copy the update. Is the Applications folder writable?".into());
    }
    std::fs::rename(&bundle, &old).map_err(|e| format!("Couldn't replace Retype: {e}"))?;
    if let Err(e) = std::fs::rename(&staged, &bundle) {
        let _ = std::fs::rename(&old, &bundle);
        return Err(format!("Couldn't replace Retype: {e}"));
    }
    let _ = std::fs::remove_dir_all(&old);
    // Relaunch once this process (and its single-instance lock) is gone.
    let _ = Command::new("sh").arg("-c").arg(format!("sleep 1; open '{}'", bundle.display().to_string().replace('\'', "'\\''"))).spawn();
    Ok(())
}

#[cfg(windows)]
pub fn apply(setup: &Path) -> Result<(), String> {
    // Inno Setup: closes Retype, upgrades in place, starts it again ([Run] entry).
    std::process::Command::new(setup)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't start the installer: {e}"))
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn apply(_: &Path) -> Result<(), String> {
    Err("Automatic install isn't available on this platform.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_order() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("1.10.0", "1.9.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn picks_this_platforms_installer() {
        let names: Vec<(String, String)> = ["Retype-0.2.0-mac-arm64.dmg", "Retype-0.2.0-mac-intel.dmg", "Retype-0.2.0-setup.exe"].iter().map(|n| (n.to_string(), format!("https://x/{n}"))).collect();
        let got = pick_asset(&names).map(|(n, _)| n);
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(got.as_deref(), Some("Retype-0.2.0-mac-arm64.dmg"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(got.as_deref(), Some("Retype-0.2.0-mac-intel.dmg"));
        } else if cfg!(windows) {
            assert_eq!(got.as_deref(), Some("Retype-0.2.0-setup.exe"));
        }
    }
}
