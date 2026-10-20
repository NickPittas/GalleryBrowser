use std::path::PathBuf;
use std::time::SystemTime;

use gallerybrowser_core::{Entry, MediaKind};

use crate::Result;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioStreamMeta {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: Option<u32>,
}

/// ComfyUI generation parameters parsed from PNG text chunks `prompt` / `workflow` (JSON).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComfyMetadata {
    pub positive: Option<String>,
    pub negative: Option<String>,
    pub seed: Option<i64>,
    pub steps: Option<i64>,
    pub cfg: Option<f64>,
    pub sampler: Option<String>,
    pub scheduler: Option<String>,
    pub model: Option<String>,
    pub prompt_json: Option<String>,
    pub workflow_json: Option<String>,
}

/// Everything the info pane shows. Every field optional: a failed probe still yields path/kind and the
/// error text (never panics on missing size etc.).
#[derive(Clone, Debug, Default)]
pub struct MediaMetadata {
    pub path: PathBuf,
    pub kind: Option<MediaKind>,
    pub file_size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub format: Option<String>,
    pub container: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_secs: Option<f64>,
    pub frame_rate: Option<f64>,
    pub frame_count: Option<u64>,
    pub video_codec: Option<String>,
    pub pixel_format: Option<String>,
    pub bit_rate: Option<u64>,
    pub color: Option<String>,
    pub has_alpha: Option<bool>,
    pub timecode: Option<String>,
    pub audio: Vec<AudioStreamMeta>,
    /// Sequence: (first, last, count, missing count).
    pub sequence_range: Option<(i64, i64, usize, usize)>,
    /// Human-readable EXIF (label, value) pairs.
    pub exif: Vec<(String, String)>,
    /// Raw PNG tEXt/iTXt/zTXt pairs (excluding huge JSON shown via `comfy`).
    pub text_chunks: Vec<(String, String)>,
    pub comfy: Option<ComfyMetadata>,
    pub error: Option<String>,
}

/// Blocking probe (call from a worker).
pub fn probe(_entry: &Entry) -> Result<MediaMetadata> {
    Err(crate::MediaError::Unsupported("stub".into()))
}

/// Background metadata probing; latest request wins (stale results dropped by generation).
pub struct MetadataWorker {}

impl MetadataWorker {
    pub fn new() -> Self {
        Self {}
    }
    pub fn request(&self, _entry: &Entry) {}
    /// Latest completed result for the latest request, if any.
    pub fn poll(&self) -> Option<MediaMetadata> {
        None
    }
}

impl Default for MetadataWorker {
    fn default() -> Self {
        Self::new()
    }
}
