use crate::state::AppState;

/// Fullscreen viewer in its own immediate viewport: viewer + transport overlay (auto-hide), next/prev
/// over visible media entries, Esc closes, F toggles OS fullscreen, zoom keys.
pub struct FullscreenState {
    pub path: std::path::PathBuf,
}

pub fn show(_ctx: &egui::Context, _state: &mut AppState) {}
