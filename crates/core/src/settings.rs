use std::path::PathBuf;

use crate::{AppDirs, Result, SortKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Theme { #[default] Dark, Light, System }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ViewMode { #[default] Grid, List }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FilterScope { #[default] Folder, Library }

/// Persisted user settings (`settings.json`). Unknown keys are ignored, missing keys default,
/// out-of-range values are clamped on load. Saved atomically (temp file + rename).
/// Every field MUST have a runtime consumer (the Python app had four dead settings).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Grid thumbnail edge in px: one of 64, 128, 256, 512 (default 128).
    pub thumbnail_size: u32,
    pub show_hidden: bool,
    /// Auto-play videos/sequences when selected in the preview pane.
    pub video_preview: bool,
    /// Thumbnail decoder threads (1..=16, default 2).
    pub thumbnail_threads: usize,
    /// Disk thumbnail cache budget in MB (default 1000).
    pub max_cache_mb: u64,
    /// Playback frame cache budget in MB (default 2048).
    pub ram_preview_mb: u64,
    pub collapse_sequences: bool,
    /// Frame rate used for image sequences (default 24).
    pub sequence_fps: f64,
    pub view_mode: ViewMode,
    pub sort_key: SortKey,
    pub sort_ascending: bool,
    pub show_preview: bool,
    pub theme: Theme,
    pub volume: f32,
    pub muted: bool,
    pub loop_playback: bool,
    pub favorites: Vec<PathBuf>,
    /// Newest first, deduplicated, at most `recents_max`.
    pub recents: Vec<PathBuf>,
    pub recents_max: usize,
    pub last_folder: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            thumbnail_size: 128,
            show_hidden: false,
            video_preview: true,
            thumbnail_threads: 2,
            max_cache_mb: 1000,
            ram_preview_mb: 2048,
            collapse_sequences: true,
            sequence_fps: 24.0,
            view_mode: ViewMode::Grid,
            sort_key: SortKey::Name,
            sort_ascending: true,
            show_preview: true,
            theme: Theme::Dark,
            volume: 1.0,
            muted: false,
            loop_playback: true,
            favorites: Vec::new(),
            recents: Vec::new(),
            recents_max: 15,
            last_folder: None,
        }
    }
}

impl Settings {
    /// Never fails: unreadable/corrupt file → defaults (logged).
    pub fn load(_dirs: &AppDirs) -> Self {
        Self::default()
    }
    pub fn save(&self, _dirs: &AppDirs) -> Result<()> {
        Ok(())
    }
    /// Push to front of recents (dedupe, truncate).
    pub fn add_recent(&mut self, _folder: PathBuf) {}
}
