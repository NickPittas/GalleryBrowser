use crate::state::AppState;

use gallerybrowser_core::Settings;

/// Editable copy of Settings; OK pushes Action::ApplySettings.
pub struct SettingsDialog {
    pub draft: Settings,
}

pub fn show(_ctx: &egui::Context, _state: &mut AppState) {}
