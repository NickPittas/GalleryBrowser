use std::path::Path;
use std::sync::Arc;

use filmcraft_media::reader::{FileReader, SharedReader, open_reader_within};
use filmcraft_media::sequence::{FrameLoader, ImageSequenceSource};
use filmcraft_media::still::StillSource;
use gallerybrowser_core::Sequence;

use crate::guard::{self, Guarded, convert};
use crate::{FrameRate, MediaError, Result, SharedSource};

/// Largest file read whole into memory to decode a still (or a sequence frame).
pub(crate) const MAX_STILL_BYTES: u64 = 1 << 30;
/// Largest non-streamable file the filmcraft openers read whole (stills ride on this path).
const MAX_WHOLE: u64 = 512 << 20;

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub(crate) fn ext_lower(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn io_err(path: &Path, e: &std::io::Error) -> MediaError {
    MediaError::Io(format!("{}: {e}", path.display()))
}

/// Read a file whole (bounded by `MAX_STILL_BYTES`) for image decoding.
pub(crate) fn read_capped(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|e| io_err(path, &e))?;
    if meta.len() > MAX_STILL_BYTES {
        return Err(MediaError::Unsupported(format!(
            "{}: file too large to decode as an image",
            path.display()
        )));
    }
    std::fs::read(path).map_err(|e| io_err(path, &e))
}

/// Open any supported file as a media source. Video containers are opened through
/// `filmcraft_media::reader::open_reader_within` + `filmcraft_codecs::reader_openers()` on a
/// `FileReader` (same call as filmcraft engine/src/media_pool.rs `probe_file`): only the container
/// index is read at open, samples on demand. Hardware decoders (`filmcraft-platform`, VA-API) are NOT
/// used. PSD/PSB and other photocraft-only formats are wrapped as still sources.
pub fn open(path: &Path) -> Result<SharedSource> {
    let name = file_name(path);
    let ext = ext_lower(path);
    if matches!(ext.as_str(), "psd" | "psb") {
        return open_psd(path, &name);
    }
    let reader: SharedReader = Arc::new(FileReader::open(path).map_err(|e| io_err(path, &e))?);
    let opened = guard::catch(|| {
        open_reader_within(
            &name,
            reader,
            &filmcraft_codecs::reader_openers(),
            &filmcraft_codecs::openers(),
            MAX_WHOLE,
        )
        .map_err(convert)
    });
    match opened {
        Ok(src) => Ok(Guarded::wrap(src)),
        // Only stills may be read whole; a damaged video must never be slurped into memory.
        Err(e) if is_av_extension(&ext) => Err(e),
        Err(e) => open_with_photocraft(path, &name).map_err(|_| e),
    }
}

fn is_av_extension(ext: &str) -> bool {
    filmcraft_media::VIDEO_EXTENSIONS.contains(&ext)
        || filmcraft_media::AUDIO_EXTENSIONS.contains(&ext)
}

/// Formats the filmcraft stills do not take (exotic TIFF, PCX, DDS, QOI, FITS, …): decode through
/// photocraft-codecs and serve the 8-bit sRGB result as a still.
fn open_with_photocraft(path: &Path, name: &str) -> Result<SharedSource> {
    let bytes = read_capped(path)?;
    if bytes.starts_with(b"8BPS") {
        return psd_still(name, &bytes);
    }
    guard::catch(|| {
        let img = photocraft_codecs::decode(&bytes)
            .map_err(|e| MediaError::Decode(format!("{name}: {e}")))?;
        let (w, h) = img.dimensions();
        let has_alpha = img.has_translucency();
        let rgba = img.to_rgba8();
        check_rgba(name, w, h, rgba.len())?;
        Ok(Guarded::wrap(Arc::new(StillSource::from_rgba(
            name,
            w,
            h,
            rgba,
            has_alpha,
            bytes.len() as u64,
        ))))
    })
}

fn open_psd(path: &Path, name: &str) -> Result<SharedSource> {
    let bytes = read_capped(path)?;
    psd_still(name, &bytes)
}

/// PSD/PSB merged composite (photocraft-psd) as a still.
fn psd_still(name: &str, bytes: &[u8]) -> Result<SharedSource> {
    guard::catch(|| {
        let file = photocraft_psd::PsdFile::from_bytes(bytes)
            .map_err(|e| MediaError::Decode(format!("{name}: {e}")))?;
        let has_alpha = file.merged_has_alpha();
        let img = file
            .composite_rgba8()
            .map_err(|e| MediaError::Decode(format!("{name}: {e}")))?;
        check_rgba(name, img.width, img.height, img.data.len())?;
        Ok(Guarded::wrap(Arc::new(StillSource::from_rgba(
            name,
            img.width,
            img.height,
            img.data,
            has_alpha,
            bytes.len() as u64,
        ))))
    })
}

fn check_rgba(name: &str, w: u32, h: u32, len: usize) -> Result<()> {
    let expect = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4));
    if w == 0 || h == 0 || expect != Some(len) {
        return Err(MediaError::Decode(format!(
            "{name}: decoded image has inconsistent size {w}x{h} ({len} bytes)"
        )));
    }
    Ok(())
}

/// Image sequence as a movie source at `fps`; frame `i` of the source is `seq.frames[i]` (sparse
/// numbering is preserved for display by `Sequence`, so there are no gaps to hold). Built on
/// `filmcraft_media::sequence::ImageSequenceSource` (lazy decoding, bounded frame cache).
pub fn open_sequence(seq: &Sequence, fps: FrameRate) -> Result<SharedSource> {
    if seq.frames.is_empty() {
        return Err(MediaError::Unsupported(
            "image sequence has no frames".into(),
        ));
    }
    let frames: Vec<Option<String>> = seq
        .frames
        .iter()
        .map(|f| Some(f.path.to_string_lossy().into_owned()))
        .collect();
    let loader: FrameLoader = Arc::new(|p: &str| {
        let meta = std::fs::metadata(p)?;
        if meta.len() > MAX_STILL_BYTES {
            return Err(std::io::Error::other("frame file too large"));
        }
        std::fs::read(p)
    });
    let name = seq.display_name();
    guard::catch(|| {
        let src = ImageSequenceSource::new(&name, frames, fps.sane(), loader).map_err(convert)?;
        Ok(Guarded::wrap(Arc::new(src)))
    })
}
