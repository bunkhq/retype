//! Regenerate the app icons from the mark's geometry:
//!     cargo run --example make_icons
//! Writes assets/icons/Retype.iconset (then `iconutil` makes Retype.icns),
//! assets/icons/retype.ico, and assets/icon-256.png (window icon).
//!
//! Shapes are drawn with signed-distance anti-aliasing, so every size is drawn
//! natively instead of being shrunk from one big bitmap. Below 48px the caret and
//! the dot get a minimum width so they don't disappear.

use image::{Rgba, RgbaImage};

const PAPER: [f32; 3] = [1.0, 1.0, 1.0];
const LINE: [f32; 3] = [0.894, 0.902, 0.918];
const INK: [f32; 3] = [0.067, 0.078, 0.094];
const YELLOW: [f32; 3] = [1.0, 0.882, 0.290];

/// Signed distance to a rounded rectangle (centre c, half-size h, radius r).
fn sd_round_rect(p: (f32, f32), c: (f32, f32), h: (f32, f32), r: f32) -> f32 {
    let qx = (p.0 - c.0).abs() - (h.0 - r);
    let qy = (p.1 - c.1).abs() - (h.1 - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

fn sd_circle(p: (f32, f32), c: (f32, f32), r: f32) -> f32 {
    ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() - r
}

fn cover(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

fn over(dst: &mut [f32; 4], rgb: [f32; 3], a: f32) {
    let out_a = a + dst[3] * (1.0 - a);
    if out_a <= 0.0 {
        return;
    }
    for i in 0..3 {
        dst[i] = (rgb[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

#[derive(Clone, Copy)]
enum Style {
    /// Apple's grid: an 824/1024 plate centred, with a soft shadow below.
    Mac,
    /// Windows: the plate nearly fills the square.
    Windows,
}

fn draw(size: u32, style: Style) -> RgbaImage {
    let s = size as f32;
    let (plate, shadow) = match style {
        Style::Mac => (s * 824.0 / 1024.0, true),
        Style::Windows => (s * if size <= 32 { 1.0 } else { 0.94 }, false),
    };
    let off = (s - plate) / 2.0;
    let (pc, ph) = ((s / 2.0, s / 2.0 - if shadow { s * 0.01 } else { 0.0 }), (plate / 2.0, plate / 2.0));
    let radius = plate * 0.225;
    let px = |u: f32| u / 100.0 * plate; // mark units (0..100) to pixels
    let at = |x: f32, y: f32| (off + px(x), off + px(y) + (pc.1 - s / 2.0));
    let small = size < 48;
    // Thicken thin parts at small sizes.
    let caret_w = px(5.0).max(if small { 1.6 } else { 0.0 });
    let dot_r = px(7.0).max(if small { 1.5 } else { 0.0 });
    let hair = (s / 512.0).max(0.6);

    RgbaImage::from_fn(size, size, |x, y| {
        let p = (x as f32 + 0.5, y as f32 + 0.5);
        let mut c = [0.0f32; 4];
        if shadow {
            // Soft, offset downward, and only where it falls outside the plate.
            let d = sd_round_rect((p.0, p.1 - s * 0.018), pc, ph, radius);
            let a = 0.16 * (1.0 - ((d + s * 0.01) / (s * 0.045)).clamp(0.0, 1.0)).powi(2);
            over(&mut c, [0.0, 0.0, 0.0], a);
        }
        let plate_d = sd_round_rect(p, pc, ph, radius);
        over(&mut c, PAPER, cover(plate_d));
        // Hairline edge so the white plate holds on white backgrounds.
        let ring = cover(plate_d.abs() - hair / 2.0) * cover(plate_d + 0.0);
        over(&mut c, LINE, ring.min(1.0) * if small { 0.0 } else { 1.0 });
        if plate_d < 0.5 {
            let inside = cover(plate_d);
            // Dot.
            let (dx, dy) = at(24.0, 62.0);
            over(&mut c, INK, cover(sd_circle(p, (dx, dy), dot_r)) * inside);
            // Yellow selection block.
            let (bx, by) = at(36.0, 34.0);
            let (bw, bh) = (px(44.0), px(34.0));
            over(&mut c, YELLOW, cover(sd_round_rect(p, (bx + bw / 2.0, by + bh / 2.0), (bw / 2.0, bh / 2.0), px(5.0))) * inside);
            // Caret at the block's right edge.
            let (cx, cy) = at(77.0, 28.0);
            let ch = px(46.0);
            over(&mut c, INK, cover(sd_round_rect(p, (cx + caret_w / 2.0, cy + ch / 2.0), (caret_w / 2.0, ch / 2.0), px(1.0).min(caret_w / 2.0))) * inside);
        }
        Rgba([(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8, (c[3] * 255.0).round() as u8])
    })
}

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let set = root.join("icons/Retype.iconset");
    std::fs::create_dir_all(&set).unwrap();
    for base in [16u32, 32, 128, 256, 512] {
        draw(base, Style::Mac).save(set.join(format!("icon_{base}x{base}.png"))).unwrap();
        draw(base * 2, Style::Mac).save(set.join(format!("icon_{base}x{base}@2x.png"))).unwrap();
    }
    draw(1024, Style::Mac).save(root.join("icon-macos-1024.png")).unwrap();

    let frames: Vec<image::codecs::ico::IcoFrame> = [16u32, 20, 24, 32, 40, 48, 64, 96, 128, 256]
        .iter()
        .map(|&n| {
            let img = draw(n, Style::Windows);
            image::codecs::ico::IcoFrame::as_png(img.as_raw(), n, n, image::ExtendedColorType::Rgba8).unwrap()
        })
        .collect();
    let f = std::fs::File::create(root.join("icons/retype.ico")).unwrap();
    image::codecs::ico::IcoEncoder::new(f).encode_images(&frames).unwrap();
    draw(256, Style::Windows).save(root.join("icon-256.png")).unwrap();
    println!("icons written to {}", root.join("icons").display());
}
