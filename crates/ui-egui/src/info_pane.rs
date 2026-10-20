//! Metadata and annotations of the focused entry.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use egui::{Color32, Pos2, Sense, Shape, Stroke, Ui, vec2};
use gallerybrowser_core::{Entry, MediaKind};
use gallerybrowser_library::Annotations;
use gallerybrowser_media::{ComfyMetadata, MediaMetadata};

use crate::state::{Action, AppState};

/// Annotations are re-read from the library at most this often (the library is the source of truth;
/// other panels change it through actions).
const ANNOTATION_TTL: Duration = Duration::from_millis(300);
const STAR_SIZE: f32 = 18.0;
const ERROR_COLOR: Color32 = Color32::from_rgb(235, 110, 100);

/// Library data of the focused entry plus the choices for the "add" combos.
#[derive(Clone)]
struct AnnotationCache {
    path: PathBuf,
    at: Instant,
    annotations: Annotations,
    all_tags: Vec<String>,
    all_collections: Vec<String>,
}

/// Metadata of the focused entry (state.current_metadata): file, video, audio, sequence, EXIF,
/// ComfyUI sections; annotations (rating stars editable, tags chips add/remove, collections).
///
/// Renders its content top-down at the current width; the caller (`app.rs`) owns the vertical
/// `ScrollArea` that provides the remaining height.
pub fn show(ui: &mut Ui, state: &mut AppState) {
    let mut actions: Vec<Action> = Vec::new();
    {
        let state_ref = &*state;
        ui.add_space(ui.spacing().item_spacing.y);
        if let Some(focused) = state_ref.pane().selection.focused.as_ref() {
            let entry = state_ref.entry(focused);
            let name = entry.map(|e| e.name.clone()).unwrap_or_else(|| {
                focused
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| focused.display().to_string())
            });
            ui.add(egui::Label::new(egui::RichText::new(name).strong().size(15.0)).wrap());
            if let Some(entry) = entry.filter(|e| e.kind != gallerybrowser_core::MediaKind::Other) {
                annotations_block(ui, state_ref, entry, &mut actions);
            }
            ui.add_space(4.0);
            match state_ref.current_metadata.as_ref() {
                Some(meta) => metadata_sections(ui, meta),
                None => {
                    ui.weak("Loading metadata…");
                }
            }
        } else {
            ui.weak("Nothing selected");
        }
    }
    if !actions.is_empty() {
        ui.ctx().data_mut(|d| {
            d.remove_temp::<Option<AnnotationCache>>(egui::Id::new("info-annotations"))
        });
        ui.ctx().request_repaint_after(Duration::from_millis(350));
        state.pending.extend(actions);
    }
}

// ---------------------------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------------------------

fn annotation_cache(ui: &Ui, state: &AppState, entry: &Entry) -> AnnotationCache {
    let id = egui::Id::new("info-annotations");
    let path = entry.representative_path();
    if let Some(c) = ui
        .data(|d| d.get_temp::<Option<AnnotationCache>>(id))
        .flatten()
        && c.path == path
        && c.at.elapsed() < ANNOTATION_TTL
    {
        return c;
    }
    let c = AnnotationCache {
        annotations: state.library.annotations(&path),
        all_tags: state.library.tags().into_iter().map(|t| t.name).collect(),
        all_collections: state
            .library
            .collections()
            .into_iter()
            .map(|c| c.name)
            .collect(),
        path,
        at: Instant::now(),
    };
    ui.data_mut(|d| d.insert_temp(id, Some(c.clone())));
    c
}

fn annotations_block(ui: &mut Ui, state: &AppState, entry: &Entry, actions: &mut Vec<Action>) {
    let cache = annotation_cache(ui, state, entry);
    egui::CollapsingHeader::new("Library")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Rating");
                if let Some(r) = rating_stars(ui, cache.annotations.rating) {
                    actions.push(Action::SetRating(r));
                }
            });
            chip_editor(
                ui,
                "Tags",
                "info-new-tag",
                &cache.annotations.tags,
                &cache.all_tags,
                Action::AddTag,
                Action::RemoveTag,
                Action::CreateTag,
                actions,
            );
            chip_editor(
                ui,
                "Collections",
                "info-new-collection",
                &cache.annotations.collections,
                &cache.all_collections,
                Action::AddToCollection,
                Action::RemoveFromCollection,
                Action::CreateCollection,
                actions,
            );
        });
}

/// Five clickable stars. Returns the rating to set: clicking the current rating clears it.
fn rating_stars(ui: &mut Ui, current: u8) -> Option<u8> {
    let current = current.min(5);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(STAR_SIZE * 5.0 + 8.0, STAR_SIZE + 2.0), Sense::click());
    let hover = resp
        .hover_pos()
        .map(|p| (((p.x - rect.left()) / STAR_SIZE).floor().clamp(0.0, 4.0) as u8) + 1);
    let shown = hover.unwrap_or(current);
    let painter = ui.painter_at(rect);
    for i in 0..5u8 {
        let centre = Pos2::new(
            rect.left() + STAR_SIZE * (f32::from(i) + 0.5),
            rect.center().y,
        );
        let filled = i < shown;
        let color = if filled {
            if hover.is_some() {
                Color32::from_rgb(255, 214, 90)
            } else {
                Color32::from_rgb(245, 190, 40)
            }
        } else {
            ui.visuals().weak_text_color()
        };
        paint_star(&painter, centre, STAR_SIZE * 0.5, filled, color);
    }
    let clicked = resp
        .clicked()
        .then(|| hover.unwrap_or(0))
        .filter(|n| *n > 0);
    resp.on_hover_text("Click a star to rate; click the current rating to clear it");
    clicked.map(|n| if n == current { 0 } else { n })
}

fn paint_star(painter: &egui::Painter, c: Pos2, r: f32, filled: bool, color: Color32) {
    let points: Vec<Pos2> = (0..10)
        .map(|k| {
            let angle = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
            let radius = if k % 2 == 0 { r } else { r * 0.45 };
            Pos2::new(c.x + radius * angle.cos(), c.y + radius * angle.sin())
        })
        .collect();
    if filled {
        for k in 0..10 {
            if let (Some(a), Some(b)) = (points.get(k), points.get((k + 1) % 10)) {
                painter.add(Shape::convex_polygon(vec![c, *a, *b], color, Stroke::NONE));
            }
        }
    } else {
        let mut outline = points;
        if let Some(first) = outline.first().copied() {
            outline.push(first);
        }
        painter.add(Shape::line(outline, Stroke::new(1.2, color)));
    }
}

/// A label, the current names as removable chips, an "add existing" combo and a "new" text field.
fn chip_editor(
    ui: &mut Ui,
    title: &str,
    new_id: &str,
    current: &[String],
    all: &[String],
    add: impl Fn(String) -> Action,
    remove: impl Fn(String) -> Action,
    create: impl Fn(String) -> Action,
    actions: &mut Vec<Action>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(title);
        if current.is_empty() {
            ui.weak("none");
        }
        for name in current {
            if ui
                .small_button(format!("{name} ×"))
                .on_hover_text("Click to remove")
                .clicked()
            {
                actions.push(remove(name.clone()));
            }
        }
    });
    ui.horizontal(|ui| {
        let available: Vec<&String> = all.iter().filter(|n| !current.contains(n)).collect();
        ui.add_enabled_ui(!available.is_empty(), |ui| {
            egui::ComboBox::from_id_salt((new_id, "combo"))
                .selected_text("Add…")
                .width(90.0)
                .show_ui(ui, |ui| {
                    for name in available {
                        if ui.selectable_label(false, name).clicked() {
                            actions.push(add(name.clone()));
                        }
                    }
                });
        });
        let buf_id = egui::Id::new((new_id, "buf"));
        let mut buf: String = ui.data(|d| d.get_temp(buf_id)).unwrap_or_default();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .hint_text("New…")
                .desired_width(90.0),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let name = buf.trim().to_string();
            if !name.is_empty() {
                actions.push(create(name.clone()));
                actions.push(add(name));
                buf.clear();
            }
        }
        ui.data_mut(|d| d.insert_temp(buf_id, buf));
    });
}

// ---------------------------------------------------------------------------------------------
// Metadata sections
// ---------------------------------------------------------------------------------------------

fn metadata_sections(ui: &mut Ui, m: &MediaMetadata) {
    if let Some(err) = &m.error {
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!("Metadata error: {err}")).color(ERROR_COLOR),
            )
            .wrap(),
        );
    }
    section(ui, "File", true, |ui| {
        grid(ui, "file", |ui| {
            row(ui, "Path", m.path.display().to_string());
            row(
                ui,
                "Size",
                m.file_size
                    .map(human_size)
                    .unwrap_or_else(|| "unknown".into()),
            );
            row_opt(ui, "Modified", m.modified.and_then(format_time_utc));
            row_opt(ui, "Format", m.format.clone());
            row_opt(ui, "Container", m.container.clone());
            row_opt(ui, "Kind", m.kind.map(kind_label));
        });
    });

    let has_picture = m.width.is_some()
        || m.height.is_some()
        || m.video_codec.is_some()
        || m.frame_rate.is_some()
        || m.duration_secs.is_some();
    if has_picture {
        let title = if m.kind == Some(MediaKind::Video) {
            "Video"
        } else {
            "Image"
        };
        section(ui, title, true, |ui| {
            grid(ui, "video", |ui| {
                if let (Some(w), Some(h)) = (m.width, m.height) {
                    row(
                        ui,
                        "Resolution",
                        format!(
                            "{w} × {h}  ({:.1} MP)",
                            f64::from(w) * f64::from(h) / 1_000_000.0
                        ),
                    );
                }
                row_opt(ui, "Duration", m.duration_secs.map(format_duration));
                row_opt(
                    ui,
                    "Frame rate",
                    m.frame_rate
                        .filter(|f| f.is_finite() && *f > 0.0)
                        .map(|f| format!("{f:.3} fps")),
                );
                row_opt(ui, "Frames", m.frame_count.map(|n| n.to_string()));
                row_opt(ui, "Codec", m.video_codec.clone());
                row_opt(ui, "Pixel format", m.pixel_format.clone());
                row_opt(ui, "Bit rate", m.bit_rate.map(format_bitrate));
                row_opt(ui, "Color", m.color.clone());
                row_opt(
                    ui,
                    "Alpha",
                    m.has_alpha
                        .map(|a| if a { "yes" } else { "no" }.to_string()),
                );
                row_opt(ui, "Timecode", m.timecode.clone());
            });
        });
    }

    if !m.audio.is_empty() {
        section(ui, "Audio", true, |ui| {
            grid(ui, "audio", |ui| {
                for (i, a) in m.audio.iter().enumerate() {
                    let mut text = format!("{}, {} Hz, {} ch", a.codec, a.sample_rate, a.channels);
                    if let Some(bits) = a.bits_per_sample {
                        text.push_str(&format!(", {bits}-bit"));
                    }
                    row(ui, &format!("Stream {}", i + 1), text);
                }
            });
        });
    }

    if let Some((first, last, count, missing)) = m.sequence_range {
        section(ui, "Sequence", true, |ui| {
            grid(ui, "sequence", |ui| {
                row(ui, "Range", format!("{first} – {last}"));
                row(ui, "Frames", count.to_string());
                row(
                    ui,
                    "Missing",
                    if missing == 0 {
                        "none".to_string()
                    } else {
                        missing.to_string()
                    },
                );
            });
        });
    }

    if !m.exif.is_empty() {
        section(ui, "EXIF", false, |ui| {
            grid(ui, "exif", |ui| {
                for (k, v) in &m.exif {
                    row(ui, k, v.clone());
                }
            });
        });
    }

    if !m.text_chunks.is_empty() {
        section(ui, "PNG text", false, |ui| {
            for (i, (k, v)) in m.text_chunks.iter().enumerate() {
                text_block(ui, k, v, ("png-text", i));
            }
        });
    }

    if let Some(c) = &m.comfy {
        section(ui, "ComfyUI", true, |ui| comfy_section(ui, c));
    }
}

fn comfy_section(ui: &mut Ui, c: &ComfyMetadata) {
    if let Some(p) = &c.positive {
        text_block(ui, "Positive prompt", p, "comfy-pos");
    }
    if let Some(n) = &c.negative {
        text_block(ui, "Negative prompt", n, "comfy-neg");
    }
    grid(ui, "comfy", |ui| {
        row_opt(ui, "Model", c.model.clone());
        row_opt(ui, "Seed", c.seed.map(|v| v.to_string()));
        row_opt(ui, "Steps", c.steps.map(|v| v.to_string()));
        row_opt(ui, "CFG", c.cfg.map(|v| format!("{v}")));
        row_opt(ui, "Sampler", c.sampler.clone());
        row_opt(ui, "Scheduler", c.scheduler.clone());
    });
    ui.horizontal(|ui| {
        if let Some(j) = &c.prompt_json
            && ui.button("Copy prompt JSON").clicked()
        {
            ui.ctx().copy_text(j.clone());
        }
        if let Some(j) = &c.workflow_json
            && ui.button("Copy workflow JSON").clicked()
        {
            ui.ctx().copy_text(j.clone());
        }
    });
}

/// A titled, selectable, wrapped (height-limited, scrollable) text with a copy button.
fn text_block(ui: &mut Ui, title: &str, text: &str, id: impl std::hash::Hash + std::fmt::Debug) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong());
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(text.to_string());
        }
    });
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(120.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add(egui::Label::new(text).wrap().selectable(true));
        });
    ui.add_space(2.0);
}

fn section(ui: &mut Ui, title: &str, open: bool, add: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(open)
        .show(ui, add);
}

fn grid(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(("info-grid", id))
        .num_columns(2)
        .spacing([10.0, 3.0])
        .striped(true)
        .show(ui, add);
}

fn row(ui: &mut Ui, label: &str, value: String) {
    ui.label(egui::RichText::new(label).weak());
    ui.add(egui::Label::new(value).wrap().selectable(true));
    ui.end_row();
}

fn row_opt(ui: &mut Ui, label: &str, value: Option<String>) {
    if let Some(v) = value {
        row(ui, label, v);
    }
}

fn kind_label(k: MediaKind) -> String {
    match k {
        MediaKind::Image => "Image",
        MediaKind::Video => "Video",
        MediaKind::Sequence => "Image sequence",
        MediaKind::Audio => "Audio",
        MediaKind::Other => "File",
    }
    .to_string()
}

// ---------------------------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------------------------

/// Binary units with one decimal (`1.5 MB`), like the Python app.
pub(crate) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit + 1 < UNITS.len() {
        v /= 1024.0;
        unit += 1;
    }
    match UNITS.get(unit) {
        Some(u) if unit > 0 => format!("{v:.1} {u}"),
        _ => format!("{bytes} B"),
    }
}

/// `H:MM:SS` from one hour up, otherwise `M:SS`; fractional seconds shown below a minute-less clip.
pub(crate) fn format_duration(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "unknown".to_string();
    }
    let whole = secs.min(1.0e9) as u64;
    let (h, m, s) = (whole / 3600, (whole / 60) % 60, whole % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Decimal Mbps with two decimals.
fn format_bitrate(bps: u64) -> String {
    format!("{:.2} Mbps", bps as f64 / 1_000_000.0)
}

/// `YYYY-MM-DD HH:MM UTC`.
fn format_time_utc(t: SystemTime) -> Option<String> {
    let secs = i64::try_from(t.duration_since(UNIX_EPOCH).ok()?.as_secs()).ok()?;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    Some(format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem / 60) % 60
    ))
}

/// Days since 1970-01-01 → (year, month, day) (Hinnant's civil_from_days).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_binary_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert!(human_size(u64::MAX).ends_with("PB"));
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(65.4), "1:05");
        assert_eq!(format_duration(3725.0), "1:02:05");
        assert_eq!(format_duration(f64::NAN), "unknown");
        assert_eq!(format_duration(-1.0), "unknown");
    }

    #[test]
    fn dates() {
        assert_eq!(
            format_time_utc(UNIX_EPOCH),
            Some("1970-01-01 00:00 UTC".to_string())
        );
        let t = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(format_time_utc(t), Some("2023-11-14 22:13 UTC".to_string()));
        let leap = UNIX_EPOCH + Duration::from_secs(1_709_164_800); // 2024-02-29
        assert_eq!(
            format_time_utc(leap),
            Some("2024-02-29 00:00 UTC".to_string())
        );
    }

    #[test]
    fn bitrate() {
        assert_eq!(format_bitrate(12_345_678), "12.35 Mbps");
    }
}
