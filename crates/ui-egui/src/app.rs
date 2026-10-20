use std::path::PathBuf;

use crate::state::AppState;

#[derive(Debug, Default)]
pub struct StartupOptions {
    /// Folders/files from the command line: a folder navigates; a file navigates to its folder,
    /// selects it and (for media) opens it in the fullscreen viewer.
    pub paths: Vec<PathBuf>,
    /// Paths forwarded by later instances (single-instance IPC) arrive here.
    pub incoming: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
}

/// eframe application: layout = menu bar + toolbar (top), breadcrumb, left side panel (tree, favorites,
/// recents, tags, collections, rating filter), central file pane, right preview+info (toggleable),
/// status bar (bottom); fullscreen viewer as a separate immediate viewport.
pub struct GalleryApp {
    pub state: AppState,
    pub incoming: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
}

impl GalleryApp {
    /// Builds all services from `AppDirs::resolve()` + `Settings::load`, opens the library
    /// (falls back to in-memory with an error status), applies theme, handles `opts.paths`
    /// (else `settings.last_folder`, else home). Err = unrecoverable startup failure message.
    pub fn new(_cc: &eframe::CreationContext<'_>, _opts: StartupOptions) -> Result<Self, String> {
        Err("stub".into())
    }
}

impl eframe::App for GalleryApp {
    /// Per-frame non-UI work: poll workers/IPC, dispatch actions, shortcuts, fullscreen viewport.
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
    /// Lay out all panels into the root `ui`.
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {}
    /// Save settings (incl. last_folder, volume), shut down preview/player and workers.
    fn on_exit(&mut self) {}
}
