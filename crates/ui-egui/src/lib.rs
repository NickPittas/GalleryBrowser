//! GalleryBrowser egui frontend (layer L5). Panels never mutate domain state directly: they read
//! `AppState` and push [`Action`]s; `actions::dispatch` applies them and `actions::refresh_view` is the
//! ONE pipeline that rebuilds the visible entry list (scan → scope/library filter → search/kind filter
//! → sort → selection reconcile).

pub mod actions;
pub mod app;
pub mod batch_rename;
pub mod breadcrumb;
pub mod file_pane;
pub mod fullscreen;
pub mod info_pane;
pub mod preview;
pub mod settings_dialog;
pub mod shortcuts;
pub mod sidebar;
pub mod state;
pub mod textures;
pub mod theme;
pub mod transport;
pub mod tree;
pub mod viewer;

pub use app::{GalleryApp, StartupOptions};
pub use state::{Action, AppState};
