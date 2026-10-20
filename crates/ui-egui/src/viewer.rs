//! Zoom/pan picture view. The fit / magnify / pan geometry is adapted from FilmCraft
//! (crates/ui-egui/src/panels/monitor_view.rs `picture_rect` and `zoom_pan_input`, and
//! panels/monitor.rs `fit`): fit or 10%–1600% magnification, the wheel zooming about the pointer, drag
//! panning clamped to the picture, pixel aspect ratio applied to the displayed width.

use egui::{
    Color32, CornerRadius, Pos2, Rect, Response, Sense, TextureHandle, Ui, UiBuilder, Vec2, pos2,
    vec2,
};

/// Smallest magnification (10%).
pub const MIN_ZOOM: f32 = 0.1;
/// Largest magnification (1600%).
pub const MAX_ZOOM: f32 = 16.0;
/// Keyboard / button zoom step.
const ZOOM_STEP: f32 = 1.25;
/// Wheel travel (points) that zooms by about 10%.
const WHEEL_NOTCH: f32 = 40.0;
const CHECKER_TILE: f32 = 16.0;

/// A view change requested by buttons or keys; applied by the next [`show_image`] call, which knows
/// the viewport size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewerCommand {
    Fit,
    /// 100%: one image pixel per physical screen pixel.
    ActualSize,
    ZoomIn,
    ZoomOut,
}

/// Zoom/pan image view state.
#[derive(Clone, Debug, Default)]
pub struct ViewerState {
    /// None = fit. `Some(1.0)` = one image pixel per physical pixel.
    pub zoom: Option<f32>,
    /// Offset of the picture centre from the view centre, in points (clamped on every frame).
    pub pan: [f32; 2],
    /// Pending zoom command (see [`ViewerCommand`]).
    pub command: Option<ViewerCommand>,
    /// Effective magnification of the last drawn frame (also valid while fitting).
    pub shown_zoom: f32,
    /// Draw the small zoom toolbar in the top-right corner while the pointer is over the view.
    pub toolbar: bool,
}

impl ViewerState {
    pub fn fit(&mut self) {
        self.zoom = None;
        self.pan = [0.0, 0.0];
    }
}

/// The picture to draw. `size` is the decoded frame size in pixels (the texture may be smaller when
/// the frame exceeded the GPU texture limit); `par` is the pixel aspect ratio (width / height of one
/// pixel); `checker` draws a checkerboard behind transparent pixels.
pub struct ViewerImage<'a> {
    pub texture: &'a TextureHandle,
    pub size: [u32; 2],
    pub par: f32,
    pub checker: bool,
}

/// Convenience wrapper: draw `texture` at its own size with square pixels.
pub fn show(ui: &mut Ui, view: &mut ViewerState, texture: Option<&TextureHandle>) -> Response {
    let image = texture.map(|t| {
        let [w, h] = t.size();
        ViewerImage {
            texture: t,
            size: [w as u32, h as u32],
            par: 1.0,
            checker: true,
        }
    });
    show_image(ui, view, image)
}

/// Fill the available space with the picture; handles zoom/pan input. Returns the interaction
/// response of the whole view.
pub fn show_image(ui: &mut Ui, view: &mut ViewerState, image: Option<ViewerImage<'_>>) -> Response {
    let (area, resp) = ui.allocate_exact_size(
        ui.available_size().max(vec2(8.0, 8.0)),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(area);
    painter.rect_filled(area, CornerRadius::ZERO, Color32::from_gray(16));
    let Some(image) = image else {
        view.command = None;
        return resp;
    };
    let ppp = ui.ctx().pixels_per_point().max(0.1);
    let Some(disp) = display_size(&image) else {
        view.command = None;
        return resp;
    };
    let pointer = resp.hover_pos();

    // Commands from buttons / keys (anchored at the view centre).
    if let Some(cmd) = view.command.take() {
        let cur = effective_zoom(&picture_rect(area, disp, view, ppp), disp, ppp);
        match cmd {
            ViewerCommand::Fit => view.fit(),
            ViewerCommand::ActualSize => apply_zoom(view, area, disp, ppp, 1.0, area.center()),
            ViewerCommand::ZoomIn => {
                apply_zoom(view, area, disp, ppp, cur * ZOOM_STEP, area.center())
            }
            ViewerCommand::ZoomOut => {
                apply_zoom(view, area, disp, ppp, cur / ZOOM_STEP, area.center())
            }
        }
    }

    // Wheel / pinch zoom about the pointer (also from fit).
    if let Some(p) = pointer {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let f = pinch * (scroll * 1.1_f32.ln() / WHEEL_NOTCH).exp();
        if f.is_finite() && (f - 1.0).abs() > 1e-4 {
            let f = f.clamp(0.5, 2.0);
            let cur = effective_zoom(&picture_rect(area, disp, view, ppp), disp, ppp);
            apply_zoom(view, area, disp, ppp, cur * f, p);
        }
    }

    // Double-click toggles fit / 100% (anchored under the pointer).
    if resp.double_clicked() {
        if view.zoom.is_some() {
            view.fit();
        } else {
            apply_zoom(view, area, disp, ppp, 1.0, pointer.unwrap_or(area.center()));
        }
    }

    // Drag pans a magnified picture.
    if view.zoom.is_some() {
        if resp.dragged() {
            let d = resp.drag_delta();
            view.pan[0] += d.x;
            view.pan[1] += d.y;
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    let pic = picture_rect(area, disp, view, ppp);
    // write the clamped pan back so it cannot wind up beyond the picture edge
    if view.zoom.is_some() {
        let c = pic.center() - area.center();
        view.pan = [c.x, c.y];
    }
    view.shown_zoom = effective_zoom(&pic, disp, ppp);

    if image.checker {
        paint_checker(&painter, pic.intersect(area));
    }
    painter.image(
        image.texture.id(),
        pic,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    if view.toolbar && ui.rect_contains_pointer(area) {
        toolbar(ui, view, area);
    }
    resp
}

/// Frame size as displayed (pixel aspect applied to the width); None for degenerate frames.
fn display_size(image: &ViewerImage<'_>) -> Option<Vec2> {
    let par = if image.par.is_finite() && image.par > 0.0 {
        image.par.clamp(0.1, 10.0)
    } else {
        1.0
    };
    let v = vec2(image.size[0] as f32 * par, image.size[1] as f32);
    (v.x >= 1.0 && v.y >= 1.0).then_some(v)
}

/// Magnification of `pic` (1.0 = one image pixel per physical pixel).
fn effective_zoom(pic: &Rect, disp: Vec2, ppp: f32) -> f32 {
    if disp.x > 0.0 {
        pic.width() * ppp / disp.x
    } else {
        1.0
    }
}

/// Largest rect of the picture's aspect centred in `area`.
fn fit_rect(area: Rect, disp: Vec2) -> Rect {
    let s = (area.width() / disp.x).min(area.height() / disp.y);
    let s = if s.is_finite() && s > 0.0 { s } else { 0.0 };
    Rect::from_center_size(area.center(), vec2(disp.x * s, disp.y * s))
}

/// The on-screen picture rect: fitted, or magnified about the area centre plus the (clamped) pan.
pub fn picture_rect(area: Rect, disp: Vec2, v: &ViewerState, ppp: f32) -> Rect {
    match v.zoom {
        None => fit_rect(area, disp),
        Some(z) => {
            let size = disp * z / ppp.max(0.1);
            let max = ((size - area.size()) / 2.0).max(Vec2::ZERO);
            let pan = vec2(v.pan[0].clamp(-max.x, max.x), v.pan[1].clamp(-max.y, max.y));
            Rect::from_center_size(area.center() + pan, size)
        }
    }
}

/// Zoom to `new_zoom` (clamped to 10%–1600%) keeping the picture point under `anchor` fixed.
fn apply_zoom(v: &mut ViewerState, area: Rect, disp: Vec2, ppp: f32, new_zoom: f32, anchor: Pos2) {
    if !new_zoom.is_finite() {
        return;
    }
    let pic = picture_rect(area, disp, v, ppp);
    let cur = effective_zoom(&pic, disp, ppp);
    let z = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let k = if cur > 0.0 { z / cur } else { 1.0 };
    let centre = anchor - (anchor - pic.center()) * k;
    let size = disp * z / ppp;
    let max = ((size - area.size()) / 2.0).max(Vec2::ZERO);
    v.zoom = Some(z);
    v.pan = [
        (centre.x - area.center().x).clamp(-max.x, max.x),
        (centre.y - area.center().y).clamp(-max.y, max.y),
    ];
}

fn paint_checker(painter: &egui::Painter, rect: Rect) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_gray(190));
    let cols = ((rect.width() / CHECKER_TILE).ceil() as usize).min(512);
    let rows = ((rect.height() / CHECKER_TILE).ceil() as usize).min(512);
    for r in 0..rows {
        for c in 0..cols {
            if (r + c) % 2 == 1 {
                let min = rect.min + vec2(c as f32 * CHECKER_TILE, r as f32 * CHECKER_TILE);
                let tile =
                    Rect::from_min_size(min, vec2(CHECKER_TILE, CHECKER_TILE)).intersect(rect);
                painter.rect_filled(tile, CornerRadius::ZERO, Color32::from_gray(140));
            }
        }
    }
}

/// Zoom toolbar: − 83% + Fit 1:1, top-right corner of the view.
fn toolbar(ui: &mut Ui, view: &mut ViewerState, area: Rect) {
    let rect = Rect::from_min_size(area.right_top() + vec2(-210.0, 6.0), vec2(204.0, 24.0))
        .intersect(area);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    egui::Frame::NONE
        .fill(Color32::from_black_alpha(150))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(&mut child, |ui| {
            ui.horizontal(|ui| {
                if ui.small_button("−").on_hover_text("Zoom out (-)").clicked() {
                    view.command = Some(ViewerCommand::ZoomOut);
                }
                ui.label(
                    egui::RichText::new(format!("{:.0}%", view.shown_zoom * 100.0))
                        .monospace()
                        .color(Color32::WHITE),
                );
                if ui.small_button("+").on_hover_text("Zoom in (+)").clicked() {
                    view.command = Some(ViewerCommand::ZoomIn);
                }
                if ui
                    .small_button("Fit")
                    .on_hover_text("Fit to view (0)")
                    .clicked()
                {
                    view.command = Some(ViewerCommand::Fit);
                }
                if ui
                    .small_button("1:1")
                    .on_hover_text("Actual size (1)")
                    .clicked()
                {
                    view.command = Some(ViewerCommand::ActualSize);
                }
            });
        });
    if view.command.is_some() {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))
    }

    #[test]
    fn fit_is_centred_and_aspect_correct() {
        let r = fit_rect(area(), vec2(100.0, 100.0));
        assert_eq!(r.size(), vec2(200.0, 200.0));
        assert_eq!(r.center(), area().center());
    }

    #[test]
    fn fit_survives_degenerate_area() {
        let r = fit_rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0)),
            vec2(100.0, 100.0),
        );
        assert_eq!(r.size(), Vec2::ZERO);
    }

    #[test]
    fn zoom_is_clamped_and_pan_stays_inside() {
        let mut v = ViewerState::default();
        let disp = vec2(1000.0, 1000.0);
        apply_zoom(&mut v, area(), disp, 1.0, 1000.0, pos2(10.0, 10.0));
        assert_eq!(v.zoom, Some(MAX_ZOOM));
        let pic = picture_rect(area(), disp, &v, 1.0);
        assert!(pic.contains_rect(area()));
        apply_zoom(&mut v, area(), disp, 1.0, 0.0001, pos2(10.0, 10.0));
        assert_eq!(v.zoom, Some(MIN_ZOOM));
    }

    #[test]
    fn zoom_keeps_point_under_anchor() {
        let mut v = ViewerState::default();
        let disp = vec2(800.0, 400.0);
        let anchor = pos2(300.0, 120.0);
        let before = picture_rect(area(), disp, &v, 1.0);
        let u = (anchor - before.min) / before.size();
        apply_zoom(&mut v, area(), disp, 1.0, 2.0, anchor);
        let after = picture_rect(area(), disp, &v, 1.0);
        let p = after.min + u * after.size();
        assert!((p - anchor).length() < 0.01);
    }

    #[test]
    fn nan_zoom_is_ignored() {
        let mut v = ViewerState::default();
        apply_zoom(
            &mut v,
            area(),
            vec2(10.0, 10.0),
            1.0,
            f32::NAN,
            pos2(0.0, 0.0),
        );
        assert_eq!(v.zoom, None);
    }
}
