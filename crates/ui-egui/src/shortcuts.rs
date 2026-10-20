use crate::state::AppState;

/// Global keyboard shortcuts (when no text field has focus): see docs/user_guide/KEYBOARD_SHORTCUTS.md.
pub fn handle(_ctx: &egui::Context, _state: &mut AppState) {}

/// (keys, description) rows for the shortcuts dialog.
pub fn table() -> Vec<(&'static str, &'static str)> {
    Vec::new()
}
