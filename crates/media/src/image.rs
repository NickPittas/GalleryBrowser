use std::sync::Arc;

use crate::{Result, VideoFrame};

/// Straight-alpha sRGB RGBA8, tightly packed (`pixels.len() == width*height*4`).
#[derive(Clone, Debug)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
}

/// Convert any decoded frame (YUV 8/16, f32, RGBA) to packed RGBA8 (via filmcraft-frame conversions).
pub fn frame_to_rgba(_frame: &VideoFrame) -> Result<RgbaImage> {
    Err(crate::MediaError::Unsupported("stub".into()))
}
