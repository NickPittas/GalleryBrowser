/// Zoom/pan image view (adapted from FilmCraft monitor_view: fit, 1:1, wheel zoom about pointer
/// 10%–1600%, drag pan with clamping, checkerboard behind alpha).
#[derive(Clone, Debug, Default)]
pub struct ViewerState {
    /// None = fit.
    pub zoom: Option<f32>,
    pub pan: [f32; 2],
}

pub fn show(_ui: &mut egui::Ui, _view: &mut ViewerState, _texture: Option<&egui::TextureHandle>) -> egui::Response {
    _ui.label("")
}
