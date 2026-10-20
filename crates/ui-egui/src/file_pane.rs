use crate::state::AppState;

/// Central pane: toolbar row (search, kind filter, sort, view mode, size slider, collapse sequences),
/// virtualized grid (thumbnail cards: image, name, sequence/video badge, rating stars) or list
/// (columns: name, kind, size, modified, rating; sortable headers). Selection via Action::Select/MoveFocus,
/// double-click → Activate, context menu, inline rename, drag out (internal) / drop in. Requests
/// thumbnails for visible rows only (priority by visibility).
#[derive(Default)]
pub struct FilePaneState {}

pub fn show(_ui: &mut egui::Ui, _state: &mut AppState, _pane: &mut FilePaneState) {}
