use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Align, Layout, RichText};
use gallerybrowser_core::{AppDirs, FilterScope, MediaKind, Settings, ViewMode};
use gallerybrowser_fileops::FileOpsWorker;
use gallerybrowser_library::{Library, RatingFilter};
use gallerybrowser_media::{MetadataWorker, Thumbnailer};

use crate::actions;
use crate::batch_rename;
use crate::file_pane;
use crate::fullscreen;
use crate::info_pane;
use crate::preview::{self, PreviewState};
use crate::settings_dialog;
use crate::shortcuts;
use crate::sidebar;
use crate::state::{Action, AppState, Dialogs, PaneState, StatusKind};
use crate::textures::TextureCache;
use crate::theme;
use crate::tree::{self, TreeState};

const MIB: u64 = 1024 * 1024;
/// Informational status messages fade after this long; errors stay until replaced.
const STATUS_TTL: Duration = Duration::from_secs(8);

#[derive(Debug, Default)]
pub struct StartupOptions {
    /// Folders/files from the command line: a folder navigates; a file navigates to its folder,
    /// selects it and (for media) opens it in the fullscreen viewer.
    pub paths: Vec<PathBuf>,
    /// Paths forwarded by later instances (single-instance IPC) arrive here.
    pub incoming: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LeftTab {
    Folders,
    Library,
}

/// eframe application: layout = menu bar (top), breadcrumb, left side panel (tree, favorites,
/// recents, tags, collections, rating filter), central file pane, right preview+info (toggleable),
/// status bar (bottom); the fullscreen viewer replaces all panels inside the main window.
pub struct GalleryApp {
    pub state: AppState,
    pub incoming: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    tree: TreeState,
    left_tab: LeftTab,
    status_seen: Option<(StatusKind, String)>,
    status_at: Instant,
    /// Last window title sent to the compositor.
    title: String,
    /// OS-fullscreen state of the window from before F was pressed in the viewer (`None` = F unused).
    os_restore: Option<bool>,
}

impl GalleryApp {
    /// Builds all services from `AppDirs::resolve()` + `Settings::load`, opens the library
    /// (falls back to in-memory with an error status), applies theme, handles `opts.paths`
    /// (else `settings.last_folder`, else home). Err = unrecoverable startup failure message.
    pub fn new(cc: &eframe::CreationContext<'_>, opts: StartupOptions) -> Result<Self, String> {
        let dirs = AppDirs::resolve()
            .map_err(|e| format!("Cannot prepare the application folders: {e}"))?;
        let settings = Settings::load(&dirs);
        let mut startup_error = None;
        let library = match Library::open(&dirs.library_dir(), dirs.legacy_database().as_deref()) {
            Ok(l) => l,
            Err(e) => {
                startup_error = Some(format!(
                    "Library could not be opened, tags and ratings will not be saved: {e}"
                ));
                Library::in_memory()
            }
        };
        let thumbs = Thumbnailer::new(actions::thumb_config(&dirs, &settings));
        let preview = PreviewState::new(settings.ram_preview_mb.saturating_mul(MIB));
        theme::apply(&cc.egui_ctx, settings.theme);

        let start_folder = settings
            .last_folder
            .clone()
            .filter(|p| p.is_dir())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|p| p.is_dir())
            });

        let mut state = AppState {
            dirs,
            settings,
            library,
            fileops: FileOpsWorker::new(),
            thumbs,
            metadata: MetadataWorker::new(),
            current_metadata: None,
            panes: [PaneState::default(), PaneState::default()],
            active: 0,
            clipboard: None,
            status: None,
            dialogs: Dialogs::default(),
            textures: TextureCache::default(),
            preview,
            fullscreen: None,
            pending: Vec::new(),
            quit_requested: false,
            folder_dialog: None,
            progress: None,
            pending_open: None,
            tree_generation: 0,
            library_generation: 0,
            poll_until: Instant::now(),
            synced_preview: None,
            synced_meta: None,
        };
        if !opts.paths.is_empty() {
            state.push(Action::OpenPaths(opts.paths));
        } else if let Some(p) = start_folder {
            state.push(Action::Navigate(p));
        }
        if state.settings.dual_pane {
            let second = state
                .settings
                .last_folder_secondary
                .clone()
                .filter(|p| p.is_dir())
                .or_else(|| state.settings.last_folder.clone().filter(|p| p.is_dir()));
            if let Some(p) = second {
                state.push(Action::FocusPane(1));
                state.push(Action::Navigate(p));
                state.push(Action::FocusPane(0));
            }
        }
        actions::dispatch_pending(&mut state, &cc.egui_ctx);
        if let Some(e) = startup_error {
            state.error(e);
        }
        Ok(Self {
            state,
            incoming: opts.incoming,
            tree: TreeState::default(),
            left_tab: LeftTab::Folders,
            status_seen: None,
            status_at: Instant::now(),
            title: String::new(),
            os_restore: None,
        })
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let state = &mut self.state;
        let sel = &state.pane().selection;
        let has_sel = !sel.items.is_empty() || sel.focused.is_some();
        let one = sel.items.len() == 1 || (sel.items.is_empty() && sel.focused.is_some());
        let many = sel.items.len() >= 2;
        let other_ok = crate::actions::other_pane_target(
            state.settings.dual_pane,
            state.pane().nav.current.as_deref(),
            state.pane_at(state.other_index()).nav.current.as_deref(),
        )
        .is_ok();
        let can_paste = state.clipboard.is_some();
        let (can_undo, can_redo) = (state.fileops.can_undo(), state.fileops.can_redo());
        let undo_label = state.fileops.undo_label();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if item(ui, "Open Folder…", "Ctrl+O", true) {
                    state.push(Action::OpenFolderDialog);
                }
                if item(
                    ui,
                    "New Folder…",
                    "Ctrl+Shift+N",
                    state.pane().nav.current.is_some(),
                ) {
                    state.push(Action::BeginNewFolder);
                }
                ui.separator();
                if item(ui, "Settings…", "", true) {
                    state.push(Action::OpenSettings);
                }
                ui.separator();
                if item(ui, "Quit", "Ctrl+Q", true) {
                    state.push(Action::Quit);
                }
            });
            ui.menu_button("Edit", |ui| {
                let undo_text = match &undo_label {
                    Some(l) => format!("Undo {l}"),
                    None => "Undo".to_string(),
                };
                if item(ui, &undo_text, "Ctrl+Z", can_undo) {
                    state.push(Action::Undo);
                }
                if item(ui, "Redo", "Ctrl+Y", can_redo) {
                    state.push(Action::Redo);
                }
                ui.separator();
                if item(ui, "Copy", "Ctrl+C", has_sel) {
                    state.push(Action::Copy);
                }
                if item(ui, "Cut", "Ctrl+X", has_sel) {
                    state.push(Action::Cut);
                }
                if item(
                    ui,
                    "Paste",
                    "Ctrl+V",
                    can_paste && state.pane().nav.current.is_some(),
                ) {
                    state.push(Action::Paste { dest: None });
                }
                ui.separator();
                if item(ui, "Copy to Other Pane", "Ctrl+Shift+C", other_ok) {
                    state.push(Action::CopyToOtherPane);
                }
                if item(ui, "Move to Other Pane", "Ctrl+Shift+M", other_ok) {
                    state.push(Action::MoveToOtherPane);
                }
                ui.separator();
                if item(ui, "Rename…", "F2", one) {
                    state.push(Action::BeginRename);
                }
                if item(ui, "Batch Rename…", "", many) {
                    state.push(Action::BeginBatchRename);
                }
                ui.separator();
                if item(ui, "Move to Trash", "Del", has_sel) {
                    state.push(Action::Trash);
                }
                if item(ui, "Delete Permanently…", "Shift+Del", has_sel) {
                    state.push(Action::RequestDeletePermanently);
                }
                ui.separator();
                if item(ui, "Select All", "Ctrl+A", !state.pane().entries.is_empty()) {
                    state.push(Action::SelectAll);
                }
            });
            ui.menu_button("View", |ui| {
                let mode = state.settings.view_mode;
                if radio_item(ui, "Grid", mode == ViewMode::Grid) {
                    state.push(Action::SetViewMode(ViewMode::Grid));
                }
                if radio_item(ui, "List", mode == ViewMode::List) {
                    state.push(Action::SetViewMode(ViewMode::List));
                }
                ui.separator();
                if check_item(ui, "Preview Pane", "Ctrl+P", state.settings.show_preview) {
                    state.push(Action::TogglePreviewPane);
                }
                if check_item(
                    ui,
                    "Collapse Sequences",
                    "",
                    state.settings.collapse_sequences,
                ) {
                    state.push(Action::SetCollapseSequences(
                        !state.settings.collapse_sequences,
                    ));
                }
                if check_item(ui, "Show All Files", "", state.settings.show_all_files) {
                    state.push(Action::SetShowAllFiles(!state.settings.show_all_files));
                }
                if check_item(ui, "Second Pane", "F3", state.settings.dual_pane) {
                    state.push(Action::ToggleDualPane);
                }
                if item(ui, "Refresh", "F5", true) {
                    state.push(Action::Refresh);
                }
                ui.separator();
                ui.menu_button("Thumbnail Size", |ui| {
                    for s in [64u32, 128, 256, 512] {
                        if radio_item(ui, &format!("{s} px"), state.settings.thumbnail_size == s) {
                            state.push(Action::SetThumbnailSize(s));
                        }
                    }
                });
            });
            ui.menu_button("Help", |ui| {
                if item(ui, "Keyboard Shortcuts", "F1", true) {
                    state.push(Action::ShowShortcuts);
                }
                if item(ui, "About GalleryBrowser", "", true) {
                    state.push(Action::ShowAbout);
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        // fade informational messages
        let current = self
            .state
            .status
            .as_ref()
            .map(|s| (s.kind.clone(), s.text.clone()));
        if current != self.status_seen {
            self.status_seen = current;
            self.status_at = Instant::now();
        }
        let state = &self.state;
        let show_status = state
            .status
            .as_ref()
            .filter(|s| s.kind == StatusKind::Error || self.status_at.elapsed() < STATUS_TTL);
        if state.status.is_some() && show_status.is_none() {
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        } else if let Some(s) = show_status
            && s.kind == StatusKind::Info
        {
            ui.ctx().request_repaint_after(
                STATUS_TTL
                    .saturating_sub(self.status_at.elapsed())
                    .max(Duration::from_millis(50)),
            );
        }

        ui.horizontal(|ui| {
            match show_status {
                Some(s) if s.kind == StatusKind::Error => {
                    ui.label(RichText::new(&s.text).color(theme::ERROR));
                }
                Some(s) => {
                    ui.label(&s.text);
                }
                None => {
                    ui.label(RichText::new(source_label(state)).color(theme::TEXT_SECONDARY));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(count_label(state)).color(theme::TEXT_SECONDARY));
                if let Some(sel) = selection_label(state) {
                    ui.separator();
                    ui.label(sel);
                }
                if let Some((done, total)) = state.progress {
                    ui.separator();
                    if total > 0 {
                        ui.add(
                            egui::ProgressBar::new((done as f32 / total as f32).clamp(0.0, 1.0))
                                .desired_width(140.0)
                                .text(format!("{done}/{total}")),
                        );
                    } else {
                        ui.spinner();
                        ui.label("Working…");
                    }
                }
                if let Some(note) = &state.pane().scan_note {
                    ui.separator();
                    ui.label(RichText::new(note).color(theme::WARNING));
                }
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let state = &mut self.state;

        if let Some((path, mut name)) = state.dialogs.rename.take() {
            let select = name
                .rfind('.')
                .filter(|&i| i > 0)
                .map_or(name.chars().count(), |i| name[..i].chars().count());
            match prompt(ctx, "Rename", "New name:", &mut name, "Rename", select) {
                Prompt::Open => state.dialogs.rename = Some((path, name)),
                Prompt::Accept => state.push(Action::Rename {
                    from: path,
                    new_name: name,
                }),
                Prompt::Cancel => {}
            }
        }
        if let Some(mut name) = state.dialogs.new_folder.take() {
            let n = name.chars().count();
            match prompt(ctx, "New Folder", "Folder name:", &mut name, "Create", n) {
                Prompt::Open => state.dialogs.new_folder = Some(name),
                Prompt::Accept => state.push(Action::NewFolder(name)),
                Prompt::Cancel => {}
            }
        }
        if let Some(mut name) = state.dialogs.new_tag.take() {
            match prompt(ctx, "New Tag", "Tag name:", &mut name, "Create", 0) {
                Prompt::Open => state.dialogs.new_tag = Some(name),
                Prompt::Accept => state.push(Action::CreateTag(name)),
                Prompt::Cancel => {}
            }
        }
        if let Some(mut name) = state.dialogs.new_collection.take() {
            match prompt(
                ctx,
                "New Collection",
                "Collection name:",
                &mut name,
                "Create",
                0,
            ) {
                Prompt::Open => state.dialogs.new_collection = Some(name),
                Prompt::Accept => state.push(Action::CreateCollection(name)),
                Prompt::Cancel => {}
            }
        }
        if let Some(paths) = state.dialogs.confirm_delete.take() {
            let mut result = None;
            egui::Window::new("Delete Permanently")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    let n = paths.len();
                    ui.label(format!(
                        "Permanently delete {n} file{}? This cannot be undone.",
                        if n == 1 { "" } else { "s" }
                    ));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        // Cancel first: it is the safe default
                        let cancel = ui.button("Cancel");
                        cancel.request_focus();
                        if cancel.clicked() {
                            result = Some(false);
                        }
                        if ui
                            .add(egui::Button::new(
                                RichText::new("Delete").color(theme::ERROR),
                            ))
                            .clicked()
                        {
                            result = Some(true);
                        }
                    });
                });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                result = Some(false);
            }
            match result {
                None => state.dialogs.confirm_delete = Some(paths),
                Some(true) => state.push(Action::DeletePermanently(paths)),
                Some(false) => {}
            }
        }
        if state.dialogs.about {
            let mut open = true;
            egui::Window::new("About GalleryBrowser")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.heading("GalleryBrowser");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label("Asset browser for video, images and image sequences.");
                    ui.add_space(6.0);
                    ui.label("License: MIT");
                    ui.add_space(6.0);
                    ui.label("Built on the ArtCraft stack: egui/eframe, with media decoding from");
                    ui.label("FilmCraft, PhotoCraft and LightCraft (MIT OR Apache-2.0).");
                });
            if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                state.dialogs.about = false;
            }
        }
        if state.dialogs.shortcuts {
            let mut open = true;
            egui::Window::new("Keyboard Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::Grid::new("shortcut_grid")
                        .num_columns(2)
                        .spacing([24.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (keys, what) in shortcuts::table() {
                                ui.label(RichText::new(keys).monospace().strong());
                                ui.label(what);
                                ui.end_row();
                            }
                        });
                });
            if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                state.dialogs.shortcuts = false;
            }
        }
        settings_dialog::show(ctx, state);
        batch_rename::show(ctx, state);
    }
}

impl eframe::App for GalleryApp {
    /// Per-frame non-UI work: poll workers/IPC, dispatch actions, shortcuts (skipped while the viewer is open).
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.incoming {
            let mut batches = Vec::new();
            while let Ok(paths) = rx.try_recv() {
                batches.push(paths);
            }
            for paths in batches {
                self.state.push(Action::OpenPaths(paths));
            }
        }
        actions::poll_workers(&mut self.state, ctx);
        shortcuts::handle(ctx, &mut self.state);
        actions::dispatch_pending(&mut self.state, ctx);

        if self.state.quit_requested {
            self.state.quit_requested = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let title = window_title(&self.state);
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Lay out the fullscreen viewer or all panels into the root `ui`.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if self.state.fullscreen.is_some() {
            fullscreen::show(ui, &mut self.state, &mut self.os_restore);
            self.dialogs(&ctx);
        } else {
            self.panels(ui);
            self.dialogs(&ctx);
        }
        actions::dispatch_pending(&mut self.state, &ctx);
        fullscreen::restore_os_fullscreen(&ctx, &self.state, &mut self.os_restore);
    }

    /// Save settings (incl. last_folder, volume), shut down preview/player and workers.
    fn on_exit(&mut self) {
        self.state.settings.last_folder = self.state.panes[0].nav.current.clone();
        if self.state.settings.dual_pane {
            self.state.settings.last_folder_secondary = self.state.panes[1].nav.current.clone();
        }
        actions::save_settings(&mut self.state);
        self.state.preview.shutdown();
    }
}

impl GalleryApp {
    /// Lay out all normal panels into the root `ui`.
    fn panels(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));

        egui::Panel::left("left_panel")
            .resizable(true)
            .default_size(260.0)
            .size_range(200.0..=420.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.left_tab, LeftTab::Folders, "Folders");
                    ui.selectable_value(&mut self.left_tab, LeftTab::Library, "Library");
                });
                ui.separator();
                match self.left_tab {
                    LeftTab::Folders => tree::show(ui, &mut self.state, &mut self.tree),
                    LeftTab::Library => sidebar::show(ui, &mut self.state),
                }
            });

        if self.state.settings.show_preview {
            egui::Panel::right("right_panel")
                .resizable(true)
                .default_size(340.0)
                .size_range(250.0..=520.0)
                .show(ui, |ui| {
                    // the preview sizes itself (picture + transport rows); info takes all the rest
                    preview::show(ui, &mut self.state);
                    ui.add_space(4.0);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("info_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| info_pane::show(ui, &mut self.state));
                });
        }

        egui::CentralPanel::no_frame().show(ui, |ui| {
            if !self.state.settings.dual_pane {
                egui::Frame::NONE
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| file_pane::show(ui, &mut self.state, 0));
                return;
            }
            // two panes side by side with a draggable divider; each is driven by the tree when active
            let pane_frame = egui::Frame::NONE.inner_margin(egui::Margin::same(6));
            let total = ui.available_width();
            egui::Panel::left("pane_left")
                .resizable(true)
                .default_size(total / 2.0)
                .size_range(240.0..=(total - 240.0).max(240.0))
                .frame(pane_frame)
                .show(ui, |ui| file_pane::show(ui, &mut self.state, 0));
            egui::CentralPanel::no_frame().show(ui, |ui| {
                pane_frame.show(ui, |ui| file_pane::show(ui, &mut self.state, 1));
            });
        });
    }
}

/// Window title: the viewed file name while the viewer is open, otherwise the current folder.
fn window_title(state: &AppState) -> String {
    if let Some(fs) = &state.fullscreen {
        let name = fs.path.file_name().map_or_else(
            || fs.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        return format!("GalleryBrowser — {name}");
    }
    match &state.pane().nav.current {
        Some(p) => format!("GalleryBrowser — {}", p.display()),
        None => "GalleryBrowser".to_string(),
    }
}

// ---- menu helpers -------------------------------------------------------------------

fn item(ui: &mut egui::Ui, label: &str, shortcut: &str, enabled: bool) -> bool {
    let mut b = egui::Button::new(label);
    if !shortcut.is_empty() {
        b = b.shortcut_text(shortcut);
    }
    let clicked = ui.add_enabled(enabled, b).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn check_item(ui: &mut egui::Ui, label: &str, shortcut: &str, checked: bool) -> bool {
    let mark = if checked { "✔  " } else { "     " };
    item(ui, &format!("{mark}{label}"), shortcut, true)
}

fn radio_item(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let mark = if selected { "●  " } else { "     " };
    item(ui, &format!("{mark}{label}"), "", true)
}

// ---- dialogs ------------------------------------------------------------------------

enum Prompt {
    Open,
    Accept,
    Cancel,
}

/// Single-line text prompt. `select_chars` characters from the start are pre-selected on first show.
fn prompt(
    ctx: &egui::Context,
    title: &str,
    label: &str,
    text: &mut String,
    ok: &str,
    select_chars: usize,
) -> Prompt {
    let mut result = Prompt::Open;
    let edit_id = egui::Id::new(("prompt_edit", title));
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(label);
            let mut out = egui::TextEdit::singleline(text)
                .id(edit_id)
                .desired_width(280.0)
                .show(ui);
            let first = ui.data_mut(|d| {
                let seen: &mut bool = d.get_temp_mut_or(edit_id.with("seen"), false);
                let first = !*seen;
                *seen = true;
                first
            });
            if first {
                out.response.request_focus();
                let range = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(select_chars),
                );
                out.state.cursor.set_char_range(Some(range));
                out.state.store(ui.ctx(), edit_id);
            }
            let enter = out.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!text.trim().is_empty(), egui::Button::new(ok))
                    .clicked()
                    || (enter && !text.trim().is_empty())
                {
                    result = Prompt::Accept;
                }
                if ui.button("Cancel").clicked() {
                    result = Prompt::Cancel;
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = Prompt::Cancel;
    }
    if !matches!(result, Prompt::Open) {
        ctx.data_mut(|d| d.remove::<bool>(edit_id.with("seen")));
    }
    result
}

// ---- status text --------------------------------------------------------------------

fn source_label(state: &AppState) -> String {
    let mut parts = vec![
        match (state.pane().filters.scope, &state.pane().nav.current) {
            (FilterScope::Library, _) => "Library".to_string(),
            (_, Some(p)) => p.display().to_string(),
            (_, None) => "No folder open".to_string(),
        },
    ];
    let q = &state.pane().filters.query;
    for t in &q.tags {
        parts.push(format!("tag:{t}"));
    }
    if let Some(c) = &q.collection {
        parts.push(format!("collection:{c}"));
    }
    match q.rating {
        Some(RatingFilter::Unrated) => parts.push("rating:unrated".into()),
        Some(RatingFilter::Exactly(n)) => parts.push(format!("rating:{n}")),
        Some(RatingFilter::AtLeast(n)) => parts.push(format!("rating:≥{n}")),
        None => {}
    }
    for k in &state.pane().filters.kinds {
        parts.push(format!(
            "type:{}",
            match k {
                MediaKind::Image => "image",
                MediaKind::Video => "video",
                MediaKind::Sequence => "sequence",
                MediaKind::Audio => "audio",
                MediaKind::Other => "other",
            }
        ));
    }
    if !state.pane().filters.search.trim().is_empty() {
        parts.push(format!("search:{}", state.pane().filters.search.trim()));
    }
    parts.join("  ·  ")
}

fn count_label(state: &AppState) -> String {
    let seqs = state
        .pane()
        .entries
        .iter()
        .filter(|e| e.sequence.is_some())
        .count();
    let total = state.pane().scanned.len();
    let shown = state.pane().entries.len();
    let mut s = if shown == total {
        format!("{shown} items")
    } else {
        format!("{shown} of {total} items")
    };
    if seqs > 0 {
        s.push_str(&format!(
            " ({seqs} sequence{})",
            if seqs == 1 { "" } else { "s" }
        ));
    }
    s
}

fn selection_label(state: &AppState) -> Option<String> {
    let sel = state.selected_entries();
    match sel.as_slice() {
        [] => None,
        [e] => Some(format!("{} — {}", e.name, format_size(e.size))),
        many => {
            let total: u64 = many.iter().fold(0u64, |a, e| a.saturating_add(e.size));
            Some(format!("{} selected — {}", many.len(), format_size(total)))
        }
    }
}

/// Powers of 1024, one decimal place.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS.get(u).copied().unwrap_or("TB"))
}

#[cfg(test)]
mod tests {
    use super::format_size;

    #[test]
    fn sizes_use_binary_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.0 GB");
    }
}
