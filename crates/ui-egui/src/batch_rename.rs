use crate::state::AppState;

use gallerybrowser_fileops::RenameRule;

/// Batch rename dialog: rule editor + live preview table (from → to, status colored); Apply enabled
/// only when plan.is_valid(); pushes Action::BatchRename(plan).
pub struct BatchRenameDialog {
    pub files: Vec<std::path::PathBuf>,
    pub rule: RenameRule,
}

pub fn show(_ctx: &egui::Context, _state: &mut AppState) {}
