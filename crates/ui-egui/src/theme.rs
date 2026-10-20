//! Visual theme. The dark palette is ported from the Python app (`resources/styles.py`).

use egui::{Color32, CornerRadius, Stroke, ThemePreference, Visuals};
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

/// Apply dark/light/system visuals (dark default matches the Python app's palette).
pub fn apply(ctx: &egui::Context, theme: Theme) {
    ctx.set_visuals_of(egui::Theme::Dark, dark());
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
    w.inactive.bg_fill = SURFACE_2;
    w.inactive.weak_bg_fill = SURFACE_2;
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    w.hovered.bg_fill = SURFACE_4;
    w.hovered.weak_bg_fill = SURFACE_4;
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER_HOVER);
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
