//! What each file type can become, in the order Retype offers it.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Video,
    Audio,
    Image,
    Subtitle,
    Data,
    /// Output only: images combine into one PDF.
    Document,
}

impl Kind {
    /// Plural noun for batch display lines ("12 photos").
    pub fn plural(self, n: usize) -> String {
        let (one, many) = match self {
            Kind::Video => ("video", "videos"),
            Kind::Audio => ("audio file", "audio files"),
            Kind::Image => ("photo", "photos"),
            Kind::Subtitle => ("subtitle file", "subtitle files"),
            Kind::Data => ("data file", "data files"),
            Kind::Document => ("document", "documents"),
        };
        if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
    }
}

pub struct Format {
    pub ext: &'static str,
    /// Uniform type identifiers, used for the macOS Quick Actions.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub utis: &'static [&'static str],
    /// Targets in offer order. The first is the default for a first-time user.
    pub targets: &'static [&'static str],
}

const V_MODERN: &[&str] = &["mp4", "mov", "mkv", "webm", "gif", "mp3", "m4a", "wav"];
const RAW_T: &[&str] = &["jpg", "png", "tiff", "heic", "webp"];

pub const FORMATS: &[Format] = &[
    // Video
    Format { ext: "mp4", utis: &["public.mpeg-4"], targets: &["mov", "mkv", "webm", "gif", "m4v", "avi", "wmv", "mp3", "m4a", "wav"] },
    Format { ext: "mov", utis: &["com.apple.quicktime-movie"], targets: &["mp4", "mkv", "webm", "gif", "m4v", "avi", "wmv", "mp3", "m4a", "wav"] },
    Format { ext: "mkv", utis: &["org.matroska.mkv"], targets: &["mp4", "mov", "webm", "gif", "avi", "mp3", "m4a", "wav"] },
    Format { ext: "webm", utis: &["org.webmproject.webm"], targets: &["mp4", "mov", "mkv", "gif", "mp3", "m4a", "wav"] },
    Format { ext: "m4v", utis: &["com.apple.m4v-video"], targets: &["mp4", "mov", "mkv", "webm", "gif", "mp3", "m4a"] },
    Format { ext: "avi", utis: &["public.avi"], targets: &["mp4", "mov", "mkv", "webm", "gif", "mp3", "wav"] },
    Format { ext: "wmv", utis: &["com.microsoft.windows-media-wmv"], targets: V_MODERN },
    Format { ext: "flv", utis: &["com.adobe.flash.video"], targets: V_MODERN },
    Format { ext: "mpg", utis: &["public.mpeg"], targets: V_MODERN },
    Format { ext: "ts", utis: &["public.mpeg-2-transport-stream"], targets: V_MODERN },
    Format { ext: "mts", utis: &["public.avchd-mpeg-2-transport-stream"], targets: V_MODERN },
    Format { ext: "3gp", utis: &["public.3gpp", "public.3gpp2"], targets: V_MODERN },
    Format { ext: "ogv", utis: &["org.xiph.ogv"], targets: V_MODERN },
    // Audio
    Format { ext: "wav", utis: &["com.microsoft.waveform-audio"], targets: &["mp3", "aiff", "m4a", "flac", "ogg", "opus"] },
    Format { ext: "aiff", utis: &["public.aiff-audio", "public.aifc-audio"], targets: &["wav", "mp3", "m4a", "flac", "ogg", "opus"] },
    Format { ext: "mp3", utis: &["public.mp3"], targets: &["wav", "m4a", "aiff", "flac", "ogg", "opus"] },
    Format { ext: "m4a", utis: &["com.apple.m4a-audio", "public.mpeg-4-audio"], targets: &["mp3", "wav", "aiff", "flac", "ogg", "opus"] },
    Format { ext: "m4b", utis: &["com.apple.protected-mpeg-4-audio-b", "com.audible.m4b-audio"], targets: &["mp3", "m4a", "wav"] },
    Format { ext: "aac", utis: &["public.aac-audio"], targets: &["m4a", "mp3", "wav", "flac"] },
    Format { ext: "flac", utis: &["org.xiph.flac"], targets: &["wav", "mp3", "aiff", "m4a", "ogg", "opus"] },
    Format { ext: "ogg", utis: &["org.xiph.ogg", "org.xiph.ogg-audio"], targets: &["mp3", "wav", "m4a", "flac", "aiff"] },
    Format { ext: "opus", utis: &["org.xiph.opus"], targets: &["mp3", "m4a", "wav", "ogg"] },
    Format { ext: "wma", utis: &["com.microsoft.windows-media-wma"], targets: &["mp3", "m4a", "wav", "flac"] },
    Format { ext: "caf", utis: &["com.apple.coreaudio-format"], targets: &["wav", "m4a", "mp3", "aiff"] },
    // Image
    Format { ext: "heic", utis: &["public.heic", "public.heif"], targets: &["jpg", "png", "webp", "avif", "tiff", "pdf"] },
    Format { ext: "png", utis: &["public.png"], targets: &["jpg", "webp", "avif", "heic", "pdf", "tiff", "gif", "bmp", "ico"] },
    Format { ext: "jpg", utis: &["public.jpeg"], targets: &["png", "webp", "avif", "heic", "pdf", "tiff", "gif", "bmp"] },
    Format { ext: "webp", utis: &["org.webmproject.webp"], targets: &["png", "jpg", "avif", "gif", "pdf", "tiff"] },
    Format { ext: "avif", utis: &["public.avif"], targets: &["jpg", "png", "webp", "pdf"] },
    Format { ext: "gif", utis: &["com.compuserve.gif"], targets: &["mp4", "webm", "png", "jpg", "webp"] },
    Format { ext: "tiff", utis: &["public.tiff"], targets: &["png", "jpg", "webp", "heic", "pdf"] },
    Format { ext: "bmp", utis: &["com.microsoft.bmp"], targets: &["png", "jpg", "webp", "tiff"] },
    Format { ext: "tga", utis: &["com.truevision.tga-image"], targets: &["png", "jpg", "webp", "tiff"] },
    Format { ext: "ico", utis: &["com.microsoft.ico"], targets: &["png", "jpg", "webp"] },
    Format { ext: "svg", utis: &["public.svg-image"], targets: &["png", "jpg", "webp", "pdf", "ico"] },
    Format { ext: "psd", utis: &["com.adobe.photoshop-image"], targets: &["png", "jpg", "tiff", "webp"] },
    // Camera RAW (decoded by macOS ImageIO)
    Format { ext: "dng", utis: &["com.adobe.raw-image"], targets: RAW_T },
    Format { ext: "cr2", utis: &["com.canon.cr2-raw-image"], targets: RAW_T },
    Format { ext: "cr3", utis: &["com.canon.cr3-raw-image"], targets: RAW_T },
    Format { ext: "nef", utis: &["com.nikon.raw-image"], targets: RAW_T },
    Format { ext: "arw", utis: &["com.sony.arw-raw-image"], targets: RAW_T },
    Format { ext: "raf", utis: &["com.fuji.raw-image"], targets: RAW_T },
    Format { ext: "orf", utis: &["com.olympus.raw-image", "com.olympus.or-raw-image"], targets: RAW_T },
    Format { ext: "rw2", utis: &["com.panasonic.rw2-raw-image"], targets: RAW_T },
    // Subtitles
    Format { ext: "srt", utis: &["public.subrip-text", "com.apple.subrip-text"], targets: &["vtt", "ass"] },
    Format { ext: "vtt", utis: &["org.w3.webvtt"], targets: &["srt", "ass"] },
    Format { ext: "ass", utis: &["public.ass-text"], targets: &["srt", "vtt"] },
    // Data
    Format { ext: "json", utis: &["public.json"], targets: &["yaml", "toml", "csv"] },
    Format { ext: "yaml", utis: &["public.yaml"], targets: &["json", "toml"] },
    Format { ext: "toml", utis: &["public.toml", "io.toml"], targets: &["json", "yaml"] },
    Format { ext: "csv", utis: &["public.comma-separated-values-text"], targets: &["json", "yaml"] },
];

/// Normalised lowercase extension (".JPEG" -> "jpg").
pub fn ext_of(path: &Path) -> Option<String> {
    let e = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(normalize(&e).to_string())
}

pub fn normalize(e: &str) -> &str {
    match e {
        "jpeg" | "jpe" | "jfif" => "jpg",
        "tif" => "tiff",
        "aif" | "aifc" => "aiff",
        "heif" | "hif" => "heic",
        "oga" => "ogg",
        "mpeg" | "m2v" => "mpg",
        "m2ts" => "mts",
        "3g2" => "3gp",
        "yml" => "yaml",
        "webvtt" => "vtt",
        "ssa" => "ass",
        _ => e,
    }
}

/// Other spellings of an extension Retype reads, for registering file types.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn aliases(ext: &str) -> &'static [&'static str] {
    match ext {
        "jpg" => &["jpeg", "jpe", "jfif"],
        "tiff" => &["tif"],
        "aiff" => &["aif", "aifc"],
        "heic" => &["heif", "hif"],
        "mpg" => &["mpeg"],
        "mts" => &["m2ts"],
        "yaml" => &["yml"],
        _ => &[],
    }
}

pub fn is_raw(ext: &str) -> bool {
    matches!(ext, "dng" | "cr2" | "cr3" | "nef" | "arw" | "raf" | "orf" | "rw2")
}

pub fn lookup(ext: &str) -> Option<&'static Format> {
    FORMATS.iter().find(|f| f.ext == ext).filter(|f| platform_reads(f.ext))
}

/// Sources this platform can decode.
fn platform_reads(ext: &str) -> bool {
    !is_raw(ext) || cfg!(target_os = "macos")
}

/// Targets offered for a source extension on this platform.
pub fn targets_for(ext: &str) -> Vec<&'static str> {
    let Some(f) = lookup(ext) else { return vec![] };
    f.targets
        .iter()
        .copied()
        .filter(|t| *t != "heic" || cfg!(target_os = "macos")) // HEIC encoding uses macOS ImageIO
        .collect()
}

pub fn kind_of_ext(ext: &str) -> Option<Kind> {
    Some(match ext {
        "mp4" | "mov" | "mkv" | "webm" | "m4v" | "avi" | "wmv" | "flv" | "mpg" | "ts" | "mts" | "3gp" | "ogv" => Kind::Video,
        "wav" | "aiff" | "mp3" | "m4a" | "m4b" | "aac" | "flac" | "ogg" | "opus" | "wma" | "caf" => Kind::Audio,
        "heic" | "png" | "jpg" | "webp" | "avif" | "gif" | "tiff" | "bmp" | "tga" | "ico" | "svg" | "psd" => Kind::Image,
        e if is_raw(e) => Kind::Image,
        "srt" | "vtt" | "ass" => Kind::Subtitle,
        "json" | "yaml" | "toml" | "csv" => Kind::Data,
        "pdf" => Kind::Document,
        _ => return None,
    })
}

/// How a conversion treats the data, which decides the quality tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cost {
    /// Container swap or lossless transcode. Same quality, fast.
    Same,
    /// Lossy but quick (audio, still images).
    Quick,
    /// Video re-encode. Slow.
    Slow,
}


/// Codec facts from ffprobe, used to decide remux vs re-encode.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub video: Option<String>,
    pub audio: Option<String>,
    pub duration: Option<f64>,
}

fn container_takes(target: &str, video: Option<&str>, audio: Option<&str>) -> bool {
    let v_ok = |list: &[&str]| video.is_none_or(|v| list.contains(&v));
    let a_ok = |list: &[&str]| audio.is_none_or(|a| list.contains(&a));
    match target {
        "mp4" | "m4v" => v_ok(&["h264", "hevc", "av1", "mpeg4"]) && a_ok(&["aac", "mp3", "alac", "ac3", "eac3", "opus"]),
        "mov" => v_ok(&["h264", "hevc", "av1", "mpeg4", "prores"]) && a_ok(&["aac", "mp3", "alac", "ac3", "eac3"]),
        "mkv" => true,
        "webm" => v_ok(&["vp8", "vp9", "av1"]) && a_ok(&["opus", "vorbis"]),
        "avi" => v_ok(&["mpeg4", "h264", "mjpeg"]) && a_ok(&["mp3", "pcm_s16le", "ac3"]),
        _ => false,
    }
}

/// Whether a video->video conversion can copy streams instead of re-encoding.
pub fn can_remux(target: &str, probe: &Probe) -> bool {
    probe.video.is_some() && container_takes(target, probe.video.as_deref(), probe.audio.as_deref())
}

pub fn cost(source: &str, target: &str, probe: Option<&Probe>) -> Cost {
    match (kind_of_ext(source), kind_of_ext(target)) {
        (Some(Kind::Video), Some(Kind::Video)) => match probe {
            Some(p) if can_remux(target, p) => Cost::Same,
            _ => Cost::Slow,
        },
        (Some(Kind::Image), Some(Kind::Video)) | (Some(Kind::Video), Some(Kind::Image)) => Cost::Slow,
        (_, Some(Kind::Audio)) => {
            let lossless = ["wav", "aiff", "flac"];
            let src_audio = probe.and_then(|p| p.audio.as_deref());
            let src_lossless = lossless.contains(&source) || src_audio.is_some_and(|a| a.starts_with("pcm") || a == "flac" || a == "alac");
            if lossless.contains(&target) && src_lossless
                || target == "m4a" && src_audio == Some("aac")
                || target == "mp3" && src_audio == Some("mp3")
                || target == "opus" && src_audio == Some("opus")
            {
                Cost::Same
            } else {
                Cost::Quick
            }
        }
        (Some(Kind::Image), Some(Kind::Image)) => {
            if ["png", "tiff", "bmp", "webp"].contains(&target) && !is_raw(source) { Cost::Same } else { Cost::Quick }
        }
        (Some(Kind::Subtitle | Kind::Data), _) => Cost::Same,
        _ => Cost::Quick,
    }
}
