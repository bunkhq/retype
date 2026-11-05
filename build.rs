//! Windows: embed the app icon and version info into retype.exe, so Explorer,
//! the taskbar, Start, and the installer all show the Retype mark.
fn main() {
    println!("cargo:rerun-if-changed=assets/icons/retype.ico");
    // Only when building on Windows: cross-checks from a Mac have no resource compiler.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && cfg!(windows) {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/retype.ico")
            .set("ProductName", "Retype")
            .set("FileDescription", "Retype")
            .set("CompanyName", "Retype")
            .set("LegalCopyright", "GPL-3.0-or-later");
        res.compile().expect("embed Windows resources");
    }
}
