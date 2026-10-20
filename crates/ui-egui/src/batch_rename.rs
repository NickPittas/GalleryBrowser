use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, Id, RichText, Sense, vec2};
use gallerybrowser_fileops::{
    CaseMode, InsertRemoveMode, NumberMode, NumberOrder, RenamePlan, RenameRule, RenameScope,
    RenameStatus, plan_renames,
};

use crate::file_pane::paint_elided;
use crate::state::{Action, AppState};

/// Batch rename dialog: rule editor + live preview table (from → to, status colored); Apply enabled
/// only when plan.is_valid(); pushes Action::BatchRename(plan).
pub struct BatchRenameDialog {
    pub files: Vec<PathBuf>,
    pub rule: RenameRule,
}

impl BatchRenameDialog {
    /// Dialog for `files` (every member file; sequences already expanded). Files are sorted and
    /// de-duplicated; the rule starts with the Python app's defaults (case sensitive find, numbers from
    /// 1 in steps of 1, three digits, `_` separator).
    pub fn new(mut files: Vec<PathBuf>) -> Self {
        files.sort();
        files.dedup();
        let rule = RenameRule {
            case_sensitive: true,
            start: 1,
            step: 1,
            padding: 3,
            separator: "_".to_owned(),
            remove_count: 1,
            ..RenameRule::default()
        };
        Self { files, rule }
    }
}

const ERROR_COLOR: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);
const OK_COLOR: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
const ROW_HEIGHT: f32 = 20.0;
const RULES_MAX_HEIGHT: f32 = 280.0;
const MAX_POSITION: usize = 999;
const TEMPLATE_TOKENS: [&str; 6] = ["{name}", "{ext}", "{n}", "{date}", "{time}", "{parent}"];

/// Plan computed for one (rule, files) pair; recomputed only when either changes.
struct PlanCache {
    rule: RenameRule,
    files: Vec<PathBuf>,
    plan: RenamePlan,
    valid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtMode {
    Keep,
    Change,
    Remove,
    /// Append another extension to the existing one (`a.jpg` → `a.jpg.bak`).
    Add,
}

/// Remembered extension editor state (the rule alone cannot tell "change to nothing" from "remove").
#[derive(Clone, Debug)]
struct ExtEditor {
    mode: ExtMode,
    text: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Summary {
    errors: usize,
    changed: usize,
    unchanged: usize,
}

fn summarize(plan: &RenamePlan) -> Summary {
    let mut s = Summary {
        errors: 0,
        changed: 0,
        unchanged: 0,
    };
    for item in &plan.items {
        match item.status {
            RenameStatus::Ok => s.changed += 1,
            RenameStatus::Unchanged => s.unchanged += 1,
            RenameStatus::Invalid(_) | RenameStatus::Collision(_) => s.errors += 1,
        }
    }
    s
}

fn status_line(s: &Summary, total: usize) -> (String, bool) {
    if s.errors > 0 {
        let noun = if s.errors == 1 { "error" } else { "errors" };
        (format!("{} {noun} — fix them to rename", s.errors), true)
    } else if s.changed == 0 {
        ("No changes".to_owned(), false)
    } else {
        (
            format!("{} of {total} files will be renamed", s.changed),
            false,
        )
    }
}

/// Rule value for an extension editor state: entered text is trimmed and stripped of leading dots.
fn extension_rule(mode: ExtMode, text: &str) -> Option<String> {
    match mode {
        ExtMode::Keep => None,
        ExtMode::Remove => Some(String::new()),
        ExtMode::Change | ExtMode::Add => Some(text.trim().trim_start_matches('.').to_owned()),
    }
}

fn ext_editor_for(rule: &RenameRule) -> ExtEditor {
    match rule.new_extension.as_deref() {
        None => ExtEditor {
            mode: ExtMode::Keep,
            text: String::new(),
        },
        Some("") if !rule.append_extension => ExtEditor {
            mode: ExtMode::Remove,
            text: String::new(),
        },
        Some(ext) => ExtEditor {
            mode: if rule.append_extension {
                ExtMode::Add
            } else {
                ExtMode::Change
            },
            text: ext.to_owned(),
        },
    }
}

/// Keep numeric rule fields inside the ranges the editor offers (the planner never sees zero steps
/// or absurd paddings; positions are 0–999, the remove count 1–999 like the Python dialog).
fn clamp_rule(rule: &mut RenameRule) {
    rule.start = rule.start.clamp(0, 99_999);
    rule.step = rule.step.clamp(1, 1_000);
    rule.padding = rule.padding.clamp(1, 10);
    rule.insert_pos = rule.insert_pos.min(MAX_POSITION);
    rule.remove_pos = rule.remove_pos.min(MAX_POSITION);
    rule.remove_count = rule.remove_count.clamp(1, MAX_POSITION);
    rule.number_insert_pos = rule.number_insert_pos.min(MAX_POSITION);
}

enum Outcome {
    Apply(RenamePlan),
    Cancel,
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.dialogs.batch_rename.take() else {
        return;
    };
    if dlg.files.is_empty() {
        state.info("Nothing to rename");
        return;
    }
    clamp_rule(&mut dlg.rule);

    let screen = ctx.content_rect().size();
    let min_size = vec2(
        820.0_f32.min(screen.x * 0.95),
        600.0_f32.min(screen.y * 0.95),
    );
    let default_size = vec2(
        860.0_f32.max(min_size.x).min(screen.x * 0.95),
        660.0_f32.max(min_size.y).min(screen.y * 0.95),
    );
    let title = format!(
        "Batch Rename — {} file{}",
        dlg.files.len(),
        if dlg.files.len() == 1 { "" } else { "s" }
    );

    let mut open = true;
    let mut outcome: Option<Outcome> = None;
    egui::Window::new(title)
        .id(Id::new("gb_batch_rename_window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .min_size(min_size)
        .default_size(default_size)
        .show(ctx, |ui| {
            outcome = dialog_ui(ui, &mut dlg);
        });

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !ctx.egui_wants_keyboard_input() {
        outcome = Some(Outcome::Cancel);
    }
    match outcome {
        Some(Outcome::Apply(plan)) => {
            state.push(Action::BatchRename(plan));
            return;
        }
        Some(Outcome::Cancel) => return,
        None => {}
    }
    if open {
        state.dialogs.batch_rename = Some(dlg);
    }
}

fn dialog_ui(ui: &mut egui::Ui, dlg: &mut BatchRenameDialog) -> Option<Outcome> {
    let ctx = ui.ctx().clone();
    let ext_id = Id::new("gb_batch_rename_ext_editor");
    let mut ext = ctx
        .data_mut(|d| d.get_temp::<ExtEditor>(ext_id))
        .unwrap_or_else(|| ext_editor_for(&dlg.rule));

    egui::ScrollArea::vertical()
        .id_salt("gb_batch_rename_rules")
        .max_height(RULES_MAX_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            rules_ui(ui, &mut dlg.rule, &mut ext);
        });
    dlg.rule.new_extension = extension_rule(ext.mode, &ext.text);
    dlg.rule.append_extension = ext.mode == ExtMode::Add;
    ctx.data_mut(|d| d.insert_temp(ext_id, ext));
    clamp_rule(&mut dlg.rule);

    // Recompute the preview only when the rule or the file list changed.
    let plan_id = Id::new("gb_batch_rename_plan");
    let cached: Option<Arc<PlanCache>> = ctx.data_mut(|d| d.get_temp::<Arc<PlanCache>>(plan_id));
    let cache = match cached {
        Some(c) if c.rule == dlg.rule && c.files == dlg.files => c,
        _ => {
            let plan = plan_renames(&dlg.rule, &dlg.files);
            let valid = plan.is_valid();
            let c = Arc::new(PlanCache {
                rule: dlg.rule.clone(),
                files: dlg.files.clone(),
                plan,
                valid,
            });
            ctx.data_mut(|d| d.insert_temp(plan_id, c.clone()));
            c
        }
    };

    ui.add_space(6.0);
    ui.separator();
    let footer_h = 44.0;
    let table_h = (ui.available_height() - footer_h).max(100.0);
    preview_ui(ui, &cache.plan, table_h);

    let summary = summarize(&cache.plan);
    let (text, is_error) = status_line(&summary, cache.plan.items.len());
    let mut outcome = None;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let color = if is_error {
            ERROR_COLOR
        } else if summary.changed > 0 {
            OK_COLOR
        } else {
            ui.visuals().weak_text_color()
        };
        ui.label(RichText::new(text).color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let apply = ui.add_enabled(
                cache.valid,
                egui::Button::new("Rename").fill(ui.visuals().selection.bg_fill),
            );
            if apply.clicked() {
                outcome = Some(Outcome::Apply(cache.plan.clone()));
            }
            if ui.button("Cancel").clicked() {
                outcome = Some(Outcome::Cancel);
            }
        });
    });
    outcome
}

fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong());
        add(ui);
    });
}

fn rules_ui(ui: &mut egui::Ui, rule: &mut RenameRule, ext: &mut ExtEditor) {
    ui.horizontal(|ui| {
        ui.label("Apply to");
        egui::ComboBox::from_id_salt("gb_br_scope")
            .selected_text(scope_label(rule.scope))
            .show_ui(ui, |ui| {
                for scope in [
                    RenameScope::Name,
                    RenameScope::Extension,
                    RenameScope::NameAndExtension,
                ] {
                    ui.selectable_value(&mut rule.scope, scope, scope_label(scope));
                }
            });
        ui.label(
            RichText::new("Find & Replace, Insert / Remove and Case act on this part.")
                .weak()
                .small(),
        );
    });

    group(ui, "Template", |ui| {
        ui.horizontal(|ui| {
            ui.label("Template");
            ui.add(
                egui::TextEdit::singleline(&mut rule.template)
                    .hint_text("{name}  (empty keeps the name)")
                    .desired_width(260.0),
            );
            ui.label(RichText::new("Insert:").weak());
            for token in TEMPLATE_TOKENS {
                if ui.small_button(token).clicked() {
                    rule.template.push_str(token);
                }
            }
        });
    });

    group(ui, "Find & Replace", |ui| {
        egui::Grid::new("gb_br_find").num_columns(2).show(ui, |ui| {
            ui.label("Find");
            ui.add(
                egui::TextEdit::singleline(&mut rule.find)
                    .hint_text("Text to find")
                    .desired_width(300.0),
            );
            ui.end_row();
            ui.label("Replace with");
            ui.add(
                egui::TextEdit::singleline(&mut rule.replace)
                    .hint_text("Replacement (empty = delete)")
                    .desired_width(300.0),
            );
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut rule.case_sensitive, "Case sensitive");
            ui.checkbox(&mut rule.use_regex, "Regular expression");
        });
    });

    group(ui, "Insert / Remove", |ui| {
        ui.horizontal(|ui| {
            ui.label("Mode");
            for mode in [
                InsertRemoveMode::Off,
                InsertRemoveMode::Insert,
                InsertRemoveMode::Remove,
            ] {
                ui.radio_value(&mut rule.insert_remove, mode, insert_remove_label(mode));
            }
        });
        match rule.insert_remove {
            InsertRemoveMode::Off => {}
            InsertRemoveMode::Insert => {
                ui.horizontal(|ui| {
                    ui.label("Text");
                    ui.add(
                        egui::TextEdit::singleline(&mut rule.insert_text)
                            .hint_text("Text to insert")
                            .desired_width(200.0),
                    );
                    ui.label("at pos");
                    ui.add(egui::DragValue::new(&mut rule.insert_pos).range(0..=MAX_POSITION))
                        .on_hover_text("0 = beginning (prefix); past the end appends");
                });
            }
            InsertRemoveMode::Remove => {
                ui.horizontal(|ui| {
                    ui.label("From pos");
                    ui.add(egui::DragValue::new(&mut rule.remove_pos).range(0..=MAX_POSITION));
                    ui.label("remove");
                    ui.add(egui::DragValue::new(&mut rule.remove_count).range(1..=MAX_POSITION));
                    ui.label("chars");
                });
            }
        }
    });

    group(ui, "Prefix / Suffix", |ui| {
        egui::Grid::new("gb_br_affix")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Prefix");
                ui.add(egui::TextEdit::singleline(&mut rule.prefix).desired_width(300.0));
                ui.end_row();
                ui.label("Suffix");
                ui.add(egui::TextEdit::singleline(&mut rule.suffix).desired_width(300.0));
                ui.end_row();
            });
    });

    group(ui, "Numbering", |ui| {
        ui.horizontal(|ui| {
            ui.label("Position");
            egui::ComboBox::from_id_salt("gb_br_numbermode")
                .selected_text(number_mode_label(rule.number_mode))
                .show_ui(ui, |ui| {
                    for mode in [
                        NumberMode::None,
                        NumberMode::Prefix,
                        NumberMode::Suffix,
                        NumberMode::InsertAt,
                        NumberMode::Replace,
                    ] {
                        ui.selectable_value(&mut rule.number_mode, mode, number_mode_label(mode));
                    }
                });
            if rule.number_mode == NumberMode::InsertAt {
                ui.add(egui::DragValue::new(&mut rule.number_insert_pos).range(0..=MAX_POSITION))
                    .on_hover_text("Character position in the name (0 = beginning)");
            }
            ui.label("Order");
            egui::ComboBox::from_id_salt("gb_br_numberorder")
                .selected_text(number_order_label(rule.number_order))
                .show_ui(ui, |ui| {
                    for order in [
                        NumberOrder::Current,
                        NumberOrder::NameAsc,
                        NumberOrder::ModifiedOldestFirst,
                        NumberOrder::ModifiedNewestFirst,
                    ] {
                        ui.selectable_value(
                            &mut rule.number_order,
                            order,
                            number_order_label(order),
                        );
                    }
                })
                .response
                .on_hover_text("Order in which numbers are assigned");
        });
        ui.horizontal(|ui| {
            ui.label("Start");
            ui.add(egui::DragValue::new(&mut rule.start).range(0..=99_999));
            ui.label("Step");
            ui.add(egui::DragValue::new(&mut rule.step).range(1..=1_000));
            ui.label("Padding");
            ui.add(egui::DragValue::new(&mut rule.padding).range(1..=10))
                .on_hover_text("Zero-fill width (3 = 001, 002 ...)");
            ui.label("Separator");
            ui.add(egui::TextEdit::singleline(&mut rule.separator).desired_width(60.0))
                .on_hover_text("Text between number and name (prefix / suffix)");
        });
        ui.label(
            RichText::new("These also feed the {n} token of the template.")
                .weak()
                .small(),
        );
    });

    group(ui, "Case", |ui| {
        ui.horizontal(|ui| {
            for (mode, label) in [
                (CaseMode::Keep, "Keep"),
                (CaseMode::Upper, "UPPERCASE"),
                (CaseMode::Lower, "lowercase"),
                (CaseMode::Title, "Title Case"),
                (CaseMode::Sentence, "Sentence case"),
            ] {
                ui.radio_value(&mut rule.case_mode, mode, label);
            }
        });
    });

    group(ui, "Extension", |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut ext.mode, ExtMode::Keep, "Keep");
            ui.radio_value(&mut ext.mode, ExtMode::Change, "Change to");
            ui.radio_value(&mut ext.mode, ExtMode::Remove, "Remove extension");
            ui.radio_value(&mut ext.mode, ExtMode::Add, "Add extension");
            ui.add_enabled(
                matches!(ext.mode, ExtMode::Change | ExtMode::Add),
                egui::TextEdit::singleline(&mut ext.text)
                    .hint_text("e.g. jpg")
                    .desired_width(100.0),
            );
        });
    });

    ui.add_space(4.0);
    if ui.button("Reset rules").clicked() {
        let fresh = BatchRenameDialog::new(Vec::new()).rule;
        *rule = fresh;
        *ext = ext_editor_for(rule);
    }
}

fn scope_label(scope: RenameScope) -> &'static str {
    match scope {
        RenameScope::Name => "Name only",
        RenameScope::Extension => "Extension only",
        RenameScope::NameAndExtension => "Name + Extension",
    }
}

fn insert_remove_label(mode: InsertRemoveMode) -> &'static str {
    match mode {
        InsertRemoveMode::Off => "Off",
        InsertRemoveMode::Insert => "Insert text",
        InsertRemoveMode::Remove => "Remove characters",
    }
}

fn number_order_label(order: NumberOrder) -> &'static str {
    match order {
        NumberOrder::Current => "Current order",
        NumberOrder::NameAsc => "By name (A-Z)",
        NumberOrder::ModifiedOldestFirst => "By date modified (oldest first)",
        NumberOrder::ModifiedNewestFirst => "By date modified (newest first)",
    }
}

fn number_mode_label(mode: NumberMode) -> &'static str {
    match mode {
        NumberMode::None => "Off",
        NumberMode::Prefix => "Prefix  (number_name)",
        NumberMode::Suffix => "Suffix  (name_number)",
        NumberMode::InsertAt => "Insert at position",
        NumberMode::Replace => "Replace the name with the number",
    }
}

fn preview_ui(ui: &mut egui::Ui, plan: &RenamePlan, height: f32) {
    let width = ui.available_width();
    let (name_w, status_w) = ((width * 0.38).floor(), (width * 0.24).floor());
    let header_font = egui::TextStyle::Body.resolve(ui.style());
    let (hdr, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let weak = ui.visuals().weak_text_color();
    for (i, title) in ["Original Name", "New Name", "Status"]
        .into_iter()
        .enumerate()
    {
        let x = hdr.min.x
            + 6.0
            + if i == 0 {
                0.0
            } else if i == 1 {
                name_w
            } else {
                name_w * 2.0
            };
        ui.painter().text(
            egui::pos2(x, hdr.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            header_font.clone(),
            weak,
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("gb_batch_rename_preview")
        .max_height(height)
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, plan.items.len(), |ui, range| {
            let font = egui::TextStyle::Body.resolve(ui.style());
            for idx in range {
                let Some(item) = plan.items.get(idx) else {
                    continue;
                };
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
                if idx % 2 == 1 {
                    ui.painter()
                        .rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
                }
                let (color, reason) = match &item.status {
                    RenameStatus::Ok => (OK_COLOR, "Will rename".to_owned()),
                    RenameStatus::Unchanged => (weak, "Unchanged".to_owned()),
                    RenameStatus::Invalid(why) | RenameStatus::Collision(why) => {
                        (ERROR_COLOR, why.clone())
                    }
                };
                let from = item
                    .from
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let to = item
                    .to
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let text_color = ui.visuals().text_color();
                let cols = [
                    (from.as_str(), text_color, 0.0, name_w - 8.0),
                    (to.as_str(), color, name_w, name_w - 8.0),
                    (
                        reason.as_str(),
                        color,
                        name_w * 2.0,
                        (status_w - 8.0).max(40.0),
                    ),
                ];
                for (text, col, offset, w) in cols {
                    let cell = egui::Rect::from_min_size(
                        egui::pos2(rect.min.x + 6.0 + offset, rect.min.y),
                        vec2(w, ROW_HEIGHT),
                    );
                    paint_elided(ui.painter(), cell, text, font.clone(), col);
                }
                resp.on_hover_text(item.from.display().to_string());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gallerybrowser_fileops::RenameItem;

    fn item(status: RenameStatus) -> RenameItem {
        RenameItem {
            from: "a".into(),
            to: "b".into(),
            status,
        }
    }

    #[test]
    fn new_sorts_dedups_and_sets_defaults() {
        let d = BatchRenameDialog::new(vec!["/b".into(), "/a".into(), "/b".into()]);
        assert_eq!(d.files, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        assert!(d.rule.case_sensitive);
        assert_eq!((d.rule.start, d.rule.step, d.rule.padding), (1, 1, 3));
        assert_eq!(d.rule.separator, "_");
        assert_eq!(d.rule.new_extension, None);
    }

    #[test]
    fn summary_counts_statuses() {
        let plan = RenamePlan {
            items: vec![
                item(RenameStatus::Ok),
                item(RenameStatus::Unchanged),
                item(RenameStatus::Invalid("x".into())),
                item(RenameStatus::Collision("y".into())),
            ],
        };
        assert_eq!(
            summarize(&plan),
            Summary {
                errors: 2,
                changed: 1,
                unchanged: 1
            }
        );
        assert_eq!(
            status_line(&summarize(&plan), 4),
            ("2 errors — fix them to rename".to_owned(), true)
        );
        let none = RenamePlan {
            items: vec![item(RenameStatus::Unchanged)],
        };
        assert_eq!(
            status_line(&summarize(&none), 1),
            ("No changes".to_owned(), false)
        );
        let some = RenamePlan {
            items: vec![item(RenameStatus::Ok), item(RenameStatus::Unchanged)],
        };
        assert_eq!(
            status_line(&summarize(&some), 2).0,
            "1 of 2 files will be renamed"
        );
    }

    #[test]
    fn extension_rule_strips_dots_and_whitespace() {
        assert_eq!(extension_rule(ExtMode::Keep, "jpg"), None);
        assert_eq!(extension_rule(ExtMode::Remove, "jpg"), Some(String::new()));
        assert_eq!(
            extension_rule(ExtMode::Change, "  ..jpg "),
            Some("jpg".to_owned())
        );
        assert_eq!(extension_rule(ExtMode::Change, ""), Some(String::new()));
    }

    #[test]
    fn clamp_rule_bounds_numbers() {
        let mut r = RenameRule {
            start: -5,
            step: 0,
            padding: 99,
            ..RenameRule::default()
        };
        clamp_rule(&mut r);
        assert_eq!((r.start, r.step, r.padding), (0, 1, 10));
        let mut r = RenameRule {
            start: 1_000_000,
            step: 5_000,
            padding: 0,
            ..RenameRule::default()
        };
        clamp_rule(&mut r);
        assert_eq!((r.start, r.step, r.padding), (99_999, 1_000, 1));
    }

    #[test]
    fn clamp_rule_bounds_positions() {
        let mut r = RenameRule {
            insert_pos: 5_000,
            remove_pos: 5_000,
            remove_count: 0,
            number_insert_pos: 5_000,
            ..RenameRule::default()
        };
        clamp_rule(&mut r);
        assert_eq!(
            (
                r.insert_pos,
                r.remove_pos,
                r.remove_count,
                r.number_insert_pos
            ),
            (999, 999, 1, 999)
        );
    }

    #[test]
    fn extension_editor_round_trips_add_mode() {
        assert_eq!(
            extension_rule(ExtMode::Add, " .bak"),
            Some("bak".to_owned())
        );
        let rule = RenameRule {
            new_extension: Some("bak".into()),
            append_extension: true,
            ..RenameRule::default()
        };
        let ed = ext_editor_for(&rule);
        assert_eq!((ed.mode, ed.text.as_str()), (ExtMode::Add, "bak"));
        let rule = RenameRule {
            new_extension: Some(String::new()),
            ..RenameRule::default()
        };
        assert_eq!(ext_editor_for(&rule).mode, ExtMode::Remove);
    }

    #[test]
    fn new_dialog_defaults_match_python() {
        let d = BatchRenameDialog::new(vec!["/a".into()]);
        assert_eq!(d.rule.scope, RenameScope::Name);
        assert_eq!(d.rule.insert_remove, InsertRemoveMode::Off);
        assert_eq!(d.rule.remove_count, 1);
        assert_eq!(d.rule.number_order, NumberOrder::Current);
    }
}
