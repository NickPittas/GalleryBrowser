use crate::state::AppState;

/// Right pane preview: still images (full decode on a worker, shown in viewer), videos and sequences
/// (Player + transport). Loads on focus change; respects settings.video_preview (autoplay).
pub struct PreviewState {}

impl PreviewState {
    pub fn new(_cache_bytes: u64) -> Self {
        Self {}
    }
    /// Point the preview at an entry (None clears). Idempotent for the same path.
    pub fn set_entry(&mut self, _entry: Option<&gallerybrowser_core::Entry>, _settings: &gallerybrowser_core::Settings) {}
    /// Stop playback and release decoders (app exit).
    pub fn shutdown(&mut self) {}
}

pub fn show(_ui: &mut egui::Ui, _state: &mut AppState) {}
