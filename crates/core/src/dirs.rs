use std::path::PathBuf;

use crate::Result;

/// Per-user directories. Every directory is created and PROBED FOR WRITABILITY (create+delete a temp file);
/// on failure falls back to `$XDG_*`-independent `~/.local/state/gallerybrowser/<kind>`, then to a
/// private temp dir. (Fixes the Python bug where an existing read-only dir was accepted.)
#[derive(Clone, Debug)]
pub struct AppDirs {
    /// `~/.config/gallerybrowser` — settings.json
    pub config: PathBuf,
    /// `~/.local/share/gallerybrowser` — library catalog
    pub data: PathBuf,
    /// `~/.cache/gallerybrowser` — thumbnails (`thumbnails/`), CACHEDIR.TAG
    pub cache: PathBuf,
    /// `$XDG_RUNTIME_DIR/gallerybrowser` (fallback: cache) — single-instance socket
    pub runtime: PathBuf,
}

impl AppDirs {
    pub fn resolve() -> Result<Self> {
        Err(crate::CoreError::NoWritableDir("config"))
    }
    /// Python-era SQLite database (`~/.cache/gallerybrowser/database.sqlite`) if present, for one-time import.
    pub fn legacy_database(&self) -> Option<PathBuf> {
        None
    }
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }
    pub fn thumbnails_dir(&self) -> PathBuf {
        self.cache.join("thumbnails")
    }
    pub fn library_dir(&self) -> PathBuf {
        self.data.join("library")
    }
    pub fn instance_socket(&self) -> PathBuf {
        self.runtime.join("instance.sock")
    }
}
