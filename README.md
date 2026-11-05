# Retype

Drop a file, get the neighbouring format back. Retype is a small desktop app for macOS and Windows that switches a file to the format next door: `.mp4` to `.mov`, `.heic` to `.jpg`, `.wav` to `.mp3`, a folder of PNGs to one PDF. Files never leave your computer. When the codecs already fit, Retype swaps the container instead of re-encoding, so the result is instant and identical in quality.

## Install

Download the latest build from [Releases](https://github.com/bunkhq/retype/releases/latest):

| System | File |
|---|---|
| macOS, Apple silicon | `Retype-<version>-mac-arm64.dmg` |
| macOS, Intel | `Retype-<version>-mac-intel.dmg` |
| Windows 10/11 (64-bit) | `Retype-<version>-setup.exe` |

**macOS:** open the DMG and drag Retype into Applications. Builds are not yet signed with an Apple Developer ID, so the first launch is blocked: open **System Settings › Privacy & Security** and click **Open Anyway** (or right-click Retype in Applications and choose **Open**). Once.

**Windows:** run the setup. It installs for your user only, with no admin prompt. Builds are not yet code-signed, so SmartScreen may warn: click **More info › Run anyway**.

Everything Retype needs (including FFmpeg) is inside the installer.

## Use

- Drop files or a folder on the card, or paste with ⌘V / Ctrl+V.
- Type to pick a format (`j` for .jpg), or scroll, or use the arrow keys. Enter converts.
- Hold ⇧ for a smaller file, ⌥ (Alt) to replace the original. Both are also toggles on the card.
- Drag the result straight into another app, or open, reveal, copy it.
- Right-click files in Finder or Explorer to convert without opening the window.
- ⌥⌘R (Ctrl+Alt+R) converts whatever is selected in Finder or Explorer.
- Closing the window keeps Retype in the menu bar / system tray.

Settings (gear, or ⌘,) cover metadata and location, where files are saved, hardware video encoding, the menu bar, the shortcut, and update checks. Full reference: [`docs/app-brief.md`](docs/app-brief.md).

## Updates

Retype checks GitHub for a newer release once a day (turn it off in Settings). When one is out, a download icon appears on the card; click it to install. On macOS the app replaces itself and relaunches; on Windows the installer runs silently and reopens Retype. That daily check is the only network request Retype makes.

To install a downloaded update by hand: `retype update-from <file.dmg | setup.exe>`.

## Formats

Video: mp4 mov mkv webm m4v avi wmv flv mpg ts mts 3gp ogv · Audio: wav aiff mp3 m4a m4b aac flac ogg opus wma caf · Images: heic png jpg webp avif gif tiff bmp tga ico svg psd, camera RAW on macOS · Images to PDF · Subtitles: srt vtt ass · Data: json yaml toml csv.

## Build from source

Needs Rust (stable) and FFmpeg on PATH for development (`brew install ffmpeg`, or `choco install ffmpeg-full`).

```sh
cargo run                                   # the window
cargo run -- open clip.mp4 photo.heic       # window with files loaded
cargo run -- convert --to mov clip.mp4      # headless, with the corner toast
cargo test                                  # engine tests (need ffmpeg)
RETYPE_THEME=dark cargo run                 # force a theme (default: follow the system)
RETYPE_INSTANCE=dev cargo run               # run beside an installed Retype, with its own prefs
cargo run --example make_icons              # redraw the app icons (then iconutil, see the example)
```

Packaging:

```sh
scripts/bundle-macos.sh [--with-ffmpeg]     # dist/Retype.app
scripts/package-macos.sh                    # dist/Retype-<version>-mac-<arch>.dmg (self-contained)
# Windows (PowerShell):
cargo build --release
scripts\fetch-ffmpeg-windows.ps1
iscc /DAppVersion=<version> installer\windows\retype.iss   # dist\Retype-<version>-setup.exe
```

Signing: set `RETYPE_SIGN_ID` ("Developer ID Application: …") and `RETYPE_NOTARY_PROFILE` (from `xcrun notarytool store-credentials`) before the macOS scripts.

## Releasing

Bump `version` in `Cargo.toml`, commit, then tag and push:

```sh
git tag v0.2.0 && git push origin v0.2.0
```

[`.github/workflows/release.yml`](.github/workflows/release.yml) builds both DMGs and the Windows installer and publishes a GitHub Release, which is what the in-app updater reads. [`ci.yml`](.github/workflows/ci.yml) builds and tests every push on macOS and Windows.

## What's here

| Path | What it is |
|---|---|
| `src/` | `app.rs` the card and its screens, `engine.rs` conversions, `formats.rs` what becomes what, `metadata.rs` GPS scrubbing, `update.rs` updater, `resident.rs` menu bar and shortcut, `integration.rs` right-click menus, `ipc.rs` single instance, `toast.rs` right-click progress panel, `platform.rs` / `macos.rs` OS bridges |
| `installer/windows/retype.iss` | Inno Setup script for `setup.exe` |
| `scripts/` | macOS bundle and DMG, FFmpeg embedding, Windows FFmpeg download |
| `assets/` | Icons (`icons/Retype.icns`, `icons/retype.ico`), fonts |
| `DESIGN.md`, `PRODUCT.md`, `docs/` | Visual system, product context, app brief |
| `brand/exploration/` | Identity exploration boards (open in a browser) |

## Licence

Retype is free software under the [GNU General Public License v3.0 or later](LICENSE). The installers bundle FFmpeg built with GPL components; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for FFmpeg, the Hanken Grotesk font (OFL) and Lucide icons (ISC).
