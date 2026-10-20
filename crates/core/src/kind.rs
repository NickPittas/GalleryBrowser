use std::path::Path;

/// What an entry is. `Sequence` is only produced by sequence detection, never by [`classify`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum MediaKind {
    Image,
    Video,
    Sequence,
    Audio,
}

/// Lower-case extensions without dot. Superset of the Python app (png jpg jpeg tif tiff bmp gif webp exr tga psd)
/// plus formats the ArtCraft decoders handle (dpx hdr dng psb).
pub const IMAGE_EXTENSIONS: &[&str] = &[];
/// Superset of the Python app (mp4 mov avi mkv mxf webm mpg mpeg) plus filmcraft_media::VIDEO_EXTENSIONS.
pub const VIDEO_EXTENSIONS: &[&str] = &[];
pub const AUDIO_EXTENSIONS: &[&str] = &[];

/// Classify by (case-insensitive) extension. `None` = not browsable media.
pub fn classify(_path: &Path) -> Option<MediaKind> {
    None
}
