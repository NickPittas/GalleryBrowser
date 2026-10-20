use std::path::PathBuf;
use std::sync::Arc;

use gallerybrowser_core::Entry;

use crate::RgbaImage;

#[derive(Clone, Debug)]
pub struct ThumbnailerConfig {
    /// `AppDirs::thumbnails_dir()`
    pub cache_dir: PathBuf,
    pub memory_bytes: u64,
    /// From `Settings::max_cache_mb`.
    pub disk_bytes: u64,
    /// From `Settings::thumbnail_threads`.
    pub threads: usize,
}

#[derive(Clone, Debug)]
pub struct ThumbResult {
    /// `Entry::path` the request was made for.
    pub path: PathBuf,
    /// Requested max edge.
    pub max_edge: u32,
    pub image: std::result::Result<Arc<RgbaImage>, String>,
}

/// Background thumbnail service: memory LRU + disk cache keyed by (path, size, mtime, max_edge).
/// Videos: frame at 10% of duration (clamped to [0, 5s]); sequences: representative frame.
/// Cancellation is generation-based: `begin_batch` invalidates queued work (no stop/restart races).
pub struct Thumbnailer {}

impl Thumbnailer {
    pub fn new(_cfg: ThumbnailerConfig) -> Self {
        Self {}
    }
    /// Apply new budgets/threads at runtime (settings dialog).
    pub fn reconfigure(&self, _cfg: ThumbnailerConfig) {}
    /// Drop all queued (not yet started) requests.
    pub fn begin_batch(&self) {}
    /// Memory-cache hit, synchronous and cheap.
    pub fn cached(&self, _entry: &Entry, _max_edge: u32) -> Option<Arc<RgbaImage>> {
        None
    }
    /// Queue generation (deduplicated). Higher priority first (visible items).
    pub fn request(&self, _entry: &Entry, _max_edge: u32, _priority: i32) {}
    pub fn poll(&self) -> Vec<ThumbResult> {
        Vec::new()
    }
    /// Delete the disk cache.
    pub fn clear_disk_cache(&self) -> crate::Result<()> {
        Ok(())
    }
}
