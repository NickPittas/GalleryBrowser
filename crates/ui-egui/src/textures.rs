use std::path::PathBuf;

/// egui texture LRU for thumbnails keyed by (Entry::path, max_edge); uploads RgbaImage once.
#[derive(Default)]
pub struct TextureCache {}

impl TextureCache {
    pub fn get(&self, _path: &std::path::Path, _max_edge: u32) -> Option<egui::TextureHandle> {
        None
    }
    pub fn insert(&mut self, _ctx: &egui::Context, _path: PathBuf, _max_edge: u32, _img: &gallerybrowser_media::RgbaImage) -> egui::TextureHandle {
        _ctx.load_texture("stub", egui::ColorImage::example(), egui::TextureOptions::LINEAR)
    }
    /// Drop textures not in `keep` (after folder change).
    pub fn retain(&mut self, _keep: &dyn Fn(&std::path::Path) -> bool) {}
}
