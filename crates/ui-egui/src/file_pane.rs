use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egui::epaint::Mesh;
use egui::text::LayoutJob;
use egui::{
    Align, Align2, Color32, FontId, Id, Key, Modifiers, Painter, Pos2, Rect, RichText, Sense,
    Shape, Stroke, StrokeKind, TextureHandle, pos2, vec2,
};
use gallerybrowser_core::{Entry, FilterScope, MediaKind, SortKey, ViewMode};
use gallerybrowser_media::Thumbnailer;

pub use crate::state::DragPaths;
use crate::state::{Action, AppState, SelectMode};
use crate::textures::{TextureCache, scrub_edge};
use crate::theme;

/// Selectable thumbnail edges (the toolbar slider steps through these).
const SIZES: [u32; 4] = [64, 128, 256, 512];
const MIN_EDGE: u32 = 32;
const MAX_EDGE: u32 = 1024;
const GAP: f32 = 4.0;
/// Space below the thumbnail square: two name rows + rating row + padding.
const CARD_FOOTER: f32 = 56.0;
const LIST_ROW_H: f32 = 22.0;
const LIST_HEADER_H: f32 = 24.0;
const COL_KIND: f32 = 90.0;
const COL_SIZE: f32 = 90.0;
const COL_DATE: f32 = 140.0;
const COL_RATING: f32 = 80.0;
/// Keep repainting at ~10 Hz for this long after the visible set changed, so thumbnails produced by
/// the workers show up even without input; failed thumbnails cannot keep the UI spinning forever.
const THUMB_REPAINT_WINDOW: f64 = 20.0;
/// A visible item still without a thumbnail is requested again after this long (bounded by the window above).
const THUMB_RETRY_SECS: f64 = 2.0;
const SEQUENCE_BLUE: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);
const STAR_GOLD: Color32 = Color32::from_rgb(0xFF, 0xC1, 0x07);
/// Hover scrubbing: frames a video/sequence card is divided into while the pointer moves across its
/// thumbnail, and the finer division used with Ctrl held ("real-time" scrubbing).
const SCRUB_SLOTS: u32 = 24;
const SCRUB_SLOTS_FINE: u32 = 200;
/// Neighbouring slots searched for an already-decoded frame to show while the exact one is made.
const SCRUB_NEAR: u32 = 12;
/// A scrub frame still missing after this long stops forcing repaints (undecodable frame).
const SCRUB_WAIT_SECS: f64 = 5.0;

/// Central pane: toolbar row (search, kind filter, sort, view mode, size slider, collapse sequences),
/// virtualized grid (thumbnail cards: image, name, sequence/video badge, rating stars) or list
/// (columns: name, kind, size, modified, rating; sortable headers). Selection via Action::Select/MoveFocus,
/// double-click → Activate, context menu, inline rename, drag out (internal) / drop in. Requests
/// thumbnails for visible rows only (priority by visibility).
///
/// Inline rename is delegated to the shell dialog: the context menu pushes `Action::BeginRename`.
#[derive(Default)]
pub struct FilePaneState {
    /// Which pane this view state belongs to (salts widget ids).
    index: usize,
    /// Search box buffer; follows `filters.search` when that changes elsewhere.
    search: String,
    search_seen: String,
    /// Layout of the last grid frame (keyboard navigation and scroll-into-view use it).
    cols: usize,
    page_items: usize,
    scroll_y: f32,
    view_h: f32,
    /// Detects `selection.focused` changes so the focused item is scrolled into view.
    last_focused: Option<PathBuf>,
    scroll_request: Option<PathBuf>,
    /// Thumbnails already requested for the current visible set (cleared when that set changes, so
    /// work dropped by `begin_batch` after a filter/sort/refresh is requested again).
    requested: HashSet<(PathBuf, u32)>,
    epoch: u64,
    repaint_until: f64,
    /// Some visible card had no texture last frame; with `requested_at` this re-queues work that
    /// `begin_batch` dropped (e.g. after a refresh that left the visible set unchanged).
    missing: bool,
    requested_at: f64,
    /// Last hovered scrub frame (path, slot, slots) and until when to keep repainting for its decode.
    scrub_target: Option<(PathBuf, u32, u32)>,
    scrub_wait_until: f64,
}

impl FilePaneState {
    /// Forget which thumbnails were requested: the thumbnailer drops queued work on `begin_batch`
    /// (any pane's refresh), so visible cards must be requested again.
    pub fn reset_requests(&mut self) {
        self.requested.clear();
        self.epoch = 0;
    }
}

/// Render pane `index`: breadcrumb row, toolbar, grid/list. The pane is made the active one for the
/// duration of the call, so everything below reads it through `state.pane()`. Any interaction inside
/// a pane other than the active one queues `Action::FocusPane` ahead of its actions.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, index: usize) {
    let index = if index < state.panes.len() { index } else { 0 };
    let real_active = state.active;
    state.active = index;
    let mut view = std::mem::take(&mut state.pane_mut().view);
    view.index = index;
    show_pane(ui, state, &mut view, real_active == index);
    state.pane_mut().view = view;
    state.active = real_active;
}

fn show_pane(ui: &mut egui::Ui, state: &mut AppState, pane: &mut FilePaneState, is_active: bool) {
    let dual = state.settings.dual_pane;
    let outer = ui.available_rect_before_wrap();
    let pending_before = state.pending.len();
    let mut acts: Vec<Action> = Vec::new();
    crate::breadcrumb::show(ui, state, is_active);
    ui.separator();
    if state.pane().filters.search != pane.search_seen {
        pane.search = state.pane().filters.search.clone();
        pane.search_seen = state.pane().filters.search.clone();
    }
    toolbar(ui, state, pane, &mut acts);
    ui.separator();

    let pane_rect = ui.available_rect_before_wrap();
    if state.pane().selection.focused != pane.last_focused {
        pane.last_focused = state.pane().selection.focused.clone();
        pane.scroll_request = state.pane().selection.focused.clone();
    }

    // Registered first so cards/rows (added later) win hit-testing; only empty space reaches it.
    let background = ui.interact(
        pane_rect,
        Id::new(("gb_filepane_background", pane.index)),
        Sense::click(),
    );
    if state.pane().entries.is_empty() {
        let filters = &state.pane().filters;
        let text =
            if filters.search.is_empty() && filters.kinds.is_empty() && filters.query.is_empty() {
                if state.settings.show_all_files {
                    "This folder is empty"
                } else {
                    "This folder has no media"
                }
            } else {
                "No items match the current filters"
            };
        ui.painter().text(
            pane_rect.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
    } else {
        match state.settings.view_mode {
            ViewMode::Grid => grid_ui(ui, state, pane, &mut acts),
            ViewMode::List => list_ui(ui, state, pane, &mut acts),
        }
    }
    if background.clicked() {
        acts.push(Action::ClearSelection);
    }
    background.context_menu(|ui| context_menu_ui(ui, state, None, &mut acts));

    if is_active {
        handle_keys(ui.ctx(), state, pane, &mut acts);
    }
    if pane.index == 0 {
        drag_overlay(ui.ctx());
    }
    external_drops(ui.ctx(), state, pane_rect, &mut acts);
    pane_drop(ui.ctx(), state, outer, &mut acts);

    let pressed_inside = ui.input(|i| {
        i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| outer.contains(p))
    });
    let acted = !acts.is_empty() || state.pending.len() > pending_before;
    state.pending.extend(acts);
    if dual && !is_active && (pressed_inside || acted) {
        let at = pending_before.min(state.pending.len());
        state.pending.insert(at, Action::FocusPane(pane.index));
    }
    if dual && is_active {
        ui.painter().rect_stroke(
            outer.shrink(1.0),
            2.0,
            Stroke::new(2.0, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
}

// ───────────────────────────── toolbar ─────────────────────────────

/// Width at which the whole toolbar fits on a single row; narrower panes split it into two rows
/// (search + filters, then view controls) instead of letting groups collide.
const TOOLBAR_ONE_ROW_WIDTH: f32 = 1100.0;

fn toolbar(ui: &mut egui::Ui, state: &AppState, pane: &mut FilePaneState, acts: &mut Vec<Action>) {
    let width = ui.available_width();
    if width >= TOOLBAR_ONE_ROW_WIDTH {
        ui.horizontal_wrapped(|ui| {
            toolbar_filters(ui, state, pane, acts, width);
            theme::group_separator(ui);
            toolbar_view(ui, state, acts);
        });
    } else {
        ui.horizontal_wrapped(|ui| toolbar_filters(ui, state, pane, acts, width));
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| toolbar_view(ui, state, acts));
    }
}

/// search | type + sort filters.
fn toolbar_filters(
    ui: &mut egui::Ui,
    state: &AppState,
    pane: &mut FilePaneState,
    acts: &mut Vec<Action>,
    pane_width: f32,
) {
    // Leave room for the clear button, both filter menus and two separators.
    let search_width = (pane_width - 320.0).clamp(90.0, 220.0);
    let resp = ui.add(
        egui::TextEdit::singleline(&mut pane.search)
            .hint_text("Search…")
            .desired_width(search_width)
            .margin(egui::vec2(6.0, 4.0)),
    );
    if resp.changed() {
        acts.push(Action::SetSearch(pane.search.clone()));
    }
    if !pane.search.is_empty()
        && theme::small_icon_button(ui, theme::Icon::Close, "Clear search")
            .on_hover_text("Clear search")
            .clicked()
    {
        pane.search.clear();
        acts.push(Action::SetSearch(String::new()));
    }

    theme::group_separator(ui);
    kind_menu(
        ui,
        &state.pane().filters.kinds,
        state.settings.show_all_files,
        acts,
    );
    sort_menu(
        ui,
        state.settings.sort_key,
        state.settings.sort_ascending,
        acts,
    );
}

/// view mode | group sequences | size slider | counts.
fn toolbar_view(ui: &mut egui::Ui, state: &AppState, acts: &mut Vec<Action>) {
    let mut mode = state.settings.view_mode;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (m, label) in [(ViewMode::Grid, "Grid"), (ViewMode::List, "List")] {
            if ui
                .add(egui::Button::selectable(mode == m, label))
                .on_hover_text(format!("{label} view"))
                .clicked()
            {
                mode = m;
            }
        }
    });
    if mode != state.settings.view_mode {
        acts.push(Action::SetViewMode(mode));
    }

    theme::group_separator(ui);
    let collapse = state.settings.collapse_sequences;
    if ui
        .add(egui::Button::selectable(collapse, "Group sequences"))
        .on_hover_text("Show image sequences as a single item")
        .clicked()
    {
        acts.push(Action::SetCollapseSequences(!collapse));
    }
    if state.active == 0 {
        let all = state.settings.show_all_files;
        if ui
            .add(egui::Button::selectable(all, "All files"))
            .on_hover_text("List every file, not only media")
            .clicked()
        {
            acts.push(Action::SetShowAllFiles(!all));
        }
        let dual = state.settings.dual_pane;
        if ui
            .add(egui::Button::selectable(dual, "2 panes"))
            .on_hover_text("Second file pane (F3)")
            .clicked()
        {
            acts.push(Action::ToggleDualPane);
        }
    }

    if state.settings.view_mode == ViewMode::Grid {
        theme::group_separator(ui);
        let mut idx = nearest_size_index(state.settings.thumbnail_size);
        let label = format!("{} px", SIZES.get(idx).copied().unwrap_or(128));
        ui.spacing_mut().slider_width = 90.0;
        let slider = egui::Slider::new(&mut idx, 0..=SIZES.len() - 1)
            .show_value(false)
            .text(label);
        if ui.add(slider).changed()
            && let Some(size) = SIZES.get(idx)
        {
            acts.push(Action::SetThumbnailSize(*size));
        }
    }

    theme::group_separator(ui);
    let selected = state.pane().selection.items.len();
    let mut counts = format!(
        "{} item{}",
        state.pane().entries.len(),
        if state.pane().entries.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    if selected > 0 {
        counts.push_str(&format!(" · {selected} selected"));
    }
    ui.label(RichText::new(counts).weak());
}

fn kind_label(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "Image",
        MediaKind::Video => "Video",
        MediaKind::Sequence => "Sequence",
        MediaKind::Audio => "Audio",
        MediaKind::Other => "Other",
    }
}

fn kind_menu(
    ui: &mut egui::Ui,
    kinds: &BTreeSet<MediaKind>,
    show_other: bool,
    acts: &mut Vec<Action>,
) {
    let title = if kinds.is_empty() {
        "Type: All".to_owned()
    } else {
        format!(
            "Type: {}",
            kinds
                .iter()
                .map(|k| kind_label(*k))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    ui.menu_button(title, |ui| {
        if ui.selectable_label(kinds.is_empty(), "All").clicked() {
            acts.push(Action::SetKinds(BTreeSet::new()));
            ui.close();
        }
        ui.separator();
        for kind in [
            MediaKind::Image,
            MediaKind::Video,
            MediaKind::Sequence,
            MediaKind::Audio,
        ]
        .into_iter()
        .chain(show_other.then_some(MediaKind::Other))
        {
            let mut on = kinds.contains(&kind);
            if ui.checkbox(&mut on, kind_label(kind)).changed() {
                let mut next = kinds.clone();
                if on {
                    next.insert(kind);
                } else {
                    next.remove(&kind);
                }
                acts.push(Action::SetKinds(next));
            }
        }
    });
}

fn sort_label(key: SortKey) -> &'static str {
    match key {
        SortKey::Name => "Name",
        SortKey::Modified => "Date",
        SortKey::Size => "Size",
        SortKey::Kind => "Type",
    }
}

fn sort_menu(ui: &mut egui::Ui, key: SortKey, ascending: bool, acts: &mut Vec<Action>) {
    let title = format!(
        "Sort: {} ({})",
        sort_label(key),
        if ascending { "asc" } else { "desc" }
    );
    ui.menu_button(title, |ui| {
        for k in [
            SortKey::Name,
            SortKey::Modified,
            SortKey::Size,
            SortKey::Kind,
        ] {
            if ui.selectable_label(key == k, sort_label(k)).clicked() {
                acts.push(Action::SetSort { key: k, ascending });
                ui.close();
            }
        }
        ui.separator();
        if ui.selectable_label(ascending, "Ascending").clicked() {
            acts.push(Action::SetSort {
                key,
                ascending: true,
            });
            ui.close();
        }
        if ui.selectable_label(!ascending, "Descending").clicked() {
            acts.push(Action::SetSort {
                key,
                ascending: false,
            });
            ui.close();
        }
    });
}

fn nearest_size_index(size: u32) -> usize {
    SIZES
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| s.abs_diff(size))
        .map(|(i, _)| i)
        .unwrap_or(1)
}

// ───────────────────────────── pure helpers ─────────────────────────────

/// Grid columns for a viewport of `width` points: `max(1, (width - 8) / (size + 12))`.
fn grid_columns(width: f32, size: u32) -> usize {
    let per = size as f32 + 12.0;
    let n = ((width - 8.0) / per).floor();
    if n.is_finite() && n >= 1.0 {
        n as usize
    } else {
        1
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS.get(unit).copied().unwrap_or("TB"))
}

/// Local time `YYYY-MM-DD HH:MM`; `—` for missing/out-of-range timestamps.
fn format_date(t: Option<SystemTime>) -> String {
    const MAX_SECS: u64 = 253_402_300_799; // 9999-12-31, keeps chrono well inside its range
    let Some(secs) = t
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .filter(|s| *s <= MAX_SECS)
    else {
        return "—".to_owned();
    };
    let Ok(secs) = i64::try_from(secs) else {
        return "—".to_owned();
    };
    match chrono::DateTime::from_timestamp(secs, 0) {
        Some(utc) => utc
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        None => "—".to_owned(),
    }
}

fn frame_count(entry: &Entry) -> usize {
    entry.sequence.as_ref().map_or(0, |s| s.frames.len())
}

fn type_text(entry: &Entry) -> String {
    match entry.kind {
        MediaKind::Image => "IMAGE".to_owned(),
        MediaKind::Video => "VIDEO".to_owned(),
        MediaKind::Audio => "AUDIO".to_owned(),
        MediaKind::Other => extension_label(entry),
        MediaKind::Sequence => format!("SEQ ({}f)", frame_count(entry)),
    }
}

/// Ctrl takes precedence over Shift.
fn select_mode(m: Modifiers) -> SelectMode {
    if m.ctrl || m.command {
        SelectMode::Toggle
    } else if m.shift {
        SelectMode::Range
    } else {
        SelectMode::Replace
    }
}

/// Scroll offset that brings the row `[top, top+row_h]` into a viewport, or `None` if already visible.
fn scroll_target(row: usize, row_h: f32, offset: f32, view_h: f32) -> Option<f32> {
    if view_h <= 0.0 || row_h <= 0.0 {
        return None;
    }
    let top = row as f32 * row_h;
    let bottom = top + row_h;
    if top < offset {
        Some(top.max(0.0))
    } else if bottom > offset + view_h {
        Some((bottom - view_h).max(0.0))
    } else {
        None
    }
}

// ───────────────────────────── painting helpers ─────────────────────────────

/// Paint `text` on one row inside `rect`, ending with `…` when it does not fit.
pub(crate) fn paint_elided(
    painter: &Painter,
    rect: Rect,
    text: &str,
    font: FontId,
    color: Color32,
) {
    if rect.width() < 4.0 {
        return;
    }
    let mut job = LayoutJob::simple(text.to_owned(), font, color, rect.width());
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    let pos = pos2(rect.min.x, rect.center().y - galley.size().y / 2.0);
    painter.galley(pos, galley, color);
}

fn star_points(c: Pos2, r: f32) -> Vec<Pos2> {
    (0..10)
        .map(|i| {
            let radius = if i % 2 == 0 { r } else { r * 0.45 };
            let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            pos2(c.x + radius * angle.cos(), c.y + radius * angle.sin())
        })
        .collect()
}

fn paint_star(painter: &Painter, c: Pos2, r: f32, color: Color32) {
    let pts = star_points(c, r);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, color);
    for p in &pts {
        mesh.colored_vertex(*p, color);
    }
    for i in 0..10u32 {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
    }
    painter.add(Shape::mesh(mesh));
}

/// Up to five stars, `rating` filled, the rest dim; left-aligned from `origin` (left-center).
fn paint_rating(painter: &Painter, origin: Pos2, r: f32, rating: u8, dim: Color32) {
    for i in 0..5u8 {
        let c = pos2(origin.x + r + i as f32 * (r * 2.0 + 1.0), origin.y);
        paint_star(painter, c, r, if i < rating { STAR_GOLD } else { dim });
    }
}

fn paint_badge(painter: &Painter, top_right: Pos2, text: &str, bg: Color32, fg: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(10.0), fg);
    let size = galley.size() + vec2(8.0, 3.0);
    let rect = Rect::from_min_size(pos2(top_right.x - size.x, top_right.y), size);
    painter.rect_filled(rect, 3.0, bg);
    painter.galley(rect.min + vec2(4.0, 1.5), galley, fg);
}

/// Simple vector icon for items whose thumbnail is not available yet.
fn paint_placeholder(painter: &Painter, area: Rect, kind: MediaKind, color: Color32) {
    let s = area.size().min_elem() * 0.5;
    let c = area.center();
    let stroke = Stroke::new(2.0, color);
    let frame = Rect::from_center_size(c, vec2(s * 1.3, s));
    match kind {
        MediaKind::Video => {
            painter.rect_stroke(frame, 4.0, stroke, StrokeKind::Inside);
            let t = s * 0.28;
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(c.x - t * 0.7, c.y - t),
                    pos2(c.x - t * 0.7, c.y + t),
                    pos2(c.x + t, c.y),
                ],
                color,
                Stroke::NONE,
            ));
        }
        MediaKind::Image => {
            painter.rect_stroke(frame, 4.0, stroke, StrokeKind::Inside);
            painter.circle_filled(
                pos2(frame.min.x + s * 0.3, frame.min.y + s * 0.28),
                s * 0.09,
                color,
            );
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(frame.min.x + s * 0.12, frame.max.y - s * 0.1),
                    pos2(frame.min.x + s * 0.5, frame.min.y + s * 0.4),
                    pos2(frame.min.x + s * 0.85, frame.max.y - s * 0.1),
                ],
                color,
                Stroke::NONE,
            ));
        }
        MediaKind::Sequence => {
            for i in 0..3 {
                let off = (2 - i) as f32 * s * 0.1;
                painter.rect_stroke(
                    frame
                        .translate(vec2(off - s * 0.1, -off + s * 0.1))
                        .shrink(s * 0.06),
                    3.0,
                    stroke,
                    StrokeKind::Inside,
                );
            }
        }
        MediaKind::Audio => {
            let head = pos2(c.x - s * 0.15, c.y + s * 0.3);
            painter.circle_filled(head, s * 0.16, color);
            painter.line_segment(
                [
                    pos2(head.x + s * 0.15, head.y),
                    pos2(head.x + s * 0.15, c.y - s * 0.4),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(head.x + s * 0.15, c.y - s * 0.4),
                    pos2(head.x + s * 0.5, c.y - s * 0.2),
                ],
                stroke,
            );
        }
        MediaKind::Other => {
            // generic document icon: page with a folded corner and text lines
            let page = Rect::from_center_size(c, vec2(s * 0.85, s * 1.15));
            let fold = page.width() * 0.32;
            painter.add(Shape::closed_line(
                vec![
                    page.left_top(),
                    pos2(page.max.x - fold, page.min.y),
                    pos2(page.max.x, page.min.y + fold),
                    page.right_bottom(),
                    page.left_bottom(),
                ],
                stroke,
            ));
            painter.add(Shape::line(
                vec![
                    pos2(page.max.x - fold, page.min.y),
                    pos2(page.max.x - fold, page.min.y + fold),
                    pos2(page.max.x, page.min.y + fold),
                ],
                stroke,
            ));
            for i in 0..3 {
                let y = page.min.y + page.height() * (0.5 + 0.16 * i as f32);
                painter.line_segment(
                    [
                        pos2(page.min.x + page.width() * 0.18, y),
                        pos2(page.max.x - page.width() * 0.18, y),
                    ],
                    stroke,
                );
            }
        }
    }
}

// ───────────────────────────── thumbnails ─────────────────────────────

/// Texture for entry `idx`: textures cache → thumbnailer memory cache (uploaded now) → request
/// generation. Cards call this every frame, so thumbnails reappear after filter/sort/refresh.
fn resolve_texture(
    state: &mut AppState,
    ctx: &egui::Context,
    requested: &mut HashSet<(PathBuf, u32)>,
    idx: usize,
    edge: u32,
    priority: i32,
) -> Option<TextureHandle> {
    let active = state.active;
    let AppState {
        panes,
        textures,
        thumbs,
        ..
    } = state;
    let entry = panes.get(active)?.entries.get(idx)?;
    if entry.kind == MediaKind::Other {
        return None;
    }
    if let Some(tex) = textures.get(&entry.path, edge) {
        return Some(tex);
    }
    if let Some(img) = thumbs.cached(entry, edge) {
        return Some(textures.insert(ctx, entry.path.clone(), edge, &img));
    }
    if requested.insert((entry.path.clone(), edge)) {
        thumbs.request(entry, edge, priority);
    }
    None
}

/// Queue (low priority) the entries just below the visible rows.
fn prefetch(
    state: &mut AppState,
    requested: &mut HashSet<(PathBuf, u32)>,
    range: std::ops::Range<usize>,
    edge: u32,
) {
    let active = state.active;
    let AppState {
        panes,
        textures,
        thumbs,
        ..
    } = state;
    let Some(entries) = panes.get(active).map(|p| &p.entries) else {
        return;
    };
    for (n, idx) in range.enumerate() {
        let Some(entry) = entries.get(idx) else { break };
        if entry.kind == MediaKind::Other {
            continue;
        }
        if textures.get(&entry.path, edge).is_some() {
            continue;
        }
        if requested.insert((entry.path.clone(), edge)) {
            thumbs.request(entry, edge, 100 - n.min(100) as i32);
        }
    }
}

/// Start a new request epoch when the visible set (scroll, filter, sort, size) changed.
fn refresh_epoch(
    state: &AppState,
    pane: &mut FilePaneState,
    ctx: &egui::Context,
    range: std::ops::Range<usize>,
    edge: u32,
) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    edge.hash(&mut hasher);
    state.pane().entries.len().hash(&mut hasher);
    for entry in state.pane().entries.get(range).unwrap_or(&[]) {
        entry.path.hash(&mut hasher);
        entry.size.hash(&mut hasher);
    }
    let epoch = hasher.finish();
    let now = ctx.input(|i| i.time);
    let retry =
        pane.missing && now < pane.repaint_until && now - pane.requested_at > THUMB_RETRY_SECS;
    if epoch != pane.epoch || retry {
        if epoch != pane.epoch {
            pane.repaint_until = now + THUMB_REPAINT_WINDOW;
        }
        pane.epoch = epoch;
        pane.requested.clear();
        pane.requested_at = now;
    }
}

fn keep_polling_thumbnails(ctx: &egui::Context, pane: &FilePaneState) {
    if ctx.input(|i| i.time) < pane.repaint_until {
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

// ───────────────────────────── grid ─────────────────────────────

fn grid_ui(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pane: &mut FilePaneState,
    acts: &mut Vec<Action>,
) {
    let size = state.settings.thumbnail_size.clamp(MIN_EDGE, MAX_EDGE);
    let side = size as f32;
    let card_w = side + 8.0;
    let card_h = side + CARD_FOOTER;
    let cols = grid_columns(ui.available_width(), size);
    pane.cols = cols;
    let n = state.pane().entries.len();
    let rows = n.div_ceil(cols);
    let row_pitch = card_h + GAP;
    pane.page_items = ((pane.view_h / row_pitch).floor().max(1.0) as usize).saturating_mul(cols);

    let mut area = egui::ScrollArea::vertical()
        .id_salt(("gb_file_grid", pane.index))
        .auto_shrink([false, false]);
    if let Some(path) = pane.scroll_request.take()
        && let Some(idx) = state.pane().entries.iter().position(|e| e.path == path)
        && let Some(y) = scroll_target(idx / cols, row_pitch, pane.scroll_y, pane.view_h)
    {
        area = area.vertical_scroll_offset(y);
    }

    let out = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            area.show_rows(ui, card_h, rows, |ui, row_range| {
                let first = row_range.start * cols;
                let last = row_range.end.saturating_mul(cols).min(n);
                refresh_epoch(state, pane, ui.ctx(), first..last, size);
                let mut any_missing = false;
                let width = ui.available_width();
                for row in row_range {
                    let (row_rect, _) = ui.allocate_exact_size(vec2(width, card_h), Sense::hover());
                    for col in 0..cols {
                        let idx = row * cols + col;
                        if idx >= n {
                            break;
                        }
                        let min = pos2(
                            row_rect.min.x + GAP + col as f32 * (card_w + GAP),
                            row_rect.min.y,
                        );
                        let rect = Rect::from_min_size(min, vec2(card_w, card_h));
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let priority = 10_000 - (idx.saturating_sub(first)).min(9_000) as i32;
                        let tex = resolve_texture(
                            state,
                            ui.ctx(),
                            &mut pane.requested,
                            idx,
                            size,
                            priority,
                        );
                        any_missing |= tex.is_none()
                            && state
                                .pane()
                                .entries
                                .get(idx)
                                .is_some_and(|e| e.kind != MediaKind::Other);
                        let Some(entry) = state.pane().entries.get(idx) else {
                            continue;
                        };
                        let selected = state.pane().selection.items.contains(&entry.path);
                        let focused =
                            state.pane().selection.focused.as_deref() == Some(entry.path.as_path());
                        let rating = state
                            .library
                            .annotations(&entry.representative_path())
                            .rating;
                        let resp = ui.interact(
                            rect,
                            Id::new(("gb_card", pane.index, &entry.path)),
                            Sense::click_and_drag(),
                        );
                        let scrub = scrub_view(state, pane, ui, idx, &resp, rect, size);
                        let Some(entry) = state.pane().entries.get(idx) else {
                            continue;
                        };
                        paint_card(
                            ui,
                            rect,
                            entry,
                            scrub.as_ref().and_then(|s| s.tex.as_ref()).or(tex.as_ref()),
                            selected,
                            focused,
                            resp.hovered(),
                            rating,
                            side,
                        );
                        if let Some(s) = &scrub {
                            paint_scrub_line(ui, rect, side, s);
                        } else if resp.hovered() {
                            resp.clone().on_hover_text(hover_text(entry));
                        }
                        item_interaction(state, acts, idx, &resp);
                    }
                }
                pane.missing = any_missing;
                prefetch(
                    state,
                    &mut pane.requested,
                    last..(last + cols * 2).min(n),
                    size,
                );
            })
        })
        .inner;
    pane.scroll_y = out.state.offset.y;
    pane.view_h = out.inner_rect.height();
    keep_polling_thumbnails(ui.ctx(), pane);
}

/// A hovered video/sequence card while the pointer scrubs across its thumbnail.
struct ScrubView {
    /// Frame to show instead of the thumbnail (`None` until the first one is decoded).
    tex: Option<TextureHandle>,
    /// Pointer position across the thumbnail, 0..=1.
    frac: f32,
    /// Ctrl held: fine slots.
    fine: bool,
}

/// Map the pointer over a hovered video/sequence thumbnail to a scrub slot, make sure its frame is
/// requested and return the best frame available (exact, nearest decoded slot, coarse slot).
/// `None` when nothing is being scrubbed: not hovered, other kinds, pointer outside the thumbnail
/// square (name/rating area) or a button held (drag and drop, rubber band).
fn scrub_view(
    state: &mut AppState,
    pane: &mut FilePaneState,
    ui: &egui::Ui,
    idx: usize,
    resp: &egui::Response,
    rect: Rect,
    edge: u32,
) -> Option<ScrubView> {
    let pos = resp.hover_pos()?;
    let (any_down, fine, now) = ui.input(|i| (i.pointer.any_down(), i.modifiers.ctrl, i.time));
    if any_down {
        return None;
    }
    let active = state.active;
    let AppState {
        panes,
        textures,
        thumbs,
        ..
    } = state;
    let entry = panes.get(active)?.entries.get(idx)?;
    if !matches!(entry.kind, MediaKind::Video | MediaKind::Sequence) {
        return None;
    }
    let thumb = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(edge as f32, edge as f32));
    if !thumb.contains(pos) {
        return None;
    }
    let frac = ((pos.x - thumb.min.x) / thumb.width().max(1.0)).clamp(0.0, 1.0);
    let slots = if fine { SCRUB_SLOTS_FINE } else { SCRUB_SLOTS };
    let slot = ((frac * slots as f32) as u32).min(slots - 1);
    let target = (entry.path.clone(), slot, slots);
    if pane.scrub_target.as_ref() != Some(&target) {
        pane.scrub_target = Some(target);
        pane.scrub_wait_until = now + SCRUB_WAIT_SECS;
    }
    let (tex, exact) = scrub_texture(textures, thumbs, ui.ctx(), entry, edge, slot, slots);
    if !exact && now < pane.scrub_wait_until {
        // the decoded frame arrives through `poll_workers`, which only runs on repaint
        ui.ctx().request_repaint_after(Duration::from_millis(30));
    }
    Some(ScrubView { tex, frac, fine })
}

/// Frame for `slot` of `slots`: the texture if uploaded, else the thumbnailer's memory cache, else a
/// request (lowest priority) plus the closest decoded slot as a stand-in. The flag tells whether
/// the returned texture is the exact frame.
fn scrub_texture(
    textures: &mut TextureCache,
    thumbs: &Thumbnailer,
    ctx: &egui::Context,
    entry: &Entry,
    edge: u32,
    slot: u32,
    slots: u32,
) -> (Option<TextureHandle>, bool) {
    let key = scrub_edge(edge, slot, slots);
    if let Some(tex) = textures.get(&entry.path, key) {
        return (Some(tex), true);
    }
    if let Some(img) = thumbs.cached_scrub(entry, edge, slot, slots) {
        return (
            Some(textures.insert(ctx, entry.path.clone(), key, &img)),
            true,
        );
    }
    thumbs.request_scrub(entry, edge, slot, slots);
    for d in 1..=SCRUB_NEAR {
        let below = slot.checked_sub(d);
        let above = slot.checked_add(d).filter(|s| *s < slots);
        for near in below.into_iter().chain(above) {
            if let Some(tex) = textures.get(&entry.path, scrub_edge(edge, near, slots)) {
                return (Some(tex), false);
            }
        }
    }
    if slots != SCRUB_SLOTS {
        let coarse = (u64::from(slot) * u64::from(SCRUB_SLOTS) / u64::from(slots)) as u32;
        let coarse = coarse.min(SCRUB_SLOTS - 1);
        if let Some(tex) = textures.get(&entry.path, scrub_edge(edge, coarse, SCRUB_SLOTS)) {
            return (Some(tex), false);
        }
    }
    (None, false)
}

/// Thin progress line along the bottom of the thumbnail square; a head dot marks fine scrubbing.
fn paint_scrub_line(ui: &egui::Ui, rect: Rect, side: f32, scrub: &ScrubView) {
    let painter = ui.painter();
    let thumb = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(side, side));
    let y = thumb.max.y - 3.0;
    let colour = ui.visuals().selection.stroke.color;
    painter.line_segment(
        [pos2(thumb.min.x, y), pos2(thumb.max.x, y)],
        Stroke::new(3.0, Color32::from_black_alpha(140)),
    );
    let head = thumb.min.x + thumb.width() * scrub.frac;
    painter.line_segment(
        [pos2(thumb.min.x, y), pos2(head, y)],
        Stroke::new(3.0, colour),
    );
    if scrub.fine {
        painter.circle_filled(pos2(head, y), 4.0, colour);
    }
}

fn hover_text(entry: &Entry) -> String {
    format!(
        "{}\n{} · {}\n{}",
        entry.name,
        type_text(entry),
        format_size(entry.size),
        format_date(entry.modified)
    )
}

fn paint_card(
    ui: &egui::Ui,
    rect: Rect,
    entry: &Entry,
    tex: Option<&TextureHandle>,
    selected: bool,
    focused: bool,
    hovered: bool,
    rating: u8,
    side: f32,
) {
    let visuals = ui.visuals();
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 6.0, visuals.selection.bg_fill.gamma_multiply(0.55));
    } else if hovered {
        painter.rect_filled(rect, 6.0, visuals.widgets.hovered.weak_bg_fill);
    }
    if selected || focused {
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(
                if focused { 2.0 } else { 1.0 },
                visuals.selection.stroke.color,
            ),
            StrokeKind::Inside,
        );
    }
    let thumb = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(side, side));
    painter.rect_filled(thumb, 4.0, visuals.extreme_bg_color);
    match tex {
        Some(tex) => {
            let sz = tex.size_vec2();
            if sz.x > 0.0 && sz.y > 0.0 {
                let scale = (side / sz.x).min(side / sz.y);
                let fit = Rect::from_center_size(thumb.center(), sz * scale);
                painter.image(
                    tex.id(),
                    fit,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        None => paint_placeholder(painter, thumb, entry.kind, visuals.weak_text_color()),
    }
    match entry.kind {
        MediaKind::Sequence => {
            let n = frame_count(entry);
            paint_badge(
                painter,
                pos2(thumb.max.x - 4.0, thumb.min.y + 4.0),
                &format!("SEQ {n}f"),
                SEQUENCE_BLUE,
                Color32::WHITE,
            );
        }
        MediaKind::Video => paint_badge(
            painter,
            pos2(thumb.max.x - 4.0, thumb.min.y + 4.0),
            "VIDEO",
            Color32::from_black_alpha(170),
            Color32::WHITE,
        ),
        MediaKind::Audio => paint_badge(
            painter,
            pos2(thumb.max.x - 4.0, thumb.min.y + 4.0),
            "AUDIO",
            Color32::from_black_alpha(170),
            Color32::WHITE,
        ),
        MediaKind::Other => paint_badge(
            painter,
            pos2(thumb.max.x - 4.0, thumb.min.y + 4.0),
            &extension_label(entry),
            Color32::from_black_alpha(170),
            Color32::WHITE,
        ),
        MediaKind::Image => {}
    }
    // Name: centered, wrapped to at most two rows, elided beyond that.
    let mut job = LayoutJob::simple(
        entry.name.clone(),
        FontId::proportional(12.0),
        visuals.text_color(),
        rect.width() - 8.0,
    );
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = true;
    job.halign = Align::Center;
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(rect.center().x, thumb.max.y + 3.0),
        galley,
        visuals.text_color(),
    );
    if rating > 0 {
        let r = 5.0;
        let total = 5.0 * (r * 2.0 + 1.0) - 1.0;
        paint_rating(
            painter,
            pos2(rect.center().x - total / 2.0, rect.max.y - 9.0),
            r,
            rating,
            visuals.weak_text_color().gamma_multiply(0.4),
        );
    }
}

// ───────────────────────────── list ─────────────────────────────

fn list_ui(
    ui: &mut egui::Ui,
    state: &mut AppState,
    pane: &mut FilePaneState,
    acts: &mut Vec<Action>,
) {
    let width = ui.available_width();
    let fixed = COL_KIND + COL_SIZE + COL_DATE + COL_RATING;
    let name_w = (width - fixed - 12.0).max(120.0);
    let columns = [
        ("Name", Some(SortKey::Name), name_w),
        ("Type", Some(SortKey::Kind), COL_KIND),
        ("Size", Some(SortKey::Size), COL_SIZE),
        ("Modified", Some(SortKey::Modified), COL_DATE),
        ("Rating", None, COL_RATING),
    ];

    // Sortable header.
    let (header, _) = ui.allocate_exact_size(vec2(width, LIST_HEADER_H), Sense::hover());
    ui.painter()
        .rect_filled(header, 0.0, ui.visuals().faint_bg_color);
    let mut x = header.min.x + 6.0;
    for (title, key, w) in columns {
        let cell = Rect::from_min_size(pos2(x, header.min.y), vec2(w, LIST_HEADER_H));
        let active = key.is_some_and(|k| k == state.settings.sort_key);
        let text_color = if active {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        };
        paint_elided(
            ui.painter(),
            cell.shrink2(vec2(2.0, 0.0)),
            title,
            FontId::proportional(13.0),
            text_color,
        );
        if let Some(k) = key {
            let resp = ui.interact(
                cell,
                Id::new(("gb_list_header", pane.index, title)),
                Sense::click(),
            );
            if resp.hovered() {
                ui.painter().rect_stroke(
                    cell,
                    2.0,
                    Stroke::new(1.0, ui.visuals().weak_text_color()),
                    StrokeKind::Inside,
                );
            }
            if resp.clicked() {
                let ascending = if active {
                    !state.settings.sort_ascending
                } else {
                    true
                };
                acts.push(Action::SetSort { key: k, ascending });
            }
            if active {
                let c = pos2(cell.max.x - 10.0, cell.center().y);
                let dir = if state.settings.sort_ascending {
                    -1.0
                } else {
                    1.0
                };
                ui.painter().add(Shape::convex_polygon(
                    vec![
                        pos2(c.x - 4.0, c.y - 2.0 * dir),
                        pos2(c.x + 4.0, c.y - 2.0 * dir),
                        pos2(c.x, c.y + 3.0 * dir),
                    ],
                    text_color,
                    Stroke::NONE,
                ));
            }
        }
        x += w;
    }

    let n = state.pane().entries.len();
    let mut area = egui::ScrollArea::vertical()
        .id_salt(("gb_file_list", pane.index))
        .auto_shrink([false, false]);
    if let Some(path) = pane.scroll_request.take()
        && let Some(idx) = state.pane().entries.iter().position(|e| e.path == path)
        && let Some(y) = scroll_target(idx, LIST_ROW_H, pane.scroll_y, pane.view_h)
    {
        area = area.vertical_scroll_offset(y);
    }
    pane.cols = 1;
    pane.page_items = ((pane.view_h / LIST_ROW_H).floor().max(1.0)) as usize;

    let out = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            area.show_rows(ui, LIST_ROW_H, n, |ui, range| {
                let font = FontId::proportional(13.0);
                let row_w = ui.available_width();
                for idx in range {
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, LIST_ROW_H), Sense::hover());
                    let Some(entry) = state.pane().entries.get(idx) else {
                        continue;
                    };
                    let selected = state.pane().selection.items.contains(&entry.path);
                    let focused =
                        state.pane().selection.focused.as_deref() == Some(entry.path.as_path());
                    let rating = state
                        .library
                        .annotations(&entry.representative_path())
                        .rating;
                    let resp = ui.interact(
                        rect,
                        Id::new(("gb_row", pane.index, &entry.path)),
                        Sense::click_and_drag(),
                    );
                    let visuals = ui.visuals();
                    let painter = ui.painter();
                    if selected {
                        painter.rect_filled(
                            rect,
                            0.0,
                            visuals.selection.bg_fill.gamma_multiply(0.55),
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 0.0, visuals.widgets.hovered.weak_bg_fill);
                    } else if idx % 2 == 1 {
                        painter.rect_filled(rect, 0.0, visuals.faint_bg_color);
                    }
                    if focused {
                        painter.rect_stroke(
                            rect,
                            0.0,
                            Stroke::new(1.0, visuals.selection.stroke.color),
                            StrokeKind::Inside,
                        );
                    }
                    let text = visuals.text_color();
                    let weak = visuals.weak_text_color();
                    let cells: [(String, Color32, f32); 4] = [
                        (entry.name.clone(), text, name_w),
                        (type_text(entry), weak, COL_KIND),
                        (format_size(entry.size), weak, COL_SIZE),
                        (format_date(entry.modified), weak, COL_DATE),
                    ];
                    let mut x = rect.min.x + 6.0;
                    for (s, color, w) in &cells {
                        paint_elided(
                            painter,
                            Rect::from_min_size(pos2(x, rect.min.y), vec2(*w - 6.0, LIST_ROW_H)),
                            s,
                            font.clone(),
                            *color,
                        );
                        x += *w;
                    }
                    if rating > 0 {
                        paint_rating(
                            painter,
                            pos2(x, rect.center().y),
                            5.0,
                            rating,
                            weak.gamma_multiply(0.4),
                        );
                    }
                    if resp.hovered() {
                        resp.clone().on_hover_text(hover_text(entry));
                    }
                    item_interaction(state, acts, idx, &resp);
                }
            })
        })
        .inner;
    pane.scroll_y = out.state.offset.y;
    pane.view_h = out.inner_rect.height();
}

// ───────────────────────────── interaction ─────────────────────────────

/// Click / double-click / right-click / drag-start / context menu shared by grid cards and list rows.
fn item_interaction(
    state: &mut AppState,
    acts: &mut Vec<Action>,
    idx: usize,
    resp: &egui::Response,
) {
    let Some(entry) = state.pane().entries.get(idx) else {
        return;
    };
    let path = entry.path.clone();
    let selected = state.pane().selection.items.contains(&path);
    if resp.double_clicked() {
        acts.push(Action::Activate(path.clone()));
    } else if resp.clicked() {
        let mode = select_mode(resp.ctx.input(|i| i.modifiers));
        acts.push(Action::Select {
            path: path.clone(),
            mode,
        });
    }
    if resp.secondary_clicked() && !selected {
        acts.push(Action::Select {
            path: path.clone(),
            mode: SelectMode::Replace,
        });
    }
    if resp.drag_started() {
        let paths = if selected {
            state.selected_member_paths()
        } else {
            state
                .pane()
                .entries
                .get(idx)
                .map(Entry::member_paths)
                .unwrap_or_default()
        };
        if !paths.is_empty() {
            resp.dnd_set_drag_payload(DragPaths(paths));
        }
    }
    resp.context_menu(|ui| context_menu_ui(ui, state, Some(idx), acts));
}

fn menu_item(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    let clicked = ui.add_enabled(enabled, egui::Button::new(label)).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Item (`target = Some(index)`) or background (`None`) context menu. Actions apply to the selection,
/// except when the target is not selected: then they apply to the target only (the right-click also
/// queues a Replace selection, which lands next frame).
fn context_menu_ui(
    ui: &mut egui::Ui,
    state: &mut AppState,
    target: Option<usize>,
    acts: &mut Vec<Action>,
) {
    let target_entry: Option<Entry> = target.and_then(|i| state.pane().entries.get(i).cloned());
    let selected: Vec<Entry> = match &target_entry {
        Some(t) if !state.pane().selection.items.contains(&t.path) => vec![t.clone()],
        _ => state.selected_entries().into_iter().cloned().collect(),
    };
    let has_sel = !selected.is_empty();
    let current_folder = state.pane().nav.current.clone();

    if let Some(t) = &target_entry {
        if menu_item(ui, "Open", true) {
            acts.push(Action::Activate(t.path.clone()));
        }
        if menu_item(ui, "Open in viewer", t.kind != MediaKind::Other) {
            acts.push(Action::OpenFullscreen(t.path.clone()));
        }
        if menu_item(ui, "Show in file manager", true) {
            acts.push(Action::RevealInFileManager(t.representative_path()));
        }
        if menu_item(ui, "Open with default app", true) {
            acts.push(Action::OpenWithDefaultApp(t.representative_path()));
        }
        if menu_item(ui, "Copy path", true) {
            acts.push(Action::CopyPathToClipboard(t.path.clone()));
        }
        ui.separator();
        if menu_item(ui, "Copy", true) {
            acts.push(Action::Copy);
        }
        if menu_item(ui, "Cut", true) {
            acts.push(Action::Cut);
        }
        if state.settings.dual_pane {
            let other_ok = crate::actions::other_pane_target(
                true,
                state.pane().nav.current.as_deref(),
                state.pane_at(state.other_index()).nav.current.as_deref(),
            )
            .is_ok();
            if menu_item(ui, "Copy to other pane", other_ok) {
                acts.push(Action::CopyToOtherPane);
            }
            if menu_item(ui, "Move to other pane", other_ok) {
                acts.push(Action::MoveToOtherPane);
            }
        }
    }
    if menu_item(
        ui,
        "Paste",
        state.clipboard.is_some() && current_folder.is_some(),
    ) {
        acts.push(Action::Paste { dest: None });
    }
    if target_entry.is_some() {
        ui.separator();
        if menu_item(ui, "Rename…", selected.len() == 1) {
            acts.push(Action::BeginRename);
        }
        let batch_ok = selected.len() > 1 || selected.iter().any(|e| e.sequence.is_some());
        if menu_item(ui, "Batch rename…", batch_ok) {
            acts.push(Action::BeginBatchRename);
        }
        ui.separator();
        if menu_item(ui, "Move to trash", true) {
            acts.push(Action::Trash);
        }
        if menu_item(ui, "Delete permanently…", true) {
            acts.push(Action::RequestDeletePermanently);
        }
        ui.separator();
        organize_menus(ui, state, &selected, has_sel, acts);
    }
    ui.separator();
    if menu_item(ui, "New folder…", current_folder.is_some()) {
        acts.push(Action::BeginNewFolder);
    }
    if target_entry.is_none() {
        if menu_item(ui, "Select all", !state.pane().entries.is_empty()) {
            acts.push(Action::SelectAll);
        }
        if menu_item(ui, "Refresh", true) {
            acts.push(Action::Refresh);
        }
    }
    if let Some(folder) = current_folder
        && menu_item(ui, "Add folder to favorites", true)
    {
        acts.push(Action::AddFavorite(folder));
    }
}

fn organize_menus(
    ui: &mut egui::Ui,
    state: &mut AppState,
    selected: &[Entry],
    has_sel: bool,
    acts: &mut Vec<Action>,
) {
    let anns: Vec<_> = selected
        .iter()
        .map(|e| state.library.annotations(&e.representative_path()))
        .collect();
    let common_rating = anns
        .first()
        .map(|a| a.rating)
        .filter(|r| anns.iter().all(|a| a.rating == *r));

    ui.add_enabled_ui(has_sel, |ui| {
        ui.menu_button("Rating", |ui| {
            for rating in 0..=5u8 {
                let label = match rating {
                    0 => "Clear rating".to_owned(),
                    1 => "1 star".to_owned(),
                    n => format!("{n} stars"),
                };
                if ui
                    .selectable_label(common_rating == Some(rating), label)
                    .clicked()
                {
                    acts.push(Action::SetRating(rating));
                    ui.close();
                }
            }
        });
        ui.menu_button("Tags", |ui| {
            let tags = state.library.tags();
            if tags.is_empty() {
                ui.label(RichText::new("No tags yet").weak());
            }
            for tag in &tags {
                let all_have = !anns.is_empty() && anns.iter().all(|a| a.tags.contains(&tag.name));
                let mut on = all_have;
                if ui.checkbox(&mut on, &tag.name).clicked() {
                    acts.push(if all_have {
                        Action::RemoveTag(tag.name.clone())
                    } else {
                        Action::AddTag(tag.name.clone())
                    });
                }
            }
            ui.separator();
            if menu_item(ui, "New tag…", true) {
                state.dialogs.new_tag = Some(String::new());
            }
        });
        ui.menu_button("Collections", |ui| {
            let collections = state.library.collections();
            if collections.is_empty() {
                ui.label(RichText::new("No collections yet").weak());
            }
            for coll in &collections {
                let all_in =
                    !anns.is_empty() && anns.iter().all(|a| a.collections.contains(&coll.name));
                let mut on = all_in;
                if ui.checkbox(&mut on, &coll.name).clicked() {
                    acts.push(if all_in {
                        Action::RemoveFromCollection(coll.name.clone())
                    } else {
                        Action::AddToCollection(coll.name.clone())
                    });
                }
            }
            ui.separator();
            if menu_item(ui, "New collection…", true) {
                state.dialogs.new_collection = Some(String::new());
            }
        });
    });
}

fn dialog_open(state: &AppState) -> bool {
    let d = &state.dialogs;
    d.settings.is_some()
        || d.batch_rename.is_some()
        || d.rename.is_some()
        || d.new_folder.is_some()
        || d.confirm_delete.is_some()
        || d.new_tag.is_some()
        || d.new_collection.is_some()
        || d.about
        || d.shortcuts
}

/// Arrows / Home / End / PageUp/Down move focus (Shift extends), Enter activates. Backspace, Ctrl+A,
/// Esc and the file-operation shortcuts belong to `shortcuts::handle`.
fn handle_keys(
    ctx: &egui::Context,
    state: &AppState,
    pane: &FilePaneState,
    acts: &mut Vec<Action>,
) {
    if ctx.egui_wants_keyboard_input()
        || state.fullscreen.is_some()
        || dialog_open(state)
        || state.pane().entries.is_empty()
    {
        return;
    }
    let grid = state.settings.view_mode == ViewMode::Grid;
    let cols = if grid { pane.cols.max(1) as isize } else { 1 };
    let page = pane.page_items.max(1) as isize;
    let end = state.pane().entries.len() as isize;
    let moves: [(Key, isize); 8] = [
        (Key::ArrowUp, -cols),
        (Key::ArrowDown, cols),
        (Key::ArrowLeft, if grid { -1 } else { 0 }),
        (Key::ArrowRight, if grid { 1 } else { 0 }),
        (Key::PageUp, -page),
        (Key::PageDown, page),
        (Key::Home, -end),
        (Key::End, end),
    ];
    for (key, delta) in moves {
        // Shift first: a plain shortcut would also match with Shift held.
        for (mods, extend) in [(Modifiers::SHIFT, true), (Modifiers::NONE, false)] {
            if ctx.input_mut(|i| i.consume_key(mods, key)) {
                if delta != 0 {
                    acts.push(Action::MoveFocus { delta, extend });
                }
                return;
            }
        }
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
        let focused = state
            .pane()
            .selection
            .focused
            .clone()
            .filter(|p| state.entry(p).is_some());
        let target = focused.or_else(|| {
            if state.pane().selection.items.len() == 1 {
                state.pane().selection.items.iter().next().cloned()
            } else {
                None
            }
        });
        if let Some(path) = target {
            acts.push(Action::Activate(path));
        }
    }
}

/// Floating label with the dragged file count while an internal drag is in progress.
fn drag_overlay(ctx: &egui::Context) {
    let Some(payload) = egui::DragAndDrop::payload::<DragPaths>(ctx) else {
        return;
    };
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    let n = payload.0.len();
    let text = if n == 1 {
        payload
            .0
            .first()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "1 file".to_owned())
    } else {
        format!("{n} files")
    };
    egui::Area::new(Id::new("gb_drag_preview"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + vec2(14.0, 14.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(text);
            });
        });
}

/// Files dropped from other applications are copied into the current folder.
fn external_drops(
    ctx: &egui::Context,
    state: &mut AppState,
    pane_rect: Rect,
    acts: &mut Vec<Action>,
) {
    let over_pane = ctx
        .pointer_latest_pos()
        .is_none_or(|p| pane_rect.contains(p));
    if over_pane && ctx.input(|i| !i.raw.hovered_files.is_empty()) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            Id::new("gb_drop_overlay"),
        ));
        let color = ctx.global_style().visuals.selection.stroke.color;
        painter.rect_stroke(
            pane_rect.shrink(3.0),
            6.0,
            Stroke::new(2.0, color),
            StrokeKind::Inside,
        );
        let msg = match &state.pane().nav.current {
            Some(dir) => format!(
                "Drop to copy into {}",
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| dir.display().to_string())
            ),
            None => "Open a folder to drop files into".to_owned(),
        };
        painter.text(
            pane_rect.center(),
            Align2::CENTER_CENTER,
            msg,
            FontId::proportional(16.0),
            color,
        );
    }
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    });
    if dropped.is_empty() || !over_pane {
        return;
    }
    match state.pane().nav.current.clone() {
        Some(dest) => acts.push(Action::Drop {
            paths: dropped,
            dest,
            copy: true,
        }),
        None => state.info("Open a folder before dropping files"),
    }
}

/// Upper-case file extension for badges ("FILE" when there is none).
fn extension_label(entry: &Entry) -> String {
    std::path::Path::new(&entry.name)
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .filter(|e| !e.is_empty() && e.chars().count() <= 8)
        .unwrap_or_else(|| "FILE".to_owned())
}

/// Dragging files over the pane's area: highlight it and, on release, move (Ctrl: copy) them into
/// this pane's folder. Only in dual-pane mode; a drag that started in this same folder is ignored.
fn pane_drop(ctx: &egui::Context, state: &AppState, outer: Rect, acts: &mut Vec<Action>) {
    if !state.settings.dual_pane {
        return;
    }
    let Some(payload) = egui::DragAndDrop::payload::<DragPaths>(ctx) else {
        return;
    };
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };
    let pane = state.pane();
    if !outer.contains(pos) || pane.filters.scope != FilterScope::Folder {
        return;
    }
    let Some(dest) = pane.nav.current.clone() else {
        return;
    };
    if pane_drop_is_noop(&payload.0, &dest) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("gb_pane_drop_overlay"),
    ));
    painter.rect_stroke(
        outer.shrink(2.0),
        4.0,
        Stroke::new(2.0, theme::ACCENT),
        StrokeKind::Inside,
    );
    if ctx.input(|i| i.pointer.any_released()) {
        let copy = ctx.input(|i| i.modifiers.ctrl);
        acts.push(Action::Drop {
            paths: payload.0.clone(),
            dest,
            copy,
        });
    }
}

/// A drop is ignored when every dragged path already lives directly in the target folder
/// (the drag started in this pane).
fn pane_drop_is_noop(paths: &[PathBuf], dest: &std::path::Path) -> bool {
    paths.iter().all(|p| p.parent() == Some(dest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn columns_follow_spec_formula() {
        assert_eq!(grid_columns(100.0, 128), 1);
        assert_eq!(grid_columns(0.0, 128), 1);
        assert_eq!(grid_columns(f32::NAN, 128), 1);
        // (860 - 8) / (128 + 12) = 6.09
        assert_eq!(grid_columns(860.0, 128), 6);
        assert_eq!(grid_columns(148.0, 128), 1);
        assert_eq!(grid_columns(148.0 + 140.0, 128), 2);
    }

    #[test]
    fn size_formatting() {
        assert_eq!(format_size(0), "0.0 B");
        assert_eq!(format_size(1023), "1023.0 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.0 GB");
        assert_eq!(format_size(u64::MAX), "16777216.0 TB");
    }

    #[test]
    fn date_formatting_is_total() {
        assert_eq!(format_date(None), "—");
        assert_eq!(format_date(Some(UNIX_EPOCH - Duration::from_secs(5))), "—");
        // `SystemTime` addition panics on overflow (Windows' range is far smaller than u64).
        if let Some(far) = UNIX_EPOCH.checked_add(Duration::from_secs(u64::MAX / 2)) {
            assert_eq!(format_date(Some(far)), "—");
        }
        let s = format_date(Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)));
        assert_eq!(s.len(), 16);
        assert!(s.starts_with("2023-11-1"));
    }

    #[test]
    fn same_folder_drop_is_ignored() {
        let dest = std::path::Path::new("/a");
        assert!(pane_drop_is_noop(&[PathBuf::from("/a/x.png")], dest));
        assert!(!pane_drop_is_noop(
            &[PathBuf::from("/a/x.png"), PathBuf::from("/b/y.png")],
            dest
        ));
    }

    #[test]
    fn ctrl_beats_shift() {
        assert_eq!(select_mode(Modifiers::NONE), SelectMode::Replace);
        assert_eq!(select_mode(Modifiers::SHIFT), SelectMode::Range);
        assert_eq!(select_mode(Modifiers::CTRL), SelectMode::Toggle);
        assert_eq!(
            select_mode(Modifiers::CTRL | Modifiers::SHIFT),
            SelectMode::Toggle
        );
    }

    #[test]
    fn scroll_target_only_when_needed() {
        // rows of 100, viewport 300 at offset 0
        assert_eq!(scroll_target(0, 100.0, 0.0, 300.0), None);
        assert_eq!(scroll_target(2, 100.0, 0.0, 300.0), None);
        assert_eq!(scroll_target(3, 100.0, 0.0, 300.0), Some(100.0));
        assert_eq!(scroll_target(1, 100.0, 250.0, 300.0), Some(100.0));
        assert_eq!(scroll_target(5, 100.0, 0.0, 0.0), None);
    }

    #[test]
    fn nearest_size() {
        assert_eq!(nearest_size_index(64), 0);
        assert_eq!(nearest_size_index(128), 1);
        assert_eq!(nearest_size_index(200), 2);
        assert_eq!(nearest_size_index(9999), 3);
        assert_eq!(nearest_size_index(0), 0);
    }

    #[test]
    fn star_polygon_has_ten_points_around_center() {
        let pts = star_points(pos2(10.0, 10.0), 5.0);
        assert_eq!(pts.len(), 10);
        // First point is the top tip.
        assert!((pts[0].x - 10.0).abs() < 1e-4 && (pts[0].y - 5.0).abs() < 1e-4);
    }
}
