//! Viewer drawn inside the main window, filling it at its current size (no automatic OS fullscreen; a
//! second OS window froze the whole app on Wayland compositors). It owns a [`MediaView`]
//! (its own `Player`, off-thread opener, texture and zoom/pan state), shows an auto-hiding header +
//! transport overlay and follows `FullscreenState::path` (the Shell changes it on
//! `Action::FullscreenStep`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{
    Color32, CornerRadius, Event, Frame, Key, Margin, Rect, Ui, UiBuilder, ViewportCommand, pos2,
};
use gallerybrowser_core::{Entry, classify};

use crate::preview::{MediaView, sync_audio_to_settings};
use crate::state::{Action, AppState};
use crate::transport;
use crate::viewer::ViewerCommand;

/// Overlay (header, transport, cursor) hides after this long without input.
const IDLE_HIDE: Duration = Duration::from_millis(2500);

/// Fullscreen viewer: viewer + transport overlay (auto-hide), next/prev over visible media entries,
/// Esc closes, F toggles OS fullscreen, zoom keys.
pub struct FullscreenState {
    /// The entry being viewed (`Entry::path`). The Shell reassigns it to step; the viewer reloads.
    pub path: PathBuf,
    view: Option<MediaView>,
    /// Path the view was last asked to open.
    loaded: Option<PathBuf>,
    name: String,
    last_activity: Instant,
    last_audio: (f32, bool, bool),
}

impl FullscreenState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            view: None,
            loaded: None,
            name: String::new(),
            last_activity: Instant::now(),
            last_audio: (1.0, false, true),
        }
    }
}

/// Called every frame while `state.fullscreen` is `Some`; fills the whole root `ui` (the main window at its
/// current size). `os_restore` holds the window's OS-fullscreen state from before the user pressed F in the
/// viewer (`None` while F has not been used); [`restore_os_fullscreen`] applies it when the viewer closes.
pub fn show(ui: &mut Ui, state: &mut AppState, os_restore: &mut Option<bool>) {
    let Some(mut fs) = state.fullscreen.take() else {
        return;
    };
    init_view(state, &mut fs);
    content(ui, state, &mut fs, os_restore);
    // a CloseFullscreen queued by `content` is applied by the dispatcher; keep the state until then
    if state.fullscreen.is_none() {
        state.fullscreen = Some(fs);
    }
}

/// After the viewer closed, put the window back into the OS-fullscreen state it had before F was used.
pub fn restore_os_fullscreen(ctx: &egui::Context, state: &AppState, os_restore: &mut Option<bool>) {
    if state.fullscreen.is_none()
        && let Some(prior) = os_restore.take()
    {
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(prior));
        ctx.request_repaint();
    }
}

/// Create the player on first use, initialised from the settings; silence the preview player.
fn init_view(state: &mut AppState, fs: &mut FullscreenState) {
    if fs.view.is_some() {
        return;
    }
    state.preview.pause();
    let bytes = state.settings.ram_preview_mb.saturating_mul(1024 * 1024);
    let view = fs.view.insert(MediaView::new(bytes));
    let s = &state.settings;
    view.player.set_volume(s.volume.clamp(0.0, 1.0));
    view.player.set_muted(s.muted);
    view.player.set_looping(s.loop_playback);
    fs.last_audio = (s.volume.clamp(0.0, 1.0), s.muted, s.loop_playback);
    view.viewer.toolbar = true;
    state
        .preview
        .set_audio(fs.last_audio.0, fs.last_audio.1, fs.last_audio.2);
}

/// Entry for `path`: the visible entry, else a transient one classified by extension.
fn resolve_entry(state: &AppState, path: &std::path::Path) -> Option<Entry> {
    if let Some(e) = state.entry(path) {
        return Some(e.clone());
    }
    let kind = classify(path)?;
    Some(Entry {
        path: path.to_path_buf(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        kind,
        size: 0,
        modified: None,
        sequence: None,
    })
}

/// Reload the view when `fs.path` changed (same open-off-thread path as the preview pane).
fn reload_if_changed(state: &AppState, fs: &mut FullscreenState) {
    if fs.loaded.as_ref() == Some(&fs.path) {
        return;
    }
    let Some(view) = fs.view.as_mut() else {
        return;
    };
    fs.loaded = Some(fs.path.clone());
    match resolve_entry(state, &fs.path) {
        Some(entry) => {
            fs.name = entry.name.clone();
            view.open(&entry, true, state.settings.sequence_fps);
        }
        None => {
            fs.name = fs
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            view.fail("Unsupported file type");
        }
    }
}

#[derive(Default)]
struct Keys {
    close: bool,
    step: isize,
    toggle: bool,
    frame: i64,
    fullscreen: bool,
    command: Option<ViewerCommand>,
    activity: bool,
}

fn read_keys(ui: &Ui) -> Keys {
    ui.input(|i| {
        let mut k = Keys::default();
        let plain = !i.modifiers.command && !i.modifiers.alt;
        if i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed() {
            k.activity = true;
        }
        if i.events
            .iter()
            .any(|e| matches!(e, Event::Key { .. } | Event::MouseWheel { .. }))
        {
            k.activity = true;
        }
        if i.key_pressed(Key::Escape) {
            k.close = true;
        }
        if plain {
            if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp) {
                k.step -= 1;
            }
            if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::PageDown) {
                k.step += 1;
            }
            k.toggle = i.key_pressed(Key::Space);
            if i.key_pressed(Key::Comma) {
                k.frame -= 1;
            }
            if i.key_pressed(Key::Period) {
                k.frame += 1;
            }
            k.fullscreen = i.key_pressed(Key::F);
            if i.key_pressed(Key::Num0) {
                k.command = Some(ViewerCommand::Fit);
            } else if i.key_pressed(Key::Num1) {
                k.command = Some(ViewerCommand::ActualSize);
            } else if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                k.command = Some(ViewerCommand::ZoomIn);
            } else if i.key_pressed(Key::Minus) {
                k.command = Some(ViewerCommand::ZoomOut);
            }
        }
        k
    })
}

fn content(
    ui: &mut Ui,
    state: &mut AppState,
    fs: &mut FullscreenState,
    os_restore: &mut Option<bool>,
) {
    let ctx = ui.ctx().clone();
    reload_if_changed(state, fs);
    let keys = read_keys(ui);
    if keys.activity {
        fs.last_activity = Instant::now();
    }
    if keys.close {
        state.push(Action::CloseFullscreen);
        ctx.request_repaint();
    }
    if keys.step != 0 {
        state.push(Action::FullscreenStep(keys.step));
        ctx.request_repaint();
    }
    if keys.fullscreen {
        let now = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        match os_restore.take() {
            // F again: go back to the state the window had before the first F
            Some(prior) => ctx.send_viewport_cmd(ViewportCommand::Fullscreen(prior)),
            None => {
                *os_restore = Some(now);
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!now));
            }
        }
    }

    let idle = fs.last_activity.elapsed();
    let visible = idle < IDLE_HIDE;
    if visible {
        ctx.request_repaint_after(IDLE_HIDE.saturating_sub(idle) + Duration::from_millis(50));
    }
    let opacity = ctx.animate_bool_with_time(
        egui::Id::new("gallerybrowser-fullscreen-overlay"),
        visible,
        0.2,
    );

    let index = state.entries.iter().position(|e| e.path == fs.path);
    let total = state.entries.len();
    let name = fs.name.clone();
    let Some(view) = fs.view.as_mut() else {
        return;
    };
    view.poll(&ctx);
    view.viewer.toolbar = visible;
    if let Some(cmd) = keys.command {
        view.viewer.command = Some(cmd);
    }
    if view.is_movie() {
        if keys.toggle {
            view.player.toggle();
        }
        if keys.frame != 0 {
            view.player.step(keys.frame);
        }
    }

    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(Color32::BLACK))
        .show(ui, |ui| {
            let resp = view.picture_ui(ui);
            let area = resp.rect;
            if !visible && resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::None);
            }
            if opacity <= 0.01 {
                return;
            }
            // header
            let header = Rect::from_min_max(area.left_top(), pos2(area.right(), area.top() + 36.0));
            overlay_bar(ui, header, opacity, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(index.is_some_and(|i| i > 0), egui::Button::new("Prev"))
                        .on_hover_text("Previous (Left)")
                        .clicked()
                    {
                        state.push(Action::FullscreenStep(-1));
                    }
                    if ui
                        .add_enabled(
                            index.is_some_and(|i| i + 1 < total),
                            egui::Button::new("Next"),
                        )
                        .on_hover_text("Next (Right)")
                        .clicked()
                    {
                        state.push(Action::FullscreenStep(1));
                    }
                    if let Some(i) = index {
                        ui.label(
                            egui::RichText::new(format!("{} / {}", i + 1, total))
                                .monospace()
                                .color(Color32::WHITE),
                        );
                    }
                    ui.label(egui::RichText::new(&name).strong().color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").on_hover_text("Close (Esc)").clicked() {
                            state.push(Action::CloseFullscreen);
                        }
                    });
                });
            });
            // transport
            if view.is_movie() {
                let h = transport::HEIGHT + 16.0;
                let bar = Rect::from_min_max(
                    pos2(area.left() + 16.0, area.bottom() - h - 12.0),
                    pos2(area.right() - 16.0, area.bottom() - 12.0),
                );
                overlay_bar(ui, bar, opacity, |ui| {
                    transport::show(ui, &mut view.player, view.sequence.as_deref())
                });
            }
        });

    sync_audio(state, fs);
}

/// Semi-transparent bar anchored at `rect` (drawn above the picture).
fn overlay_bar(ui: &mut Ui, rect: Rect, opacity: f32, add: impl FnOnce(&mut Ui)) {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.set_opacity(opacity);
    Frame::NONE
        .fill(Color32::from_black_alpha(170))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(6))
        .show(&mut child, |ui| {
            ui.set_width((rect.width() - 12.0).max(0.0));
            add(ui);
        });
}

/// Keep volume/mute/loop shared between the fullscreen player, the preview player and the settings.
fn sync_audio(state: &mut AppState, fs: &mut FullscreenState) {
    let Some(view) = fs.view.as_mut() else {
        return;
    };
    let st = view.player.state();
    let cur = (st.volume, st.muted, st.looping);
    if cur != fs.last_audio {
        fs.last_audio = cur;
        state.preview.set_audio(cur.0, cur.1, cur.2);
        sync_audio_to_settings(&mut state.settings, cur);
    } else {
        let p = state.preview.audio_settings();
        if p != cur {
            view.player.set_volume(p.0);
            view.player.set_muted(p.1);
            view.player.set_looping(p.2);
            fs.last_audio = p;
        }
    }
}
