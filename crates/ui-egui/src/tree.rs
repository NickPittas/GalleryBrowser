use crate::state::AppState;

/// Folder tree (lazy: children listed on expand via core::list_subfolders, cached; respects show_hidden),
/// roots: Home, `/`, mounted media (/media, /run/media/$USER, /mnt); auto-expands to nav.current.
/// Accepts drops (Action::Drop). Context menu: add/remove favorite, open in file manager, new folder.
#[derive(Default)]
pub struct TreeState {}

pub fn show(_ui: &mut egui::Ui, _state: &mut AppState, _tree: &mut TreeState) {}
