//! Headless conversion from the right-click menu: no main window, only a small
//! panel in the screen corner that shows progress and then becomes the file.
//! Later right-click launches join the same panel and queue.

use crate::engine::{Job, JobState, Opts, Task};
use crate::platform;
use crate::theme::{self, W};
use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const SIZE: Vec2 = Vec2::new(360.0, 96.0);
const LINGER: Duration = Duration::from_secs(4);

pub struct Toast {
    job: Job,
    sources: usize,
    first_source: PathBuf,
    target: String,
    placed: bool,
    finished_at: Option<Instant>,
    shown: Option<JobState>,
    error: Option<String>,
    thumb: Option<egui::TextureHandle>,
    drag_png: Vec<u8>,
}

fn tasks(files: &[PathBuf], target: &str) -> Vec<Task> {
    let files: Vec<PathBuf> = files.iter().filter(|s| crate::formats::ext_of(s).as_deref() != Some(target)).cloned().collect();
    if target == "pdf" {
        if files.is_empty() { vec![] } else { vec![Task { srcs: files, target: target.into(), opts: Opts { settings: crate::prefs::Prefs::load().settings, ..Opts::default() } }] }
    } else {
        files.into_iter().map(|s| Task { srcs: vec![s], target: target.into(), opts: Opts { settings: crate::prefs::Prefs::load().settings, ..Opts::default() } }).collect()
    }
}

impl Toast {
    pub fn new(cc: &eframe::CreationContext<'_>, sources: Vec<PathBuf>, target: String) -> Self {
        theme::install(&cc.egui_ctx);
        crate::ipc::set_waker(&cc.egui_ctx);
        crate::platform::round_corners(cc);
        let c = cc.egui_ctx.clone();
        theme::spring_set(&cc.egui_ctx, egui::Id::unique("toast-in"), 60.0, 0.0);
        let files = crate::app::expand(&sources);
        let job = Job::start(tasks(&files, &target), move || c.request_repaint());
        Toast {
            job,
            sources: files.len(),
            first_source: files.first().cloned().unwrap_or_default(),
            target,
            placed: false,
            finished_at: None,
            shown: None,
            error: None,
            thumb: None,
            drag_png: vec![],
        }
    }
}

impl eframe::App for Toast {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        crate::platform::clear_color()
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::sync(&ctx);

        // Files from other right-click launches join this panel.
        for msg in crate::ipc::take() {
            let target = msg.target.unwrap_or_else(|| self.target.clone());
            let files = crate::app::expand(&msg.files);
            self.sources += files.len();
            self.job.push(tasks(&files, &target));
            self.finished_at = None;
        }

        // Place in the screen corner once the monitor size is known, then show.
        if !self.placed {
            if let Some(m) = ctx.input(|i| i.viewport().monitor_size) {
                let pos = if cfg!(target_os = "macos") { Pos2::new(m.x - SIZE.x - 14.0, 38.0) } else { Pos2::new(m.x - SIZE.x - 14.0, m.y - SIZE.y - 62.0) };
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                self.placed = true;
            }
            ctx.request_repaint();
        }

        let s = self.job.snapshot();
        if s.done && self.finished_at.is_none() {
            self.error = if let Some((p, e)) = s.failures.first() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("A file");
                Some(if s.outputs.is_empty() { format!("{name} {e}.") } else { format!("Saved {}. {name} {e}.", s.outputs.len()) })
            } else if s.outputs.is_empty() {
                Some(format!("Already .{}.", self.target))
            } else {
                None
            };
            let (tex, png) = crate::app::thumb_and_drag_image(&ctx, s.thumb.as_ref());
            self.thumb = tex;
            self.drag_png = png;
            self.shown = Some(s.clone());
            self.finished_at = Some(Instant::now());
        }
        let outputs = self.shown.as_ref().map(|s| s.outputs.clone()).unwrap_or_default();
        let ready = self.finished_at.is_some() && !outputs.is_empty();

        // Slides in from the screen edge on a spring.
        // Not on Windows: its opaque window would show the card sliding inside a fixed box.
        let slide = theme::spring_with(&ctx, egui::Id::unique("toast-in"), 0.0, 520.0, 26.0);
        let slide = if crate::platform::TRANSPARENT_WINDOW { slide } else { 0.0 };
        let card = if crate::platform::TRANSPARENT_WINDOW { ui.max_rect().shrink(1.0) } else { ui.max_rect() }.translate(Vec2::new(slide, 0.0));
        let p = ui.painter().clone();
        let radius = CornerRadius::same(crate::platform::card_radius(14));
        p.rect_filled(card, radius, theme::paper());
        if !crate::platform::os_rounds_corners() {
            p.rect_stroke(card, radius, Stroke::new(1.0, theme::line()), StrokeKind::Inside);
        }

        let resp = ui.interact(card, ui.scope_id().with("toast"), Sense::click_and_drag());
        let resp = if ready { resp.on_hover_text("Drag into any app. Click to open.") } else { resp };
        if ready && (resp.hovered() || resp.dragged()) {
            ctx.set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        }
        if ready && resp.drag_started() {
            platform::start_file_drag(frame, outputs.clone(), self.drag_png.clone());
        }
        if ready && resp.clicked() {
            platform::open(&outputs[0]);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.job.cancel();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Tile.
        let tile = Rect::from_min_size(card.min + Vec2::new(14.0, 14.0), Vec2::splat(card.height() - 28.0));
        p.rect_filled(tile, CornerRadius::same(6), theme::wash());
        match (&self.thumb, ready) {
            (Some(t), true) => {
                let s = t.size_vec2();
                let k = (tile.width() / s.x).min(tile.height() / s.y);
                p.image(t.id(), Rect::from_center_size(tile.center(), s * k), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            _ => {
                let k = crate::formats::kind_of_ext(&self.target).unwrap_or(crate::formats::Kind::Data);
                theme::icon(&p, tile.center() - Vec2::new(0.0, 7.0), crate::app::kind_icon(k), 20.0, theme::soft());
                p.text(tile.center() + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, format!(".{}", self.target), theme::font(11.5, W::Semibold), theme::soft());
            }
        }
        p.rect_stroke(tile, CornerRadius::same(6), Stroke::new(1.0, theme::line()), StrokeKind::Inside);

        // Name: the output once it exists, the source with its new extension before that.
        let x = tile.max.x + 14.0;
        let max_w = card.max.x - x - 14.0;
        let name = if ready && outputs.len() == 1 {
            outputs[0].file_name().and_then(|n| n.to_str()).unwrap_or("").to_string()
        } else if ready {
            format!("{} files", outputs.len())
        } else if self.sources == 1 || self.target == "pdf" {
            format!("{}.{}", self.first_source.file_stem().and_then(|s| s.to_str()).unwrap_or("file"), self.target)
        } else {
            format!("{} files to .{}", self.sources, self.target)
        };
        let name = theme::fit_middle(&p, &name, 16.0, W::Bold, -0.02, max_w - 6.0);
        let g = theme::galley(&p, &name, 16.0, W::Bold, if ready { theme::ON_YELLOW } else { theme::ink() }, -0.02);
        let npos = Pos2::new(x + 3.0, tile.min.y + 4.0);
        if ready {
            p.rect_filled(Rect::from_min_size(npos - Vec2::new(3.0, 0.0), g.size() + Vec2::new(6.0, 1.0)), CornerRadius::same(3), theme::YELLOW);
        }
        p.galley(npos, g, if ready { theme::ON_YELLOW } else { theme::ink() });

        let line_y = tile.max.y - 18.0;
        match (&self.error, self.finished_at) {
            (Some(e), Some(_)) => {
                theme::icon_text(&p, Pos2::new(x, line_y), theme::icon::ALERT, &theme::fit(&p, e, 13.5, W::Regular, 0.0, max_w - 22.0), 13.5, W::Regular, theme::ink());
            }
            (None, Some(_)) => {
                let bytes: u64 = outputs.iter().filter_map(|o| std::fs::metadata(o).ok()).map(|m| m.len()).sum();
                theme::icon_text(&p, Pos2::new(x, line_y), theme::icon::CHECK, &crate::engine::human_size(bytes), 13.5, W::Regular, theme::soft());
                if resp.hovered() {
                    theme::icon(&p, Pos2::new(card.max.x - 20.0, card.center().y), theme::icon::GRIP, 16.0, theme::soft());
                }
            }
            _ => {
                let track = Rect::from_min_size(Pos2::new(x, line_y + 7.0), Vec2::new(max_w, 3.0));
                p.rect_filled(track, CornerRadius::same(2), theme::line());
                let shown = ctx.animate_value_with_time(ui.scope_id().with("p"), s.progress, 0.25);
                p.rect_filled(Rect::from_min_size(track.min, Vec2::new(max_w * shown, 3.0)), CornerRadius::same(2), theme::ink());
                if s.total > 1 {
                    let label = format!("{} of {}", (s.current + 1).min(s.total), s.total);
                    p.text(Pos2::new(track.max.x, track.min.y - 6.0), Align2::RIGHT_BOTTOM, label, theme::font(12.5, W::Regular), theme::soft());
                }
            }
        }

        // Close a few seconds after finishing, unless the pointer is on it.
        if let Some(t) = self.finished_at {
            let linger = if self.error.is_some() { LINGER + Duration::from_secs(2) } else { LINGER };
            if resp.hovered() || resp.dragged() {
                self.finished_at = Some(Instant::now());
            } else if t.elapsed() >= linger {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}
