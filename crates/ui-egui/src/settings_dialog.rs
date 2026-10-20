use gallerybrowser_core::{Settings, Theme};

use crate::state::{Action, AppState};

/// Editable copy of Settings; OK pushes Action::ApplySettings.
pub struct SettingsDialog {
    pub draft: Settings,
    pub tab: Tab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    General,
    Performance,
}

impl SettingsDialog {
    pub fn new(settings: &Settings) -> Self {
        Self {
            draft: settings.clone(),
            tab: Tab::General,
        }
    }
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut dlg) = state.dialogs.settings.take() else {
        return;
    };
    let mut keep_open = true;
    let mut apply = false;
    let mut clear_cache = false;
    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .min_width(480.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut dlg.tab, Tab::General, "General");
                ui.selectable_value(&mut dlg.tab, Tab::Performance, "Performance");
            });
            ui.separator();
            let d = &mut dlg.draft;
            match dlg.tab {
                Tab::General => {
                    egui::Grid::new("settings_general")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Default thumbnail size");
                            egui::ComboBox::from_id_salt("thumb_size")
                                .selected_text(format!("{} px", d.thumbnail_size))
                                .show_ui(ui, |ui| {
                                    for s in [64u32, 128, 256, 512] {
                                        ui.selectable_value(
                                            &mut d.thumbnail_size,
                                            s,
                                            format!("{s} px"),
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label("Theme");
                            egui::ComboBox::from_id_salt("theme")
                                .selected_text(match d.theme {
                                    Theme::Dark => "Dark",
                                    Theme::Light => "Light",
                                    Theme::System => "System",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut d.theme, Theme::Dark, "Dark");
                                    ui.selectable_value(&mut d.theme, Theme::Light, "Light");
                                    ui.selectable_value(&mut d.theme, Theme::System, "System");
                                });
                            ui.end_row();
                            ui.label("Show hidden files and folders");
                            ui.checkbox(&mut d.show_hidden, "");
                            ui.end_row();
                            ui.label("Collapse image sequences");
                            ui.checkbox(&mut d.collapse_sequences, "");
                            ui.end_row();
                            ui.label("Auto-play video previews");
                            ui.checkbox(&mut d.video_preview, "");
                            ui.end_row();
                            ui.label("Image sequence frame rate");
                            ui.add(
                                egui::DragValue::new(&mut d.sequence_fps)
                                    .range(1.0..=240.0)
                                    .speed(0.1)
                                    .suffix(" fps"),
                            );
                            ui.end_row();
                            ui.label("Recent folders to remember");
                            ui.add(egui::DragValue::new(&mut d.recents_max).range(0..=100));
                            ui.end_row();
                        });
                }
                Tab::Performance => {
                    egui::Grid::new("settings_perf")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Thumbnail threads");
                            ui.add(egui::DragValue::new(&mut d.thumbnail_threads).range(1..=16));
                            ui.end_row();
                            ui.label("Thumbnail disk cache");
                            ui.add(
                                egui::DragValue::new(&mut d.max_cache_mb)
                                    .range(100..=100_000)
                                    .speed(50)
                                    .suffix(" MB"),
                            );
                            ui.end_row();
                            ui.label("Playback frame cache");
                            ui.add(
                                egui::DragValue::new(&mut d.ram_preview_mb)
                                    .range(256..=65_536)
                                    .speed(64)
                                    .suffix(" MB"),
                            );
                            ui.end_row();
                            ui.label("Thumbnail cache");
                            if ui.button("Clear cache").clicked() {
                                clear_cache = true;
                            }
                            ui.end_row();
                        });
                }
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    apply = true;
                }
                if ui.button("Cancel").clicked() {
                    keep_open = false;
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        keep_open = false;
    }
    if clear_cache {
        match state.thumbs.clear_disk_cache() {
            Ok(()) => state.info("Thumbnail cache cleared"),
            Err(e) => state.error(format!("Could not clear thumbnail cache: {e}")),
        }
    }
    if apply {
        state.push(Action::ApplySettings(dlg.draft.clone()));
    } else if keep_open {
        state.dialogs.settings = Some(dlg);
    }
}
