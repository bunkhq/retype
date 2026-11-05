# Retype app brief

Confirmed 2026-11-04. Stack: Rust + egui (eframe). Ships on macOS and Windows together. Visual system: `DESIGN.md` (unchanged by this brief).

## 1. Job and mode

- **Mode:** Operate. Success means a converted file exists and is already on its way to the next app, in under 3 seconds of user effort.
- **Users:** people holding a file in the wrong format, often in the middle of a task in another app. They don't want to stay in Retype. The app is a doorway, not a workspace.

## 2. One window, five states

A small fixed window, about 480×320. The title bar blends into the content (transparent, full-size content view on macOS; plain on Windows). No toolbar, sidebar, settings panel, or history list.

```
EMPTY                 OVER (dragging in)     PICK
┌──────────────────┐  ┌┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┐  ┌────────────────────────────┐
│                  │  ┆                  ┆  │ From interview.mp4 · 1.2 GB│
│ Drop a file.|    │  ┆ interview.[mov]  ┆  │        mkv                 │
│                  │  ┆                  ┆  │ interview.[mov]|           │
│ or click to pick │  ┆                  ┆  │        webm                │
└──────────────────┘  └┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┘  │ .mov .mkv .webm .gif .mp3  │
                                           │ [Save as .mov] Same quality│
                                           └────────────────────────────┘
WORKING               DONE
┌──────────────────┐  ┌────────────────────────────┐
│ interview.[mov]  │  │ [▣] interview.mov          │  ← the whole row is the file
│ ▬▬▬▬▬▬▬───── 62% │  │     drag it anywhere       │
│ Saving…  Esc     │  │ [Open]  Show in Finder     │
└──────────────────┘  │ Saved next to the original.│
                      └────────────────────────────┘
```

- **Empty:** "Drop a file." in display type with the blinking caret, like a text field waiting for input. Clicking anywhere, or ⌘O / Ctrl+O, opens the file dialog.
- **Over:** while a file is still being dragged in, the window already shows its name with the most likely target extension highlighted. egui exposes the hovered path on both OSes through `raw.hovered_files`. Releasing the file lands in Pick with the right answer already selected.
- **Pick:** the default target is the most likely one for the source type (.heic→.jpg, .mkv→.mp4, .mov→.mp4, .wav→.mp3). Retype remembers the last choice per source type, so a repeat job is drop then Enter. Scroll, arrow keys, or chips change the target. Enter, clicking the highlight, or clicking a chip converts.
- **Working:** real progress. A remux finishes almost instantly; never hold it on screen to show an animation. Esc cancels and deletes the partial file.
- **Done:** see section 3. Dropping a new file on Done (or the empty card) starts over; there is no reset button. On the picker, or while converting, dropped files join the list; hold Shift while dropping to start over instead.

## 3. The converted file behaves like a real file

- **The Done row is the file.** It shows the OS's own thumbnail or icon (macOS: QuickLook `QLThumbnailGenerator`; Windows: `IShellItemImageFactory`) next to the output name in the selection-yellow fill.
- **The whole row is a native drag source.** On drag start, Retype starts a real OS drag session: `NSDraggingSession` with a `public.file-url` item on macOS, `DoDragDrop` with `CF_HDROP` on Windows. The file drops into Slack, Premiere, Mail, browser upload fields, and Finder/Explorer folders exactly like a file dragged from the file manager. The OS default operation is copy; the platform modifier key moves.
- **Implementation:** use the `drag` crate (CrabNebula drag-rs), which takes eframe's raw window handle. Fall back to hand-written objc2 / `windows` crate code only if it falls short.
- **Keyboard equivalents:**

| Action | macOS | Windows |
|---|---|---|
| Open in default app | Enter | Enter |
| Reveal in file manager | ⌘R | Ctrl+R |
| Copy as a real file (pastes into chat apps and folders) | ⌘C | Ctrl+C |
| Quick Look | Space | n/a |

- **Batch:** dragging the Done row drags every output file as one multi-item drag.

## 4. Right-click in Finder and Explorer

The menu reads: **Retype ▸ .mov · .mkv · .webm · .gif · ─ · Open in Retype…**

- The submenu lists the same ordered targets as the app, filtered by the selected files' type. Mixed selections show only targets every file can become.
- Picking a format runs a headless conversion with the same binary (`retype convert --to mov <paths>`). The main window does not open.
- A small toast panel appears instead (top-right on macOS, bottom-right on Windows). It shows the thumbnail, the name, and the 3px progress bar. When done it is draggable exactly like the Done row, and clicking it opens the file. It closes 4 seconds after finishing unless the pointer is over it.
- **macOS:** a Finder Sync extension (a small Swift appex inside the .app that calls the Rust binary) monitoring `/`. It is the only way to get a dynamic submenu, and users must enable it once in System Settings › Extensions. Also ship a Services / Quick Action item, "Convert with Retype…", as a fallback that needs no enabling and opens the app with the file loaded.
- **Windows 11:** an `IExplorerCommand` COM DLL with subcommands (Rust, `windows` crate), registered through a sparse MSIX package so it appears in the modern menu, not only under "Show more options". Classic registry verbs cover Windows 10.

## 5. Batches

- One drop can hold many files. The source line reads "From 12 .heic files". The display line becomes `12 photos.[jpg]`, so the extension stays the picker.
- Working shows "4 of 12". Done reads "Saved 12 files next to the originals." The row shows a small stack of thumbnails and drags all 12.
- Mixed drops: one picker line per kind, stacked. Tab moves between lines; one button reads "Save all".
- Partial failure: "Saved 11. IMG_2050.heic couldn't be read." The failed file stays listed. Never fail silently.

## 6. Edge states

- **Name collision:** never overwrite. `interview 2.mov` on macOS, `interview (2).mov` on Windows.
- **Unwritable folder:** save to Downloads and say so in the status line.
- **Unsupported type:** "Can't convert .xyz yet. Try a video, audio, or image file." The window stays in Empty with the file name struck through in soft text.
- **Slow encode:** the "Re-encodes, takes longer" tag shows before the user commits; then a real percentage and time remaining.
- **Settings:** app menu only, three items: where to save (next to the original / ask), context-menu integration on/off, reset remembered choices.
- **Copy:** privacy line is "Stays on this computer." (cross-platform).

## 7. Constraints and open decisions

- **egui:** bundle Hanken Grotesk 400/600/700 through `FontDefinitions`. Read the OS Reduce Motion setting to stop the caret blink. Use the system focus ring style.
- **Neighbour colour:** resolved to #8a9098 (3.3:1).
- **Engine:** bundle an LGPL ffmpeg build or link to it; HEIC decoding via ImageIO on macOS and libheif on Windows.
- **Distribution:** the Finder extension requires signing and notarization. The Windows 11 menu requires package identity (sparse MSIX).
- **Anti-goals:** no queue view, conversion history, quality sliders, accounts, or onboarding carousel. The only onboarding is one line in Empty on first launch: "Right-click any file › Retype to skip this window.", with a link to enable the Finder extension on macOS.

## Build status (2026-11-04)

### Interaction

| Input | Where | Does |
|---|---|---|
| Drop / click / ⌘O | anywhere | Load files or folders (folders recurse, up to 2000 files) |
| ⌘V | anywhere | Load a copied file, or save a copied image to Downloads and load it |
| Dock icon, Open With, `open -a Retype` | macOS | Same as a drop, cold or warm |
| Second launch | any | Hands its files to the open window and exits |
| Type letters | Pick | Jump to the extension: `j` .jpg, `we` .webm, `m` `m` cycles the m's |
| Scroll, arrows | Pick | Step the reel |
| Enter / chip / highlight | Pick | Convert |
| ⇧ held | Pick | Smaller file (video ≤1280px long edge at CRF 28, images ≤2048px, lower audio bitrate) |
| ⌥ held | Pick | Replace: originals go to the Trash after the new file is saved |
| Tab | Pick, mixed drop | Next kind |
| Esc | Pick / Working | Clear / cancel |
| Drag row | Done, toast | Native file drag |
| Enter, ⌘R, ⌘C, Space | Done | Open, reveal, copy as file, Quick Look |
| ⌘Z | Done | New files to the Trash, back to Pick |
| Type / arrows | Done | Back to Pick with the same files, for another format |
| shrink / trash icons | Pick | Sticky Smaller / Replace toggles (same as holding ⇧ / ⌥) |
| × on a row | Pick, mixed drop | Leave that kind out |
| Pin | title row | On top of other windows (on by default) |
| Drag the card | any | Move the window (no title bar) |
| ⌘W / × | any | Close |
| Back: mouse button 4, ⌘[ (macOS), Alt+← (Windows) | Pick, Working, Done | Same as Esc |
| + / Esc / Back | Done | New file (back to the empty card). Dropping a file on Done works too; the outline returns to say so |
| ← / Esc / Back | Pick, Working | Start over. From Working this cancels and deletes anything already written, unless Replace already sent originals to the Trash |

### Formats

- Video: mp4 mov mkv webm m4v avi wmv flv mpg ts mts 3gp ogv. Remuxes when the codecs fit.
- Audio: wav aiff mp3 m4a m4b aac flac ogg opus wma caf.
- Images: heic png jpg webp avif gif tiff bmp tga ico svg psd, plus camera RAW on macOS (dng cr2 cr3 nef arw raf orf rw2). EXIF orientation is applied; metadata, including location, is not carried into re-encoded stills.
- Images to .pdf: one page per image; a batch becomes one PDF.
- Subtitles: srt vtt ass. Data: json yaml toml csv.

### Shipped
- Single instance per role (window, toast) over a localhost socket guarded by a lock file and token. Right-click launches merge into one toast and one queue.
- macOS document types in Info.plist; `application:openURLs:` added to winit's delegate at will-finish-launching.
- Probing and thumbnails off the UI thread. Temp files are unique per job; a finished file never replaces an existing one.
- Trash goes through NSFileManager on macOS, so no Finder automation prompt.
- Tests: 16 engine tests (`cargo test`), including remux, smaller output, replace, queue merging, AVIF, SVG, PDF, subtitles, data.

### Settings (gear on the empty card, or ⌘, / Ctrl+,)

| Setting | Default | Does |
|---|---|---|
| Keep photo dates and camera info | on | EXIF (capture date, camera, lens) carried into converted photos. Off: stripped. The colour profile is always kept. |
| Keep location | off | GPS kept in photos and videos. Off: GPS directory and its values are zeroed in place (photos) or the location keys cleared (video). Disabled when metadata is off. |
| Save to Downloads | off | Instead of next to the original |
| Hardware video encoding (macOS) | on | H.264 through VideoToolbox on Apple silicon; libx264 otherwise |
| Stay in the menu bar when closed | on | ⌘W / × parks Retype in the menu bar: the window turns invisible and click-through, the Dock icon goes, jobs keep running and post a notification. The menu-bar icon has Open, Convert Finder selection, Settings…, Quit. |
| ⌥⌘R (Ctrl+Alt+R) converts the Finder / Explorer selection | on | Global shortcut. macOS asks once for permission to read Finder's selection. |
| Finish converting after closing | on | Only when the menu-bar option is off: ⌘W / × mid-conversion makes the window invisible and click-through, finishes, posts a system notification, then quits. Files dropped on the app meanwhile bring it back and queue. |
| Keep window on top | on | Same as the pin |
| Finder / Explorer right-click menu | — | Install or remove |

### Photo fidelity
- macOS, photo to photo (JPEG, HEIC, TIFF, RAW → JPEG, HEIC, TIFF, PNG): ImageIO (`sips`) does the conversion, so the colour profile and all EXIF survive; GPS is then scrubbed from the output (JPEG, TIFF, PNG) or from a copy of the source before writing HEIC. CR3 and RAF can't be scrubbed in place and fall back to the plain pipeline (which drops their EXIF).
- Everything else: the colour profile is carried into JPEG, PNG, WebP and TIFF, and EXIF (orientation reset to upright, GPS scrubbed per settings) into JPEG, PNG and WebP. AVIF and HEIC written from the plain pipeline lose EXIF.

### Robustness
- Temp files are listed in `<cache>/Retype/parts.txt`; on launch, any older than 3 minutes are deleted (a crash or force-quit leftover).
- The window remembers its position.
- On launch, an installed copy of Retype (inside a `.app`, or outside `target\` on Windows) re-points the right-click menu at itself if the app was moved. Dev builds never touch it.
- Accessibility: every control reports a label (and on/off state) to screen readers through AccessKit; Tab moves focus with a 2px ink ring.

### Size estimate
With Smaller on, the action row shows "≈ 5.0 MB": images are converted for real into a temp file; video and audio encode 3 seconds from the middle and scale by duration. Batches scale the first file's estimate by the kind's total size. Runs off the UI thread; recomputed when the files, target or settings change.

### Shipping
- `scripts/bundle-macos.sh --with-ffmpeg` embeds ffmpeg, ffprobe and their libraries (16 dylibs, ~36 MB) with load paths rewritten into the app; verified to run with no Homebrew paths. Licence: Homebrew's ffmpeg includes GPL parts (x264, x265), so an app shipping it is GPL. For a closed-source release, build ffmpeg with `--disable-gpl --disable-nonfree` and set `FFMPEG_BIN_DIR`.
- `RETYPE_SIGN_ID` signs with hardened runtime (Apple Events entitlement for the Finder shortcut); `RETYPE_NOTARY_PROFILE` notarizes and staples. Untested here: no Developer ID on this machine.
- `.github/workflows/ci.yml` builds and tests on macOS and Windows and uploads the macOS app and the Windows exe. Not yet run: nothing is pushed.

### Still to do
- macOS Finder Sync extension for a top-level "Retype ▸" submenu (today it lives under Quick Actions).
- Windows 11 `IExplorerCommand` handler plus sparse MSIX. Windows is type-checked only; nothing has run on Windows yet.
- An LGPL-only ffmpeg build for a closed-source release (the bundling script takes it via `FFMPEG_BIN_DIR`). This ffmpeg has no libvorbis, so .ogg output falls back to Opus in Ogg.
- PDF to images, RAW on Windows (libraw), fonts.
- Pre-converting the likely target while the user picks (not built: wasted work on large files).
- A Developer ID to actually sign and notarize; a Windows installer. Settings for save location.
