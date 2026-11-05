# Third-party notices

Retype is licensed under the GNU General Public License v3.0 or later (see `LICENSE`).
It includes or depends on the following.

## Bundled in the installers

- **FFmpeg** (ffmpeg, ffprobe and their libraries). Built with GPL components (x264, x265),
  so the bundled FFmpeg is under the GNU GPL. Source: https://ffmpeg.org.
  macOS builds bundle Homebrew's FFmpeg; Windows builds bundle
  https://github.com/BtbN/FFmpeg-Builds (`win64-gpl-shared`), whose licence text ships as
  `FFMPEG-LICENSE.txt` next to `retype.exe`.
- **Hanken Grotesk** typeface, SIL Open Font License 1.1 (`assets/fonts/OFL.txt`).
- **Lucide** icons (font build), ISC License (`assets/fonts/LUCIDE-LICENSE.txt`).

## Rust crates

Compiled into `retype` (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0 or similar permissive
licences): eframe/egui/winit, image, resvg/usvg, psd, arboard, trash, drag, rfd, tray-icon,
muda, global-hotkey, ureq, rustls, serde/serde_json, serde_yaml_ng, toml, csv, chrono,
dirs, objc2, windows-sys and their dependencies. Run `cargo tree` for the full list and
each crate's `Cargo.toml` for its licence.
