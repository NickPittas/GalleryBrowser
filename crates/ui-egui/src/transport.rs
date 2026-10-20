//! Shared playback controls (preview pane and fullscreen viewer).

use egui::{
    Button, Color32, CornerRadius, Label, Pos2, Rect, Response, RichText, Sense, Shape, Slider,
    Stroke, StrokeKind, Ui, vec2,
};
use gallerybrowser_core::Sequence;
use gallerybrowser_media::Tick;
use gallerybrowser_player::{Player, PlayerState};

/// Playback speeds offered by the speed combo.
pub const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

/// Height of one control row of buttons.
const ROW_HEIGHT: f32 = 24.0;
/// Height of the scrubber row.
const SCRUB_HEIGHT: f32 = 20.0;
/// Vertical gap between rows.
const ROW_GAP: f32 = 4.0;
/// Horizontal gap between controls.
const ITEM_GAP: f32 = 6.0;

/// Height reserved for the three control rows (scrubber, play/step/readout, speed/loop/volume),
/// including the gaps between them.
pub const HEIGHT: f32 = SCRUB_HEIGHT + 2.0 * ROW_HEIGHT + 2.0 * ROW_GAP;

/// Shared playback controls: play/pause, stop, step ±1, timeline scrubber (drag does not get
/// overwritten by playback), time/frame readout (sequences show source frame numbers), speed combo,
/// loop, volume + mute. `sequence` gives source frame numbers for display.
///
/// Three rows: scrubber; transport buttons + readout; speed, loop, mute, volume. Nothing is painted
/// outside the allocated rows, so it lays out cleanly at any width of 260 px or more.
pub fn show(ui: &mut Ui, player: &mut Player, sequence: Option<&Sequence>) {
    let st = player.state();
    ui.add_enabled_ui(st.loaded, |ui| {
        ui.vertical(|ui| {
            {
                let spacing = ui.spacing_mut();
                spacing.item_spacing = vec2(ITEM_GAP, ROW_GAP);
                spacing.interact_size.y = ROW_HEIGHT;
            }
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), SCRUB_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| scrubber(ui, player, &st),
            );
            ui.horizontal(|ui| playback_row(ui, player, &st, sequence));
            ui.horizontal(|ui| audio_row(ui, player, &st));
        });
    });
}

fn playback_row(ui: &mut Ui, player: &mut Player, st: &PlayerState, sequence: Option<&Sequence>) {
    if icon_button(
        ui,
        if st.playing { Icon::Pause } else { Icon::Play },
        "Play / Pause (Space)",
    )
    .clicked()
    {
        player.toggle();
    }
    if icon_button(ui, Icon::Stop, "Stop (back to the first frame)").clicked() {
        player.stop();
    }
    if icon_button(ui, Icon::StepBack, "Previous frame (,)").clicked() {
        player.step(-1);
    }
    if icon_button(ui, Icon::StepForward, "Next frame (.)").clicked() {
        player.step(1);
    }
    ui.add(Label::new(RichText::new(readout(st, sequence)).monospace()).truncate());
}

fn audio_row(ui: &mut Ui, player: &mut Player, st: &PlayerState) {
    let current = nearest_speed(st.speed);
    egui::ComboBox::from_id_salt("transport-speed")
        .width(58.0)
        .selected_text(speed_label(current))
        .show_ui(ui, |ui| {
            for s in SPEEDS {
                if ui
                    .selectable_label((s - current).abs() < f32::EPSILON, speed_label(s))
                    .clicked()
                {
                    player.set_speed(s);
                }
            }
        });
    if nofocus(ui.add(Button::selectable(st.looping, "Loop")))
        .on_hover_text("Loop playback")
        .clicked()
    {
        player.set_looping(!st.looping);
    }
    ui.add_enabled_ui(st.has_audio, |ui| {
        let speaker = if st.muted || st.volume <= 0.0 {
            Icon::Muted
        } else {
            Icon::Speaker
        };
        if icon_button(ui, speaker, if st.muted { "Unmute" } else { "Mute" }).clicked() {
            player.set_muted(!st.muted);
        }
        let mut volume = st.volume.clamp(0.0, 1.0);
        let width = (ui.available_width() - ITEM_GAP).clamp(32.0, 110.0);
        ui.spacing_mut().slider_width = width;
        let resp = nofocus(ui.add(Slider::new(&mut volume, 0.0..=1.0).show_value(false)))
            .on_hover_text("Volume")
            .on_disabled_hover_text("This media has no audio");
        if resp.changed() {
            player.set_volume(volume);
            if st.muted && volume > 0.0 {
                player.set_muted(false);
            }
        }
    });
}

/// Timeline slider. While the slider is being dragged its own value is displayed (stored in egui
/// memory) so playback position updates cannot overwrite the user's target.
fn scrubber(ui: &mut Ui, player: &mut Player, st: &PlayerState) {
    let id = ui.id().with("scrub");
    let scrub: Option<f64> = ui.data(|d| d.get_temp(id));
    let width = (ui.available_width() - 12.0).max(40.0);
    {
        let spacing = ui.spacing_mut();
        spacing.slider_width = width;
        spacing.interact_size.y = SCRUB_HEIGHT;
    }
    let last = st.frame_count.saturating_sub(1);
    let by_frames = st.frame_count > 1;
    let by_seconds = !by_frames && st.duration.0 > 0;
    let mut value = scrub.unwrap_or_else(|| {
        if by_frames {
            st.frame_index.clamp(0, last) as f64
        } else {
            st.position.seconds().max(0.0)
        }
    });
    let max = if by_frames {
        last as f64
    } else {
        st.duration.seconds().max(0.0)
    };
    let resp = ui.add_enabled(
        by_frames || by_seconds,
        Slider::new(&mut value, 0.0..=max.max(f64::MIN_POSITIVE))
            .show_value(false)
            .trailing_fill(true),
    );
    if resp.changed() {
        if by_frames {
            player.seek_frame(value.round() as i64);
        } else {
            player.seek(Tick::from_seconds_f64(value));
        }
    }
    let dragging = resp.dragged();
    ui.data_mut(|d| {
        if dragging {
            d.insert_temp(id, value);
        } else {
            d.remove_temp::<f64>(id);
        }
    });
    nofocus(resp);
}

/// Controls must never keep keyboard focus: Space/arrow keys belong to the viewer shortcuts.
fn nofocus(resp: Response) -> Response {
    if resp.has_focus() {
        resp.surrender_focus();
    }
    resp
}

fn nearest_speed(speed: f32) -> f32 {
    SPEEDS
        .iter()
        .copied()
        .min_by(|a, b| (a - speed).abs().total_cmp(&(b - speed).abs()))
        .unwrap_or(1.0)
}

fn speed_label(s: f32) -> String {
    format!("{s}×")
}

/// `HH:MM:SS:FF` (non-drop) for frame `frame` at `fps`.
pub fn timecode(frame: i64, fps: f64) -> String {
    let base = if fps.is_finite() {
        fps.round().clamp(1.0, 1000.0) as i64
    } else {
        24
    };
    let frame = frame.max(0);
    let ff = frame % base;
    let total = frame / base;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    format!("{h:02}:{m:02}:{s:02}:{ff:02}")
}

fn readout(st: &PlayerState, sequence: Option<&Sequence>) -> String {
    let count = st.frame_count.max(0);
    let idx = if count > 0 {
        st.frame_index.clamp(0, count - 1)
    } else {
        0
    };
    if let Some(seq) = sequence {
        let number = usize::try_from(idx)
            .ok()
            .and_then(|i| seq.frames.get(i))
            .map(|f| f.number)
            .unwrap_or(idx);
        return format!("{number}  ({}/{})", idx.saturating_add(1), count.max(1));
    }
    let fps = st.frame_rate.sane().as_f64();
    let dur_frames = st.frame_rate.frame_at(st.duration);
    let mut s = format!("{} / {}", timecode(idx, fps), timecode(dur_frames, fps));
    if count > 0 {
        s.push_str(&format!("  f{}/{}", idx.saturating_add(1), count));
    }
    s
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    Play,
    Pause,
    Stop,
    StepBack,
    StepForward,
    Speaker,
    Muted,
}

/// Small button with a vector-drawn icon (no dependence on font glyph coverage).
fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, ROW_HEIGHT), Sense::click());
    let resp = nofocus(resp).on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let vis = ui.style().interact(&resp);
        let rounding = CornerRadius::same(4);
        let fill = if ui.is_enabled() {
            vis.bg_fill
        } else {
            ui.visuals().widgets.inactive.bg_fill.gamma_multiply(0.5)
        };
        ui.painter().rect_filled(rect, rounding, fill);
        ui.painter()
            .rect_stroke(rect, rounding, vis.bg_stroke, StrokeKind::Inside);
        let color = if ui.is_enabled() {
            vis.fg_stroke.color
        } else {
            ui.visuals().weak_text_color()
        };
        paint_icon(ui, icon, rect.center(), color);
    }
    resp
}

fn tri(a: Pos2, b: Pos2, c: Pos2, color: Color32) -> Shape {
    Shape::convex_polygon(vec![a, b, c], color, Stroke::NONE)
}

fn paint_icon(ui: &Ui, icon: Icon, c: Pos2, color: Color32) {
    let p = ui.painter();
    let s = 6.0;
    let bar = |x0: f32, x1: f32| {
        Rect::from_min_max(Pos2::new(c.x + x0, c.y - s), Pos2::new(c.x + x1, c.y + s))
    };
    match icon {
        Icon::Play => {
            p.add(tri(
                Pos2::new(c.x - s * 0.6, c.y - s),
                Pos2::new(c.x - s * 0.6, c.y + s),
                Pos2::new(c.x + s, c.y),
                color,
            ));
        }
        Icon::Pause => {
            p.rect_filled(bar(-s * 0.8, -s * 0.15), CornerRadius::ZERO, color);
            p.rect_filled(bar(s * 0.15, s * 0.8), CornerRadius::ZERO, color);
        }
        Icon::Stop => {
            p.rect_filled(
                Rect::from_center_size(c, vec2(s * 1.7, s * 1.7)),
                CornerRadius::ZERO,
                color,
            );
        }
        Icon::StepBack => {
            p.rect_filled(bar(-s, -s * 0.6), CornerRadius::ZERO, color);
            p.add(tri(
                Pos2::new(c.x + s, c.y - s),
                Pos2::new(c.x + s, c.y + s),
                Pos2::new(c.x - s * 0.4, c.y),
                color,
            ));
        }
        Icon::StepForward => {
            p.rect_filled(bar(s * 0.6, s), CornerRadius::ZERO, color);
            p.add(tri(
                Pos2::new(c.x - s, c.y - s),
                Pos2::new(c.x - s, c.y + s),
                Pos2::new(c.x + s * 0.4, c.y),
                color,
            ));
        }
        Icon::Speaker | Icon::Muted => {
            let x = c.x - s * 0.9;
            p.rect_filled(
                Rect::from_min_max(
                    Pos2::new(x, c.y - s * 0.4),
                    Pos2::new(x + s * 0.6, c.y + s * 0.4),
                ),
                CornerRadius::ZERO,
                color,
            );
            p.add(Shape::convex_polygon(
                vec![
                    Pos2::new(x + s * 0.6, c.y - s * 0.4),
                    Pos2::new(x + s * 1.4, c.y - s * 0.9),
                    Pos2::new(x + s * 1.4, c.y + s * 0.9),
                    Pos2::new(x + s * 0.6, c.y + s * 0.4),
                ],
                color,
                Stroke::NONE,
            ));
            let stroke = Stroke::new(1.5, color);
            if icon == Icon::Muted {
                let (x0, x1) = (c.x + s * 0.7, c.x + s * 1.5);
                p.line_segment(
                    [Pos2::new(x0, c.y - s * 0.5), Pos2::new(x1, c.y + s * 0.5)],
                    stroke,
                );
                p.line_segment(
                    [Pos2::new(x0, c.y + s * 0.5), Pos2::new(x1, c.y - s * 0.5)],
                    stroke,
                );
            } else {
                p.line_segment(
                    [
                        Pos2::new(c.x + s * 0.8, c.y - s * 0.4),
                        Pos2::new(c.x + s * 0.8, c.y + s * 0.4),
                    ],
                    stroke,
                );
                p.line_segment(
                    [
                        Pos2::new(c.x + s * 1.3, c.y - s * 0.7),
                        Pos2::new(c.x + s * 1.3, c.y + s * 0.7),
                    ],
                    stroke,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecode_rolls_over() {
        assert_eq!(timecode(0, 24.0), "00:00:00:00");
        assert_eq!(timecode(24 * 3600 + 25, 24.0), "01:00:01:01");
        assert_eq!(timecode(-5, 24.0), "00:00:00:00");
    }

    #[test]
    fn timecode_survives_hostile_rate() {
        assert_eq!(timecode(10, f64::NAN), "00:00:00:10");
        assert_eq!(timecode(10, 0.0), "00:00:10:00");
    }

    #[test]
    fn nearest_speed_snaps() {
        assert_eq!(nearest_speed(1.1), 1.0);
        assert_eq!(nearest_speed(3.0), 2.0);
        assert_eq!(nearest_speed(0.1), 0.25);
    }
}
