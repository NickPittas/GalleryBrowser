use std::path::Path;

use gallerybrowser_core::Sequence;

use crate::{FrameRate, Result, SharedSource};

/// Open any supported file as a media source. Video containers are opened through
/// `filmcraft_media::reader::open_reader_within` + `filmcraft_codecs::reader_openers()` on a
/// `FileReader` (see filmcraft engine/src/media_pool.rs). Hardware decoders (`filmcraft-platform`,
/// VA-API) are NOT used. PSD/PSB and other photocraft-only formats are wrapped as still sources.
pub fn open(_path: &Path) -> Result<SharedSource> {
    Err(crate::MediaError::Unsupported("stub".into()))
}

/// Image sequence as a movie source at `fps`; gaps hold the previous frame; frame `i` of the source
/// is `seq.frames[i]` (sparse numbering preserved for display via `Sequence`).
pub fn open_sequence(_seq: &Sequence, _fps: FrameRate) -> Result<SharedSource> {
    Err(crate::MediaError::Unsupported("stub".into()))
}
