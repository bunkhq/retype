//! Conversion engine. Everything runs locally:
//! - video, audio, subtitles, AVIF: ffmpeg
//! - stills: the `image` crate; HEIC and camera RAW through macOS ImageIO (`sips`)
//! - SVG: resvg; PSD: the `psd` crate; PDF: written directly
//! - JSON / YAML / TOML / CSV: serde

use crate::formats::{self, Kind, Probe};
use crate::metadata;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Locate a bundled or installed helper binary (ffmpeg, ffprobe).
/// Finder-launched apps do not inherit the shell PATH, so check the usual
/// install locations too.
pub fn tool(name: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
            dirs.push(d.join("../Resources")); // macOS .app bundle
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(&exe_name)).find(|p| p.is_file())
}

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

const NEED_FFMPEG: &str = "needs ffmpeg. Install it with: brew install ffmpeg";

pub fn probe(path: &Path) -> Option<Probe> {
    let ffprobe = tool("ffprobe")?;
    let out = quiet(&mut Command::new(ffprobe))
        .args(["-v", "error", "-show_entries", "stream=codec_type,codec_name:format=duration", "-of", "json"])
        .arg(path)
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let mut p = Probe::default();
    for s in v["streams"].as_array().into_iter().flatten() {
        let name = s["codec_name"].as_str().map(str::to_string);
        match s["codec_type"].as_str() {
            // Cover art arrives as a video stream; it is not video.
            Some("video") if p.video.is_none() && !matches!(name.as_deref(), Some("mjpeg" | "png")) => p.video = name,
            Some("audio") if p.audio.is_none() => p.audio = name,
            _ => {}
        }
    }
    p.duration = v["format"]["duration"].as_str().and_then(|d| d.parse().ok());
    Some(p)
}

/// `dir/stem.ext`, or the platform's numbered variant when that name is taken.
/// Never overwrites. Falls back to Downloads when the folder is not writable.
pub fn output_path(src: &Path, target: &str) -> (PathBuf, bool) {
    output_path_in(src, target, false)
}

/// Like `output_path`, optionally saving into Downloads (the "Save to Downloads" setting).
pub fn output_path_in(src: &Path, target: &str, downloads: bool) -> (PathBuf, bool) {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let mut dir = src.parent().map(Path::to_path_buf).unwrap_or_default();
    if downloads {
        if let Some(d) = dirs::download_dir() {
            dir = d;
        }
    }
    let mut fell_back = false;
    if !writable(&dir) {
        if let Some(d) = dirs::download_dir() {
            dir = d;
            fell_back = true;
        }
    }
    let mut n = 1;
    loop {
        let name = match n {
            1 => format!("{stem}.{target}"),
            n if cfg!(windows) => format!("{stem} ({n}).{target}"),
            n => format!("{stem} {n}.{target}"),
        };
        let p = dir.join(name);
        if !p.exists() {
            return (p, fell_back);
        }
        n += 1;
    }
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(".retype-write-test");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

// ---------------------------------------------------------------- jobs

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Opts {
    /// Trade quality for size (⇧).
    pub smaller: bool,
    /// Move the originals to the Trash once the new file is saved (⌥).
    pub replace: bool,
    /// Metadata, location, hardware encoding, save location.
    pub settings: crate::prefs::Settings,
}

pub struct Task {
    /// Usually one file; several when images combine into one PDF.
    pub srcs: Vec<PathBuf>,
    pub target: String,
    pub opts: Opts,
}

/// What the UI reads. Cloned out of the lock on each frame.
#[derive(Default, Clone)]
pub struct JobState {
    /// 0.0..=1.0 across the whole job.
    pub progress: f32,
    pub current: usize,
    pub total: usize,
    pub done: bool,
    pub cancelled: bool,
    pub outputs: Vec<PathBuf>,
    pub failures: Vec<(PathBuf, String)>,
    /// Originals moved to the Trash by Replace.
    pub trashed: Vec<PathBuf>,
    pub re_encoded: bool,
    pub fell_back_to_downloads: bool,
    pub thumb: Option<image::RgbaImage>,
}

struct Shared {
    queue: VecDeque<Task>,
    state: JobState,
    running: bool,
    completed: usize,
}

type Notify = Arc<dyn Fn() + Send + Sync>;

/// A queue of conversions on one worker thread. More tasks can join while it
/// runs, which is how right-click batches from several processes merge.
#[derive(Clone)]
pub struct Job {
    shared: Arc<Mutex<Shared>>,
    cancel: Arc<AtomicBool>,
    notify: Notify,
}

impl Job {
    pub fn start(tasks: Vec<Task>, notify: impl Fn() + Send + Sync + 'static) -> Job {
        let job = Job {
            shared: Arc::new(Mutex::new(Shared { queue: VecDeque::new(), state: JobState::default(), running: false, completed: 0 })),
            cancel: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(notify),
        };
        job.push(tasks);
        job
    }

    pub fn push(&self, tasks: Vec<Task>) {
        let mut s = self.shared.lock().unwrap();
        s.state.total += tasks.len();
        s.queue.extend(tasks);
        if s.queue.is_empty() && !s.running {
            s.state.done = true;
            drop(s);
            (self.notify)();
            return;
        }
        s.state.done = false;
        if !s.running {
            s.running = true;
            let job = self.clone();
            std::thread::spawn(move || job.work());
        }
    }

    pub fn snapshot(&self) -> JobState {
        self.shared.lock().unwrap().state.clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn work(&self) {
        loop {
            let task = {
                let mut s = self.shared.lock().unwrap();
                match s.queue.pop_front() {
                    Some(t) if !self.cancel.load(Ordering::Relaxed) => {
                        s.state.current = s.completed;
                        t
                    }
                    _ => {
                        s.queue.clear();
                        break;
                    }
                }
            };
            (self.notify)();
            let (out, fell_back) = output_path_in(&task.srcs[0], &task.target, task.opts.settings.save_to_downloads);
            let report = |frac: f32| {
                let mut s = self.shared.lock().unwrap();
                s.state.progress = (s.completed as f32 + frac.clamp(0.0, 1.0)) / s.state.total.max(1) as f32;
                drop(s);
                (self.notify)();
            };
            let result = convert(&task.srcs, &out, &task.target, task.opts, &self.cancel, &report);
            let mut trashed = Vec::new();
            if result.is_ok() && task.opts.replace {
                for src in &task.srcs {
                    if crate::platform::trash(src).is_ok() {
                        trashed.push(src.clone());
                    }
                }
            }
            let mut s = self.shared.lock().unwrap();
            s.completed += 1;
            s.state.progress = s.completed as f32 / s.state.total.max(1) as f32;
            match result {
                Ok((re, saved)) => {
                    s.state.outputs.push(saved);
                    s.state.re_encoded |= re;
                    s.state.fell_back_to_downloads |= fell_back;
                    s.state.trashed.extend(trashed);
                }
                Err(_) if self.cancel.load(Ordering::Relaxed) => {}
                Err(e) => s.state.failures.push((task.srcs[0].clone(), e)),
            }
        }
        // Thumbnail of the first result, made off the UI thread.
        let first = {
            let s = self.shared.lock().unwrap();
            if s.state.thumb.is_none() { s.state.outputs.first().cloned() } else { None }
        };
        let thumb = first.and_then(|p| thumbnail(&p, 192));
        let mut s = self.shared.lock().unwrap();
        if thumb.is_some() {
            s.state.thumb = thumb;
        }
        s.running = false;
        if s.queue.is_empty() {
            s.state.cancelled = self.cancel.load(Ordering::Relaxed);
            s.state.done = true;
            drop(s);
        } else {
            // Tasks arrived while the thumbnail was being made.
            s.running = true;
            drop(s);
            let job = self.clone();
            std::thread::spawn(move || job.work());
        }
        (self.notify)();
    }
}

/// Temporary sibling path, so a half-written file never sits at the final name.
/// Move the finished temp file to `out`, or the next free name if another job
/// took `out` meanwhile. A hard link fails atomically when the name exists, so a
/// file is never replaced; volumes without hard links fall back to a checked rename.
fn place_without_clobber(part: &Path, out: &Path, target: &str) -> Result<PathBuf, String> {
    let mut dest = out.to_path_buf();
    for _ in 0..1000 {
        match std::fs::hard_link(part, &dest) {
            Ok(()) => {
                let _ = std::fs::remove_file(part);
                return Ok(dest);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => dest = output_path(&dest, target).0,
            Err(_) => {
                if dest.exists() {
                    dest = output_path(&dest, target).0;
                    continue;
                }
                std::fs::rename(part, &dest).map_err(|e| e.to_string())?;
                return Ok(dest);
            }
        }
    }
    Err("couldn't find a free file name".into())
}

/// Estimated output size in bytes. Images are converted for real into a temp
/// file; video and audio encode 3 seconds from the middle and scale up.
pub fn estimate(src: &Path, target: &str, opts: Opts) -> Option<u64> {
    let source = formats::ext_of(src)?;
    let kind = formats::kind_of_ext(&source)?;
    let out = temp(&format!("estimate.{target}"));
    let never = AtomicBool::new(false);
    let size = match kind {
        Kind::Video | Kind::Audio => {
            let duration = probe(src)?.duration?;
            let len = duration.min(3.0);
            let start = ((duration - len) / 2.0).max(0.0);
            convert_av_range(src, &source, &out, target, opts, &never, &|_| {}, Some((start, len))).ok()?;
            let sampled = std::fs::metadata(&out).ok()?.len() as f64;
            (sampled * duration / len.max(0.1)) as u64
        }
        _ => {
            let (_, saved) = convert(&[src.to_path_buf()], &out, target, opts, &never, &|_| {}).ok()?;
            let n = std::fs::metadata(&saved).ok()?.len();
            let _ = std::fs::remove_file(&saved);
            n
        }
    };
    let _ = std::fs::remove_file(&out);
    Some(size)
}

/// Temp files in flight, so ones left by a crash or force-quit can be removed on
/// the next launch. A plain list in the cache folder; entries older than a few
/// minutes that still exist are leftovers (a live encode keeps its file fresh).
pub mod parts {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime};

    static LOCK: Mutex<()> = Mutex::new(());

    fn list() -> Option<PathBuf> {
        Some(dirs::cache_dir()?.join("Retype").join("parts.txt"))
    }

    fn read() -> Vec<PathBuf> {
        list().and_then(|l| std::fs::read_to_string(l).ok()).map(|s| s.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()).unwrap_or_default()
    }

    fn write(paths: &[PathBuf]) {
        if let Some(l) = list() {
            if let Some(d) = l.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let body: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
            let _ = std::fs::write(l, body);
        }
    }

    pub fn register(p: &Path) {
        let _g = LOCK.lock().unwrap();
        let mut v = read();
        v.push(p.to_path_buf());
        write(&v);
    }

    pub fn unregister(p: &Path) {
        let _g = LOCK.lock().unwrap();
        let mut v = read();
        v.retain(|x| x != p);
        write(&v);
    }

    /// Delete leftovers from earlier runs. Returns how many were removed.
    pub fn sweep() -> usize {
        let _g = LOCK.lock().unwrap();
        let stale = Duration::from_secs(180);
        let mut keep = Vec::new();
        let mut removed = 0;
        for p in read() {
            let age = std::fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
            match age {
                None => {} // already gone
                Some(a) if a > stale && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains(".retype-part-")) => {
                    if std::fs::remove_file(&p).is_ok() {
                        removed += 1;
                    }
                }
                Some(_) => keep.push(p),
            }
        }
        write(&keep);
        removed
    }
}

/// Unique per call, so two jobs aiming at the same name never share a temp file.
fn part_path(out: &Path) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let name = out.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    out.with_file_name(format!(".{stem}.retype-part-{}-{n}.{ext}", std::process::id()))
}

/// VideoToolbox H.264 with constant quality: Apple silicon and an ffmpeg built with it.
fn hardware_h264() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64")) && has_encoder("h264_videotoolbox")
}

/// Whether this ffmpeg build has an encoder (checked once per process).
fn has_encoder(name: &str) -> bool {
    static LIST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let list = LIST.get_or_init(|| {
        tool("ffmpeg")
            .and_then(|ff| quiet(&mut Command::new(ff)).args(["-hide_banner", "-encoders"]).output().ok())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    });
    list.split_whitespace().any(|w| w == name)
}

fn temp(name: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir().join(format!("retype-{}-{n}-{name}", std::process::id()))
}

/// Convert one task. Returns whether the data was re-encoded, and where it was saved
/// (normally `out`; the next free name if `out` was taken while converting).
pub fn convert(srcs: &[PathBuf], out: &Path, target: &str, opts: Opts, cancel: &AtomicBool, report: &dyn Fn(f32)) -> Result<(bool, PathBuf), String> {
    let src = &srcs[0];
    let source = formats::ext_of(src).unwrap_or_default();
    let sk = formats::kind_of_ext(&source).ok_or("isn't a type Retype converts")?;
    let tk = formats::kind_of_ext(target).ok_or("can't become that type")?;
    let part = part_path(out);
    parts::register(&part);
    let result = match (sk, tk) {
        (Kind::Image, Kind::Document) => images_to_pdf(srcs, &part, opts, report),
        (Kind::Image, Kind::Image) => convert_image(src, &source, &part, target, opts),
        (Kind::Data, Kind::Data) => convert_data(src, &source, &part, target),
        (Kind::Subtitle, Kind::Subtitle) => convert_subtitle(src, &part),
        _ => convert_av(src, &source, &part, target, opts, cancel, report),
    };
    let placed = match result {
        Ok(re) => place_without_clobber(&part, out, target).map(|dest| (re, dest)),
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_file(&part);
    parts::unregister(&part);
    placed
}

// ---------------------------------------------------------------- video and audio

/// Scale so the long edge is at most `max`, keeping even dimensions.
fn long_edge(max: u32) -> String {
    format!("scale='if(gt(iw,ih),min({max},iw),-2)':'if(gt(iw,ih),-2,min({max},ih))',scale=trunc(iw/2)*2:trunc(ih/2)*2")
}

fn convert_av(src: &Path, source: &str, part: &Path, target: &str, opts: Opts, cancel: &AtomicBool, report: &dyn Fn(f32)) -> Result<bool, String> {
    convert_av_range(src, source, part, target, opts, cancel, report, None)
}

/// `sample`: encode only `(start, seconds)` of the input (used for size estimates).
#[allow(clippy::too_many_arguments)]
fn convert_av_range(src: &Path, source: &str, part: &Path, target: &str, opts: Opts, cancel: &AtomicBool, report: &dyn Fn(f32), sample: Option<(f64, f64)>) -> Result<bool, String> {
    let ffmpeg = tool("ffmpeg").ok_or(NEED_FFMPEG)?;
    let probe = probe(src).unwrap_or_default();
    let cost = formats::cost(source, target, Some(&probe));
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-nostdin".into(), "-y".into()];
    if let Some((start, len)) = sample {
        args.extend(["-ss".into(), format!("{start:.2}"), "-t".into(), format!("{len:.2}")]);
    }
    args.extend(["-i".into(), src.display().to_string()]);
    let mut a = |xs: &[&str]| args.extend(xs.iter().map(|s| s.to_string()));
    let small = opts.smaller;
    let mut re_encoded = true;
    let set = opts.settings;
    if !set.keep_metadata {
        a(&["-map_metadata", "-1"]);
    } else if !set.keep_location {
        // QuickTime and MP4 keep GPS as plain metadata keys.
        a(&["-metadata", "location=", "-metadata", "location-eng=", "-metadata", "com.apple.quicktime.location.ISO6709="]);
    }
    match formats::kind_of_ext(target).ok_or("can't become that type")? {
        Kind::Video if formats::can_remux(target, &probe) && !small => {
            a(&["-map", "0:v:0", "-map", "0:a?", "-c", "copy"]);
            if matches!(target, "mp4" | "mov" | "m4v") {
                a(&["-movflags", "+faststart"]);
            }
            re_encoded = false;
        }
        Kind::Video => {
            let scale = if small { long_edge(1280) } else { "scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string() };
            match target {
                "webm" => a(&["-c:v", "libvpx-vp9", "-crf", if small { "40" } else { "32" }, "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "4", "-c:a", "libopus", "-b:a", if small { "96k" } else { "160k" }]),
                "avi" => a(&["-c:v", "mpeg4", "-q:v", if small { "6" } else { "2" }, "-c:a", "libmp3lame", "-q:a", "2"]),
                "wmv" => a(&["-c:v", "wmv2", "-q:v", if small { "8" } else { "3" }, "-c:a", "wmav2", "-b:a", "192k"]),
                _ => {
                    if set.hardware_video && hardware_h264() {
                        // VideoToolbox: several times faster than x264, constant-quality mode on Apple silicon.
                        a(&["-c:v", "h264_videotoolbox", "-q:v", if small { "48" } else { "68" }, "-pix_fmt", "yuv420p"]);
                    } else {
                        a(&["-c:v", "libx264", "-crf", if small { "28" } else { "18" }, "-preset", "veryfast", "-pix_fmt", "yuv420p"]);
                    }
                    a(&["-c:a", "aac", "-b:a", if small { "128k" } else { "192k" }]);
                    if target != "mkv" {
                        a(&["-movflags", "+faststart"]);
                    }
                }
            }
            a(&["-map", "0:v:0", "-map", "0:a?", "-vf", &scale]);
        }
        Kind::Image if target == "gif" => {
            let w = if small { "480" } else { "640" };
            let fps = if small { "12" } else { "15" };
            let graph = format!("fps={fps},scale='min({w},iw)':-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a");
            a(&["-filter_complex", &graph, "-loop", "0"]);
        }
        Kind::Audio => {
            a(&["-vn", "-map", "0:a:0"]);
            if set.keep_metadata {
                a(&["-map_metadata", "0"]);
            }
            let copy = cost == formats::Cost::Same && matches!(target, "m4a" | "mp3" | "opus") && !small;
            re_encoded = !(copy || cost == formats::Cost::Same);
            match target {
                _ if copy => a(&["-c:a", "copy"]),
                "mp3" => a(&["-c:a", "libmp3lame", "-q:a", if small { "6" } else { "2" }]),
                "m4a" => a(&["-c:a", "aac", "-b:a", if small { "128k" } else { "256k" }]),
                "wav" => a(&["-c:a", "pcm_s16le"]),
                "aiff" => a(&["-c:a", "pcm_s16be"]),
                "flac" => a(&["-c:a", "flac"]),
                // Vorbis when the build has it; otherwise Opus, which Ogg also carries.
                "ogg" if has_encoder("libvorbis") => a(&["-c:a", "libvorbis", "-q:a", if small { "3" } else { "6" }]),
                "ogg" => a(&["-c:a", "libopus", "-b:a", if small { "64k" } else { "160k" }]),
                "opus" => a(&["-c:a", "libopus", "-b:a", if small { "64k" } else { "160k" }]),
                _ => {}
            }
        }
        _ => return Err("can't become that type".into()),
    }
    args.extend(["-progress".into(), "pipe:1".into(), "-nostats".into(), part.display().to_string()]);
    run_ffmpeg(&ffmpeg, &args, probe.duration.unwrap_or(0.0), cancel, report)?;
    Ok(re_encoded)
}

fn run_ffmpeg(ffmpeg: &Path, args: &[String], duration: f64, cancel: &AtomicBool, report: &dyn Fn(f32)) -> Result<(), String> {
    let mut child: Child = quiet(&mut Command::new(ffmpeg))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Drain stderr on its own thread so a chatty ffmpeg cannot block on a full pipe.
    let stderr = child.stderr.take().unwrap();
    let err_tail = std::thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            tail.push(line);
            if tail.len() > 6 {
                tail.remove(0);
            }
        }
        tail
    });
    let stdout = child.stdout.take().unwrap();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break;
        }
        if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
            if duration > 0.0 {
                report((us / 1e6 / duration) as f32);
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let tail = err_tail.join().unwrap_or_default();
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    if !status.success() {
        return Err(friendly_ffmpeg_error(&tail));
    }
    Ok(())
}

fn friendly_ffmpeg_error(tail: &[String]) -> String {
    let joined = tail.join("\n");
    if joined.contains("Invalid data found") || joined.contains("moov atom not found") || joined.contains("does not contain any stream") {
        "couldn't be read".into()
    } else if joined.contains("matches no streams") {
        "has no audio to keep".into()
    } else if joined.contains("Unknown encoder") {
        "needs an ffmpeg build with more encoders".into()
    } else {
        tail.last().cloned().unwrap_or_else(|| "failed".into())
    }
}

fn ffmpeg_simple(input: &Path, extra: &[&str], out: &Path) -> Result<(), String> {
    let ffmpeg = tool("ffmpeg").ok_or(NEED_FFMPEG)?;
    let o = quiet(&mut Command::new(ffmpeg)).args(["-v", "error", "-nostdin", "-y", "-i"]).arg(input).args(extra).arg(out).output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err("couldn't be read".into()) }
}

// ---------------------------------------------------------------- subtitles

fn convert_subtitle(src: &Path, part: &Path) -> Result<bool, String> {
    ffmpeg_simple(src, &[], part)?;
    Ok(false)
}

// ---------------------------------------------------------------- images

/// What a photo carries besides pixels: its colour profile and its EXIF block
/// (raw TIFF bytes, orientation already applied to the pixels and reset to 1).
#[derive(Default, Clone)]
pub struct Meta {
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
}

/// Decode any supported still into memory, upright.
pub fn load_image(src: &Path, source: &str) -> Result<image::DynamicImage, String> {
    load_image_meta(src, source).map(|(i, _)| i)
}

pub fn load_image_meta(src: &Path, source: &str) -> Result<(image::DynamicImage, Meta), String> {
    let via_png = |f: &dyn Fn(&Path) -> Result<(), String>| {
        let tmp = temp("decode.png");
        f(&tmp)?;
        let r = read_with_meta(&tmp);
        let _ = std::fs::remove_file(&tmp);
        r
    };
    match source {
        "heic" if cfg!(target_os = "macos") => via_png(&|t| sips(src, "png", t)),
        "heic" | "avif" => via_png(&|t| ffmpeg_simple(src, &["-frames:v", "1"], t)),
        s if formats::is_raw(s) => via_png(&|t| sips(src, "png", t)),
        "svg" => render_svg(src).map(|i| (i, Meta::default())),
        "psd" => {
            let bytes = std::fs::read(src).map_err(|e| e.to_string())?;
            let psd = psd::Psd::from_bytes(&bytes).map_err(|_| "couldn't be read".to_string())?;
            let img = image::RgbaImage::from_raw(psd.width(), psd.height(), psd.rgba()).map(image::DynamicImage::ImageRgba8).ok_or("couldn't be read")?;
            Ok((img, Meta::default()))
        }
        _ => read_with_meta(src),
    }
}

fn read_with_meta(path: &Path) -> Result<(image::DynamicImage, Meta), String> {
    use image::ImageDecoder;
    let reader = image::ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|_| "couldn't be read".to_string())?;
    let icc = decoder.icc_profile().ok().flatten();
    let mut exif = decoder.exif_metadata().ok().flatten();
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(|_| "couldn't be read".to_string())?;
    img.apply_orientation(orientation);
    if let Some(e) = exif.as_mut() {
        // The pixels are upright now; don't let viewers rotate them again.
        let _ = image::metadata::Orientation::remove_from_exif_chunk(e);
    }
    Ok((img, Meta { icc, exif }))
}

fn sips(src: &Path, format: &str, out: &Path) -> Result<(), String> {
    sips_with(src, format, out, &[])
}

fn sips_with(src: &Path, format: &str, out: &Path, extra: &[&str]) -> Result<(), String> {
    let ok = Command::new("sips").args(["-s", "format", format]).args(extra).arg(src).arg("--out").arg(out).output().map(|o| o.status.success()).unwrap_or(false);
    if ok { Ok(()) } else { Err("couldn't be read".into()) }
}

fn render_svg(src: &Path) -> Result<image::DynamicImage, String> {
    use resvg::{tiny_skia, usvg};
    let data = std::fs::read(src).map_err(|e| e.to_string())?;
    let mut opt = usvg::Options { resources_dir: src.parent().map(Path::to_path_buf), ..Default::default() };
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(&data, &opt).map_err(|_| "couldn't be read".to_string())?;
    let size = tree.size();
    // Icons are often tiny; render with at least 1024px on the long edge.
    let long = size.width().max(size.height()).max(1.0);
    let scale = (1024.0 / long).clamp(1.0, 256.0);
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("is too large")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let mut rgba = image::RgbaImage::new(pixmap.width(), pixmap.height());
    for (dst, src) in rgba.pixels_mut().zip(pixmap.pixels()) {
        let c = src.demultiply();
        *dst = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(image::DynamicImage::ImageRgba8(rgba))
}

fn shrink(img: image::DynamicImage, max: u32) -> image::DynamicImage {
    if img.width().max(img.height()) > max { img.resize(max, max, image::imageops::FilterType::Lanczos3) } else { img }
}

/// Photo to photo on macOS through ImageIO (`sips`), which keeps the colour
/// profile and all EXIF, then scrubs GPS if the settings say so. Used when both
/// ends are photo formats ImageIO writes; returns false when it doesn't apply.
fn convert_photo_with_imageio(src: &Path, source: &str, part: &Path, target: &str, opts: Opts) -> Result<bool, String> {
    if !cfg!(target_os = "macos") || !opts.settings.keep_metadata {
        return Ok(false);
    }
    let photo_source = matches!(source, "jpg" | "heic" | "tiff") || formats::is_raw(source);
    let format = match target {
        "jpg" => "jpeg",
        "heic" => "heic",
        "tiff" => "tiff",
        "png" => "png",
        _ => return Ok(false),
    };
    if !photo_source {
        return Ok(false);
    }
    let strip = !opts.settings.keep_location;
    // HEIC can't be scrubbed after writing, so scrub a copy of the source first.
    let mut input = src.to_path_buf();
    let mut tmp_src = None;
    if strip && target == "heic" {
        let t = temp(&format!("src.{source}"));
        std::fs::copy(src, &t).map_err(|e| e.to_string())?;
        if !metadata::strip_gps_file(&t)? {
            // A format we can't scrub in place (CR3, RAF): use the plain pipeline.
            let _ = std::fs::remove_file(&t);
            return Ok(false);
        }
        input = t.clone();
        tmp_src = Some(t);
    }
    let quality = if opts.smaller { "78" } else { "92" };
    let mut extra: Vec<&str> = Vec::new();
    if matches!(target, "jpg" | "heic") {
        extra.extend(["-s", "formatOptions", quality]);
    }
    if opts.smaller {
        extra.extend(["-Z", "2048"]);
    }
    let r = sips_with(&input, format, part, &extra);
    if let Some(t) = tmp_src {
        let _ = std::fs::remove_file(t);
    }
    r?;
    if strip && target != "heic" {
        metadata::strip_gps_file(part)?;
    }
    Ok(true)
}

fn write_jpeg(img: &image::DynamicImage, path: &Path, quality: u8, meta: &Meta) -> Result<(), String> {
    use image::ImageEncoder;
    let flat = flatten_on_white(img);
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(f), quality);
    if let Some(icc) = &meta.icc {
        let _ = enc.set_icc_profile(icc.clone());
    }
    if let Some(exif) = &meta.exif {
        let _ = enc.set_exif_metadata(exif.clone());
    }
    enc.write_image(flat.as_raw(), flat.width(), flat.height(), image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())
}

/// Encode with the colour profile, and EXIF where the format holds it.
fn write_with_meta(img: &image::DynamicImage, path: &Path, fmt: image::ImageFormat, meta: &Meta) -> Result<(), String> {
    use image::ImageEncoder;
    let f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    let err = |e: image::ImageError| e.to_string();
    macro_rules! go {
        ($enc:expr, $img:expr) => {{
            let mut enc = $enc;
            if let Some(icc) = &meta.icc {
                let _ = enc.set_icc_profile(icc.clone());
            }
            if let Some(exif) = &meta.exif {
                let _ = enc.set_exif_metadata(exif.clone());
            }
            $img.write_with_encoder(enc).map_err(err)
        }};
    }
    match fmt {
        image::ImageFormat::Png => go!(image::codecs::png::PngEncoder::new(f), img),
        image::ImageFormat::WebP => go!(image::codecs::webp::WebPEncoder::new_lossless(f), image::DynamicImage::ImageRgba8(img.to_rgba8())),
        image::ImageFormat::Tiff => {
            drop(f);
            let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
            go!(image::codecs::tiff::TiffEncoder::new(std::io::BufWriter::new(file)), img)
        }
        other => {
            drop(f);
            img.save_with_format(path, other).map_err(err)
        }
    }
}

fn convert_image(src: &Path, source: &str, part: &Path, target: &str, opts: Opts) -> Result<bool, String> {
    if convert_photo_with_imageio(src, source, part, target, opts)? {
        return Ok(true);
    }
    let (mut img, mut meta) = load_image_meta(src, source)?;
    if !opts.settings.keep_metadata {
        meta.exif = None;
    } else if !opts.settings.keep_location {
        if let Some(e) = meta.exif.as_mut() {
            metadata::strip_gps(e);
        }
    }
    if opts.smaller {
        img = shrink(img, 2048);
    }
    let write = |img: &image::DynamicImage, path: &Path, fmt: image::ImageFormat| write_with_meta(img, path, fmt, &meta);
    let rgba = |img: &image::DynamicImage| image::DynamicImage::ImageRgba8(img.to_rgba8());
    match target {
        "jpg" => write_jpeg(&img, part, if opts.smaller { 78 } else { 92 }, &meta)?,
        "png" => write(&img, part, image::ImageFormat::Png)?,
        "webp" => write(&img, part, image::ImageFormat::WebP)?,
        "tiff" => write(&img, part, image::ImageFormat::Tiff)?,
        "bmp" => write(&rgba(&img), part, image::ImageFormat::Bmp)?,
        "gif" => write(&rgba(&img), part, image::ImageFormat::Gif)?,
        "ico" => write(&shrink(img, 256), part, image::ImageFormat::Ico)?,
        "avif" => {
            // AV1 stills through ffmpeg; flatten alpha so transparent areas stay white.
            let tmp = temp("avif-src.png");
            write(&image::DynamicImage::ImageRgb8(flatten_on_white(&img)), &tmp, image::ImageFormat::Png)?;
            let crf = if opts.smaller { "38" } else { "26" };
            let r = ffmpeg_simple(&tmp, &["-frames:v", "1", "-c:v", "libsvtav1", "-crf", crf, "-pix_fmt", "yuv420p", "-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "-f", "avif"], part);
            let _ = std::fs::remove_file(&tmp);
            r.map_err(|_| "couldn't be saved as AVIF".to_string())?;
        }
        "heic" => {
            let tmp = temp("heic-src.png");
            write(&img, &tmp, image::ImageFormat::Png)?;
            let r = sips(&tmp, "heic", part);
            let _ = std::fs::remove_file(&tmp);
            r.map_err(|_| "couldn't be saved as HEIC".to_string())?;
        }
        _ => return Err("can't become that type".into()),
    }
    Ok(opts.smaller || formats::cost(source, target, None) != formats::Cost::Same)
}

fn flatten_on_white(img: &image::DynamicImage) -> image::RgbImage {
    let rgba = img.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = p[3] as u32;
        let mix = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])])
    })
}

// ---------------------------------------------------------------- PDF

/// One page per image, in the order given. Pages keep the image's aspect with
/// the long edge at A4 height (842pt).
fn images_to_pdf(srcs: &[PathBuf], part: &Path, opts: Opts, report: &dyn Fn(f32)) -> Result<bool, String> {
    let mut pages: Vec<(Vec<u8>, u32, u32)> = Vec::new();
    for (i, src) in srcs.iter().enumerate() {
        let ext = formats::ext_of(src).unwrap_or_default();
        let mut img = load_image(src, &ext)?;
        img = shrink(img, if opts.smaller { 1600 } else { 3000 });
        let flat = flatten_on_white(&img);
        let mut jpg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, if opts.smaller { 75 } else { 90 }).encode_image(&flat).map_err(|e| e.to_string())?;
        pages.push((jpg, flat.width(), flat.height()));
        report((i + 1) as f32 / srcs.len() as f32);
    }
    let mut pdf: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let n = pages.len();
    // Objects: 1 catalog, 2 pages, then per page: page, contents, image.
    let mut obj = |pdf: &mut Vec<u8>, body: &[u8]| {
        offsets.push(pdf.len());
        let id = offsets.len();
        pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut pdf, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: String = (0..n).map(|i| format!("{} 0 R ", 3 + i * 3)).collect();
    obj(&mut pdf, format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>").as_bytes());
    for (i, (jpg, w, h)) in pages.iter().enumerate() {
        let (pid, cid, iid) = (3 + i * 3, 4 + i * 3, 5 + i * 3);
        let k = 842.0 / (*w.max(h) as f64);
        let (pw, ph) = (*w as f64 * k, *h as f64 * k);
        let _ = pid;
        obj(&mut pdf, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 {iid} 0 R >> >> /Contents {cid} 0 R >>").as_bytes());
        let content = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im0 Do Q");
        obj(&mut pdf, format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes());
        let mut body = format!("<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", jpg.len()).into_bytes();
        body.extend_from_slice(jpg);
        body.extend_from_slice(b"\nendstream");
        obj(&mut pdf, &body);
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    std::fs::File::create(part).and_then(|mut f| f.write_all(&pdf)).map_err(|e| e.to_string())?;
    Ok(true)
}

// ---------------------------------------------------------------- data

fn convert_data(src: &Path, source: &str, part: &Path, target: &str) -> Result<bool, String> {
    let text = std::fs::read_to_string(src).map_err(|_| "isn't readable text".to_string())?;
    let bad = |what: &str| format!("isn't valid {what}");
    let value: serde_json::Value = match source {
        "json" => serde_json::from_str(&text).map_err(|_| bad("JSON"))?,
        "yaml" => serde_yaml_ng::from_str(&text).map_err(|_| bad("YAML"))?,
        "toml" => {
            let t: toml::Value = toml::from_str(&text).map_err(|_| bad("TOML"))?;
            serde_json::to_value(t).map_err(|e| e.to_string())?
        }
        "csv" => csv_to_value(&text)?,
        _ => return Err("can't become that type".into()),
    };
    let out = match target {
        "json" => serde_json::to_string_pretty(&value).map_err(|e| e.to_string())? + "\n",
        "yaml" => serde_yaml_ng::to_string(&value).map_err(|e| e.to_string())?,
        "toml" => {
            if !value.is_object() {
                return Err("needs a top-level object to become TOML".into());
            }
            toml::to_string_pretty(&value).map_err(|_| "has values TOML can't hold (like null)".to_string())?
        }
        "csv" => value_to_csv(&value)?,
        _ => return Err("can't become that type".into()),
    };
    std::fs::write(part, out).map_err(|e| e.to_string())?;
    Ok(false)
}

fn csv_to_value(text: &str) -> Result<serde_json::Value, String> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let headers = rdr.headers().map_err(|_| "isn't valid CSV".to_string())?.clone();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|_| "isn't valid CSV".to_string())?;
        let mut obj = serde_json::Map::new();
        for (h, v) in headers.iter().zip(rec.iter()) {
            let val = if let Ok(i) = v.parse::<i64>() {
                serde_json::Value::from(i)
            } else if let Ok(f) = v.parse::<f64>().map_err(|_| ()).and_then(|f| if f.is_finite() { Ok(f) } else { Err(()) }) {
                serde_json::Value::from(f)
            } else if v == "true" || v == "false" {
                serde_json::Value::Bool(v == "true")
            } else {
                serde_json::Value::String(v.to_string())
            };
            obj.insert(h.to_string(), val);
        }
        rows.push(serde_json::Value::Object(obj));
    }
    Ok(serde_json::Value::Array(rows))
}

fn value_to_csv(value: &serde_json::Value) -> Result<String, String> {
    let rows = value.as_array().ok_or("needs to be a list of records to become CSV")?;
    let mut headers: Vec<String> = Vec::new();
    for r in rows {
        let o = r.as_object().ok_or("needs to be a list of records to become CSV")?;
        for k in o.keys() {
            if !headers.contains(k) {
                headers.push(k.clone());
            }
        }
    }
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(&headers).map_err(|e| e.to_string())?;
    for r in rows {
        let o = r.as_object().unwrap();
        let cells: Vec<String> = headers
            .iter()
            .map(|h| match o.get(h) {
                None | Some(serde_json::Value::Null) => String::new(),
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v @ (serde_json::Value::Array(_) | serde_json::Value::Object(_))) => v.to_string(),
                Some(v) => v.to_string(),
            })
            .collect();
        w.write_record(&cells).map_err(|e| e.to_string())?;
    }
    String::from_utf8(w.into_inner().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- thumbnails

/// Small RGBA thumbnail for the UI and the drag image. None when the file has no picture.
pub fn thumbnail(path: &Path, max: u32) -> Option<image::RgbaImage> {
    let ext = formats::ext_of(path)?;
    match formats::kind_of_ext(&ext)? {
        Kind::Image => load_image(path, &ext).ok().map(|i| i.thumbnail(max, max).to_rgba8()),
        Kind::Video => {
            let tmp = temp("thumb.png");
            let scale = format!("scale='min({max},iw)':-2");
            let ok = tool("ffmpeg").is_some_and(|ff| {
                quiet(&mut Command::new(ff)).args(["-v", "error", "-y", "-ss", "0.5", "-i"]).arg(path).args(["-frames:v", "1", "-vf", &scale]).arg(&tmp).output().map(|o| o.status.success()).unwrap_or(false)
            });
            let img = ok.then(|| image::open(&tmp).ok()).flatten().map(|i| i.thumbnail(max, max).to_rgba8());
            let _ = std::fs::remove_file(&tmp);
            img
        }
        _ => None,
    }
}

pub fn human_size(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 { format!("{bytes} B") } else if v < 10.0 { format!("{v:.1} {}", U[i]) } else { format!("{v:.0} {}", U[i]) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn fixtures() -> PathBuf {
        let d = std::env::temp_dir().join("retype-fixtures-2");
        std::fs::create_dir_all(&d).unwrap();
        let ff = tool("ffmpeg").expect("ffmpeg for tests");
        let make = |name: &str, args: &[&str]| {
            let out = d.join(name);
            if !out.exists() {
                let ok = Command::new(&ff).args(["-v", "error", "-y"]).args(args).arg(&out).status().unwrap().success();
                assert!(ok, "fixture {name}");
            }
        };
        make("clip.mp4", &["-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=2", "-f", "lavfi", "-i", "sine=duration=2", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"]);
        make("clip.ts", &["-i", d.join("clip.mp4").to_str().unwrap(), "-c", "copy"]);
        make("clip.wmv", &["-i", d.join("clip.mp4").to_str().unwrap(), "-c:v", "wmv2", "-c:a", "wmav2"]);
        make("tone.wav", &["-f", "lavfi", "-i", "sine=duration=1"]);
        make("tone.opus", &["-f", "lavfi", "-i", "sine=duration=1", "-c:a", "libopus"]);
        make("pic.png", &["-f", "lavfi", "-i", "color=c=red@0.5:size=64x48,format=rgba", "-frames:v", "1"]);
        make("big.png", &["-f", "lavfi", "-i", "testsrc=size=3000x2000", "-frames:v", "1"]);
        let write = |name: &str, body: &str| {
            let p = d.join(name);
            if !p.exists() {
                std::fs::write(p, body).unwrap();
            }
        };
        write("icon.svg", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10"><rect width="10" height="10" fill="#ffe14a"/></svg>"##);
        write("subs.srt", "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n");
        write("data.json", r#"[{"name":"a","n":1},{"name":"b","n":2,"extra":true}]"#);
        write("conf.json", r#"{"title":"Retype","window":{"w":500,"h":380}}"#);
        write("rows.csv", "name,n\na,1\nb,2.5\n");
        d
    }

    fn run_opts(src: &str, target: &str, opts: Opts) -> (PathBuf, bool) {
        let d = fixtures();
        let src = d.join(src);
        let (out, _) = output_path(&src, target);
        let (re, out) = convert(&[src], &out, target, opts, &AtomicBool::new(false), &|_| {}).unwrap_or_else(|e| panic!("{target}: {e}"));
        assert!(out.exists() && std::fs::metadata(&out).unwrap().len() > 0, "{out:?}");
        (out, re)
    }

    fn run(src: &str, target: &str) -> (PathBuf, bool) {
        run_opts(src, target, Opts::default())
    }

    fn done(p: PathBuf) {
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn remux_mp4_to_mov_is_lossless() {
        let (out, re) = run("clip.mp4", "mov");
        assert!(!re, "h264/aac into mov should remux");
        assert_eq!(probe(&out).unwrap().video.as_deref(), Some("h264"));
        done(out);
    }

    #[test]
    fn webm_reencodes() {
        let (out, re) = run("clip.mp4", "webm");
        assert!(re);
        assert_eq!(probe(&out).unwrap().video.as_deref(), Some("vp9"));
        done(out);
    }

    #[test]
    fn smaller_video_is_smaller() {
        let (a, _) = run("clip.mp4", "webm");
        let (b, re) = run_opts("clip.mp4", "webm", Opts { smaller: true, ..Opts::default() });
        assert!(re);
        assert!(std::fs::metadata(&b).unwrap().len() < std::fs::metadata(&a).unwrap().len());
        done(a);
        done(b);
    }

    #[test]
    fn more_video_and_audio() {
        for (s, t) in [("clip.mp4", "gif"), ("clip.mp4", "mp3"), ("clip.mp4", "wmv"), ("clip.ts", "mp4"), ("clip.wmv", "mp4"), ("tone.opus", "mp3"), ("tone.wav", "opus"), ("tone.wav", "aiff"), ("tone.wav", "flac")] {
            done(run(s, t).0);
        }
    }

    #[test]
    fn images() {
        for (s, t) in [("pic.png", "jpg"), ("pic.png", "webp"), ("pic.png", "ico"), ("pic.png", "tiff"), ("pic.png", "avif"), ("icon.svg", "png")] {
            done(run(s, t).0);
        }
    }

    #[test]
    fn avif_round_trip() {
        let (avif, _) = run("big.png", "avif");
        let img = load_image(&avif, "avif").unwrap();
        assert_eq!((img.width(), img.height()), (3000, 2000));
        done(avif);
    }

    #[test]
    fn svg_renders_large() {
        let img = load_image(&fixtures().join("icon.svg"), "svg").unwrap();
        assert_eq!(img.width(), 1024);
    }

    #[test]
    fn smaller_image_caps_long_edge() {
        let (out, _) = run_opts("big.png", "jpg", Opts { smaller: true, ..Opts::default() });
        assert_eq!(image::open(&out).unwrap().width(), 2048);
        done(out);
    }

    #[test]
    fn images_combine_into_one_pdf() {
        let d = fixtures();
        let srcs = vec![d.join("pic.png"), d.join("big.png")];
        let (out, _) = output_path(&srcs[0], "pdf");
        let (_, out) = convert(&srcs, &out, "pdf", Opts::default(), &AtomicBool::new(false), &|_| {}).unwrap();
        let pdf = std::fs::read(&out).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert_eq!(String::from_utf8_lossy(&pdf).matches("/Type /Page ").count(), 2);
        #[cfg(target_os = "macos")]
        {
            // ImageIO must be able to open it.
            let png = temp("pdfcheck.png");
            sips(&out, "png", &png).expect("macOS reads the PDF");
            let _ = std::fs::remove_file(png);
        }
        done(out);
    }

    #[test]
    fn subtitles() {
        let (out, _) = run("subs.srt", "vtt");
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.starts_with("WEBVTT") && text.contains("Hello"));
        done(out);
    }

    #[test]
    fn data_formats() {
        let (y, _) = run("conf.json", "yaml");
        assert!(std::fs::read_to_string(&y).unwrap().contains("title: Retype"));
        let (t, _) = run("conf.json", "toml");
        assert!(std::fs::read_to_string(&t).unwrap().contains("[window]"));
        let (c, _) = run("data.json", "csv");
        assert_eq!(std::fs::read_to_string(&c).unwrap().lines().next(), Some("name,n,extra"));
        let (j, _) = run("rows.csv", "json");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&j).unwrap()).unwrap();
        assert_eq!(v[1]["n"], serde_json::json!(2.5));
        for p in [y, t, c, j] {
            done(p);
        }
        let d = fixtures();
        let (out, _) = output_path(&d.join("data.json"), "toml");
        assert!(convert(&[d.join("data.json")], &out, "toml", Opts::default(), &AtomicBool::new(false), &|_| {}).is_err());
        assert!(!out.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn heic_round_trip() {
        let (heic, _) = run("pic.png", "heic");
        let (out, _) = output_path(&heic, "jpg");
        let (_, out) = convert(&[heic.clone()], &out, "jpg", Opts::default(), &AtomicBool::new(false), &|_| {}).unwrap();
        done(out);
        done(heic);
    }

    #[test]
    fn replace_trashes_the_original() {
        let d = fixtures();
        let src = d.join("replace-me.wav");
        std::fs::copy(d.join("tone.wav"), &src).unwrap();
        let job = Job::start(vec![Task { srcs: vec![src.clone()], target: "mp3".into(), opts: Opts { replace: true, ..Opts::default() } }], || {});
        while !job.snapshot().done {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let s = job.snapshot();
        assert_eq!(s.outputs.len(), 1);
        assert!(!src.exists(), "original should be in the Trash");
        assert_eq!(s.trashed, vec![src]);
        done(s.outputs[0].clone());
    }

    #[test]
    fn job_accepts_tasks_while_running() {
        let d = fixtures();
        let job = Job::start(vec![Task { srcs: vec![d.join("tone.wav")], target: "ogg".into(), opts: Opts::default() }], || {});
        job.push(vec![Task { srcs: vec![d.join("pic.png")], target: "bmp".into(), opts: Opts::default() }]);
        while !job.snapshot().done {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let s = job.snapshot();
        assert_eq!((s.total, s.outputs.len()), (2, 2), "{:?}", s.failures);
        assert!((s.progress - 1.0).abs() < 1e-6);
        s.outputs.into_iter().for_each(done);
    }

    #[test]
    fn never_overwrites() {
        let src = fixtures().join("tone.wav");
        let (a, _) = output_path(&src, "wav");
        assert!(a.file_name().unwrap().to_str().unwrap().contains('2'));
    }

    #[test]
    fn png_alpha_flattens_to_white_jpg() {
        let (out, _) = run("pic.png", "jpg");
        let px = image::open(&out).unwrap().to_rgb8().get_pixel(5, 5).0;
        assert!(px[0] > 240 && px[1] > 100 && px[1] < 160, "half-red on white, got {px:?}");
        done(out);
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use crate::prefs::Settings;
    use image::ImageDecoder;
    use std::sync::atomic::AtomicBool;

    fn p3() -> Vec<u8> {
        std::fs::read("/System/Library/ColorSync/Profiles/Display P3.icc").unwrap_or_else(|_| b"fake-icc-profile-bytes".to_vec())
    }

    /// A JPEG with a P3 profile and EXIF holding a date, a camera, and GPS.
    fn photo(dir: &Path, name: &str) -> PathBuf {
        use image::ImageEncoder;
        let path = dir.join(name);
        let img = image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 128]));
        let f = std::fs::File::create(&path).unwrap();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(f, 90);
        enc.set_icc_profile(p3()).unwrap();
        enc.set_exif_metadata(crate::metadata::tests::sample_exif()).unwrap();
        enc.write_image(img.as_raw(), 64, 48, image::ExtendedColorType::Rgb8).unwrap();
        path
    }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("retype-meta-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn conv(src: &Path, target: &str, settings: Settings) -> PathBuf {
        let (out, _) = output_path(src, target);
        convert(&[src.to_path_buf()], &out, target, Opts { settings, ..Opts::default() }, &AtomicBool::new(false), &|_| {}).unwrap_or_else(|e| panic!("{target}: {e}")).1
    }

    fn meta_of(path: &Path) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        let mut d = image::ImageReader::open(path).unwrap().with_guessed_format().unwrap().into_decoder().unwrap();
        (d.icc_profile().ok().flatten(), d.exif_metadata().ok().flatten())
    }

    fn date_in(bytes: &[u8]) -> bool {
        bytes.windows(19).any(|w| w == b"2026:11:04 21:00:00")
    }

    #[test]
    fn webp_keeps_profile_and_date_drops_location() {
        let d = dir("webp");
        let out = conv(&photo(&d, "a.jpg"), "webp", Settings::default());
        let (icc, exif) = meta_of(&out);
        assert_eq!(icc.as_deref(), Some(&p3()[..]), "colour profile carried");
        let exif = exif.expect("exif carried");
        assert!(date_in(&exif) && !crate::metadata::has_gps(&exif));
    }

    #[test]
    fn keep_location_setting_keeps_gps() {
        let d = dir("keepgps");
        let out = conv(&photo(&d, "a.jpg"), "webp", Settings { keep_location: true, ..Settings::default() });
        assert!(crate::metadata::has_gps(&meta_of(&out).1.unwrap()));
    }

    #[test]
    fn metadata_off_drops_exif_but_keeps_colour() {
        let d = dir("nometa");
        let out = conv(&photo(&d, "a.jpg"), "png", Settings { keep_metadata: false, ..Settings::default() });
        let (icc, exif) = meta_of(&out);
        assert!(icc.is_some(), "colour is not metadata to drop");
        assert!(exif.is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn imageio_path_keeps_everything_but_location() {
        let d = dir("imageio");
        let src = photo(&d, "a.jpg");
        let heic = conv(&src, "heic", Settings::default());
        // Read the HEIC back through ImageIO into a JPEG and inspect that.
        let back = conv(&heic, "jpg", Settings { keep_location: true, ..Settings::default() });
        let bytes = std::fs::read(&back).unwrap();
        assert!(date_in(&bytes), "capture date survives jpg → heic → jpg");
        assert!(!crate::metadata::file_has_gps(&back), "location was removed on the way in");
        assert!(meta_of(&back).0.is_some(), "colour profile survives");
        let tiff = conv(&src, "tiff", Settings::default());
        assert!(!crate::metadata::file_has_gps(&tiff) && date_in(&std::fs::read(&tiff).unwrap()));
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn hardware_video_encodes() {
        if !hardware_h264() {
            return;
        }
        let d = dir("vt");
        let src = d.join("v.webm");
        let ff = tool("ffmpeg").unwrap();
        assert!(Command::new(&ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=1", "-c:v", "libvpx-vp9"]).arg(&src).status().unwrap().success());
        let out = conv(&src, "mp4", Settings::default());
        assert_eq!(probe(&out).unwrap().video.as_deref(), Some("h264"));
    }

    #[test]
    fn sweep_removes_stale_temp_files() {
        let d = dir("sweep");
        let stale = d.join(".x.retype-part-1-1.mp4");
        std::fs::write(&stale, b"half").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(&stale).unwrap().set_modified(old).unwrap();
        parts::register(&stale);
        let fresh = d.join(".y.retype-part-1-2.mp4");
        std::fs::write(&fresh, b"live").unwrap();
        parts::register(&fresh);
        parts::sweep();
        assert!(!stale.exists() && fresh.exists());
        parts::unregister(&fresh);
    }

    #[test]
    fn save_to_downloads_setting() {
        let src = std::env::temp_dir().join("x.wav");
        let (out, _) = output_path_in(&src, "mp3", true);
        if let Some(dl) = dirs::download_dir() {
            assert_eq!(out.parent(), Some(dl.as_path()));
        }
    }
}

#[cfg(test)]
mod estimate_tests {
    use super::*;

    #[test]
    fn estimates_are_in_the_right_range() {
        let d = std::env::temp_dir().join(format!("retype-est-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let src = d.join("e.mp4");
        let ff = tool("ffmpeg").unwrap();
        assert!(Command::new(&ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25:duration=8", "-c:v", "libx264", "-pix_fmt", "yuv420p"]).arg(&src).status().unwrap().success());
        let opts = Opts { smaller: true, ..Opts::default() };
        let est = estimate(&src, "mkv", opts).unwrap() as f64;
        let (out, _) = output_path(&src, "mkv");
        let (_, out) = convert(&[src.clone()], &out, "mkv", opts, &AtomicBool::new(false), &|_| {}).unwrap();
        let real = std::fs::metadata(&out).unwrap().len() as f64;
        assert!(est > real * 0.4 && est < real * 2.5, "estimate {est} vs real {real}");
    }
}
