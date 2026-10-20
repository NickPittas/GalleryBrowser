//! UI-free playback engine. One `Player` per view (preview pane, fullscreen). Decodes ahead on worker
//! threads into a RAM-budgeted frame cache; audio (cpal) is the master clock when the source has audio,
//! otherwise a monotonic clock. Generation counters drop stale decodes after `load`/`seek`.
//! Volume, mute, speed and loop survive `load` (Python bugs: they silently reset).

use std::sync::Arc;
use std::time::Instant;

use gallerybrowser_media::{FrameRate, SharedSource, Tick, VideoFrame};

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub loaded: bool,
    pub playing: bool,
    pub position: Tick,
    pub duration: Tick,
    pub frame_rate: FrameRate,
    /// 0-based frame index at `position`.
    pub frame_index: i64,
    pub frame_count: i64,
    pub speed: f32,
    pub volume: f32,
    pub muted: bool,
    pub looping: bool,
    pub has_audio: bool,
    /// Cached frames ahead of the playhead / frames wanted (0..=1) for a buffer indicator.
    pub buffered: f32,
    /// Last decode/audio error, shown by the UI.
    pub error: Option<String>,
}

pub struct Player {}

impl Player {
    /// `cache_bytes` from `Settings::ram_preview_mb`. Audio device failure → silent playback + `error`.
    pub fn new(_cache_bytes: u64) -> Self {
        Self {}
    }
    /// Replace the source; resets position/duration/cache; keeps volume/mute/speed/loop.
    /// Shows the first frame without starting playback unless `autoplay`.
    pub fn load(&mut self, _source: SharedSource, _autoplay: bool) {}
    /// Unload (selection cleared / non-media).
    pub fn unload(&mut self) {}
    pub fn play(&mut self) {}
    pub fn pause(&mut self) {}
    pub fn toggle(&mut self) {}
    /// Pause and return to the first frame; the source stays loaded (Play resumes).
    pub fn stop(&mut self) {}
    pub fn seek(&mut self, _t: Tick) {}
    pub fn seek_frame(&mut self, _index: i64) {}
    /// Pause and move by `delta` frames (clamped).
    pub fn step(&mut self, _delta: i64) {}
    /// 0.1..=8.0; negative not supported.
    pub fn set_speed(&mut self, _speed: f32) {}
    pub fn set_volume(&mut self, _v: f32) {}
    pub fn set_muted(&mut self, _m: bool) {}
    pub fn set_looping(&mut self, _l: bool) {}
    pub fn set_cache_bytes(&mut self, _bytes: u64) {}
    /// Advance the clock; returns a new frame to display when it changed since the last call.
    /// Call once per UI frame; the UI should `request_repaint` while `state().playing`.
    pub fn tick(&mut self, _now: Instant) -> Option<Arc<VideoFrame>> {
        None
    }
    pub fn state(&self) -> PlayerState {
        PlayerState {
            loaded: false,
            playing: false,
            position: Tick(0),
            duration: Tick(0),
            frame_rate: FrameRate::default(),
            frame_index: 0,
            frame_count: 0,
            speed: 1.0,
            volume: 1.0,
            muted: false,
            looping: true,
            has_audio: false,
            buffered: 0.0,
            error: None,
        }
    }
}
