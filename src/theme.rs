//! Retype visual system in egui terms. Source of truth: DESIGN.md.

use eframe::egui::{self, Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke, Vec2, text::LayoutJob};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const YELLOW: Color32 = Color32::from_rgb(0xff, 0xe1, 0x4a);
/// Text on the yellow fill, in both themes (14.2:1).
pub const ON_YELLOW: Color32 = Color32::from_rgb(0x11, 0x14, 0x18);

static DARK: AtomicBool = AtomicBool::new(false);

/// Follow the system appearance. Called at the top of every frame.
pub fn sync(ctx: &egui::Context) {
    DARK.store(ctx.theme() == egui::Theme::Dark, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

const fn pick(light: Color32, dark: Color32, is_dark: bool) -> Color32 {
    if is_dark { dark } else { light }
}

/// Text and the primary button fill.
pub fn ink() -> Color32 {
    pick(Color32::from_rgb(0x11, 0x14, 0x18), Color32::from_rgb(0xec, 0xee, 0xf1), is_dark())
}
pub fn ink_hover() -> Color32 {
    pick(Color32::from_rgb(0x2a, 0x2f, 0x36), Color32::from_rgb(0xd2, 0xd6, 0xdc), is_dark())
}
/// Secondary text. 4.9:1 on light paper, 6.3:1 on dark paper.
pub fn soft() -> Color32 {
    pick(Color32::from_rgb(0x6a, 0x70, 0x79), Color32::from_rgb(0x96, 0x9d, 0xa7), is_dark())
}
/// Neighbour extensions in the reel. ~3.3:1; large, decorative, and repeated by the chips.
pub fn neighbour() -> Color32 {
    pick(Color32::from_rgb(0x8a, 0x90, 0x98), Color32::from_rgb(0x6c, 0x73, 0x7d), is_dark())
}
pub fn line() -> Color32 {
    pick(Color32::from_rgb(0xe4, 0xe6, 0xea), Color32::from_rgb(0x2e, 0x32, 0x39), is_dark())
}
pub fn wash() -> Color32 {
    pick(Color32::from_rgb(0xf1, 0xf2, 0xf4), Color32::from_rgb(0x25, 0x28, 0x2e), is_dark())
}
/// The window surface.
pub fn paper() -> Color32 {
    pick(Color32::WHITE, Color32::from_rgb(0x17, 0x19, 0x1d), is_dark())
}

pub const DISPLAY: f32 = 44.0;
pub const BODY: f32 = 15.0;
pub const LABEL: f32 = 15.0;
pub const CAPTION: f32 = 13.5;

#[derive(Clone, Copy)]
pub enum W {
    Regular,
    Semibold,
    Bold,
}

fn family(w: W) -> FontFamily {
    FontFamily::Name(match w {
        W::Regular => "hk400",
        W::Semibold => "hk600",
        W::Bold => "hk700",
    }
    .into())
}

pub fn font(size: f32, w: W) -> FontId {
    FontId::new(size, family(w))
}

pub fn install(ctx: &egui::Context) {
    // Development override; the app follows the system appearance otherwise.
    match std::env::var("RETYPE_THEME").as_deref() {
        Ok("dark") => ctx.set_theme(egui::Theme::Dark),
        Ok("light") => ctx.set_theme(egui::Theme::Light),
        _ => {}
    }
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        ("hk400", &include_bytes!("../assets/fonts/HankenGrotesk-400.ttf")[..]),
        ("hk600", &include_bytes!("../assets/fonts/HankenGrotesk-600.ttf")[..]),
        ("hk700", &include_bytes!("../assets/fonts/HankenGrotesk-700.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
        // Each weight is its own family, with egui's default fonts behind it for glyphs
        // Hanken Grotesk lacks (⌘, ⏎, ›).
        let mut chain = vec![name.to_string()];
        chain.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), chain);
    }
    fonts.font_data.insert("lucide".into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/lucide.ttf"))));
    fonts.families.insert(FontFamily::Name("icons".into()), vec!["lucide".into()]);
    let regular = fonts.families[&FontFamily::Name("hk400".into())].clone();
    fonts.families.insert(FontFamily::Proportional, regular);
    ctx.set_fonts(fonts);

    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let dark = theme == egui::Theme::Dark;
        let (paper, ink, line) = (pick(Color32::WHITE, Color32::from_rgb(0x17, 0x19, 0x1d), dark), pick(ON_YELLOW, Color32::from_rgb(0xec, 0xee, 0xf1), dark), pick(Color32::from_rgb(0xe4, 0xe6, 0xea), Color32::from_rgb(0x2e, 0x32, 0x39), dark));
        ctx.style_mut_of(theme, |s| {
            s.visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
            s.visuals.panel_fill = paper;
            s.visuals.window_fill = paper;
            s.visuals.selection.bg_fill = YELLOW;
            s.visuals.selection.stroke = Stroke::new(1.0, ink);
            s.visuals.override_text_color = Some(ink);
            s.interaction.selectable_labels = false;
            s.interaction.tooltip_delay = 0.35;
            // Tooltips: paper card, hairline, small soft shadow, body type.
            s.visuals.window_stroke = Stroke::new(1.0, line);
            s.visuals.window_corner_radius = egui::CornerRadius::same(6);
            s.visuals.popup_shadow = egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(if dark { 90 } else { 22 }) };
            s.spacing.tooltip_width = 260.0;
            s.text_styles.insert(egui::TextStyle::Body, font(13.5, W::Regular));
        });
    }
}

/// Lay out one line of text with tracking (in em).
pub fn galley(p: &Painter, text: &str, size: f32, w: W, color: Color32, tracking_em: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat { font_id: font(size, w), color, extra_letter_spacing: tracking_em * size, ..Default::default() },
    );
    job.wrap.max_width = f32::INFINITY;
    p.layout_job(job)
}

pub fn width(p: &Painter, text: &str, size: f32, w: W, tracking_em: f32) -> f32 {
    galley(p, text, size, w, ink(), tracking_em).size().x
}

/// Draw text with its top-left at `pos`; returns the drawn rect.
pub fn text(p: &Painter, pos: Pos2, s: &str, size: f32, w: W, color: Color32, tracking_em: f32) -> Rect {
    let g = galley(p, s, size, w, color, tracking_em);
    let r = Rect::from_min_size(pos, g.size());
    p.galley(pos, g, color);
    r
}

/// Shorten `s` with a middle-free trailing ellipsis until it fits `max_w`.
pub fn fit(p: &Painter, s: &str, size: f32, w: W, tracking_em: f32, max_w: f32) -> String {
    if width(p, s, size, w, tracking_em) <= max_w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if width(p, &cand, size, w, tracking_em) <= max_w { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
}

/// Shorten from the middle, keeping the end of a file name (often a number or date).
pub fn fit_middle(p: &Painter, s: &str, size: f32, w: W, tracking_em: f32, max_w: f32) -> String {
    if width(p, s, size, w, tracking_em) <= max_w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let make = |keep: usize| {
        let head = (keep + 1) / 2;
        let tail = keep / 2;
        let h: String = chars[..head].iter().collect();
        let t: String = chars[chars.len() - tail..].iter().collect();
        format!("{}…{}", h.trim_end(), t.trim_start())
    };
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if width(p, &make(mid), size, w, tracking_em) <= max_w { lo = mid } else { hi = mid - 1 }
    }
    make(lo)
}

/// A rounded dashed outline whose dashes travel along it as `offset` grows.
/// Dashes are placed by distance along the closed path and wrap across its start,
/// so there is no seam anywhere, at any offset. The period is fitted so a whole
/// number of dashes fills the perimeter.
pub fn marching_round_rect(p: &Painter, r: Rect, radius: f32, stroke: Stroke, offset: f32) {
    let pts = round_rect_path(r, radius);
    let mut cum = vec![0.0f32];
    for w in pts.windows(2) {
        cum.push(cum.last().unwrap() + w[0].distance(w[1]));
    }
    let perimeter = *cum.last().unwrap();
    if perimeter <= 0.0 {
        return;
    }
    let n = (perimeter / 14.0).round().max(1.0) as usize;
    let period = perimeter / n as f32;
    let dash = period * 0.57;
    let phase = offset.rem_euclid(period);
    // Points along the path between two distances (both within 0..=perimeter).
    let span = |a: f32, b: f32| -> Vec<Pos2> {
        let at = |d: f32| -> Pos2 {
            let i = cum.partition_point(|&c| c <= d).clamp(1, pts.len() - 1);
            let (c0, c1) = (cum[i - 1], cum[i]);
            let t = if c1 > c0 { (d - c0) / (c1 - c0) } else { 0.0 };
            pts[i - 1] + (pts[i] - pts[i - 1]) * t
        };
        let mut out = vec![at(a)];
        out.extend(cum.iter().enumerate().filter(|(_, c)| **c > a && **c < b).map(|(i, _)| pts[i]));
        out.push(at(b));
        out
    };
    for k in 0..n {
        let start = phase + k as f32 * period;
        let end = start + dash;
        let (a, b) = (start % perimeter, end % perimeter);
        if b > a {
            p.line(span(a, b), stroke);
        } else {
            // This dash crosses the start of the path: draw both halves.
            p.line(span(a, perimeter), stroke);
            p.line(span(0.0, b), stroke);
        }
    }
}

fn round_rect_path(r: Rect, radius: f32) -> Vec<Pos2> {
    let mut pts = vec![Pos2::new(r.center().x, r.min.y)];
    let corners = [
        (Pos2::new(r.max.x - radius, r.min.y + radius), -90.0f32),
        (Pos2::new(r.max.x - radius, r.max.y - radius), 0.0),
        (Pos2::new(r.min.x + radius, r.max.y - radius), 90.0),
        (Pos2::new(r.min.x + radius, r.min.y + radius), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=12 {
            let a = (start + i as f32 * 7.5).to_radians();
            pts.push(c + Vec2::new(a.cos(), a.sin()) * radius);
        }
    }
    pts.push(pts[0]);
    pts
}


/// Lucide icons (ISC). Codepoints from lucide-static 1.60.0 `font/codepoints.json`.
pub mod icon {
    pub const FOLDER_OPEN: char = '\u{e247}';
    pub const PASTE: char = '\u{e3e8}';
    pub const VIDEO: char = '\u{e321}';
    pub const AUDIO: char = '\u{e55e}';
    pub const IMAGE: char = '\u{e31c}';
    pub const TEXT: char = '\u{e0cc}';
    pub const DATA: char = '\u{e36b}';
    pub const FILES: char = '\u{e0cf}';
    pub const ZAP: char = '\u{e1b4}';
    pub const HOURGLASS: char = '\u{e296}';
    pub const SHRINK: char = '\u{e220}';
    pub const TRASH: char = '\u{e18e}';
    pub const X: char = '\u{e1b2}';
    pub const CHECK: char = '\u{e06c}';
    pub const COPY: char = '\u{e09e}';
    pub const FOLDER: char = '\u{e0d7}';
    pub const UNDO: char = '\u{e2a1}';
    pub const GRIP: char = '\u{e0eb}';
    pub const PIN: char = '\u{e259}';
    pub const PIN_OFF: char = '\u{e2b6}';
    pub const BACK: char = '\u{e048}';
    pub const PLUS: char = '\u{e13d}';
    pub const SETTINGS: char = '\u{e154}';
    pub const UPDATE: char = '\u{e0b2}';
    pub const SWAP: char = '\u{e24a}';
    pub const ALERT: char = '\u{e077}';
    pub const DOWNLOAD: char = '\u{e0b2}';
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}

/// Draw one icon centred on `center`.
pub fn icon(p: &Painter, center: Pos2, glyph: char, size: f32, color: Color32) -> Rect {
    p.text(center, egui::Align2::CENTER_CENTER, glyph, icon_font(size), color)
}

/// Icon then text on one line, vertically centred on `y`; returns the full rect.
pub fn icon_text(p: &Painter, pos: Pos2, glyph: char, s: &str, size: f32, w: W, color: Color32) -> Rect {
    let isz = size + 2.0;
    let g = galley(p, s, size, w, color, 0.0);
    let h = g.size().y;
    let ir = icon(p, Pos2::new(pos.x + isz / 2.0, pos.y + h / 2.0), glyph, isz, color);
    let tr = Rect::from_min_size(Pos2::new(ir.max.x + 6.0, pos.y), g.size());
    p.galley(tr.min, g, color);
    ir.union(tr)
}

/// A critically-underdamped spring kept in egui memory under `id`.
/// Snappy (settles in ~250ms) with a small overshoot. Returns the current value.
pub fn spring(ctx: &egui::Context, id: egui::Id, target: f32) -> f32 {
    spring_with(ctx, id, target, 520.0, 30.0)
}

pub fn spring_with(ctx: &egui::Context, id: egui::Id, target: f32, stiffness: f32, damping: f32) -> f32 {
    if crate::platform::reduce_motion_cached() {
        return target;
    }
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 1.0 / 30.0);
    let (mut x, mut v) = ctx.data(|d| d.get_temp::<(f32, f32)>(id)).unwrap_or((target, 0.0));
    // Semi-implicit Euler in small steps for stability.
    let steps = 4;
    let h = dt / steps as f32;
    for _ in 0..steps {
        let a = -stiffness * (x - target) - damping * v;
        v += a * h;
        x += v * h;
    }
    if (x - target).abs() < 0.01 && v.abs() < 0.05 {
        x = target;
        v = 0.0;
    } else {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(id, (x, v)));
    x
}

/// Set a spring's position (and keep its target), e.g. to start an entrance from 0.
pub fn spring_set(ctx: &egui::Context, id: egui::Id, x: f32, v: f32) {
    ctx.data_mut(|d| d.insert_temp(id, (x, v)));
}
