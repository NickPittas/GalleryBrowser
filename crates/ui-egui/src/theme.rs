//! Visual theme. The dark palette is ported from the Python app (`resources/styles.py`).

use egui::{
    Color32, CornerRadius, Pos2, Response, Sense, Shape, Stroke, StrokeKind, ThemePreference,
    Visuals, WidgetInfo, WidgetType, pos2, vec2,
};
use gallerybrowser_core::Theme;

pub const BG: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x0f);
pub const SURFACE: Color32 = Color32::from_rgb(0x18, 0x18, 0x18);
pub const SURFACE_2: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
pub const SURFACE_3: Color32 = Color32::from_rgb(0x25, 0x25, 0x25);
pub const SURFACE_4: Color32 = Color32::from_rgb(0x2c, 0x2c, 0x2c);
pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
pub const BORDER_HOVER: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x66, 0x66, 0x66);
pub const ACCENT: Color32 = Color32::from_rgb(0x21, 0x96, 0xf3);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x42, 0xa5, 0xf5);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(0x19, 0x76, 0xd2);
pub const ACCENT_SURFACE: Color32 = Color32::from_rgb(0x1a, 0x3a, 0x5c);
pub const SUCCESS: Color32 = Color32::from_rgb(0x4c, 0xaf, 0x50);
pub const WARNING: Color32 = Color32::from_rgb(0xff, 0x98, 0x00);
pub const ERROR: Color32 = Color32::from_rgb(0xf4, 0x43, 0x36);
/// Folder icons, favourites, rating stars.
pub const GOLD: Color32 = Color32::from_rgb(0xe2, 0xb3, 0x40);
/// Raised button surface (clearly lighter than `SURFACE` panels) and its hover/border colours.
const BUTTON_FILL: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x2e);
const BUTTON_FILL_HOVER: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
const BUTTON_STROKE: Color32 = Color32::from_rgb(0x42, 0x42, 0x42);
const BUTTON_STROKE_HOVER: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x5a);

/// Apply dark/light/system visuals (dark default matches the Python app's palette).
pub fn apply(ctx: &egui::Context, theme: Theme) {
    ctx.set_visuals_of(egui::Theme::Dark, dark());
    ctx.all_styles_mut(|s| {
        s.spacing.button_padding = vec2(8.0, 3.0);
        s.spacing.interact_size.y = 24.0;
    });
    ctx.set_visuals_of(egui::Theme::Light, light());
    ctx.set_theme(match theme {
        Theme::Dark => ThemePreference::Dark,
        Theme::Light => ThemePreference::Light,
        Theme::System => ThemePreference::System,
    });
}

fn dark() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = SURFACE;
    v.window_fill = SURFACE_3;
    v.window_stroke = Stroke::new(1.0, BORDER_HOVER);
    v.extreme_bg_color = BG;
    v.faint_bg_color = SURFACE_2;
    v.code_bg_color = SURFACE_2;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARNING;
    v.error_fg_color = ERROR;
    v.selection.bg_fill = ACCENT_SURFACE;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    let radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = SURFACE;
    w.noninteractive.weak_bg_fill = SURFACE;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    w.inactive.bg_fill = BUTTON_FILL;
    w.inactive.weak_bg_fill = BUTTON_FILL;
    w.inactive.bg_stroke = Stroke::new(1.0, BUTTON_STROKE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    w.hovered.bg_fill = BUTTON_FILL_HOVER;
    w.hovered.weak_bg_fill = BUTTON_FILL_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, BUTTON_STROKE_HOVER);
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.bg_fill = ACCENT_PRESSED;
    w.active.weak_bg_fill = ACCENT_PRESSED;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(2.0, Color32::WHITE);
    w.open.bg_fill = SURFACE_3;
    w.open.weak_bg_fill = SURFACE_3;
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = radius;
    }
    v
}

fn light() -> Visuals {
    let mut v = Visuals::light();
    v.hyperlink_color = ACCENT_PRESSED;
    v.selection.bg_fill = Color32::from_rgb(0xbb, 0xde, 0xfb);
    v.selection.stroke = Stroke::new(1.0, ACCENT_PRESSED);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xbc, 0xbc, 0xbc));
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x8a, 0x8a, 0x8a));
    let radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = radius;
    }
    v
}

/// Glyph-free toolbar icons painted with shapes, so they render regardless of the loaded fonts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Back,
    Forward,
    Up,
    Refresh,
    Close,
}

/// Icon edge length in points.
const ICON_SIZE: f32 = 16.0;

/// Framed square-ish icon button using the app-wide raised button look; the caller adds tooltips.
/// Disabled when the surrounding `Ui` is disabled (use `ui.add_enabled_ui`).
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str) -> Response {
    let size = vec2(ICON_SIZE + 14.0, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let visuals = *ui.style().interact(&response);
        let painter = ui.painter();
        painter.rect(
            rect.expand(visuals.expansion),
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        paint_icon(
            painter,
            icon,
            rect.center(),
            ICON_SIZE,
            Stroke::new(1.7, visuals.fg_stroke.color),
        );
    }
    response
}

/// Small borderless icon button (e.g. clear-search ×).
pub fn small_icon_button(ui: &mut egui::Ui, icon: Icon, label: &str) -> Response {
    let size = vec2(20.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let visuals = *ui.style().interact(&response);
        let painter = ui.painter();
        if response.hovered() || response.has_focus() {
            painter.rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        }
        let color = if response.hovered() {
            Color32::WHITE
        } else {
            TEXT_SECONDARY
        };
        paint_icon(painter, icon, rect.center(), 12.0, Stroke::new(1.5, color));
    }
    response
}

/// Vertical divider between toolbar groups.
pub fn group_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 18.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let x = rect.center().x.round() + 0.5;
        ui.painter().line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, BUTTON_STROKE_HOVER),
        );
    }
}

/// Small right-pointing chevron placed between breadcrumb chips.
pub fn path_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_icon(
            ui.painter(),
            Icon::Forward,
            rect.center(),
            8.0,
            Stroke::new(1.3, TEXT_MUTED),
        );
    }
}

/// Paint `icon` centred on `c` inside a `size`×`size` box.
fn paint_icon(painter: &egui::Painter, icon: Icon, c: Pos2, size: f32, stroke: Stroke) {
    let h = size * 0.5;
    match icon {
        Icon::Back => {
            let (dx, dy) = (h * 0.4, h * 0.62);
            painter.add(Shape::line(
                vec![
                    pos2(c.x + dx, c.y - dy),
                    pos2(c.x - dx, c.y),
                    pos2(c.x + dx, c.y + dy),
                ],
                stroke,
            ));
        }
        Icon::Forward => {
            let (dx, dy) = (h * 0.4, h * 0.62);
            painter.add(Shape::line(
                vec![
                    pos2(c.x - dx, c.y - dy),
                    pos2(c.x + dx, c.y),
                    pos2(c.x - dx, c.y + dy),
                ],
                stroke,
            ));
        }
        Icon::Up => {
            let (dx, dy) = (h * 0.6, h * 0.8);
            painter.add(Shape::line(
                vec![
                    pos2(c.x - dx, c.y - dy + h * 0.55),
                    pos2(c.x, c.y - dy),
                    pos2(c.x + dx, c.y - dy + h * 0.55),
                ],
                stroke,
            ));
            painter.line_segment([pos2(c.x, c.y - dy), pos2(c.x, c.y + dy)], stroke);
        }
        Icon::Refresh => {
            let r = h * 0.72;
            let start = -60.0_f32.to_radians();
            let end = 240.0_f32.to_radians();
            let steps = 24;
            let points: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let t = start + (end - start) * (i as f32 / steps as f32);
                    pos2(c.x + r * t.cos(), c.y + r * t.sin())
                })
                .collect();
            painter.add(Shape::line(points, stroke));
            // Arrowhead at the arc end, pointing along the direction of travel.
            let tip = pos2(c.x + r * end.cos(), c.y + r * end.sin());
            let tangent = vec2(-end.sin(), end.cos());
            let normal = vec2(-tangent.y, tangent.x);
            let s = h * 0.55;
            painter.line_segment([tip, tip - tangent * s + normal * s * 0.85], stroke);
            painter.line_segment([tip, tip - tangent * s - normal * s * 0.85], stroke);
        }
        Icon::Close => {
            let d = h * 0.5;
            painter.line_segment([pos2(c.x - d, c.y - d), pos2(c.x + d, c.y + d)], stroke);
            painter.line_segment([pos2(c.x - d, c.y + d), pos2(c.x + d, c.y - d)], stroke);
        }
    }
}
