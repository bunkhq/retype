//! The Retype window: a floating card with five states (Empty, Over, Pick,
//! Working, Done). The card is as tall as its content and springs between sizes.

use crate::engine::{self, Job, JobState, Opts, Task};
use crate::formats::{self, Cost, Kind, Probe};
use crate::platform;
use crate::prefs::Prefs;
use crate::theme::{self, W, icon};
use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, Id, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const WIDTH: f32 = 460.0;
/// Starting height; the card resizes to its content on the first frame.
pub const START_HEIGHT: f32 = 170.0;
const PAD: f32 = 28.0;
const RADIUS: u8 = 14;
const BODY_SMALL: f32 = 14.0;
/// Folder drops stop collecting after this many files.
const FOLDER_LIMIT: usize = 2000;

pub fn kind_icon(k: Kind) -> char {
    match k {
        Kind::Video => icon::VIDEO,
        Kind::Audio => icon::AUDIO,
        Kind::Image | Kind::Document => icon::IMAGE,
        Kind::Subtitle => icon::TEXT,
        Kind::Data => icon::DATA,
    }
}

/// Shortcut labels for tooltips.
fn keys(mac: &str, other: &str) -> String {
    if cfg!(target_os = "macos") { mac.to_string() } else { other.to_string() }
}

pub struct Group {
    pub kind: Kind,
    pub files: Vec<PathBuf>,
    pub targets: Vec<&'static str>,
    pub sel: usize,
    pub probe: Option<Probe>,
    pub bytes: u64,
}

impl Group {
    fn source_ext(&self) -> String {
        formats::ext_of(&self.files[0]).unwrap_or_default()
    }

    fn single_ext(&self) -> bool {
        let first = self.source_ext();
        self.files.iter().all(|f| formats::ext_of(f).as_deref() == Some(&first))
    }

    pub fn target(&self) -> &'static str {
        self.targets[self.sel]
    }

    /// The stem shown in display type: the file name, or "12 photos".
    fn stem(&self) -> String {
        if self.files.len() == 1 {
            self.files[0].file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string()
        } else {
            self.kind.plural(self.files.len())
        }
    }

    fn cost(&self, opts: Opts) -> Cost {
        let c = formats::cost(&self.source_ext(), self.target(), self.probe.as_ref());
        if opts.smaller && self.shrinkable() {
            if self.kind == Kind::Video { Cost::Slow } else { Cost::Quick }
        } else {
            c
        }
    }

    /// Whether Smaller means anything for this group.
    fn shrinkable(&self) -> bool {
        matches!(self.kind, Kind::Video | Kind::Audio | Kind::Image) && !matches!(self.target(), "wav" | "aiff" | "flac")
    }

    fn step(&mut self, d: i32) {
        let n = self.targets.len() as i32;
        self.sel = ((self.sel as i32 + d).rem_euclid(n)) as usize;
    }
}

/// Folders become the supported files inside them (recursively, hidden files skipped).
pub fn expand(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if out.len() >= FOLDER_LIMIT {
                return;
            }
            if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')) {
                continue;
            }
            if p.is_dir() {
                walk(&p, out);
            } else if formats::ext_of(&p).is_some_and(|e| formats::lookup(&e).is_some()) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() { walk(p, &mut out) } else { out.push(p.clone()) }
    }
    out
}

/// Split paths into one group per kind, plus the paths Retype cannot read.
pub fn group_files(paths: &[PathBuf], prefs: &Prefs) -> (Vec<Group>, Vec<PathBuf>) {
    let mut groups: Vec<Group> = Vec::new();
    let mut rejected = Vec::new();
    for p in paths {
        let Some(ext) = formats::ext_of(p).filter(|e| formats::lookup(e).is_some()) else {
            rejected.push(p.clone());
            continue;
        };
        let kind = formats::kind_of_ext(&ext).unwrap();
        let bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        match groups.iter_mut().find(|g| g.kind == kind) {
            Some(g) => {
                g.files.push(p.clone());
                g.bytes += bytes;
            }
            None => groups.push(Group { kind, files: vec![p.clone()], targets: vec![], sel: 0, probe: None, bytes }),
        }
    }
    for g in &mut groups {
        // Offer the union of targets, most common source first. A file already in
        // the chosen format is left alone.
        let mut counts: Vec<(String, usize)> = Vec::new();
        for f in &g.files {
            let e = formats::ext_of(f).unwrap_or_default();
            match counts.iter_mut().find(|(x, _)| *x == e) {
                Some((_, n)) => *n += 1,
                None => counts.push((e, 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        let mut t: Vec<&'static str> = Vec::new();
        for (e, _) in &counts {
            for x in formats::targets_for(e) {
                if !t.contains(&x) {
                    t.push(x);
                }
            }
        }
        // Never default to a format most of the files are already in.
        let main = counts[0].0.clone();
        t.retain(|x| *x != main || counts.len() > 1);
        if counts.len() > 1 {
            if let Some(i) = t.iter().position(|x| *x == main) {
                let x = t.remove(i);
                t.push(x);
            }
        }
        g.targets = t;
        g.sel = prefs.last_target.get(&main).and_then(|l| g.targets.iter().position(|x| x == l)).unwrap_or(0);
    }
    (groups, rejected)
}

struct Finished {
    state: JobState,
    thumb: Option<egui::TextureHandle>,
    drag_png: Vec<u8>,
    sources: usize,
    copied_at: Option<Instant>,
    /// When Done appeared; the drop-zone outline returns after the entrance settles.
    at: Instant,
}

enum Screen {
    Empty,
    Pick,
    Working(Job),
    Done(Box<Finished>),
    Settings,
}

pub struct RetypeApp {
    prefs: Prefs,
    screen: Screen,
    groups: Vec<Group>,
    active: usize,
    notice: Option<(char, String, Instant)>,
    rejected_name: Option<String>,
    scroll_acc: f32,
    typed: String,
    typed_at: Instant,
    probes: Option<mpsc::Receiver<(usize, Probe)>>,
    /// Sticky toggles; holding ⇧ / ⌥ turns them on for one action.
    smaller_on: bool,
    replace_on: bool,
    reduce_motion: bool,
    integration_on: bool,
    started: Instant,
    sized: bool,
    last_height: f32,
    last_width: f32,
    /// Where the card is heading (last frame's measured content).
    goal: Vec2,
    shadow_dirty: bool,
    hovering: bool,
    /// Back navigation this frame (mouse Back button, ⌘[, Alt+←); acts like Esc.
    back_nav: bool,
    /// Hidden and finishing a job after the window was closed.
    background: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Files that arrived mid-conversion; loaded when it ends.
    pending: Vec<PathBuf>,
    /// Options of the running conversion, for files that join it.
    job_opts: Opts,
    /// The notice is the "drop to add" hint shown while files hover.
    drop_hint: bool,
    pos_saved_at: Instant,
    resident: crate::resident::Resident,
    /// Closed into the menu bar: invisible, click-through, still running.
    parked: bool,
    quitting: bool,
    /// Size estimate for the Smaller option: (what it was computed for, result).
    estimate: Option<(String, std::sync::Arc<std::sync::Mutex<Option<Option<u64>>>>)>,
    update: crate::update::Shared,
    last_update_check: Option<Instant>,
    /// The user asked (menu): report "up to date" too.
    manual_update_check: bool,
}

const REEL: &str = "reel";
const DONE_IN: &str = "done-in";

impl RetypeApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Vec<PathBuf>) -> Self {
        theme::install(&cc.egui_ctx);
        crate::ipc::set_waker(&cc.egui_ctx);
        platform::round_corners(cc);
        #[cfg(target_os = "macos")]
        crate::macos::watch_paste_key();
        let mut app = RetypeApp {
            prefs: Prefs::load(),
            screen: Screen::Empty,
            groups: vec![],
            active: 0,
            notice: None,
            rejected_name: None,
            scroll_acc: 0.0,
            typed: String::new(),
            typed_at: Instant::now(),
            probes: None,
            smaller_on: false,
            replace_on: false,
            reduce_motion: platform::reduce_motion_cached(),
            integration_on: crate::integration::installed(),
            started: Instant::now(),
            sized: false,
            last_height: START_HEIGHT,
            last_width: WIDTH,
            goal: Vec2::new(WIDTH, START_HEIGHT),
            shadow_dirty: true,
            hovering: false,
            back_nav: false,
            background: Default::default(),
            pending: Vec::new(),
            job_opts: Opts::default(),
            drop_hint: false,
            pos_saved_at: Instant::now(),
            resident: Default::default(),
            parked: false,
            quitting: false,
            estimate: None,
            update: std::sync::Arc::new(std::sync::Mutex::new(crate::update::State::Idle)),
            last_update_check: None,
            manual_update_check: false,
        };
        if !initial.is_empty() {
            app.load(&initial);
        }
        app
    }

    fn say(&mut self, glyph: char, s: impl Into<String>) {
        self.notice = Some((glyph, s.into(), Instant::now()));
    }

    fn load(&mut self, paths: &[PathBuf]) {
        if let Screen::Working(job) = &self.screen {
            job.cancel();
        }
        let had_folder = paths.iter().any(|p| p.is_dir());
        let files = expand(paths);
        let (groups, rejected) = group_files(&files, &self.prefs);
        self.notice = None;
        self.rejected_name = None;
        self.typed.clear();
        if groups.is_empty() {
            self.groups.clear();
            self.screen = Screen::Empty;
            if had_folder && rejected.is_empty() {
                self.say(icon::ALERT, "Nothing in that folder Retype can convert.");
                return;
            }
            let ext = rejected.first().and_then(|p| p.extension()).and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
            self.say(icon::ALERT, match ext {
                Some(e) => format!("Can't convert .{e} yet."),
                None => "Can't convert that yet.".into(),
            });
            self.rejected_name = rejected.first().and_then(|p| p.file_name()).and_then(|n| n.to_str()).map(str::to_string);
            return;
        }
        if !rejected.is_empty() {
            let n = rejected.len();
            self.say(icon::ALERT, if n == 1 { "Skipped 1 file Retype can't convert.".to_string() } else { format!("Skipped {n} files Retype can't convert.") });
        } else if files.len() >= FOLDER_LIMIT {
            self.say(icon::ALERT, format!("Took the first {FOLDER_LIMIT} files."));
        }
        // Probe codecs off the UI thread; the quality badge updates when they land.
        let (tx, rx) = mpsc::channel();
        let firsts: Vec<(usize, PathBuf)> = groups.iter().enumerate().filter(|(_, g)| matches!(g.kind, Kind::Video | Kind::Audio)).map(|(i, g)| (i, g.files[0].clone())).collect();
        let ctx = crate::ipc::waker();
        std::thread::spawn(move || {
            for (i, f) in firsts {
                if let Some(p) = engine::probe(&f) {
                    let _ = tx.send((i, p));
                    if let Some(c) = &ctx {
                        c.request_repaint();
                    }
                }
            }
        });
        self.probes = Some(rx);
        self.groups = groups;
        self.active = 0;
        self.screen = Screen::Pick;
    }

    /// Files dropped on a card that already has some: they join the list instead
    /// of replacing it. On Pick they merge in, keeping each kind's chosen format.
    /// While converting, files that can become the running format join the queue;
    /// the rest open once it ends. Shift while dropping starts over (`load`).
    fn add(&mut self, paths: &[PathBuf]) {
        let mut known: std::collections::HashSet<PathBuf> = self.groups.iter().flat_map(|g| g.files.iter().cloned()).chain(self.pending.iter().cloned()).collect();
        let fresh: Vec<PathBuf> = expand(paths).into_iter().filter(|f| known.insert(f.clone())).collect();
        match &self.screen {
            Screen::Pick => {
                if fresh.is_empty() {
                    self.say(icon::FILES, "Already in the list.");
                    return;
                }
                let chosen: Vec<(Kind, &'static str)> = self.groups.iter().map(|g| (g.kind, g.target())).collect();
                let active = self.groups.get(self.active).map(|g| g.kind);
                let all: Vec<PathBuf> = self.groups.iter().flat_map(|g| g.files.iter().cloned()).chain(fresh.iter().cloned()).collect();
                self.load(&all);
                for g in &mut self.groups {
                    if let Some(i) = chosen.iter().find(|(k, _)| *k == g.kind).and_then(|(_, t)| g.targets.iter().position(|x| x == t)) {
                        g.sel = i;
                    }
                }
                self.active = active.and_then(|k| self.groups.iter().position(|g| g.kind == k)).unwrap_or(0);
                if self.notice.is_none() {
                    self.say(icon::PLUS, added_line(fresh.len()));
                }
            }
            Screen::Working(job) if !job.is_cancelled() => {
                let job = job.clone();
                let (groups, rejected) = group_files(&fresh, &self.prefs);
                let (mut tasks, mut joined, mut later) = (Vec::new(), 0, 0);
                for ng in groups {
                    let Some(g) = self.groups.iter_mut().find(|g| g.kind == ng.kind) else {
                        later += ng.files.len();
                        self.pending.extend(ng.files);
                        continue;
                    };
                    let target = g.target();
                    // Files already in the target format join too, and are left alone (as in `convert`).
                    let (fits, rest): (Vec<PathBuf>, Vec<PathBuf>) = ng.files.into_iter().partition(|f| {
                        let e = formats::ext_of(f).unwrap_or_default();
                        e == target || formats::targets_for(&e).contains(&target)
                    });
                    later += rest.len();
                    self.pending.extend(rest);
                    let todo: Vec<PathBuf> = fits.iter().filter(|f| formats::ext_of(f).as_deref() != Some(target)).cloned().collect();
                    if target == "pdf" {
                        if !todo.is_empty() {
                            tasks.push(Task { srcs: todo, target: target.into(), opts: self.job_opts });
                        }
                    } else {
                        tasks.extend(todo.into_iter().map(|f| Task { srcs: vec![f], target: target.into(), opts: self.job_opts }));
                    }
                    g.bytes += fits.iter().map(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0)).sum::<u64>();
                    joined += fits.len();
                    g.files.extend(fits);
                }
                job.push(tasks);
                let mut line = match (joined, later) {
                    (0, 0) if fresh.is_empty() => "Already in the list.".to_string(),
                    (0, 0) => String::new(),
                    (n, 0) => added_line(n),
                    (0, 1) => "That file opens when this finishes.".into(),
                    (0, m) => format!("Those {m} files open when this finishes."),
                    (n, m) => format!("{}. {m} more open when this finishes.", added_line(n).trim_end_matches('.')),
                };
                if !rejected.is_empty() {
                    let n = rejected.len();
                    let skipped = if n == 1 { "Skipped 1 file Retype can't convert.".to_string() } else { format!("Skipped {n} files Retype can't convert.") };
                    line = if line.is_empty() { skipped } else { format!("{line} {skipped}") };
                }
                let glyph = if joined + later == 0 { icon::ALERT } else { icon::PLUS };
                self.say(glyph, line);
            }
            _ => self.load(paths),
        }
    }

    /// Never a blocking dialog: a modal panel runs a nested event loop inside this
    /// frame, and an event arriving during it (e.g. a drag over the window) makes
    /// winit re-enter itself and abort. The chosen files arrive through the inbox.
    fn choose_files(&mut self) {
        #[cfg(target_os = "macos")]
        crate::macos::choose_files();
        #[cfg(not(target_os = "macos"))]
        {
            let pick = rfd::AsyncFileDialog::new().set_title("Choose files to convert").pick_files();
            std::thread::spawn(move || {
                if let Some(handles) = block_on(pick) {
                    let files: Vec<PathBuf> = handles.iter().map(|h| h.path().to_path_buf()).collect();
                    if !files.is_empty() {
                        crate::ipc::deliver(crate::ipc::Msg { files, target: None });
                    }
                }
            });
        }
    }

    /// ⌘V: a copied file, or a copied image saved to Downloads first.
    fn paste(&mut self) {
        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        if let Ok(files) = cb.get().file_list() {
            if !files.is_empty() {
                self.load(&files);
                return;
            }
        }
        if let Ok(img) = cb.get_image() {
            let Some(rgba) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) else { return };
            let dir = dirs::download_dir().or_else(dirs::desktop_dir).unwrap_or_else(std::env::temp_dir);
            let name = chrono::Local::now().format("Pasted image %Y-%m-%d at %H.%M.%S.png").to_string();
            let path = dir.join(name);
            match rgba.save(&path) {
                Ok(()) => {
                    self.load(std::slice::from_ref(&path));
                    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or("Downloads").to_string();
                    self.say(icon::DOWNLOAD, format!("Pasted into {folder}"));
                }
                Err(_) => self.say(icon::ALERT, "Couldn't save the pasted image."),
            }
            return;
        }
        self.say(icon::PASTE, "Copy a file or an image first.");
    }

    fn convert(&mut self, ctx: &egui::Context, opts: Opts) {
        let mut tasks = Vec::new();
        for g in &self.groups {
            let target = g.target();
            self.prefs.last_target.insert(g.source_ext(), target.to_string());
            let files: Vec<PathBuf> = g.files.iter().filter(|f| formats::ext_of(f).as_deref() != Some(target)).cloned().collect();
            if target == "pdf" {
                // Images combine into one PDF, in the order they were dropped.
                if !files.is_empty() {
                    tasks.push(Task { srcs: files, target: target.into(), opts });
                }
            } else {
                tasks.extend(files.into_iter().map(|f| Task { srcs: vec![f], target: target.into(), opts }));
            }
        }
        self.prefs.save();
        self.job_opts = opts;
        self.notice = None;
        let c = ctx.clone();
        self.screen = Screen::Working(Job::start(tasks, move || c.request_repaint()));
    }

    fn finish(&mut self, ctx: &egui::Context) {
        let Screen::Working(job) = &self.screen else { return };
        let s = job.snapshot();
        if !self.pending.is_empty() {
            let next = std::mem::take(&mut self.pending);
            self.load(&next);
            return;
        }
        if s.cancelled && s.trashed.is_empty() {
            for o in &s.outputs {
                let _ = std::fs::remove_file(o);
            }
            self.screen = Screen::Pick;
            self.say(icon::X, "Cancelled");
            return;
        }
        if s.outputs.is_empty() {
            self.screen = Screen::Pick;
            let line = failure_line(&s.failures, 0);
            self.say(icon::ALERT, line);
            return;
        }
        let (thumb, drag_png) = thumb_and_drag_image(ctx, s.thumb.as_ref());
        let sources = self.groups.iter().map(|g| g.files.len()).sum();
        if !s.trashed.is_empty() {
            for g in &mut self.groups {
                g.files.retain(|f| !s.trashed.contains(f));
            }
            self.groups.retain(|g| !g.files.is_empty());
        }
        self.replace_on = false;
        // Entrance: the tile pops and the highlight sweeps across the new name.
        theme::spring_set(ctx, Id::unique(DONE_IN), 0.0, 0.0);
        self.screen = Screen::Done(Box::new(Finished { state: s, thumb, drag_png, sources, copied_at: None, at: Instant::now() }));
    }

    /// ⌘Z on Done: the new files go to the Trash.
    fn undo(&mut self) {
        let Screen::Done(f) = &self.screen else { return };
        let replaced = !f.state.trashed.is_empty();
        let failed = f.state.outputs.iter().filter(|o| platform::trash(o).is_err()).count();
        if failed > 0 {
            self.say(icon::ALERT, "Couldn't move the new file to the Trash.");
        } else if replaced {
            self.say(icon::TRASH, "Undone. Originals are in the Trash.");
        } else {
            self.say(icon::UNDO, "Undone");
        }
        self.screen = if self.groups.is_empty() { Screen::Empty } else { Screen::Pick };
    }

    /// Back from Done to the picker, same files, to make another format.
    fn again(&mut self) -> bool {
        if self.groups.is_empty() {
            return false;
        }
        self.notice = None;
        self.screen = Screen::Pick;
        true
    }

    /// Estimated total size with Smaller on: Some(Some(n)) known, Some(None) can't
    /// estimate, None still working. Recomputed when the files or target change.
    fn smaller_estimate(&mut self, ctx: &egui::Context, opts: Opts) -> Option<Option<u64>> {
        let key = self.groups.iter().map(|g| format!("{}>{}", g.files[0].display(), g.target())).collect::<Vec<_>>().join("|") + &format!("{:?}", opts.settings);
        if self.estimate.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
            let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
            let jobs: Vec<(PathBuf, String, u64, u64)> = self.groups.iter().filter(|g| g.shrinkable()).map(|g| (g.files[0].clone(), g.target().to_string(), std::fs::metadata(&g.files[0]).map(|m| m.len()).unwrap_or(1).max(1), g.bytes)).collect();
            let out = slot.clone();
            let c = ctx.clone();
            std::thread::spawn(move || {
                // One sample per kind, scaled by that kind's total size.
                let mut sum = 0u64;
                let mut ok = true;
                for (f, t, first, total) in jobs {
                    match engine::estimate(&f, &t, opts) {
                        Some(n) => sum += (n as f64 * total as f64 / first as f64) as u64,
                        None => ok = false,
                    }
                }
                *out.lock().unwrap() = Some(ok.then_some(sum));
                c.request_repaint();
            });
            self.estimate = Some((key, slot));
        }
        *self.estimate.as_ref().unwrap().1.lock().unwrap()
    }

    /// Kick the reel: the extension springs in from the previous slot.
    fn bump_reel(&self, ctx: &egui::Context, dir: f32) {
        theme::spring_set(ctx, Id::unique(REEL), dir, 0.0);
    }

    /// Type to pick: "j" jumps to .jpg, "we" to .webm, "m" "m" cycles the m's.
    fn type_char(&mut self, ctx: &egui::Context, c: char) {
        if !c.is_ascii_alphanumeric() {
            return;
        }
        let c = c.to_ascii_lowercase();
        if self.typed_at.elapsed() > Duration::from_millis(900) {
            self.typed.clear();
        }
        self.typed_at = Instant::now();
        self.typed.push(c);
        let g = &mut self.groups[self.active];
        let before = g.sel;
        let n = g.targets.len();
        let repeat = self.typed.len() > 1 && self.typed.chars().all(|x| x == c);
        let found = if repeat {
            (1..=n).map(|k| (before + k) % n).find(|&i| g.targets[i].starts_with(c))
        } else {
            g.targets.iter().position(|t| t.starts_with(self.typed.as_str())).or_else(|| {
                // No match for the whole run; start over from this letter.
                self.typed = c.to_string();
                g.targets.iter().position(|t| t.starts_with(c))
            })
        };
        if let Some(i) = found {
            if i != before {
                g.sel = i;
                self.bump_reel(ctx, if i > before { 1.0 } else { -1.0 });
            }
            self.notice = None;
        }
    }
}

fn added_line(n: usize) -> String {
    if n == 1 { "Added 1 file.".into() } else { format!("Added {n} files.") }
}

fn failure_line(failures: &[(PathBuf, String)], saved: usize) -> String {
    let name = |p: &PathBuf| p.file_name().and_then(|n| n.to_str()).unwrap_or("A file").to_string();
    match (saved, failures) {
        (_, []) => String::new(),
        (0, [(p, e)]) => format!("{} {e}.", name(p)),
        (0, f) => format!("None of the {} files converted. {} {}.", f.len(), name(&f[0].0), f[0].1),
        (s, [(p, e)]) => format!("Saved {s}. {} {e}.", name(p)),
        (s, f) => format!("Saved {s}. {} files couldn't be converted.", f.len()),
    }
}

pub fn thumb_and_drag_image(ctx: &egui::Context, img: Option<&image::RgbaImage>) -> (Option<egui::TextureHandle>, Vec<u8>) {
    let tex = img.map(|i| {
        let ci = egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw());
        ctx.load_texture("done-thumb", ci, egui::TextureOptions::LINEAR)
    });
    let drag = match img {
        Some(i) => image::DynamicImage::ImageRgba8(i.clone()).thumbnail(96, 96).to_rgba8(),
        None => file_glyph(),
    };
    let mut png = Vec::new();
    let _ = image::DynamicImage::ImageRgba8(drag).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png);
    (tex, png)
}

/// Drag image for files with no picture: a paper sheet with the selection block.
fn file_glyph() -> image::RgbaImage {
    let (w, h) = (64u32, 80u32);
    image::RgbaImage::from_fn(w, h, |x, y| {
        let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
        let block = (16..50).contains(&x) && (44..60).contains(&y);
        let dot = (x as i32 - 10).pow(2) + (y as i32 - 55).pow(2) <= 9;
        if edge {
            image::Rgba([0xc9, 0xcc, 0xd2, 255])
        } else if block {
            image::Rgba([0xff, 0xe1, 0x4a, 255])
        } else if dot {
            image::Rgba([0x11, 0x14, 0x18, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    })
}

impl eframe::App for RetypeApp {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        platform::clear_color()
    }

    /// Runs every frame, and also while the window is hidden (when `ui` doesn't):
    /// the menu-bar icon, the shortcut, and a parked job finishing.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Menu-bar icon and global shortcut follow the settings.
        self.resident.sync(self.prefs.settings);
        for action in crate::resident::take() {
            match action {
                crate::resident::Action::Show => self.unpark(ctx),
                crate::resident::Action::Settings => {
                    self.unpark(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    // A running conversion keeps its screen; settings open once it's done.
                    if !matches!(self.screen, Screen::Working(_)) {
                        self.notice = None;
                        self.screen = Screen::Settings;
                    }
                }
                crate::resident::Action::ConvertSelection => {
                    let sel = platform::file_manager_selection();
                    self.unpark(ctx);
                    if sel.is_empty() {
                        self.say(icon::ALERT, format!("Select files in {} first", platform::file_manager_name()));
                    } else if matches!(self.screen, Screen::Working(_)) {
                        self.pending.extend(sel);
                    } else {
                        self.load(&sel);
                    }
                }
                crate::resident::Action::CheckUpdates => {
                    self.unpark(ctx);
                    self.manual_update_check = true;
                    self.last_update_check = Some(Instant::now());
                    let c = ctx.clone();
                    crate::update::check_in_background(self.update.clone(), move || c.request_repaint());
                }
                crate::resident::Action::Quit => {
                    self.quitting = true;
                    self.save_window_pos(ctx, true);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        if !self.is_hidden() {
            return;
        }
        if crate::ipc::peek() {
            // Files from another launch: show the window so `ui` runs and takes them.
            if cfg!(windows) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.request_repaint();
            }
            return;
        }
        // Closed: keep the event loop alive. Parked jobs still finish and notify.
        if self.parked {
            if let Screen::Working(job) = &self.screen {
                if job.snapshot().done {
                    let s = job.snapshot();
                    self.finish(ctx);
                    platform::notify("Retype", &done_message(&s));
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        #[cfg(target_os = "macos")]
        crate::macos::refresh_shadow();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::sync(&ctx);
        let full = ui.max_rect();

        // Notices clear themselves; problems stay a little longer.
        if let Some((g, _, at)) = &self.notice {
            let ttl = Duration::from_millis(if *g == icon::ALERT { 6000 } else { 3000 });
            match ttl.checked_sub(at.elapsed()) {
                Some(left) => ctx.request_repaint_after(left),
                None => self.notice = None,
            }
        }

        if self.is_hidden() && !crate::ipc::peek() {
            // Closed (macOS: the window is still there, transparent and click-through,
            // so `ui` keeps running). Draw nothing; `logic` does the rest.
            return;
        }

        // The card springs toward its goal size inside the window; the OS window
        // itself resizes at most once per change (see the end of this function),
        // because live window resizes are what stutter.
        let cw = theme::spring_with(&ctx, Id::unique("card-w"), self.goal.x, 700.0, 42.0);
        let ch = theme::spring_with(&ctx, Id::unique("card-h"), self.goal.y, 700.0, 42.0);
        let card = Rect::from_min_size(full.min, Vec2::new(cw.min(full.width()).round(), ch.min(full.height()).round()));
        let fill_slot = ui.painter().add(egui::Shape::Noop);
        ui.set_clip_rect(card);
        let full = card;

        // Drag the card by its background. Registered first, so every control sits above it.
        // The OS needs the drag to start on the mouse-down itself.
        // Not focusable, so Tab goes straight to the controls.
        let bg = ui.interact(full, ui.scope_id().with("bg"), Sense::drag().difference(Sense::FOCUSABLE));
        if bg.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        // Files from another launch, the Dock, or Open With. A launch without files
        // (e.g. from the Start menu while closed to the tray) just brings the card back.
        let msgs = crate::ipc::take();
        if !msgs.is_empty() {
            let inbox: Vec<PathBuf> = msgs.into_iter().flat_map(|m| m.files).collect();
            if self.background.swap(false, std::sync::atomic::Ordering::Relaxed) {
                // Hidden while finishing: come back, and pick these up once the job is done.
                show_window(&ctx, true);
                self.sized = false;
            }
            self.unpark(&ctx);
            if matches!(self.screen, Screen::Working(_)) {
                self.pending.extend(inbox);
            } else if !inbox.is_empty() {
                self.load(&inbox);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Incoming drops, paste, open, close.
        let (hovered, dropped, paste_event, focused) = ctx.input(|i| {
            (
                i.raw.hovered_files.iter().filter_map(|f| f.path.clone()).collect::<Vec<_>>(),
                i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect::<Vec<_>>(),
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
                i.focused,
            )
        });
        // Files arriving: a trackpad tick and a quick squeeze of the card on entry,
        // a firmer tick and a bounce on drop.
        let kick_id = Id::unique("drop-kick");
        if !hovered.is_empty() && !self.hovering {
            platform::haptic(false);
            theme::spring_set(&ctx, kick_id, 1.0, 0.0);
        }
        self.hovering = !hovered.is_empty();
        // A card that already has files adds to them; Shift starts over. Say which
        // while the files hover.
        let stacks = matches!(self.screen, Screen::Pick | Screen::Working(_));
        let shift = (self.hovering || !dropped.is_empty()) && stacks && platform::shift_down();
        if self.hovering && stacks {
            let (g, s) = if shift { (icon::SWAP, "Drop to start over with these.") } else { (icon::PLUS, "Drop to add to the list. Shift starts over.") };
            self.notice = Some((g, s.into(), Instant::now()));
            self.drop_hint = true;
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if self.drop_hint {
            self.notice = None;
            self.drop_hint = false;
        }
        if !dropped.is_empty() {
            platform::haptic(true);
            theme::spring_set(&ctx, kick_id, -0.6, 0.0);
            self.notice = None;
            self.drop_hint = false;
            if stacks && !shift { self.add(&dropped) } else { self.load(&dropped) }
        }
        let paste = platform::take_paste_key(focused) || paste_event;
        if paste && !matches!(self.screen, Screen::Working(_)) {
            self.paste();
        }
        let (open_key, close_key, settings_key) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::COMMAND, Key::O), i.consume_key(egui::Modifiers::COMMAND, Key::W), i.consume_key(egui::Modifiers::COMMAND, Key::Comma)));
        if settings_key && !matches!(self.screen, Screen::Working(_)) {
            self.notice = None;
            self.screen = Screen::Settings;
        }
        if open_key {
            self.choose_files();
        }
        // Read before the screens, so Pick's arrow keys don't swallow Alt+←.
        self.back_nav = ctx.input_mut(|i| {
            let mouse = i.pointer.button_pressed(egui::PointerButton::Extra1);
            let key = if cfg!(target_os = "macos") {
                i.consume_key(egui::Modifiers::COMMAND, Key::OpenBracket)
            } else {
                i.consume_key(egui::Modifiers::ALT, Key::ArrowLeft)
            };
            mouse || key
        });
        if close_key {
            self.close(&ctx);
        }
        // A close from the system (e.g. the Dock's Quit is separate and not cancellable).
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            if self.resident.in_menu_bar() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.park(&ctx);
            } else if self.hide_for_background(&ctx) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        self.save_window_pos(&ctx, false);
        if let Some(rx) = &self.probes {
            while let Ok((i, p)) = rx.try_recv() {
                if let Some(g) = self.groups.get_mut(i) {
                    g.probe = Some(p);
                }
            }
        }
        if let Screen::Working(job) = &self.screen {
            if job.snapshot().done {
                self.finish(&ctx);
            }
        }

        self.poll_updates(&ctx);

        // Title row: pin and close, top right.
        let row_y = full.min.y + PAD;
        let close = icon_button(ui, Pos2::new(full.max.x - PAD - 2.0, row_y), icon::X, &format!("Close  {}", keys("⌘W", "Ctrl+W")), false, "close", 28.0);
        if close.clicked() {
            self.close(&ctx);
        }
        // A newer Retype: one click installs it.
        let available = match &*self.update.lock().unwrap() {
            crate::update::State::Available(r) => Some(r.clone()),
            _ => None,
        };
        if let Some(r) = available {
            let tip = if r.asset.is_some() { format!("Update to Retype {}", r.version) } else { format!("Retype {} is out. Open the release page.", r.version) };
            if icon_toggle(ui, Pos2::new(full.max.x - PAD - 66.0, row_y), icon::UPDATE, &tip, true, "update", 28.0).clicked() {
                if r.asset.is_some() {
                    let c = ctx.clone();
                    crate::update::install_in_background(r, self.update.clone(), move || c.request_repaint());
                } else {
                    platform::open_url(&r.page);
                }
            }
        }
        let pinned = self.prefs.settings.keep_on_top;
        let pin_tip = if pinned { "Floating on top. Click to unpin." } else { "Keep on top" };
        if icon_toggle(ui, Pos2::new(full.max.x - PAD - 34.0, row_y), if pinned { icon::PIN } else { icon::PIN_OFF }, pin_tip, pinned, "pin", 28.0).clicked() {
            self.set_on_top(&ctx, !pinned);
        }

        // Lay out at the width the card is heading to, so nothing reflows mid-spring.
        let target_w = self.desired_width(ui.painter());
        let c = Rect::from_min_max(Pos2::new(full.min.x + PAD, full.min.y + PAD - 9.0), Pos2::new(full.min.x + target_w - PAD, f32::INFINITY));
        // Every screen lays out top-down and returns where its content ends.
        let bottom = match &self.screen {
            Screen::Empty => self.draw_empty(ui, c),
            Screen::Settings => self.draw_settings(ui, c),
            Screen::Pick => self.draw_pick(ui, c),
            Screen::Working(_) => self.draw_working(ui, c),
            Screen::Done(_) => self.draw_done(ui, c, frame),
        };

        // The card surface, under everything drawn above. It squeezes in and springs
        // back (with a wobble) when files arrive; content stays still.
        let dragging = !hovered.is_empty();
        let kick = theme::spring_with(&ctx, kick_id, 0.0, 900.0, 14.0);
        let surface = full.shrink(6.0 * kick);
        let base_tint = if theme::is_dark() { 0.045 } else { 0.08 };
        let tint = if dragging { base_tint } else { 0.0 } + 1.25 * base_tint * kick.max(0.0);
        let fill = theme::paper().lerp_to_gamma(theme::YELLOW, tint.clamp(0.0, 0.25));
        let radius = CornerRadius::same(platform::card_radius(RADIUS));
        ui.painter().set(fill_slot, egui::Shape::rect_filled(surface, radius, fill));
        if !platform::os_rounds_corners() {
            ui.painter().rect_stroke(surface, radius, Stroke::new(1.0, theme::line()), StrokeKind::Inside);
        }

        // Drop zone: a yellow dashed outline that keeps moving while the card is
        // waiting for a file, and while files are dragged over it.
        // On Done the outline comes back, dimmer and slower, once the entrance has
        // settled: the card is still a drop zone for the next file.
        let done_fade = match &self.screen {
            Screen::Done(f) => {
                let t = f.at.elapsed().as_secs_f32();
                let k = ((t - 0.8) / 0.5).clamp(0.0, 1.0);
                let k = if self.reduce_motion { (t >= 0.8) as u8 as f32 } else { k * k * (3.0 - 2.0 * k) };
                if k < 1.0 {
                    ctx.request_repaint();
                }
                Some(k)
            }
            _ => None,
        };
        let idle = matches!(self.screen, Screen::Empty);
        if idle || dragging || done_fade.is_some_and(|k| k > 0.0) {
            let speed = if dragging { 60.0 } else if idle { 22.0 } else { 12.0 };
            let offset = if self.reduce_motion { 0.0 } else { self.started.elapsed().as_secs_f32() * speed };
            let alpha = if dragging || idle { 1.0 } else { 0.55 * done_fade.unwrap_or(1.0) };
            let stroke = Stroke::new(if dragging { 3.0 } else { 2.0 }, theme::YELLOW.gamma_multiply(alpha));
            theme::marching_round_rect(ui.painter(), surface.shrink(9.0), (RADIUS - 7) as f32, stroke, offset);
            if !self.reduce_motion {
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        }

        // Size: measure, then move the card and the window.
        let goal = Vec2::new(target_w, (bottom - full.min.y + PAD).round().max(120.0));
        if !self.sized {
            theme::spring_set(&ctx, Id::unique("card-w"), goal.x, 0.0);
            theme::spring_set(&ctx, Id::unique("card-h"), goal.y, 0.0);
            self.sized = true;
        }
        self.goal = goal;
        // Grow the window at once (the card then springs out into it); shrink it
        // only after the card has finished springing in. One OS resize either way.
        let settle = |cur: f32, goal: f32, win: f32| if goal > win { goal } else if (cur - goal).abs() < 0.5 { goal } else { win };
        let win = Vec2::new(settle(cw, goal.x, self.last_width), settle(ch, goal.y, self.last_height));
        let animating = (cw - goal.x).abs() >= 0.5 || (ch - goal.y).abs() >= 0.5;
        if (win.x - self.last_width).abs() >= 1.0 || (win.y - self.last_height).abs() >= 1.0 {
            self.last_width = win.x;
            self.last_height = win.y;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(win));
        }
        if animating || self.shadow_dirty {
            // The shadow follows the card's pixels; recompute it while the card moves.
            #[cfg(target_os = "macos")]
            crate::macos::refresh_shadow();
            self.shadow_dirty = animating;
        }
    }
}

/// Widest extension a group can show, so the stem's truncation and the card's
/// width do not change while scrolling through targets.
fn widest_ext(p: &egui::Painter, targets: &[&'static str], size: f32) -> f32 {
    targets.iter().map(|t| theme::width(p, t, size, W::Bold, -0.035)).fold(0.0, f32::max)
}

fn chip_widths(p: &egui::Painter, targets: &[&'static str]) -> Vec<f32> {
    targets.iter().map(|t| theme::width(p, &format!(".{t}"), theme::LABEL, W::Semibold, 0.0) + 20.0).collect()
}

/// Wrap chips into as few rows as fit `inner`, with rows of similar length
/// instead of one full row and a lonely last chip.
fn chip_rows(widths: &[f32], inner: f32) -> Vec<usize> {
    // Chips hang 10px past the text column on each side; 2px of slack absorbs rounding.
    let avail = inner + 22.0;
    let total: f32 = widths.iter().sum::<f32>() + 4.0 * widths.len().saturating_sub(1) as f32;
    if total <= avail {
        return Vec::new();
    }
    let rows = (total / avail).ceil().max(1.0);
    let goal = (total / rows).max(widths.iter().cloned().fold(0.0, f32::max));
    let mut breaks = Vec::new(); // index where each new row starts
    let mut x = 0.0;
    for (i, w) in widths.iter().enumerate() {
        let next = if x == 0.0 { *w } else { x + 4.0 + w };
        if x > 0.0 && (next > avail || x >= goal - 1.0) {
            breaks.push(i);
            x = *w;
        } else {
            x = next;
        }
    }
    breaks
}

const MIN_INNER: f32 = 376.0;
const MAX_INNER: f32 = 596.0;

impl RetypeApp {
    /// The card width this screen wants: wide enough for the whole name with its
    /// widest extension, and for the chips in balanced rows, within limits.
    fn desired_width(&self, p: &egui::Painter) -> f32 {
        let inner = match &self.screen {
            Screen::Empty | Screen::Settings => MIN_INNER,
            Screen::Pick | Screen::Working(_) => {
                let multi = self.groups.len() > 1;
                let size = if multi { 28.0 } else { theme::DISPLAY };
                let name = self
                    .groups
                    .iter()
                    .map(|g| {
                        theme::width(p, &g.stem(), size, W::Bold, -0.035) + theme::width(p, ".", size, W::Bold, -0.035) + widest_ext(p, &g.targets, size) + size * 0.24 + 16.0 + if multi { 44.0 } else { 0.0 }
                    })
                    .fold(0.0, f32::max);
                let chips = self
                    .groups
                    .iter()
                    .map(|g| {
                        let w = chip_widths(p, &g.targets);
                        w.iter().sum::<f32>() + 4.0 * w.len().saturating_sub(1) as f32 - 20.0
                    })
                    .fold(0.0, f32::max);
                // A single row when it fits; otherwise the widest the card goes, split evenly.
                let chips = if chips <= MAX_INNER { chips.ceil() + 1.0 } else { MAX_INNER };
                name.max(chips)
            }
            Screen::Done(f) => {
                let name = if f.state.outputs.len() > 1 {
                    format!("{} files", f.state.outputs.len())
                } else {
                    f.state.outputs[0].file_name().and_then(|n| n.to_str()).unwrap_or("").to_string()
                };
                110.0 + theme::width(p, &name, 24.0, W::Bold, -0.02) + 44.0
            }
        };
        (inner.clamp(MIN_INNER, MAX_INNER) + 2.0 * PAD).ceil()
    }

    fn draw_notice(&self, p: &egui::Painter, pos: Pos2, max_w: f32) -> f32 {
        match &self.notice {
            Some((g, n, _)) => {
                let n = theme::fit(p, n, theme::BODY, W::Regular, 0.0, max_w - 24.0);
                theme::icon_text(p, pos, *g, &n, theme::BODY, W::Regular, theme::ink()).max.y
            }
            None => pos.y,
        }
    }

    fn caret(&self, ui: &egui::Ui, top: Pos2, h: f32) {
        let on = self.reduce_motion || (self.started.elapsed().as_millis() % 1100) < 550;
        if on {
            ui.painter().rect_filled(Rect::from_min_size(top, Vec2::new((theme::DISPLAY * 0.05).max(2.0), h)), 0.0, theme::ink());
        }
        if !self.reduce_motion {
            let ms = 550 - (self.started.elapsed().as_millis() % 550) as u64;
            ui.ctx().request_repaint_after(Duration::from_millis(ms + 5));
        }
    }

    /// Daily background check, plus surfacing install progress and results.
    fn poll_updates(&mut self, ctx: &egui::Context) {
        use crate::update::State;
        let due = self.last_update_check.is_none_or(|t| t.elapsed() > Duration::from_secs(24 * 3600));
        if self.prefs.settings.check_updates && due && matches!(*self.update.lock().unwrap(), State::Idle | State::UpToDate | State::Failed(_)) {
            self.last_update_check = Some(Instant::now());
            let c = ctx.clone();
            crate::update::check_in_background(self.update.clone(), move || c.request_repaint());
        }
        let state = self.update.lock().unwrap().clone();
        match state {
            State::Installing(msg) => self.notice = Some((icon::UPDATE, msg, Instant::now())),
            State::Failed(e) if self.manual_update_check || self.notice.as_ref().is_some_and(|n| n.0 == icon::UPDATE) => {
                self.say(icon::ALERT, format!("Update: {e}"));
                self.manual_update_check = false;
                *self.update.lock().unwrap() = State::Idle;
            }
            State::UpToDate if self.manual_update_check => {
                self.say(icon::CHECK, format!("Retype {} is up to date", crate::update::VERSION));
                self.manual_update_check = false;
            }
            State::Available(r) if self.manual_update_check => {
                self.say(icon::UPDATE, format!("Retype {} is available. Click the download icon.", r.version));
                self.manual_update_check = false;
            }
            _ => {}
        }
        // Keep the hourly-ish timer going while the app sits idle.
        ctx.request_repaint_after(Duration::from_secs(3600));
    }

    /// Remember where the user left the window (debounced; always on close).
    fn save_window_pos(&mut self, ctx: &egui::Context, now: bool) {
        let Some(r) = ctx.input(|i| i.viewport().outer_rect) else { return };
        let pos = [r.min.x.round(), r.min.y.round()];
        if self.prefs.window_pos == Some(pos) {
            return;
        }
        let dragging = ctx.input(|i| i.pointer.any_down());
        if now || (!dragging && self.pos_saved_at.elapsed() > Duration::from_millis(800)) {
            self.prefs.window_pos = Some(pos);
            self.prefs.save();
            self.pos_saved_at = Instant::now();
        }
    }

    /// ⌘W / ×. Mid-conversion (with the setting on) the window hides and the job
    /// finishes; otherwise the app closes.
    fn close(&mut self, ctx: &egui::Context) {
        self.save_window_pos(ctx, true);
        if self.resident.in_menu_bar() {
            self.park(ctx);
            return;
        }
        if !self.hide_for_background(ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Close into the menu bar: invisible and click-through, no Dock icon. A job in
    /// progress keeps going and notifies when it ends.
    fn park(&mut self, ctx: &egui::Context) {
        if self.parked {
            return;
        }
        self.parked = true;
        show_window(ctx, false);
        platform::set_dock_icon(false);
        if !matches!(self.screen, Screen::Working(_)) {
            // A fresh card next time, unless there is a result worth coming back to.
            if !matches!(self.screen, Screen::Done(_)) {
                self.groups.clear();
                self.screen = Screen::Empty;
            }
        }
        self.notice = None;
    }

    fn unpark(&mut self, ctx: &egui::Context) {
        if !self.parked {
            return;
        }
        self.parked = false;
        self.sized = false;
        show_window(ctx, true);
        platform::set_dock_icon(true);
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_for_background(&mut self, ctx: &egui::Context) -> bool {
        let Screen::Working(job) = &self.screen else { return false };
        if !self.prefs.settings.finish_after_close {
            return false;
        }
        if !self.background.swap(true, std::sync::atomic::Ordering::Relaxed) {
            show_window(ctx, false);
            finish_in_background(job.clone(), self.background.clone());
        }
        true
    }

    /// Closed into the menu bar, or finishing a job after the window closed.
    fn is_hidden(&self) -> bool {
        self.parked || self.background.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn set_on_top(&mut self, ctx: &egui::Context, on: bool) {
        self.prefs.settings.keep_on_top = on;
        self.prefs.save();
        let level = if on { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    }

    // ---------------------------------------------------------------- Settings

    fn draw_settings(&mut self, ui: &mut egui::Ui, c: Rect) -> f32 {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        if back_button(ui, c) || esc || self.back_nav {
            self.screen = if self.groups.is_empty() { Screen::Empty } else { Screen::Pick };
            return c.min.y + 100.0;
        }
        theme::text(&p, c.min + Vec2::new(32.0, 1.0), "Settings", 15.0, W::Semibold, theme::ink(), 0.0);

        let mut st = self.prefs.settings;
        let fm = platform::file_manager_name();
        let tray = platform::tray_name();
        let tray_tip = format!("Closing the window keeps Retype in the {tray}, ready for the shortcut");
        let finish_tip = format!("Without the {tray}: closing mid-conversion lets it finish, then notifies you");
        let mut menu_on = self.integration_on;
        let mut rows: Vec<(String, &mut bool, bool, &str)> = vec![
            ("Keep photo dates and camera info".into(), &mut st.keep_metadata, true, "Capture date, camera, lens and orientation stay with converted photos"),
            ("Keep location".into(), &mut st.keep_location, true, "GPS position in photos and videos. Off, so converted files are safer to share."),
            ("Save to Downloads".into(), &mut st.save_to_downloads, true, "Instead of next to the original"),
        ];
        if cfg!(target_os = "macos") {
            rows.push(("Hardware video encoding".into(), &mut st.hardware_video, true, "Faster video conversions using the Mac's media engine"));
        }
        rows.push((format!("Stay in the {tray} when closed"), &mut st.menu_bar, true, &tray_tip));
        let shortcut_label = format!("{} converts the {fm} selection", crate::resident::shortcut_label());
        rows.push((shortcut_label, &mut st.global_shortcut, true, "Works from any app"));
        rows.push(("Finish converting after closing".into(), &mut st.finish_after_close, true, &finish_tip));
        rows.push(("Keep window on top".into(), &mut st.keep_on_top, true, "Float above other windows"));
        rows.push(("Check for updates".into(), &mut st.check_updates, true, "Asks GitHub once a day whether a newer Retype exists. Nothing else leaves this computer."));
        rows.push((format!("{fm} right-click menu"), &mut menu_on, true, "Convert files without opening this window"));

        let mut y = c.min.y + 38.0;
        let row_h = 38.0;
        let disabled_location = !self.prefs.settings.keep_metadata;
        // With the menu bar / tray on, closing never quits, so "finish after closing" doesn't apply.
        let disabled_finish = self.prefs.settings.menu_bar;
        for (i, (label, value, _, tip)) in rows.iter_mut().enumerate() {
            let enabled = !(i == 1 && disabled_location) && !(disabled_finish && label.starts_with("Finish converting"));
            let row = Rect::from_min_size(Pos2::new(c.min.x - 10.0, y), Vec2::new(c.width() + 20.0, row_h));
            let resp = ui.interact(row, ui.scope_id().with(("setting", i)), if enabled { Sense::click() } else { Sense::hover() });
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, enabled, **value, label.clone()));
            if resp.hovered() && enabled {
                p.rect_filled(row, CornerRadius::same(8), theme::wash());
                ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
            focus_ring(ui, &resp, row);
            let color = if enabled { theme::ink() } else { theme::soft() };
            theme::text(&p, Pos2::new(c.min.x, row.center().y - 9.0), label, theme::BODY, W::Regular, color, 0.0);
            let on = **value && enabled;
            switch(ui, Pos2::new(c.max.x - 18.0, row.center().y), on, resp.id, enabled);
            let resp = resp.on_hover_text(*tip);
            if resp.clicked() && enabled {
                **value = !**value;
            }
            y += row_h + 2.0;
        }
        drop(rows);

        // Apply what changed.
        if menu_on != self.integration_on {
            let res = if menu_on { crate::integration::install() } else { crate::integration::uninstall() };
            match res {
                Ok(()) if menu_on => self.say(icon::CHECK, format!("Right-click a file › {}", crate::integration::menu_path())),
                Ok(()) => self.say(icon::CHECK, "Removed from the right-click menu"),
                Err(e) => self.say(icon::ALERT, e),
            }
            self.integration_on = crate::integration::installed();
        }
        if st.keep_on_top != self.prefs.settings.keep_on_top {
            self.set_on_top(&ctx, st.keep_on_top);
        }
        if st != self.prefs.settings {
            self.prefs.settings = st;
            self.prefs.save();
        }
        let mut bottom = y - 2.0;
        if self.notice.is_some() {
            bottom = self.draw_notice(&p, Pos2::new(c.min.x, bottom + 12.0), c.width());
        }
        bottom
    }

    // ---------------------------------------------------------------- Empty

    fn draw_empty(&mut self, ui: &mut egui::Ui, c: Rect) -> f32 {
        let p = ui.painter().clone();

        let mut y = c.min.y;
        if let Some(name) = &self.rejected_name {
            let r = theme::text(&p, Pos2::new(c.min.x, y), name, BODY_SMALL, W::Regular, theme::soft(), 0.0);
            p.line_segment([Pos2::new(r.min.x, r.center().y), Pos2::new(r.max.x, r.center().y)], Stroke::new(1.0, theme::soft()));
            y = r.max.y + 6.0;
        }
        let r = theme::text(&p, Pos2::new(c.min.x, y + 4.0), "Drop a file.", theme::DISPLAY, W::Bold, theme::ink(), -0.035);
        self.caret(ui, Pos2::new(r.max.x + 4.0, r.min.y + 4.0), r.height() - 6.0);

        // One row: ways in on the left, privacy and the right-click menu on the right.
        let ry = r.max.y + 26.0;
        let choose = icon_button(ui, Pos2::new(c.min.x + 14.0, ry), icon::FOLDER_OPEN, &format!("Choose files or a folder  {}", keys("⌘O", "Ctrl+O")), false, "choose", 36.0);
        let paste = icon_button(ui, Pos2::new(c.min.x + 14.0 + 40.0, ry), icon::PASTE, &format!("Paste a file or image  {}", keys("⌘V", "Ctrl+V")), false, "paste", 36.0);
        if choose.clicked() {
            self.choose_files();
        }
        if paste.clicked() {
            self.paste();
        }
        if icon_button(ui, Pos2::new(c.max.x - 14.0, ry), icon::SETTINGS, &format!("Settings  {}", keys("⌘,", "Ctrl+,")), false, "settings", 36.0).clicked() {
            self.notice = None;
            self.screen = Screen::Settings;
        }

        let mut bottom = ry + 18.0;
        if self.notice.is_some() {
            bottom = self.draw_notice(&p, Pos2::new(c.min.x, bottom + 14.0), c.width());
        }
        bottom
    }

    // ---------------------------------------------------------------- Pick

    fn draw_pick(&mut self, ui: &mut egui::Ui, c: Rect) -> f32 {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        let mods = ctx.input(|i| i.modifiers);

        // Keyboard, typing, and wheel. Tab switches kinds only when there are several;
        // otherwise it moves keyboard focus between controls.
        let none = egui::Modifiers::NONE;
        let multi_groups = self.groups.len() > 1;
        let (prev, next, enter, tab, esc, back, text) = ctx.input_mut(|i| {
            (
                i.consume_key(none, Key::ArrowUp) || i.consume_key(none, Key::ArrowLeft),
                i.consume_key(none, Key::ArrowDown) || i.consume_key(none, Key::ArrowRight),
                i.key_pressed(Key::Enter),
                multi_groups && i.consume_key(none, Key::Tab),
                i.consume_key(none, Key::Escape),
                i.consume_key(none, Key::Backspace),
                i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect::<String>(),
            )
        });
        if !mods.alt {
            for ch in text.chars() {
                self.type_char(&ctx, ch);
            }
        }
        if back {
            self.typed.clear();
        }
        let wheel: f32 = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .map(|e| match e {
                    egui::Event::MouseWheel { unit, delta, .. } => {
                        let d = delta.y + delta.x;
                        if *unit == egui::MouseWheelUnit::Point { d } else { d * 45.0 }
                    }
                    _ => 0.0,
                })
                .sum()
        });
        self.scroll_acc += wheel;
        let mut step = -(prev as i32) + next as i32;
        while self.scroll_acc.abs() >= 45.0 {
            step += if self.scroll_acc > 0.0 { -1 } else { 1 };
            self.scroll_acc -= 45.0 * self.scroll_acc.signum();
        }
        if step != 0 {
            self.groups[self.active].step(step);
            self.bump_reel(&ctx, step.signum() as f32);
            self.notice = None;
            self.typed.clear();
        }
        if tab && self.groups.len() > 1 {
            self.active = (self.active + 1) % self.groups.len();
            self.typed.clear();
        }
        if esc || self.back_nav {
            self.groups.clear();
            self.screen = Screen::Empty;
            self.notice = None;
            return c.min.y + 100.0;
        }
        let mut go = enter;
        let multi = self.groups.len() > 1;

        // Source line: kind icon, name, size. Shares the row with pin and close.
        let total: u64 = self.groups.iter().map(|g| g.bytes).sum();
        let files: usize = self.groups.iter().map(|g| g.files.len()).sum();
        let (glyph, src) = if files == 1 {
            let n = self.groups[0].files[0].file_name().and_then(|n| n.to_str()).unwrap_or("");
            (kind_icon(self.groups[0].kind), format!("{n} · {}", engine::human_size(total)))
        } else if !multi && self.groups[0].single_ext() {
            (kind_icon(self.groups[0].kind), format!("{files} .{} files · {}", self.groups[0].source_ext(), engine::human_size(total)))
        } else {
            (icon::FILES, format!("{files} files · {}", engine::human_size(total)))
        };
        if back_button(ui, c) {
            self.groups.clear();
            self.screen = Screen::Empty;
            self.notice = None;
            return c.min.y + 100.0;
        }
        let src = theme::fit_middle(&p, &src, BODY_SMALL, W::Regular, 0.0, c.width() - 120.0);
        theme::icon_text(&p, c.min + Vec2::new(32.0, 2.0), glyph, &src, BODY_SMALL, W::Regular, theme::soft());

        let mut y = c.min.y + 30.0;
        let reel = theme::spring_with(&ctx, Id::unique(REEL), 0.0, 620.0, 24.0);

        let mut remove: Option<usize> = None;
        if multi {
            // One smaller picker line per kind; the active one carries the reel and caret.
            let size = 28.0;
            y += 4.0;
            for gi in 0..self.groups.len() {
                let g = &self.groups[gi];
                let active = gi == self.active;
                let color = if active { theme::ink() } else { theme::soft() };
                let row = Rect::from_min_size(Pos2::new(c.min.x - 8.0, y), Vec2::new(c.width() + 16.0, size + 10.0));
                let r = ui.interact(row, ui.scope_id().with(("grp", gi)), Sense::click());
                let hl = display_line(&p, Pos2::new(c.min.x, y + 4.0), c.width() - 44.0, &g.stem(), g.target(), size, color, if active { reel } else { 0.0 }, Some(Id::unique(("ext", gi))), widest_ext(&p, &g.targets, size));
                if active {
                    self.caret(ui, Pos2::new(hl.max.x + 3.0, hl.min.y + 2.0), hl.height() - 4.0);
                }
                if r.hovered() {
                    ctx.set_cursor_icon(CursorIcon::PointingHand);
                }
                if r.clicked() {
                    self.active = gi;
                }
                if ui.rect_contains_pointer(row) {
                    let x = icon_button(ui, Pos2::new(c.max.x - 8.0, row.center().y), icon::X, "Leave these out", false, ("rm", gi), 28.0);
                    if x.clicked() {
                        remove = Some(gi);
                    }
                }
                y += size + 10.0;
            }
            y += 6.0;
        } else {
            let g = &self.groups[0];
            let top = y + 22.0;
            let hl = display_line(&p, Pos2::new(c.min.x, top), c.width(), &g.stem(), g.target(), theme::DISPLAY, theme::ink(), reel, Some(Id::unique(("ext", 0))), widest_ext(&p, &g.targets, theme::DISPLAY));
            // Neighbours above and below the highlight: the reel.
            let n = g.targets.len();
            if n > 1 {
                let ns = theme::DISPLAY * 0.42;
                let prev = g.targets[(g.sel + n - 1) % n];
                let next = g.targets[(g.sel + 1) % n];
                let x = hl.min.x + 5.0;
                let dy = reel * 8.0;
                theme::text(&p, Pos2::new(x, hl.min.y - ns - 6.0 + dy), prev, ns, W::Semibold, theme::neighbour(), -0.01);
                if n > 2 {
                    theme::text(&p, Pos2::new(x, hl.max.y + 4.0 + dy), next, ns, W::Semibold, theme::neighbour(), -0.01);
                }
            }
            self.caret(ui, Pos2::new(hl.max.x + 3.0, hl.min.y + 3.0), hl.height() - 6.0);
            let hr = ui.interact(hl, ui.scope_id().with("hl"), Sense::click());
            if hr.hovered() {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
            if hr.clicked() {
                go = true;
            }
            y = hl.max.y + 32.0;
        }
        if let Some(gi) = remove {
            self.groups.remove(gi);
            self.active = self.active.min(self.groups.len().saturating_sub(1));
            if self.groups.is_empty() {
                self.screen = Screen::Empty;
            }
            return y;
        }

        // Chips for the active group, in balanced rows. The selection snaps.
        let g = &self.groups[self.active];
        let chip_h = 30.0;
        let widths = chip_widths(&p, &g.targets);
        let breaks = chip_rows(&widths, c.width());
        let mut rects = Vec::with_capacity(g.targets.len());
        let mut x = c.min.x - 10.0;
        for (i, w) in widths.iter().enumerate() {
            if breaks.contains(&i) {
                x = c.min.x - 10.0;
                y += chip_h + 4.0;
            }
            rects.push(Rect::from_min_size(Pos2::new(x, y), Vec2::new(*w, chip_h)));
            x += w + 4.0;
        }
        let sel = rects[g.sel];
        let mut clicked_chip = None;
        for (i, r) in rects.iter().enumerate() {
            let resp = ui.interact(*r, ui.scope_id().with(("chip", self.active, i)), Sense::click());
            if resp.hovered() && i != g.sel {
                p.rect_filled(*r, CornerRadius::same(6), theme::wash());
                ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.clicked() {
                clicked_chip = Some(i);
            }
        }
        p.rect_filled(sel, CornerRadius::same(6), theme::YELLOW);
        for (i, (r, t)) in rects.iter().zip(&g.targets).enumerate() {
            let fg = if i == g.sel { theme::ON_YELLOW } else if ui.rect_contains_pointer(*r) { theme::ink() } else { theme::soft() };
            p.text(r.center(), Align2::CENTER_CENTER, format!(".{t}"), theme::font(theme::LABEL, W::Semibold), fg);
        }
        if let Some(i) = clicked_chip {
            self.groups[self.active].sel = i;
            go = true;
        }
        y += chip_h + 16.0;

        // Action row: Save, then the two option toggles, then the quality badge.
        let shrinkable = self.groups.iter().any(|g| g.shrinkable());
        let smaller = (self.smaller_on || mods.shift) && shrinkable;
        let replace = self.replace_on || mods.alt;
        let opts = Opts { smaller, replace, settings: self.prefs.settings };
        let g = &self.groups[self.active];
        let label = match (multi, replace, smaller) {
            (false, false, false) => format!("Save as .{}", g.target()),
            (false, false, true) => format!("Save smaller .{}", g.target()),
            (false, true, false) => format!("Replace with .{}", g.target()),
            (false, true, true) => format!("Replace with smaller .{}", g.target()),
            (true, false, false) => "Save all".into(),
            (true, false, true) => "Save all smaller".into(),
            (true, true, false) => "Replace all".into(),
            (true, true, true) => "Replace all, smaller".into(),
        };
        let br = primary_button(ui, Pos2::new(c.min.x, y), &label, "save");
        if br.clicked() {
            go = true;
        }
        let mut ix = br.rect.max.x + 26.0;
        if shrinkable {
            let tip = format!("Smaller file  {}", keys("⇧", "Shift"));
            if icon_button(ui, Pos2::new(ix, br.rect.center().y), icon::SHRINK, &tip, smaller, "smaller", 36.0).clicked() {
                self.smaller_on = !self.smaller_on;
            }
            ix += 40.0;
        }
        let tip = format!("Replace the original  {}", keys("⌥", "Alt"));
        if icon_button(ui, Pos2::new(ix, br.rect.center().y), icon::TRASH, &tip, replace, "replace", 36.0).clicked() {
            self.replace_on = !self.replace_on;
        }
        ix += 30.0;
        let cost = self.groups.iter().map(|g| g.cost(opts)).max().unwrap_or(Cost::Quick);
        let badge = match cost {
            Cost::Same => Some((icon::ZAP, "Lossless", "Same quality. Swaps the container, or copies the data as is.")),
            Cost::Slow => Some((icon::HOURGLASS, "Slow", "Re-encodes the video, so it takes longer")),
            Cost::Quick => None,
        };
        if let Some((glyph, word, tip)) = badge {
            let r = theme::icon_text(&p, Pos2::new(ix, br.rect.center().y - 9.0), glyph, word, theme::CAPTION, W::Semibold, theme::soft());
            ui.interact(r.expand(4.0), ui.scope_id().with("badge"), Sense::hover()).on_hover_text(tip);
            ix = r.max.x + 14.0;
        }
        // With Smaller on: what it comes out at.
        if smaller {
            let label = match self.smaller_estimate(&ctx, opts) {
                Some(Some(n)) => format!("≈ {}", engine::human_size(n)),
                Some(None) => String::new(),
                None => "≈ …".into(),
            };
            if !label.is_empty() {
                let r = theme::text(&p, Pos2::new(ix, br.rect.center().y - 9.0), &label, theme::CAPTION, W::Semibold, theme::soft(), 0.0);
                let total: u64 = self.groups.iter().map(|g| g.bytes).sum();
                ui.interact(r.expand(4.0), ui.scope_id().with("est"), Sense::hover()).on_hover_text(format!("Estimated size, from {}", engine::human_size(total)));
            }
        }
        let mut bottom = br.rect.max.y;
        if self.notice.is_some() {
            bottom = self.draw_notice(&p, Pos2::new(c.min.x, bottom + 16.0), c.width());
        }

        if go {
            self.notice = None;
            self.convert(&ctx, opts);
        }
        bottom
    }

    // ---------------------------------------------------------------- Working

    fn draw_working(&mut self, ui: &mut egui::Ui, c: Rect) -> f32 {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        let Screen::Working(job) = &self.screen else { return c.min.y };
        let job = job.clone();
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) || self.back_nav {
            job.cancel();
        }
        let s = job.snapshot();

        let multi = self.groups.len() > 1;
        let files: usize = self.groups.iter().map(|g| g.files.len()).sum();
        let (glyph, src) = if files == 1 {
            (kind_icon(self.groups[0].kind), self.groups[0].files[0].file_name().and_then(|n| n.to_str()).unwrap_or("").to_string())
        } else {
            (icon::FILES, format!("{files} files"))
        };
        if back_button(ui, c) {
            // Stop, throw away anything already written, and start over.
            job.cancel();
            std::thread::spawn(move || {
                while !job.snapshot().done {
                    std::thread::sleep(Duration::from_millis(30));
                }
                // With Replace, finished originals are already in the Trash: keep those results.
                let s = job.snapshot();
                if s.trashed.is_empty() {
                    for o in s.outputs {
                        let _ = std::fs::remove_file(o);
                    }
                }
            });
            self.groups.clear();
            self.screen = Screen::Empty;
            self.notice = None;
            return c.min.y + 100.0;
        }
        theme::icon_text(&p, c.min + Vec2::new(32.0, 2.0), glyph, &theme::fit_middle(&p, &src, BODY_SMALL, W::Regular, 0.0, c.width() - 120.0), BODY_SMALL, W::Regular, theme::soft());
        let mut y = c.min.y + 40.0;
        let size = if multi { 28.0 } else { theme::DISPLAY };
        for (gi, g) in self.groups.iter().enumerate() {
            let r = display_line(&p, Pos2::new(c.min.x, y), c.width(), &g.stem(), g.target(), size, theme::ink(), 0.0, Some(Id::unique(("ext", gi))), widest_ext(&p, &g.targets, size));
            y = r.max.y + 10.0;
        }

        // Progress: 3px hairline track, ink fill on a spring; count and cancel at the end.
        let row_y = y + 22.0;
        let shown = theme::spring_with(&ctx, Id::unique("progress"), s.progress, 300.0, 34.0);
        let track = Rect::from_min_size(Pos2::new(c.min.x, row_y - 1.5), Vec2::new(c.width() - 90.0, 3.0));
        p.rect_filled(track, CornerRadius::same(2), theme::line());
        p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * shown.clamp(0.0, 1.0), 3.0)), CornerRadius::same(2), theme::ink());
        let label = if s.total > 1 { format!("{}/{}", (s.current + 1).min(s.total), s.total) } else { format!("{}%", (s.progress * 100.0).round() as i32) };
        p.text(Pos2::new(track.max.x + 12.0, row_y), Align2::LEFT_CENTER, label, theme::font(theme::CAPTION, W::Semibold), theme::soft());
        if icon_button(ui, Pos2::new(c.max.x - 14.0, row_y), icon::X, "Cancel  Esc", false, "cancel", 32.0).clicked() {
            job.cancel();
        }
        let mut bottom = row_y + 16.0;
        if self.notice.is_some() {
            bottom = self.draw_notice(&p, Pos2::new(c.min.x, bottom + 14.0), c.width());
        }
        bottom
    }

    // ---------------------------------------------------------------- Done

    fn draw_done(&mut self, ui: &mut egui::Ui, c: Rect, frame: &eframe::Frame) -> f32 {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        let cmd = egui::Modifiers::COMMAND;
        let none = egui::Modifiers::NONE;
        let (open, reveal, copy, look, esc, undo, step, text) = ctx.input_mut(|i| {
            (
                i.consume_key(none, Key::Enter),
                i.consume_key(cmd, Key::R),
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)) || i.consume_key(cmd, Key::C),
                i.consume_key(none, Key::Space),
                i.consume_key(none, Key::Escape),
                i.consume_key(cmd, Key::Z),
                -((i.consume_key(none, Key::ArrowUp) || i.consume_key(none, Key::ArrowLeft)) as i32) + (i.consume_key(none, Key::ArrowDown) || i.consume_key(none, Key::ArrowRight)) as i32,
                i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect::<String>(),
            )
        });
        if undo {
            self.undo();
            return c.min.y + 100.0;
        }
        // Typing or arrows go straight back to the picker for another format.
        if (step != 0 || text.chars().any(|c| c.is_ascii_alphanumeric())) && self.again() {
            if step != 0 {
                self.groups[self.active].step(step);
                self.bump_reel(&ctx, step.signum() as f32);
            }
            for ch in text.chars() {
                self.type_char(&ctx, ch);
            }
            return c.min.y + 100.0;
        }
        let can_again = !self.groups.is_empty();
        let first_source = self.groups.first().map(|g| (g.kind, g.files[0].clone()));
        let Screen::Done(f) = &mut self.screen else { return c.min.y };
        let outputs = f.state.outputs.clone();
        let stacked = outputs.len() > 1;
        let enter = theme::spring_with(&ctx, Id::unique(DONE_IN), 1.0, 420.0, 20.0);

        // Source line.
        let (glyph, src) = if f.sources == 1 {
            let path = f.state.trashed.first().cloned().or(first_source.as_ref().map(|s| s.1.clone()));
            let n = path.and_then(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string)).unwrap_or_default();
            (first_source.map(|s| kind_icon(s.0)).unwrap_or(icon::FILES), n)
        } else {
            (icon::FILES, format!("{} files", f.sources))
        };
        let back = icon_button(ui, Pos2::new(c.min.x + 10.0, c.min.y + 10.0), icon::PLUS, "New file  Esc", false, "new", 28.0).clicked();
        theme::icon_text(&p, c.min + Vec2::new(32.0, 2.0), glyph, &theme::fit_middle(&p, &src, BODY_SMALL, W::Regular, 0.0, c.width() - 120.0), BODY_SMALL, W::Regular, theme::soft());

        // The file row. The whole row is a real file: drag it into any app.
        let row = Rect::from_min_size(Pos2::new(c.min.x - 10.0, c.min.y + 32.0), Vec2::new(c.width() + 20.0, 96.0));
        let resp = ui.interact(row, ui.scope_id().with("file"), Sense::click_and_drag());
        let hot = resp.hovered() || resp.dragged();
        let lift = theme::spring(&ctx, Id::unique("row-hot"), if hot { 1.0 } else { 0.0 });
        if lift > 0.01 {
            p.rect_filled(row, CornerRadius::same(10), theme::wash().gamma_multiply(lift.clamp(0.0, 1.0)));
        }
        if hot {
            ctx.set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
            icon(&p, Pos2::new(row.max.x - 20.0, row.center().y), icon::GRIP, 18.0, theme::soft());
        }
        let resp = resp.on_hover_text(if stacked { "Drag the files into any app" } else { "Drag into any app. Double-click to open." });
        if resp.drag_started() {
            platform::start_file_drag(frame, outputs.clone(), f.drag_png.clone());
        }
        if resp.double_clicked() {
            platform::open(&outputs[0]);
        }

        // Tile pops in on the entrance spring.
        let tile_size = 72.0 * (0.82 + 0.18 * enter);
        let tile = Rect::from_center_size(row.min + Vec2::new(10.0 + 36.0, row.height() / 2.0), Vec2::splat(tile_size));
        if stacked {
            for k in (1..3.min(outputs.len())).rev() {
                let o = k as f32 * 5.0 * enter;
                let r = tile.translate(Vec2::new(o, -o));
                p.rect_filled(r, CornerRadius::same(6), theme::paper());
                p.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, theme::line()), StrokeKind::Inside);
            }
        }
        p.rect_filled(tile, CornerRadius::same(6), theme::paper());
        match &f.thumb {
            Some(t) => {
                let s = t.size_vec2();
                let k = (tile.width() / s.x).min(tile.height() / s.y);
                p.image(t.id(), Rect::from_center_size(tile.center(), s * k), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                let ext = formats::ext_of(&outputs[0]).unwrap_or_default();
                let k = formats::kind_of_ext(&ext).unwrap_or(Kind::Data);
                icon(&p, tile.center() - Vec2::new(0.0, 7.0), kind_icon(k), 24.0, theme::soft());
                p.text(tile.center() + Vec2::new(0.0, 16.0), Align2::CENTER_CENTER, format!(".{ext}"), theme::font(12.5, W::Semibold), theme::soft());
            }
        }
        p.rect_stroke(tile, CornerRadius::same(6), Stroke::new(1.0, theme::line()), StrokeKind::Inside);

        // Name with the highlight sweeping across it, as if just selected.
        let tx = row.min.x + 10.0 + 72.0 + 18.0;
        let name = if stacked { format!("{} files", outputs.len()) } else { outputs[0].file_name().and_then(|n| n.to_str()).unwrap_or("").to_string() };
        let size = 24.0;
        let name = theme::fit_middle(&p, &name, size, W::Bold, -0.02, row.max.x - tx - 44.0);
        let g = theme::galley(&p, &name, size, W::Bold, theme::ON_YELLOW, -0.02);
        let npos = Pos2::new(tx + 4.0, row.center().y - g.size().y + 2.0);
        let full_hl = Rect::from_min_size(npos - Vec2::new(4.0, 1.0), g.size() + Vec2::new(8.0, 3.0));
        let hl = Rect::from_min_size(full_hl.min, Vec2::new(full_hl.width() * enter.clamp(0.0, 1.05), full_hl.height()));
        p.rect_filled(hl, CornerRadius::same(3), theme::YELLOW);
        // Ink text where the highlight hasn't reached yet, dark-on-yellow where it has.
        p.with_clip_rect(Rect::from_min_max(Pos2::new(hl.max.x, full_hl.min.y), full_hl.max)).galley(npos, theme::galley(&p, &name, size, W::Bold, theme::ink(), -0.02), theme::ink());
        p.with_clip_rect(hl).galley(npos, g, theme::ON_YELLOW);
        let total_bytes: u64 = outputs.iter().filter_map(|o| std::fs::metadata(o).ok()).map(|m| m.len()).sum();
        theme::text(&p, Pos2::new(tx, full_hl.max.y + 8.0), &engine::human_size(total_bytes), theme::CAPTION, W::Regular, theme::soft(), 0.0);

        // Actions: Open, then icons.
        let y = row.max.y + 14.0;
        let ob = primary_button(ui, Pos2::new(c.min.x, y), if stacked { "Open first" } else { "Open" }, "open");
        let cy = ob.rect.center().y;
        let mut ix = ob.rect.max.x + 26.0;
        let reveal_tip = format!("Show in {}  {}", platform::file_manager_name(), keys("⌘R", "Ctrl+R"));
        let rb = icon_button(ui, Pos2::new(ix, cy), icon::FOLDER, &reveal_tip, false, "reveal", 36.0);
        ix += 40.0;
        let copied = f.copied_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1500));
        if copied {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        let copy_tip = if copied { "Copied".to_string() } else { format!("Copy, then paste into any app  {}", keys("⌘C", "Ctrl+C")) };
        let cb = icon_button(ui, Pos2::new(ix, cy), if copied { icon::CHECK } else { icon::COPY }, &copy_tip, copied, "copy", 36.0);
        ix += 40.0;
        let ab = if can_again { Some(icon_button(ui, Pos2::new(ix, cy), icon::SWAP, "Another format  ← →", false, "again", 36.0)) } else { None };
        if can_again {
            ix += 40.0;
        }
        let ub = icon_button(ui, Pos2::new(ix, cy), icon::UNDO, &format!("Undo  {}", keys("⌘Z", "Ctrl+Z")), false, "undo", 36.0);

        // Status: only what the file row can't say.
        let st = &f.state;
        let status: Option<(char, String)> = if !st.failures.is_empty() {
            Some((icon::ALERT, failure_line(&st.failures, outputs.len())))
        } else if st.fell_back_to_downloads {
            Some((icon::DOWNLOAD, "Saved to Downloads (that folder is read-only)".into()))
        } else if !st.trashed.is_empty() {
            Some((icon::TRASH, if st.trashed.len() == 1 { "Original in the Trash".into() } else { "Originals in the Trash".into() }))
        } else {
            None
        };
        let mut bottom = ob.rect.max.y;
        if let Some((gl, s)) = status {
            let s = theme::fit(&p, &s, theme::BODY, W::Regular, 0.0, c.width() - 24.0);
            bottom = theme::icon_text(&p, Pos2::new(c.min.x, bottom + 16.0), gl, &s, theme::BODY, W::Regular, theme::ink()).max.y;
        }

        if open || ob.clicked() {
            platform::open(&outputs[0]);
        }
        if reveal || rb.clicked() {
            platform::reveal(&outputs);
        }
        if copy || cb.clicked() {
            platform::copy_files(&outputs);
            f.copied_at = Some(Instant::now());
        }
        if look {
            platform::quick_look(&outputs[0]);
        }
        if ub.clicked() {
            self.undo();
        } else if ab.is_some_and(|b| b.clicked()) {
            self.again();
        } else if esc || back || self.back_nav {
            self.groups.clear();
            self.screen = Screen::Empty;
            self.notice = None;
        }
        bottom
    }
}

/// "interview" "." [mov]: the file name in display type with the extension
/// highlighted like a text selection. The stem truncates; the extension never does.
/// With `anim`, the highlight's width springs when the extension changes.
/// Returns the highlight rect.
#[allow(clippy::too_many_arguments)]
fn display_line(p: &egui::Painter, pos: Pos2, max_w: f32, stem: &str, ext: &str, size: f32, color: Color32, reel: f32, anim: Option<Id>, reserve: f32) -> Rect {
    let tr = -0.035;
    let pad = size * 0.12;
    let ext_w = theme::width(p, ext, size, W::Bold, tr);
    let dot_w = theme::width(p, ".", size, W::Bold, tr);
    // Truncate against the widest extension in the list, so the name stays put while scrolling.
    let room = max_w - ext_w.max(reserve) - dot_w - pad * 2.0 - 8.0;
    let stem = theme::fit_middle(p, stem, size, W::Bold, tr, room);
    let sr = theme::text(p, pos, &stem, size, W::Bold, color, tr);
    let dr = theme::text(p, Pos2::new(sr.max.x, pos.y), ".", size, W::Bold, color, tr);
    let ex = Pos2::new(dr.max.x + pad, pos.y);
    let w = match anim {
        Some(id) => theme::spring(p.ctx(), id, ext_w + pad * 2.0),
        None => ext_w + pad * 2.0,
    };
    let hl = Rect::from_min_max(Pos2::new(dr.max.x, pos.y + size * 0.04), Pos2::new(dr.max.x + w, pos.y + sr.height() - size * 0.02));
    p.rect_filled(hl, CornerRadius::same(4), theme::YELLOW);
    let clip = p.with_clip_rect(hl);
    theme::text(&clip, ex + Vec2::new(0.0, reel * size * 0.55), ext, size, W::Bold, theme::ON_YELLOW, tr);
    hl
}

/// Press feedback shared by buttons: a quick spring down to 94% while held.
fn press_scale(ui: &egui::Ui, resp: &egui::Response) -> f32 {
    let target = if resp.is_pointer_button_down_on() { 0.94 } else { 1.0 };
    theme::spring_with(ui.ctx(), resp.id.with("press"), target, 900.0, 28.0)
}

fn primary_button(ui: &mut egui::Ui, pos: Pos2, label: &str, id: &str) -> egui::Response {
    let w = theme::width(ui.painter(), label, 16.0, W::Semibold, 0.0) + 32.0;
    let r = Rect::from_min_size(pos, Vec2::new(w, 42.0));
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    focus_ring(ui, &resp, r);
    let k = press_scale(ui, &resp);
    let dr = Rect::from_center_size(r.center(), r.size() * k);
    let bg = if resp.hovered() { theme::ink_hover() } else { theme::ink() };
    ui.painter().rect_filled(dr, CornerRadius::same(8), bg);
    ui.painter().text(dr.center(), Align2::CENTER_CENTER, label, theme::font(16.0 * k, W::Semibold), theme::paper());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

/// Square icon button. `active` fills it with the selection yellow (a toggle that is on),
/// and the fill pops in on a spring.
pub fn icon_button(ui: &mut egui::Ui, center: Pos2, glyph: char, tip: &str, active: bool, id: impl std::hash::Hash + std::fmt::Debug, size: f32) -> egui::Response {
    let r = Rect::from_center_size(center, Vec2::splat(size));
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
    // Screen readers get the tooltip text without the shortcut.
    let name = tip.split("  ").next().unwrap_or(tip).to_string();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, name.clone()));
    focus_ring(ui, &resp, r);
    let k = press_scale(ui, &resp);
    let on = theme::spring_with(ui.ctx(), resp.id.with("on"), if active { 1.0 } else { 0.0 }, 700.0, 22.0);
    let p = ui.painter();
    if resp.hovered() && on < 0.99 {
        p.rect_filled(Rect::from_center_size(center, r.size() * k), CornerRadius::same(8), theme::wash());
    }
    if on > 0.01 {
        p.rect_filled(Rect::from_center_size(center, r.size() * k * on.min(1.08)), CornerRadius::same(8), theme::YELLOW);
    }
    let color = if active { theme::ON_YELLOW } else if resp.hovered() { theme::ink() } else { theme::soft() };
    icon(p, center, glyph, (size * 0.5 * k).round(), color);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

/// A setting that stays on (pin, right-click menu): ink icon with a small yellow
/// dot when on, soft when off. Lighter than `icon_button`'s yellow fill, which is
/// kept for choices that apply to the next conversion.
pub fn icon_toggle(ui: &mut egui::Ui, center: Pos2, glyph: char, tip: &str, on: bool, id: impl std::hash::Hash + std::fmt::Debug, size: f32) -> egui::Response {
    let resp = icon_button(ui, center, glyph, tip, false, id, size);
    let dot = theme::spring_with(ui.ctx(), resp.id.with("dot"), if on { 1.0 } else { 0.0 }, 700.0, 20.0);
    let p = ui.painter();
    if on && !resp.hovered() {
        icon(p, center, glyph, (size * 0.5).round(), theme::ink());
    }
    if dot > 0.01 {
        let at = center + Vec2::new(size * 0.26, size * 0.26);
        p.circle_filled(at, 3.2 * dot, theme::YELLOW);
        p.circle_stroke(at, 3.2 * dot, Stroke::new(1.0, theme::paper()));
    }
    resp
}

/// Start over: back to the empty card. Sits at the left of the source row.
fn back_button(ui: &mut egui::Ui, c: Rect) -> bool {
    icon_button(ui, Pos2::new(c.min.x + 10.0, c.min.y + 10.0), icon::BACK, "Start over  Esc", false, "back", 28.0).clicked()
}

/// Show or hide the window without ending the app. macOS keeps it on screen, fully
/// transparent and click-through. Windows hides it outright: DWM still draws a border
/// and shadow around an undecorated transparent window, and it would stay in the taskbar.
fn show_window(ctx: &egui::Context, shown: bool) {
    ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(!shown));
    if cfg!(windows) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(shown));
    }
}

/// After the window closes mid-conversion: wait for the job, tell the user, quit.
/// If files arrive meanwhile, `background` is cleared and the app stays.
fn finish_in_background(job: Job, background: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    std::thread::spawn(move || {
        while !job.snapshot().done {
            std::thread::sleep(Duration::from_millis(100));
        }
        if !background.load(Ordering::Relaxed) {
            return;
        }
        platform::notify("Retype", &done_message(&job.snapshot()));
        std::process::exit(0);
    });
}

/// Keyboard focus: a 2px ink ring just outside the control.
fn focus_ring(ui: &egui::Ui, resp: &egui::Response, r: Rect) {
    if resp.has_focus() {
        ui.painter().rect_stroke(r.expand(2.0), CornerRadius::same(10), Stroke::new(2.0, theme::ink()), StrokeKind::Outside);
    }
}

/// On/off switch: yellow track with an on-yellow knob when on, hairline track when off.
fn switch(ui: &egui::Ui, center: Pos2, on: bool, id: Id, enabled: bool) {
    let p = ui.painter();
    let track = Rect::from_center_size(center, Vec2::new(34.0, 20.0));
    let k = theme::spring_with(ui.ctx(), id.with("knob"), if on { 1.0 } else { 0.0 }, 700.0, 30.0);
    let fill = if on { theme::YELLOW } else { theme::line() };
    p.rect_filled(track, CornerRadius::same(10), if enabled { fill } else { fill.gamma_multiply(0.5) });
    let x = track.min.x + 10.0 + k * 14.0;
    let knob = if on { theme::ON_YELLOW } else { theme::paper() };
    p.circle_filled(Pos2::new(x, center.y), 7.0, knob);
    if !on {
        p.circle_stroke(Pos2::new(x, center.y), 7.0, Stroke::new(1.0, theme::soft().gamma_multiply(0.5)));
    }
}

fn done_message(s: &JobState) -> String {
    match (s.outputs.len(), s.failures.len()) {
        (0, _) => "Couldn't convert the file.".to_string(),
        (1, 0) => format!("Saved {}", s.outputs[0].file_name().and_then(|n| n.to_str()).unwrap_or("the file")),
        (n, 0) => format!("Saved {n} files"),
        (n, f) => format!("Saved {n} files. {f} couldn't be converted."),
    }
}

/// Minimal executor for one future on a worker thread (rfd's async dialog).
#[cfg(not(target_os = "macos"))]
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(std::sync::Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}
