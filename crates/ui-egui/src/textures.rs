use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Upper bound on cached thumbnail textures.
const DEFAULT_MAX_COUNT: usize = 2000;
/// Upper bound on the RGBA bytes held by cached thumbnail textures.
const DEFAULT_MAX_BYTES: usize = 384 * 1024 * 1024;

struct Slot {
    handle: egui::TextureHandle,
    bytes: usize,
    /// Last-use stamp; updated through `&self` in [`TextureCache::get`].
    used: AtomicU64,
}

/// egui texture LRU for thumbnails keyed by (Entry::path, max_edge); uploads RgbaImage once.
/// Bounded by entry count and by RGBA bytes; the least recently used textures are evicted first.
pub struct TextureCache {
    slots: HashMap<(PathBuf, u32), Slot>,
    clock: AtomicU64,
    bytes: usize,
    max_count: usize,
    max_bytes: usize,
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::with_limits(DEFAULT_MAX_COUNT, DEFAULT_MAX_BYTES)
    }
}

impl TextureCache {
    /// Cache bounded by `max_count` textures and `max_bytes` of RGBA data (each at least 1).
    pub fn with_limits(max_count: usize, max_bytes: usize) -> Self {
        Self {
            slots: HashMap::new(),
            clock: AtomicU64::new(0),
            bytes: 0,
            max_count: max_count.max(1),
            max_bytes: max_bytes.max(1),
        }
    }

    fn tick(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// RGBA bytes currently held.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn get(&self, path: &std::path::Path, max_edge: u32) -> Option<egui::TextureHandle> {
        // The map is keyed by an owned PathBuf; build the key without touching the filesystem.
        let slot = self.slots.get(&(path.to_path_buf(), max_edge))?;
        slot.used.store(self.tick(), Ordering::Relaxed);
        Some(slot.handle.clone())
    }

    pub fn insert(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        max_edge: u32,
        img: &gallerybrowser_media::RgbaImage,
    ) -> egui::TextureHandle {
        let color = color_image(img);
        let bytes = color.pixels.len().saturating_mul(4);
        let name = format!("thumb:{}@{max_edge}", path.display());
        let handle = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);
        let slot = Slot {
            handle: handle.clone(),
            bytes,
            used: AtomicU64::new(self.tick()),
        };
        if let Some(old) = self.slots.insert((path, max_edge), slot) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.evict();
        handle
    }

    /// Drop textures not in `keep` (after folder change).
    pub fn retain(&mut self, keep: &dyn Fn(&std::path::Path) -> bool) {
        let mut freed = 0usize;
        self.slots.retain(|(path, _), slot| {
            let keep_it = keep(path);
            if !keep_it {
                freed = freed.saturating_add(slot.bytes);
            }
            keep_it
        });
        self.bytes = self.bytes.saturating_sub(freed);
    }

    /// Drop everything (e.g. after the disk cache was cleared).
    pub fn clear(&mut self) {
        self.slots.clear();
        self.bytes = 0;
    }

    /// Evict least-recently-used slots until both limits hold (the newest slot always survives).
    fn evict(&mut self) {
        while self.slots.len() > 1
            && (self.slots.len() > self.max_count || self.bytes > self.max_bytes)
        {
            let oldest = self
                .slots
                .iter()
                .min_by_key(|(_, s)| s.used.load(Ordering::Relaxed))
                .map(|(k, _)| k.clone());
            let Some(key) = oldest else { break };
            if let Some(slot) = self.slots.remove(&key) {
                self.bytes = self.bytes.saturating_sub(slot.bytes);
            }
        }
    }

    #[cfg(test)]
    fn contains(&self, path: &std::path::Path, max_edge: u32) -> bool {
        self.slots.contains_key(&(path.to_path_buf(), max_edge))
    }
}

/// Texture-cache `max_edge` key of one hover-scrub frame (`slot` of `slots`) of a card whose
/// thumbnails are `edge` wide. Bit 31 keeps it apart from every real thumbnail edge; slots, slot
/// and edge take 10 bits each (a larger edge shares the top key, which only costs a re-upload).
pub fn scrub_edge(edge: u32, slot: u32, slots: u32) -> u32 {
    let slots = slots.clamp(1, 1024);
    let slot = slot.min(slots - 1);
    0x8000_0000 | ((slots - 1) << 20) | (slot << 10) | edge.min(0x3FF)
}

/// Straight-alpha RGBA8 → egui image. A malformed buffer (length not `w*h*4`, or an empty image)
/// becomes a 1×1 transparent image instead of panicking.
fn color_image(img: &gallerybrowser_media::RgbaImage) -> egui::ColorImage {
    let (w, h) = (img.width as usize, img.height as usize);
    let expected = w.checked_mul(h).and_then(|n| n.checked_mul(4));
    match expected {
        Some(n) if n > 0 && img.pixels.len() == n => {
            egui::ColorImage::from_rgba_unmultiplied([w, h], img.pixels.as_slice())
        }
        _ => egui::ColorImage::filled([1, 1], egui::Color32::TRANSPARENT),
    }
}

#[cfg(test)]
#[allow(unused_must_use)]
mod tests {
    use super::*;
    use gallerybrowser_media::RgbaImage;
    use std::path::Path;
    use std::sync::Arc;

    fn img(w: u32, h: u32) -> RgbaImage {
        RgbaImage {
            width: w,
            height: h,
            pixels: Arc::new(vec![255; (w * h * 4) as usize]),
        }
    }

    #[test]
    fn evicts_least_recently_used_by_count() {
        let ctx = egui::Context::default();
        let mut c = TextureCache::with_limits(2, usize::MAX);
        c.insert(&ctx, "a".into(), 128, &img(2, 2));
        c.insert(&ctx, "b".into(), 128, &img(2, 2));
        // Touch `a` so `b` becomes the oldest.
        assert!(c.get(Path::new("a"), 128).is_some());
        c.insert(&ctx, "c".into(), 128, &img(2, 2));
        assert_eq!(c.len(), 2);
        assert!(c.contains(Path::new("a"), 128));
        assert!(!c.contains(Path::new("b"), 128));
        assert!(c.contains(Path::new("c"), 128));
    }

    #[test]
    fn evicts_by_bytes_and_tracks_total() {
        let ctx = egui::Context::default();
        let mut c = TextureCache::with_limits(100, 2 * 4 * 4 + 1);
        c.insert(&ctx, "a".into(), 64, &img(2, 2));
        c.insert(&ctx, "b".into(), 64, &img(2, 2));
        assert_eq!(c.bytes(), 32);
        c.insert(&ctx, "c".into(), 64, &img(2, 2));
        assert_eq!(c.len(), 2);
        assert_eq!(c.bytes(), 32);
        assert!(!c.contains(Path::new("a"), 64));
    }

    #[test]
    fn keys_by_edge_and_replaces_same_key() {
        let ctx = egui::Context::default();
        let mut c = TextureCache::default();
        c.insert(&ctx, "a".into(), 64, &img(2, 2));
        c.insert(&ctx, "a".into(), 128, &img(4, 4));
        assert_eq!(c.len(), 2);
        c.insert(&ctx, "a".into(), 64, &img(4, 4));
        assert_eq!(c.len(), 2);
        assert_eq!(c.bytes(), 4 * 4 * 4 * 2);
    }

    #[test]
    fn retain_filters_and_updates_bytes() {
        let ctx = egui::Context::default();
        let mut c = TextureCache::default();
        c.insert(&ctx, "keep".into(), 64, &img(2, 2));
        c.insert(&ctx, "drop".into(), 64, &img(2, 2));
        c.retain(&|p| p == Path::new("keep"));
        assert_eq!(c.len(), 1);
        assert_eq!(c.bytes(), 16);
        assert!(c.get(Path::new("drop"), 64).is_none());
    }

    #[test]
    fn malformed_image_does_not_panic() {
        let ctx = egui::Context::default();
        let mut c = TextureCache::default();
        let bad = RgbaImage {
            width: 10,
            height: 10,
            pixels: Arc::new(vec![0; 3]),
        };
        let h = c.insert(&ctx, "bad".into(), 64, &bad);
        assert_eq!(h.size(), [1, 1]);
        let empty = RgbaImage {
            width: 0,
            height: 0,
            pixels: Arc::new(Vec::new()),
        };
        c.insert(&ctx, "empty".into(), 64, &empty);
        assert_eq!(c.len(), 2);
    }
}
