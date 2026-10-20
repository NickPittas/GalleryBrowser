//! Right-pane preview and the shared [`MediaView`] (player + off-thread opener + texture + viewer) that
//! the fullscreen viewport reuses.
//!
//! Data flow: `Entry` → [`Loader`] worker thread (`gallerybrowser_media::open` / `open_sequence`, or a
//! full still decode) → UI thread → `Player::load` (time-based media) or texture upload (stills) →
//! `Player::tick` → `frame_to_rgba` → `TextureHandle::set`. Opening never blocks the UI thread and the
//! newest request always wins (generation counters).

use std::borrow::Cow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, Pos2, Rect, Response, TextureHandle, TextureOptions, Ui, vec2};
use gallerybrowser_core::{Entry, MediaKind, Sequence, Settings};
use gallerybrowser_media::{
    FrameRate, FrameRequest, RgbaImage, SharedSource, Tick, VideoFrame, frame_to_rgba, open,
    open_sequence,
};
use gallerybrowser_player::Player;

use crate::state::AppState;
use crate::transport;
use crate::viewer::{self, ViewerImage, ViewerState};

const TEXTURE_OPTIONS: TextureOptions = TextureOptions::LINEAR;

// ---------------------------------------------------------------------------------------------
// Off-thread opening
// ---------------------------------------------------------------------------------------------

/// What the worker should open.
enum LoadTarget {
    /// Decode frame 0 of an image file completely.
    Still(PathBuf),
    /// Open a video/audio container (streaming; no frame is decoded here).
    Movie(PathBuf),
    /// Open an image sequence as a movie at this rate.
    Sequence(Arc<Sequence>, f64),
}

/// A decoded still, ready for texture upload.
struct Picture {
    image: RgbaImage,
    par: f32,
    /// Some pixel is not fully opaque (draw the checkerboard behind it).
    checker: bool,
}

enum Loaded {
    Still(Picture),
    Source(SharedSource),
}

struct LoadRequest {
    generation: u64,
    target: LoadTarget,
}

struct LoadResult {
    generation: u64,
    outcome: Result<Loaded, String>,
}

/// One worker thread that opens media for a view. Latest request wins; results of superseded
/// requests are dropped by generation.
struct Loader {
    tx: Option<Sender<LoadRequest>>,
    rx: Receiver<LoadResult>,
    generation: u64,
    repaint: Arc<OnceLock<egui::Context>>,
}

impl Loader {
    fn new() -> Self {
        let (req_tx, req_rx) = channel::<LoadRequest>();
        let (res_tx, res_rx) = channel::<LoadResult>();
        let repaint: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let worker_repaint = Arc::clone(&repaint);
        let spawned = std::thread::Builder::new()
            .name("gb-media-open".into())
            .spawn(move || worker(req_rx, res_tx, worker_repaint));
        let tx = match spawned {
            Ok(_) => Some(req_tx),
            Err(e) => {
                log::error!("cannot start the media open thread: {e}");
                None
            }
        };
        Self {
            tx,
            rx: res_rx,
            generation: 0,
            repaint,
        }
    }

    fn set_context(&self, ctx: &egui::Context) {
        let _ = self.repaint.set(ctx.clone());
    }

    /// Queue `target`; every earlier request becomes stale.
    fn request(&mut self, target: LoadTarget) -> Result<(), String> {
        self.generation = self.generation.wrapping_add(1);
        match &self.tx {
            Some(tx) => tx
                .send(LoadRequest {
                    generation: self.generation,
                    target,
                })
                .map_err(|_| "the media loader stopped".to_string()),
            None => Err("the media loader is unavailable".to_string()),
        }
    }

    /// Make every outstanding request stale.
    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// Next result belonging to the newest request, if any.
    fn try_recv(&self) -> Option<LoadResult> {
        loop {
            match self.rx.try_recv() {
                Ok(r) if r.generation == self.generation => return Some(r),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    /// Stop accepting work; the thread ends after any decode already running.
    fn shutdown(&mut self) {
        self.invalidate();
        self.tx = None;
    }
}

fn worker(
    rx: Receiver<LoadRequest>,
    tx: Sender<LoadResult>,
    repaint: Arc<OnceLock<egui::Context>>,
) {
    while let Ok(mut req) = rx.recv() {
        // skip everything that is already stale
        while let Ok(newer) = rx.try_recv() {
            req = newer;
        }
        let generation = req.generation;
        let target = req.target;
        let outcome = catch_unwind(AssertUnwindSafe(|| run_load(target)))
            .unwrap_or_else(|p| Err(format!("the decoder crashed: {}", panic_text(p.as_ref()))));
        if tx
            .send(LoadResult {
                generation,
                outcome,
            })
            .is_err()
        {
            break;
        }
        if let Some(ctx) = repaint.get() {
            ctx.request_repaint();
        }
    }
}

fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown error".to_string()
    }
}

fn run_load(target: LoadTarget) -> Result<Loaded, String> {
    match target {
        LoadTarget::Still(path) => {
            let source = open(&path).map_err(|e| format!("{}: {e}", file_label(&path)))?;
            let frame = source
                .video_frame(FrameRequest::full(Tick(0)))
                .map_err(|e| format!("{}: {e}", file_label(&path)))?;
            Ok(Loaded::Still(
                picture_from_frame(&frame, true)
                    .map_err(|e| format!("{}: {e}", file_label(&path)))?,
            ))
        }
        LoadTarget::Movie(path) => Ok(Loaded::Source(
            open(&path).map_err(|e| format!("{}: {e}", file_label(&path)))?,
        )),
        LoadTarget::Sequence(seq, fps) => {
            let fps = if fps.is_finite() && fps > 0.0 {
                fps.clamp(0.1, 1000.0)
            } else {
                24.0
            };
            Ok(Loaded::Source(
                open_sequence(&seq, FrameRate::from_f64(fps))
                    .map_err(|e| format!("image sequence: {e}"))?,
            ))
        }
    }
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Convert a decoded frame to a tightly packed RGBA picture (row stride handled by `frame_to_rgba`).
fn picture_from_frame(frame: &VideoFrame, scan_alpha: bool) -> Result<Picture, String> {
    let image = frame_to_rgba(frame).map_err(|e| e.to_string())?;
    let par = if frame.par.0 > 0 && frame.par.1 > 0 {
        frame.par.0 as f32 / frame.par.1 as f32
    } else {
        1.0
    };
    let checker = scan_alpha && has_transparency(&image);
    Ok(Picture {
        image,
        par,
        checker,
    })
}

fn has_transparency(image: &RgbaImage) -> bool {
    image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p.get(3).is_some_and(|a| *a < 255))
}

/// Box-decimate `image` by an integer factor so both sides fit `max_side`. `None` = already fits.
fn decimate_to_fit(image: &RgbaImage, max_side: usize) -> Option<([usize; 2], Vec<u8>)> {
    let (w, h) = (image.width as usize, image.height as usize);
    let max_side = max_side.max(1);
    let n = w.max(h).div_ceil(max_side);
    if n <= 1 {
        return None;
    }
    let (ow, oh) = ((w / n).max(1), (h / n).max(1));
    let mut out = Vec::with_capacity(ow.checked_mul(oh)?.checked_mul(4)?);
    for oy in 0..oh {
        for ox in 0..ow {
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for y in (oy * n)..((oy + 1) * n).min(h) {
                for x in (ox * n)..((ox + 1) * n).min(w) {
                    let i = (y * w + x) * 4;
                    if let Some(px) = image.pixels.get(i..i + 4) {
                        for (s, v) in sum.iter_mut().zip(px) {
                            *s += u32::from(*v);
                        }
                        count += 1;
                    }
                }
            }
            let count = count.max(1);
            out.extend(sum.iter().map(|s| (s / count) as u8));
        }
    }
    Some(([ow, oh], out))
}

// ---------------------------------------------------------------------------------------------
// MediaView
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    Empty,
    Loading,
    Still,
    Movie,
    Failed(String),
}

/// Player + opener + current texture + viewer for one view (preview pane or fullscreen window).
pub struct MediaView {
    pub player: Player,
    pub viewer: ViewerState,
    loader: Loader,
    phase: Phase,
    autoplay: bool,
    /// Sequence being played (source frame numbers for the transport readout).
    pub sequence: Option<Arc<Sequence>>,
    texture: Option<TextureHandle>,
    /// The texture holds a picture of the CURRENT media (false while loading / after failure).
    tex_visible: bool,
    frame_size: [u32; 2],
    par: f32,
    checker: bool,
    check_alpha: bool,
    has_video: bool,
    frame_error: Option<String>,
}

impl MediaView {
    pub fn new(cache_bytes: u64) -> Self {
        Self {
            player: Player::new(cache_bytes),
            viewer: ViewerState {
                toolbar: true,
                ..ViewerState::default()
            },
            loader: Loader::new(),
            phase: Phase::Empty,
            autoplay: false,
            sequence: None,
            texture: None,
            tex_visible: false,
            frame_size: [0, 0],
            par: 1.0,
            checker: false,
            check_alpha: true,
            has_video: true,
            frame_error: None,
        }
    }

    /// True while a time-based source (video, sequence, audio) is loaded.
    pub fn is_movie(&self) -> bool {
        self.phase == Phase::Movie
    }

    /// Drop everything shown so far (no dangling texture/player state).
    pub fn clear(&mut self) {
        self.loader.invalidate();
        self.player.unload();
        self.reset_picture();
        self.sequence = None;
        self.phase = Phase::Empty;
    }

    fn reset_picture(&mut self) {
        self.tex_visible = false;
        self.frame_error = None;
        self.viewer.fit();
        self.viewer.command = None;
        self.check_alpha = true;
        self.checker = false;
    }

    /// Show an error instead of the media.
    pub fn fail(&mut self, message: impl Into<String>) {
        let message = message.into();
        log::warn!("preview: {message}");
        self.loader.invalidate();
        self.player.unload();
        self.reset_picture();
        self.sequence = None;
        self.phase = Phase::Failed(message);
    }

    /// Start opening `entry` (off the UI thread). Autoplays time-based media when `autoplay`.
    pub fn open(&mut self, entry: &Entry, autoplay: bool, sequence_fps: f64) {
        self.player.unload();
        self.reset_picture();
        self.autoplay = autoplay;
        self.sequence = None;
        let target = match (entry.kind, &entry.sequence) {
            (MediaKind::Sequence, Some(seq)) => {
                self.sequence = Some(Arc::clone(seq));
                LoadTarget::Sequence(Arc::clone(seq), sequence_fps)
            }
            (MediaKind::Sequence, None) => {
                self.fail("The image sequence has no frames");
                return;
            }
            (MediaKind::Image, _) => LoadTarget::Still(entry.path.clone()),
            (MediaKind::Video | MediaKind::Audio, _) => LoadTarget::Movie(entry.path.clone()),
        };
        match self.loader.request(target) {
            Ok(()) => self.phase = Phase::Loading,
            Err(e) => self.fail(e),
        }
    }

    pub fn shutdown(&mut self) {
        self.loader.shutdown();
        self.player.unload();
        self.texture = None;
        self.tex_visible = false;
        self.phase = Phase::Empty;
        self.sequence = None;
    }

    /// Per-UI-frame work: collect finished opens, advance the player, upload new frames.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.loader.set_context(ctx);
        while let Some(result) = self.loader.try_recv() {
            match result.outcome {
                Ok(Loaded::Still(pic)) => match self.upload(ctx, &pic.image) {
                    Ok(()) => {
                        self.par = pic.par;
                        self.checker = pic.checker;
                        self.phase = Phase::Still;
                    }
                    Err(e) => self.fail(e),
                },
                Ok(Loaded::Source(source)) => {
                    self.has_video = source.info().video.is_some();
                    self.player.load(source, self.autoplay);
                    self.phase = Phase::Movie;
                }
                Err(e) => self.fail(e),
            }
        }
        match self.phase {
            Phase::Loading => ctx.request_repaint_after(Duration::from_millis(100)),
            Phase::Movie => self.poll_player(ctx),
            _ => {}
        }
    }

    fn poll_player(&mut self, ctx: &egui::Context) {
        if let Some(frame) = self.player.tick(Instant::now()) {
            match picture_from_frame(&frame, self.check_alpha) {
                Ok(pic) => match self.upload(ctx, &pic.image) {
                    Ok(()) => {
                        self.par = pic.par;
                        if self.check_alpha {
                            self.checker = pic.checker;
                            self.check_alpha = false;
                        }
                        self.frame_error = None;
                    }
                    Err(e) => self.frame_error = Some(e),
                },
                Err(e) => self.frame_error = Some(e),
            }
        }
        if self.player.needs_repaint() {
            ctx.request_repaint_after(Duration::from_millis(if self.player.state().playing {
                0
            } else {
                30
            }));
        }
    }

    /// Upload `image` into the texture in place (`TextureHandle::set`), decimating frames larger than
    /// the GPU texture limit. `frame_size` keeps the true size so 1:1 stays pixel-exact in layout terms.
    fn upload(&mut self, ctx: &egui::Context, image: &RgbaImage) -> Result<(), String> {
        let (w, h) = (image.width as usize, image.height as usize);
        let expected = w.checked_mul(h).and_then(|p| p.checked_mul(4));
        if w == 0 || h == 0 || expected != Some(image.pixels.len()) {
            return Err(format!(
                "malformed frame ({}x{}, {} bytes)",
                image.width,
                image.height,
                image.pixels.len()
            ));
        }
        let max_side = ctx.input(|i| i.max_texture_side).max(256);
        let (size, data): ([usize; 2], Cow<'_, [u8]>) = match decimate_to_fit(image, max_side) {
            Some((size, data)) => (size, Cow::Owned(data)),
            None => ([w, h], Cow::Borrowed(image.pixels.as_slice())),
        };
        if size[0].checked_mul(size[1]).and_then(|p| p.checked_mul(4)) != Some(data.len()) {
            return Err("malformed frame".to_string());
        }
        let color = ColorImage::from_rgba_unmultiplied(size, &data);
        match &mut self.texture {
            Some(tex) => tex.set(color, TEXTURE_OPTIONS),
            None => {
                self.texture =
                    Some(ctx.load_texture("gallerybrowser-preview", color, TEXTURE_OPTIONS))
            }
        }
        self.frame_size = [image.width, image.height];
        self.tex_visible = true;
        Ok(())
    }

    /// Draw the picture (or the loading/error state) into all remaining space.
    pub fn picture_ui(&mut self, ui: &mut Ui) -> Response {
        let image = match (&self.texture, self.tex_visible) {
            (Some(texture), true) => Some(ViewerImage {
                texture,
                size: self.frame_size,
                par: self.par,
                checker: self.checker,
            }),
            _ => None,
        };
        let resp = viewer::show_image(ui, &mut self.viewer, image);
        let area = resp.rect;
        let message: Option<(String, Color32)> = match &self.phase {
            Phase::Empty => Some(("Select an item to preview".to_string(), Color32::GRAY)),
            Phase::Loading => Some(("Loading…".to_string(), Color32::GRAY)),
            Phase::Failed(e) => Some((
                format!("Cannot preview this file\n{e}"),
                Color32::from_rgb(235, 110, 100),
            )),
            Phase::Still => None,
            Phase::Movie => {
                let st = self.player.state();
                if let Some(e) = self.frame_error.clone().or(st.error) {
                    Some((e, Color32::from_rgb(235, 110, 100)))
                } else if !self.tex_visible && !self.has_video {
                    Some(("Audio only".to_string(), Color32::GRAY))
                } else if !self.tex_visible {
                    Some(("Loading…".to_string(), Color32::GRAY))
                } else {
                    None
                }
            }
        };
        if let Some((text, color)) = message {
            let failed = color != Color32::GRAY;
            overlay_text(ui, area, &text, color, failed && self.tex_visible);
        }
        resp
    }

    /// Display aspect (width / height, pixel aspect applied) of the shown picture; 16:9 while
    /// nothing is visible yet.
    pub fn aspect(&self) -> f32 {
        let [w, h] = self.frame_size;
        if self.tex_visible && w > 0 && h > 0 && self.par.is_finite() && self.par > 0.0 {
            (w as f32 * self.par / h as f32).clamp(0.2, 8.0)
        } else {
            16.0 / 9.0
        }
    }
}

/// Centered (or, for errors over a picture, bottom-anchored on a dark strip) wrapped text.
fn overlay_text(ui: &Ui, area: Rect, text: &str, color: Color32, bottom: bool) {
    let painter = ui.painter_at(area);
    let galley = painter.layout(
        text.to_string(),
        egui::FontId::proportional(14.0),
        color,
        (area.width() - 24.0).max(40.0),
    );
    let size = galley.size();
    let pos = if bottom {
        Pos2::new(
            area.center().x - size.x / 2.0,
            area.bottom() - size.y - 10.0,
        )
    } else {
        area.center() - size / 2.0
    };
    if bottom {
        painter.rect_filled(
            Rect::from_min_size(pos, size).expand2(vec2(8.0, 4.0)),
            4.0,
            Color32::from_black_alpha(190),
        );
    }
    painter.galley(pos, galley, color);
}

// ---------------------------------------------------------------------------------------------
// PreviewState
// ---------------------------------------------------------------------------------------------

/// Right pane preview: still images (full decode on a worker, shown in viewer), videos and sequences
/// (Player + transport). Loads on focus change; respects settings.video_preview (autoplay).
pub struct PreviewState {
    view: MediaView,
    /// Identity of what the view shows: entry path and (for sequences) its frame count.
    key: Option<(PathBuf, usize)>,
    audio_applied: bool,
    cache_bytes: u64,
}

impl PreviewState {
    pub fn new(cache_bytes: u64) -> Self {
        Self {
            view: MediaView::new(cache_bytes),
            key: None,
            audio_applied: false,
            cache_bytes,
        }
    }

    /// Point the preview at an entry (None clears). Idempotent for the same path.
    pub fn set_entry(&mut self, entry: Option<&Entry>, settings: &Settings) {
        if !self.audio_applied {
            self.view.player.set_volume(settings.volume.clamp(0.0, 1.0));
            self.view.player.set_muted(settings.muted);
            self.view.player.set_looping(settings.loop_playback);
            self.audio_applied = true;
        }
        let bytes = settings.ram_preview_mb.saturating_mul(1024 * 1024);
        if bytes != self.cache_bytes {
            self.set_cache_bytes(bytes);
        }
        match entry {
            None => {
                self.key = None;
                self.view.clear();
            }
            Some(e) => {
                let key = (
                    e.path.clone(),
                    e.sequence.as_ref().map_or(0, |s| s.frames.len()),
                );
                if self.key.as_ref() == Some(&key) {
                    return;
                }
                self.key = Some(key);
                self.view
                    .open(e, settings.video_preview, settings.sequence_fps);
            }
        }
    }

    /// Stop playback and release decoders (app exit).
    pub fn shutdown(&mut self) {
        self.key = None;
        self.view.shutdown();
    }

    /// Playback frame-cache budget in bytes (`ram_preview_mb * 1 MiB`).
    pub fn set_cache_bytes(&mut self, bytes: u64) {
        self.cache_bytes = bytes;
        self.view.player.set_cache_bytes(bytes);
    }

    /// Step ±`delta` frames (',' / '.'); no-op unless a video/sequence is loaded.
    pub fn step_frame(&mut self, delta: i32) {
        if self.view.is_movie() {
            self.view.player.step(i64::from(delta));
        }
    }

    /// Space: play/pause; no-op unless a video/sequence is loaded.
    pub fn toggle_play(&mut self) {
        if self.view.is_movie() {
            self.view.player.toggle();
        }
    }

    /// Pause playback (e.g. when the fullscreen viewer opens).
    pub fn pause(&mut self) {
        self.view.player.pause();
    }

    /// Current (volume, muted, looping). Until the first `set_entry` these are the player defaults —
    /// persist `AppState::settings` (kept in sync by [`show`]) rather than calling this before then.
    pub fn audio_settings(&self) -> (f32, bool, bool) {
        let st = self.view.player.state();
        (st.volume, st.muted, st.looping)
    }

    /// Adopt volume/mute/loop chosen elsewhere (fullscreen viewer).
    pub fn set_audio(&mut self, volume: f32, muted: bool, looping: bool) {
        self.view.player.set_volume(volume.clamp(0.0, 1.0));
        self.view.player.set_muted(muted);
        self.view.player.set_looping(looping);
    }
}

/// Write volume/mute/loop back to the settings when they differ.
pub(crate) fn sync_audio_to_settings(settings: &mut Settings, audio: (f32, bool, bool)) {
    let (volume, muted, looping) = audio;
    if (settings.volume - volume).abs() > 1e-4 {
        settings.volume = volume.clamp(0.0, 1.0);
    }
    settings.muted = muted;
    settings.loop_playback = looping;
}

/// Largest share of the panel height the side preview picture may take.
const PICTURE_MAX_FRACTION: f32 = 0.4;
/// Smallest picture height of the side preview.
const PICTURE_MIN_HEIGHT: f32 = 120.0;

/// Side-preview picture height: aspect-fit to `width`, capped at 40% of `panel_height`, never below
/// [`PICTURE_MIN_HEIGHT`].
fn picture_height(width: f32, panel_height: f32, aspect: f32) -> f32 {
    let cap = (panel_height * PICTURE_MAX_FRACTION).max(PICTURE_MIN_HEIGHT);
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        16.0 / 9.0
    };
    (width / aspect).clamp(PICTURE_MIN_HEIGHT, cap)
}

/// Right-panel preview: the picture in an aspect-fit box (never filling the panel), then the
/// transport for time-based media, then a separator; the info pane follows below.
pub fn show(ui: &mut Ui, state: &mut AppState) {
    let ctx = ui.ctx().clone();
    let preview = &mut state.preview;
    preview.view.poll(&ctx);

    let width = ui.available_width().max(8.0);
    let height = picture_height(width, ui.available_height(), preview.view.aspect());
    ui.allocate_ui(vec2(width, height), |ui| {
        preview.view.picture_ui(ui);
    });
    if preview.view.is_movie() {
        ui.add_space(ui.spacing().item_spacing.y);
        let view = &mut preview.view;
        transport::show(ui, &mut view.player, view.sequence.as_deref());
    }
    ui.add_space(4.0);
    ui.separator();

    if preview.audio_applied {
        let audio = preview.audio_settings();
        sync_audio_to_settings(&mut state.settings, audio);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, fill: [u8; 4]) -> RgbaImage {
        let mut px = Vec::new();
        for _ in 0..(w * h) {
            px.extend_from_slice(&fill);
        }
        RgbaImage {
            width: w,
            height: h,
            pixels: Arc::new(px),
        }
    }

    #[test]
    fn picture_height_fits_width_and_is_capped() {
        // 340 wide 16:9 -> 191, under the 40% cap of an 800 px panel (320).
        assert!((picture_height(340.0, 800.0, 16.0 / 9.0) - 191.25).abs() < 0.01);
        // Tall media is capped at 40% of the panel height.
        assert!((picture_height(340.0, 800.0, 0.5) - 320.0).abs() < 0.01);
        // Tiny panels keep the minimum; bad aspect falls back to 16:9.
        assert!((picture_height(340.0, 100.0, 0.5) - PICTURE_MIN_HEIGHT).abs() < 0.01);
        assert!((picture_height(340.0, 800.0, f32::NAN) - 191.25).abs() < 0.01);
    }

    #[test]
    fn decimation_halves_oversized_images() {
        let img = image(10, 6, [10, 20, 30, 255]);
        let (size, data) = decimate_to_fit(&img, 5).expect("needs decimation");
        assert_eq!(size, [5, 3]);
        assert_eq!(data.len(), 5 * 3 * 4);
        assert_eq!(&data[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn decimation_skips_images_that_fit() {
        assert!(decimate_to_fit(&image(4, 4, [0, 0, 0, 255]), 4).is_none());
    }

    #[test]
    fn decimation_tolerates_truncated_pixel_data() {
        let img = RgbaImage {
            width: 8,
            height: 8,
            pixels: Arc::new(vec![1, 2, 3, 4]),
        };
        let (size, data) = decimate_to_fit(&img, 4).expect("needs decimation");
        assert_eq!(data.len(), size[0] * size[1] * 4);
    }

    #[test]
    fn transparency_scan() {
        assert!(!has_transparency(&image(2, 2, [1, 2, 3, 255])));
        assert!(has_transparency(&image(2, 2, [1, 2, 3, 0])));
    }

    #[test]
    fn audio_sync_updates_settings() {
        let mut s = Settings::default();
        sync_audio_to_settings(&mut s, (0.25, true, false));
        assert_eq!((s.volume, s.muted, s.loop_playback), (0.25, true, false));
    }

    #[test]
    fn loader_drops_stale_results() {
        let mut loader = Loader::new();
        let missing = PathBuf::from("/nonexistent/gallerybrowser-test.mp4");
        loader.request(LoadTarget::Movie(missing.clone())).ok();
        loader.invalidate();
        std::thread::sleep(Duration::from_millis(300));
        assert!(loader.try_recv().is_none());
        loader.request(LoadTarget::Movie(missing)).ok();
        let mut got = None;
        for _ in 0..100 {
            got = loader.try_recv();
            if got.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(matches!(got.map(|r| r.outcome), Some(Err(_))));
    }
}
