# Product

<!-- impeccable:product-schema 1 -->

## Platform

Native desktop app, macOS and Windows shipped together (decided 2026-11-04). UI is custom-drawn with egui in the Retype visual system, not native platform controls; OS integration (drag-out, Finder/Explorer context menu, thumbnails) is native per platform.

## Stack

Rust + egui (eframe) for the app (2026-11-04). macOS Finder Sync extension is a small Swift appex calling the Rust binary; Windows context menu is a Rust `IExplorerCommand` COM DLL in a sparse MSIX package. Brand exploration is a static HTML page. App UX and flow: `docs/app-brief.md`.

## Users and job

People who hold a file in one format and need the adjacent one right now: video editors, designers, podcasters, and everyday users (mp4 to mov, heic to jpg, wav to aiff, mkv to mp4). The job is a one-step swap, not a media-processing session.

## What it makes possible

Drop a file, get the neighbouring format back, with nothing to configure.

## Core interaction (confirmed 2026-11-04)

Drag and drop a file, quickly move through the formats it can become, convert with one click. Speed and lack of friction are the product.

## Confirmed app decisions (2026-11-04)

- A converted file is a real file in the app: the Done row is a native OS drag source, and can be opened, revealed, or copied to the clipboard as a file.
- Right-click in Finder/Explorer shows a **Retype ▸** submenu of target formats; picking one converts in place with a small toast, no main window.
- One drop can hold many files; same-type batches share one picker, mixed drops get one picker per kind.

## Position and mechanism

- **Local and private.** Files never leave the device. No uploads, no accounts, no servers.
- **Lossless by default.** Where formats share codecs, it remuxes the container instead of re-encoding, so the swap is instant and quality is untouched.

## Brand commitments

- Name: **Retype** (chosen 2026-11-04 from the round-2 exploration). Trademark, App Store and domain checks not done yet.
- Identity direction: Retype. White, ink, and a text-selection yellow; one typeface (Hanken Grotesk); the extension after the dot is the format picker. Exploration lives in `brand/exploration/`.
- Voice: plain and direct. No corny buzzwords or marketing slogans (user, 2026-11-04).
- Identity: a strong brand, but restrained. Not hyper-flashy; the brand lives in details around a fast, simple interaction (user, 2026-11-04, after rejecting a first round of heavily themed directions).

## Open decisions

- Name clearance (trademark, App Store, domain).
- Media engine: bundled LGPL ffmpeg build vs linking; HEIC decode on Windows.
- Pricing / distribution model.
