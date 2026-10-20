//! Central action dispatcher and the single refresh pipeline. (Shell slice.)

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gallerybrowser_core::sequence::split_numbered;
use gallerybrowser_core::{
    AppDirs, Entry, FilterScope, MediaKind, ScanOptions, Settings, classify, scan_folder,
    sort_entries,
};
use gallerybrowser_fileops::{ConflictPolicy, FileOp, OpEvent, OpOutcome};
use gallerybrowser_media::ThumbnailerConfig;

use crate::batch_rename::BatchRenameDialog;
use crate::fullscreen::FullscreenState;
use crate::settings_dialog::SettingsDialog;
use crate::state::{Action, AppState, ClipMode, Clipboard, SelectMode};
use crate::theme;

const MIB: u64 = 1024 * 1024;
/// Dispatch rounds per frame (actions may enqueue more); bounds accidental loops.
const MAX_ROUNDS: usize = 8;

/// Thumbnailer configuration derived from settings (used at startup and on ApplySettings).
pub fn thumb_config(dirs: &AppDirs, s: &Settings) -> ThumbnailerConfig {
    ThumbnailerConfig {
        cache_dir: dirs.thumbnails_dir(),
        memory_bytes: 256 * MIB,
        disk_bytes: s.max_cache_mb.saturating_mul(MIB),
        threads: s.thumbnail_threads.clamp(1, 16),
    }
}

/// Apply every queued action in order (actions may enqueue more; bounded to avoid loops).
pub fn dispatch_pending(state: &mut AppState, ctx: &egui::Context) {
    for _ in 0..MAX_ROUNDS {
        if state.pending.is_empty() {
            break;
        }
        let actions = std::mem::take(&mut state.pending);
        for a in actions {
            dispatch(state, ctx, a);
        }
    }
    if !state.pending.is_empty() {
        ctx.request_repaint();
    }
}

pub fn dispatch(state: &mut AppState, ctx: &egui::Context, action: Action) {
    match action {
        // ---- navigation ----
        Action::Navigate(p) => navigate(state, p),
        Action::OpenFolderDialog => open_folder_dialog(state, ctx),
        Action::Back => {
            if let Some(prev) = state.pane_mut().nav.back.pop() {
                if let Some(cur) = state.pane().nav.current.clone() {
                    state.pane_mut().nav.forward.push(cur);
                }
                let pane = state.active;
                set_current(state, pane, prev, false);
            }
        }
        Action::Forward => {
            if let Some(next) = state.pane_mut().nav.forward.pop() {
                if let Some(cur) = state.pane().nav.current.clone() {
                    state.pane_mut().nav.back.push(cur);
                }
                let pane = state.active;
                set_current(state, pane, next, false);
            }
        }
        Action::Up => {
            let parent = state
                .pane()
                .nav
                .current
                .as_ref()
                .and_then(|c| c.parent())
                .map(Path::to_path_buf);
            if let Some(parent) = parent {
                navigate(state, parent);
            }
        }
        Action::Refresh => {
            state.tree_generation = state.tree_generation.wrapping_add(1);
            refresh_all(state, true);
        }

        // ---- selection ----
        Action::Select { path, mode } => select(state, path, mode),
        Action::SelectAll => {
            let p = state.pane_mut();
            p.selection.items = p.entries.iter().map(|e| e.path.clone()).collect();
            if p.selection.focused.is_none() {
                p.selection.focused = p.entries.first().map(|e| e.path.clone());
            }
            if p.selection.anchor.is_none() {
                p.selection.anchor = p.selection.focused.clone();
            }
        }
        Action::ClearSelection => {
            state.pane_mut().selection.items.clear();
            state.pane_mut().selection.anchor = None;
        }
        Action::MoveFocus { delta, extend } => move_focus(state, delta, extend),
        Action::Activate(p) => activate(state, p),

        // ---- file operations ----
        Action::Copy => clipboard_set(state, ClipMode::Copy),
        Action::Cut => clipboard_set(state, ClipMode::Cut),
        Action::Paste { dest } => paste(state, dest),
        Action::Drop { paths, dest, copy } => drop_paths(state, paths, dest, copy),
        Action::Trash => {
            let members = target_members(state);
            if members.is_empty() {
                state.info("Nothing selected");
            } else {
                submit(state, FileOp::Trash { paths: members });
            }
        }
        Action::RequestDeletePermanently => {
            let members = target_members(state);
            if members.is_empty() {
                state.info("Nothing selected");
            } else {
                state.dialogs.confirm_delete = Some(members);
            }
        }
        Action::DeletePermanently(paths) => {
            if !paths.is_empty() {
                submit(state, FileOp::Delete { paths });
            }
        }
        Action::BeginRename => begin_rename(state),
        Action::Rename { from, new_name } => rename(state, from, &new_name),
        Action::BeginBatchRename => begin_batch_rename(state),
        Action::BatchRename(plan) => {
            state.dialogs.batch_rename = None;
            if plan.is_valid() {
                submit(state, FileOp::BatchRename { plan });
            } else {
                state.error("Batch rename plan has conflicts; nothing was renamed");
            }
        }
        Action::BeginNewFolder => {
            if state.pane().nav.current.is_some()
                && state.pane().filters.scope == FilterScope::Folder
            {
                state.dialogs.new_folder = Some("New Folder".to_string());
            } else {
                state.error("Open a folder (Folder scope) before creating a new folder");
            }
        }
        Action::NewFolder(name) => new_folder(state, &name),
        Action::Undo => match state.fileops.undo() {
            Some(_) => state.info("Undoing…"),
            None => state.info("Nothing to undo"),
        },
        Action::Redo => match state.fileops.redo() {
            Some(_) => state.info("Redoing…"),
            None => state.info("Nothing to redo"),
        },
        Action::RevealInFileManager(p) => {
            if let Err(msg) = reveal_in_file_manager(&p) {
                state.error(msg);
            }
        }
        Action::OpenWithDefaultApp(p) => {
            let target = state.entry(&p).map(Entry::representative_path).unwrap_or(p);
            if let Err(e) = open::that_detached(&target) {
                state.error(format!("Could not open {}: {e}", target.display()));
            }
        }
        Action::CopyPathToClipboard(p) => {
            ctx.copy_text(p.display().to_string());
            state.info("Path copied to clipboard");
        }

        // ---- organization ----
        Action::SetRating(n) => {
            let ids = target_ids(state);
            annotate(state, &ids, "Rating", |lib, ids| lib.set_rating(ids, n));
        }
        Action::AddTag(tag) => {
            let tag = tag.trim().to_string();
            let ids = target_ids(state);
            if !tag.is_empty() {
                annotate(state, &ids, "Tag", |lib, ids| lib.add_tag(ids, &tag));
            }
        }
        Action::RemoveTag(tag) => {
            let ids = target_ids(state);
            annotate(state, &ids, "Tag", |lib, ids| lib.remove_tag(ids, &tag));
        }
        Action::CreateTag(name) => {
            let name = name.trim().to_string();
            let exists = state.library.tags().iter().any(|t| t.name == name);
            if !name.is_empty() && !exists {
                match state.library.create_tag(&name) {
                    Ok(()) => state.info(format!("Created tag “{name}”")),
                    Err(e) => state.error(format!("Could not create tag: {e}")),
                }
                bump_library(state);
            }
        }
        Action::DeleteTag(name) => {
            match state.library.delete_tag(&name) {
                Ok(()) => state.info(format!("Deleted tag “{name}”")),
                Err(e) => state.error(format!("Could not delete tag: {e}")),
            }
            bump_library(state);
            for i in 0..state.panes.len() {
                if state.pane_at(i).filters.query.tags.contains(&name) {
                    state
                        .pane_at_mut(i)
                        .filters
                        .query
                        .tags
                        .retain(|t| *t != name);
                    refresh_view(state, i, true);
                }
            }
        }
        Action::AddToCollection(name) => {
            let name = name.trim().to_string();
            let ids = target_ids(state);
            if !name.is_empty() {
                annotate(state, &ids, "Collection", |lib, ids| {
                    lib.add_to_collection(ids, &name)
                });
            }
        }
        Action::RemoveFromCollection(name) => {
            let ids = target_ids(state);
            annotate(state, &ids, "Collection", |lib, ids| {
                lib.remove_from_collection(ids, &name)
            });
        }
        Action::CreateCollection(name) => {
            let name = name.trim().to_string();
            let exists = state.library.collections().iter().any(|c| c.name == name);
            if !name.is_empty() && !exists {
                match state.library.create_collection(&name) {
                    Ok(()) => state.info(format!("Created collection “{name}”")),
                    Err(e) => state.error(format!("Could not create collection: {e}")),
                }
                bump_library(state);
            }
        }
        Action::DeleteCollection(name) => {
            match state.library.delete_collection(&name) {
                Ok(()) => state.info(format!("Deleted collection “{name}”")),
                Err(e) => state.error(format!("Could not delete collection: {e}")),
            }
            bump_library(state);
            for i in 0..state.panes.len() {
                if state.pane_at(i).filters.query.collection.as_deref() == Some(name.as_str()) {
                    state.pane_at_mut(i).filters.query.collection = None;
                    refresh_view(state, i, true);
                }
            }
        }

        // ---- view / filters ----
        Action::SetScope(scope) => {
            if state.pane().filters.scope != scope {
                state.pane_mut().filters.scope = scope;
                refresh_active(state, true);
            }
        }
        Action::SetQuery(q) => {
            state.pane_mut().filters.query = q;
            refresh_active(state, true);
        }
        Action::SetSearch(s) => {
            state.pane_mut().filters.search = s;
            refresh_active(state, false);
        }
        Action::SetKinds(k) => {
            state.pane_mut().filters.kinds = k;
            refresh_active(state, false);
        }
        Action::SetSort { key, ascending } => {
            state.settings.sort_key = key;
            state.settings.sort_ascending = ascending;
            refresh_all(state, false);
        }
        Action::SetViewMode(m) => state.settings.view_mode = m,
        Action::SetThumbnailSize(s) => {
            state.settings.thumbnail_size = s.clamp(64, 512);
            refresh_all(state, false);
        }
        Action::SetCollapseSequences(b) => {
            // Goes through the one pipeline so active library filters still apply (Python bug #3).
            state.settings.collapse_sequences = b;
            refresh_all(state, true);
        }
        Action::TogglePreviewPane => state.settings.show_preview = !state.settings.show_preview,
        Action::FocusPane(i) => state.set_active(i),
        Action::ToggleDualPane => toggle_dual_pane(state),
        Action::SetShowAllFiles(b) => {
            state.settings.show_all_files = b;
            save_settings(state);
            refresh_all(state, true);
        }
        Action::CopyToOtherPane => transfer_to_other_pane(state, true),
        Action::MoveToOtherPane => transfer_to_other_pane(state, false),
        Action::AddFavorite(p) => {
            if !state.settings.favorites.contains(&p) {
                state.settings.favorites.push(p);
            }
        }
        Action::RemoveFavorite(p) => state.settings.favorites.retain(|f| *f != p),

        // ---- windows ----
        Action::OpenSettings => state.dialogs.settings = Some(SettingsDialog::new(&state.settings)),
        Action::ApplySettings(s) => apply_settings(state, ctx, s),
        Action::OpenFullscreen(p) => open_fullscreen(state, p),
        Action::CloseFullscreen => {
            state.fullscreen = None;
        }
        Action::FullscreenStep(delta) => fullscreen_step(state, delta),
        Action::ShowAbout => state.dialogs.about = true,
        Action::ShowShortcuts => state.dialogs.shortcuts = true,
        Action::Quit => state.quit_requested = true,
        Action::OpenPaths(paths) => open_paths(state, ctx, paths),
    }
    sync_focus(state);
}

/// Shows `p` in the platform file manager. Linux opens the folder (`p` itself, or the containing
/// folder of a file) with the default handler; macOS and Windows select the item in Finder/Explorer.
#[cfg(not(any(target_os = "macos", windows)))]
fn reveal_in_file_manager(p: &Path) -> Result<(), String> {
    let dir = if p.is_dir() { Some(p) } else { p.parent() };
    match dir {
        Some(d) => open::that_detached(d)
            .map_err(|e| format!("Could not open file manager for {}: {e}", d.display())),
        None => Err(format!("No containing folder for {}", p.display())),
    }
}

#[cfg(target_os = "macos")]
fn reveal_in_file_manager(p: &Path) -> Result<(), String> {
    let status = std::process::Command::new("open")
        .arg("-R")
        .arg(p)
        .status()
        .map_err(|e| format!("Could not open Finder for {}: {e}", p.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not reveal {} in Finder ({status})",
            p.display()
        ))
    }
}

#[cfg(windows)]
fn reveal_in_file_manager(p: &Path) -> Result<(), String> {
    // Explorer reports failure with a non-zero exit code even when it succeeded, so only the spawn is
    // checked. The whole `/select,<path>` is one argument, as Explorer expects.
    let mut arg = std::ffi::OsString::from("/select,");
    arg.push(p);
    std::process::Command::new("explorer")
        .arg(arg)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not open Explorer for {}: {e}", p.display()))
}

// ======================================================================================
// refresh pipeline
// ======================================================================================

/// Rescan (if `rescan`) and rebuild pane `pane`'s entries; reconcile its selection; request thumbnails.
pub fn refresh_view(state: &mut AppState, pane: usize, rescan: bool) {
    let pane = if pane < state.panes.len() { pane } else { 0 };
    let scope = state.pane_at(pane).filters.scope;
    if rescan {
        state.pane_at_mut(pane).scan_note = None;
        let scanned = match scope {
            FilterScope::Folder => scan_current(state, pane),
            FilterScope::Library => library_entries(state, pane),
        };
        state.pane_at_mut(pane).scanned = scanned;
    }

    let mut entries: Vec<Entry> = state.pane_at(pane).scanned.clone();
    {
        let filters = &state.pane_at(pane).filters;
        if scope == FilterScope::Folder && !filters.query.is_empty() {
            entries = state.library.filter_entries(entries, &filters.query);
        }
        if !filters.kinds.is_empty() {
            entries.retain(|e| filters.kinds.contains(&e.kind));
        }
        let needle = filters.search.trim().to_lowercase();
        if !needle.is_empty() {
            entries.retain(|e| e.name.to_lowercase().contains(&needle));
        }
    }
    sort_entries(
        &mut entries,
        state.settings.sort_key,
        state.settings.sort_ascending,
    );
    let p = state.pane_at_mut(pane);
    p.entries = entries;

    // reconcile selection / anchor / focus with what is visible
    let visible: HashSet<&PathBuf> = p.entries.iter().map(|e| &e.path).collect();
    p.selection.items.retain(|q| visible.contains(q));
    if p.selection
        .anchor
        .as_ref()
        .is_some_and(|q| !visible.contains(q))
    {
        p.selection.anchor = None;
    }
    if p.selection
        .focused
        .as_ref()
        .is_some_and(|q| !visible.contains(q))
    {
        p.selection.focused = None;
    }
    drop(visible);

    if let Some(sel) = p.pending_select.take()
        && p.entries.iter().any(|e| e.path == sel)
    {
        p.selection.items = BTreeSet::from([sel.clone()]);
        p.selection.anchor = Some(sel.clone());
        p.selection.focused = Some(sel);
    }

    // `begin_batch` drops queued thumbnail work for every pane, so each pane re-requests its cards.
    state.thumbs.begin_batch();
    for pane_state in &mut state.panes {
        pane_state.view.reset_requests();
    }
    {
        let mut keep: HashSet<&Path> = HashSet::new();
        for pane_state in &state.panes {
            keep.extend(pane_state.entries.iter().map(|e| e.path.as_path()));
        }
        state.textures.retain(&|p: &Path| keep.contains(p));
    }
    state.poll_until = Instant::now() + Duration::from_secs(5);

    if let Some(p) = state.pending_open.take() {
        open_fullscreen(state, p);
    }
    sync_focus(state);
}

/// Refresh the active pane.
fn refresh_active(state: &mut AppState, rescan: bool) {
    let pane = state.active;
    refresh_view(state, pane, rescan);
}

/// Refresh every shown pane (settings that affect all panes changed).
pub fn refresh_all(state: &mut AppState, rescan: bool) {
    for i in 0..state.visible_panes() {
        refresh_view(state, i, rescan);
    }
}

fn scan_options(state: &AppState) -> ScanOptions {
    ScanOptions {
        show_hidden: state.settings.show_hidden,
        collapse_sequences: state.settings.collapse_sequences,
        show_all_files: state.settings.show_all_files,
        ..ScanOptions::default()
    }
}

fn scan_current(state: &mut AppState, pane: usize) -> Vec<Entry> {
    let Some(dir) = state.pane_at(pane).nav.current.clone() else {
        return Vec::new();
    };
    match scan_folder(&dir, &scan_options(state)) {
        Ok(v) => v,
        Err(e) => {
            state.error(format!("Cannot read folder: {e}"));
            Vec::new()
        }
    }
}

/// Library scope: catalog paths matching the query that still exist, as browsable entries.
/// Sequence identities (`shot_####.exr`) are resolved by scanning their folder once.
fn library_entries(state: &mut AppState, pane: usize) -> Vec<Entry> {
    let paths = state.library.query(&state.pane_at(pane).filters.query);
    let opts = scan_options(state);
    let mut folders: HashMap<PathBuf, Vec<Entry>> = HashMap::new();
    let mut out = Vec::with_capacity(paths.len());
    let mut skipped = 0usize;
    for p in paths {
        let is_pattern = p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains('#'));
        if is_pattern {
            let Some(parent) = p.parent() else { continue };
            if !folders.contains_key(parent) {
                let scanned = scan_folder(
                    parent,
                    &ScanOptions {
                        collapse_sequences: true,
                        ..opts.clone()
                    },
                )
                .unwrap_or_default();
                folders.insert(parent.to_path_buf(), scanned);
            }
            if let Some(e) = folders
                .get(parent)
                .and_then(|v| v.iter().find(|e| e.path == p))
            {
                out.push(e.clone());
            } else {
                skipped += 1;
            }
            continue;
        }
        let Some(kind) = classify(&p) else {
            skipped += 1;
            continue;
        };
        match std::fs::metadata(&p) {
            Ok(m) if m.is_file() => out.push(Entry {
                name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: p,
                kind,
                size: m.len(),
                modified: m.modified().ok(),
                sequence: None,
            }),
            _ => skipped += 1,
        }
    }
    if skipped > 0 {
        state.pane_at_mut(pane).scan_note = Some(format!(
            "{skipped} catalog item(s) are missing or unsupported"
        ));
    }
    out
}

/// Push the focused entry to the preview pane and metadata worker when it changed.
fn sync_focus(state: &mut AppState) {
    let focused = state.pane().selection.focused.clone();

    if state.synced_meta != focused {
        state.synced_meta = focused.clone();
        state.current_metadata = None;
        if let Some(e) = focused.as_ref().and_then(|p| state.entry(p)).cloned() {
            state.metadata.request(&e);
        }
    }

    // the fullscreen viewer owns playback while it is open
    let desired = if state.fullscreen.is_some() {
        None
    } else {
        focused
    };
    if state.synced_preview != desired {
        state.synced_preview = desired.clone();
        let entry = desired.as_ref().and_then(|p| state.entry(p)).cloned();
        state.preview.set_entry(entry.as_ref(), &state.settings);
    }
}

// ======================================================================================
// navigation
// ======================================================================================

fn navigate(state: &mut AppState, p: PathBuf) {
    let pane = state.active;
    navigate_pane(state, pane, p);
}

/// Open `p` (folder, or a file's parent with the file selected) in pane `pane`.
fn navigate_pane(state: &mut AppState, pane: usize, p: PathBuf) {
    if p.is_file() {
        if let Some(parent) = p.parent().map(Path::to_path_buf) {
            state.pane_at_mut(pane).pending_select = Some(p);
            navigate_pane(state, pane, parent);
        }
        return;
    }
    if !p.is_dir() {
        state.error(format!("Not a folder: {}", p.display()));
        return;
    }
    if state.pane_at(pane).nav.current.as_deref() == Some(p.as_path()) {
        refresh_view(state, pane, true);
        return;
    }
    let nav = &mut state.pane_at_mut(pane).nav;
    if let Some(cur) = nav.current.clone() {
        nav.back.push(cur);
    }
    nav.forward.clear();
    set_current(state, pane, p, true);
}

/// Make `p` the current folder (history already adjusted by the caller).
fn set_current(state: &mut AppState, pane: usize, p: PathBuf, record: bool) {
    if !p.is_dir() {
        state.error(format!("Folder no longer exists: {}", p.display()));
        return;
    }
    let pane_state = state.pane_at_mut(pane);
    pane_state.nav.current = Some(p.clone());
    pane_state.selection.items.clear();
    pane_state.selection.anchor = None;
    pane_state.selection.focused = None;
    if record {
        state.settings.add_recent(p.clone());
    }
    if pane == 0 {
        state.settings.last_folder = Some(p);
    } else {
        state.settings.last_folder_secondary = Some(p);
    }
    refresh_view(state, pane, true);
}

fn open_folder_dialog(state: &mut AppState, ctx: &egui::Context) {
    if state.folder_dialog.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let start = state.pane().nav.current.clone();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new()
        .name("folder-dialog".into())
        .spawn(move || {
            let mut dlg = rfd::FileDialog::new().set_title("Open Folder");
            if let Some(s) = start {
                dlg = dlg.set_directory(s);
            }
            // a send error only means the app is gone
            let _ = tx.send(dlg.pick_folder());
            ctx.request_repaint();
        });
    match spawned {
        Ok(_) => state.folder_dialog = Some(rx),
        Err(e) => state.error(format!("Could not open the folder dialog: {e}")),
    }
}

fn open_paths(state: &mut AppState, ctx: &egui::Context, paths: Vec<PathBuf>) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    for p in paths {
        if p.is_dir() {
            navigate(state, p);
            return;
        }
        if p.is_file()
            && let Some(parent) = p.parent().map(Path::to_path_buf)
        {
            let media = classify(&p).is_some();
            state.pane_mut().pending_select = Some(p.clone());
            if media {
                state.pending_open = Some(p);
            }
            navigate(state, parent);
            return;
        }
        state.error(format!(
            "Cannot open {}: no such file or folder",
            p.display()
        ));
    }
}

// ======================================================================================
// selection
// ======================================================================================

fn index_of(state: &AppState, p: &Path) -> Option<usize> {
    state.pane().entries.iter().position(|e| e.path == p)
}

fn select(state: &mut AppState, path: PathBuf, mode: SelectMode) {
    let Some(ti) = index_of(state, &path) else {
        return;
    };
    match mode {
        SelectMode::Replace => {
            let sel = &mut state.pane_mut().selection;
            sel.items = BTreeSet::from([path.clone()]);
            sel.anchor = Some(path.clone());
        }
        SelectMode::Toggle => {
            let sel = &mut state.pane_mut().selection;
            if !sel.items.remove(&path) {
                sel.items.insert(path.clone());
            }
            sel.anchor = Some(path.clone());
        }
        SelectMode::Range => {
            let ai = state
                .pane()
                .selection
                .anchor
                .as_deref()
                .and_then(|a| index_of(state, a))
                .or_else(|| {
                    state
                        .pane()
                        .selection
                        .focused
                        .as_deref()
                        .and_then(|a| index_of(state, a))
                })
                .unwrap_or(ti);
            let p = state.pane_mut();
            if p.selection.anchor.is_none() {
                p.selection.anchor = p.entries.get(ai).map(|e| e.path.clone());
            }
            let (lo, hi) = (ai.min(ti), ai.max(ti));
            p.selection.items = p
                .entries
                .iter()
                .skip(lo)
                .take(hi - lo + 1)
                .map(|e| e.path.clone())
                .collect();
        }
    }
    state.pane_mut().selection.focused = Some(path);
}

fn move_focus(state: &mut AppState, delta: isize, extend: bool) {
    let len = state.pane().entries.len();
    if len == 0 {
        return;
    }
    let cur = state
        .pane()
        .selection
        .focused
        .as_deref()
        .and_then(|p| index_of(state, p));
    let target = match cur {
        Some(i) => (i as isize)
            .saturating_add(delta)
            .clamp(0, len as isize - 1) as usize,
        None if delta >= 0 => 0,
        None => len - 1,
    };
    let Some(path) = state.pane().entries.get(target).map(|e| e.path.clone()) else {
        return;
    };
    let mode = if extend {
        SelectMode::Range
    } else {
        SelectMode::Replace
    };
    let sel = &mut state.pane_mut().selection;
    if extend && sel.anchor.is_none() {
        sel.anchor = sel.focused.clone();
    }
    select(state, path, mode);
}

fn activate(state: &mut AppState, p: PathBuf) {
    if p.is_dir() {
        navigate(state, p);
        return;
    }
    let Some(kind) = state.entry(&p).map(|e| e.kind) else {
        return;
    };
    select(state, p.clone(), SelectMode::Replace);
    match kind {
        MediaKind::Audio | MediaKind::Other => {
            state.pending.push(Action::OpenWithDefaultApp(p));
        }
        _ => open_fullscreen(state, p),
    }
}

/// Entries an operation applies to: the active pane's selection, else its focused entry.
fn target_entries(state: &AppState) -> Vec<Entry> {
    let sel: Vec<Entry> = state.selected_entries().into_iter().cloned().collect();
    if !sel.is_empty() {
        return sel;
    }
    state
        .pane()
        .selection
        .focused
        .as_deref()
        .and_then(|p| state.entry(p))
        .cloned()
        .into_iter()
        .collect()
}

fn target_members(state: &AppState) -> Vec<PathBuf> {
    target_entries(state)
        .iter()
        .flat_map(Entry::member_paths)
        .collect()
}

/// Library identities (`Entry::path`) of the targets.
fn target_ids(state: &AppState) -> Vec<PathBuf> {
    target_entries(state).into_iter().map(|e| e.path).collect()
}

// ======================================================================================
// file operations
// ======================================================================================

fn submit(state: &mut AppState, op: FileOp) {
    state.fileops.submit(op);
    state.progress = Some((0, 0));
    state.poll_until = Instant::now() + Duration::from_secs(3);
}

fn clipboard_set(state: &mut AppState, mode: ClipMode) {
    let paths = target_members(state);
    if paths.is_empty() {
        state.info("Nothing selected");
        return;
    }
    let n = target_entries(state).len();
    state.clipboard = Some(Clipboard { mode, paths });
    let verb = if mode == ClipMode::Copy {
        "Copied"
    } else {
        "Cut"
    };
    state.info(format!("{verb} {n} item{}", if n == 1 { "" } else { "s" }));
}

fn paste(state: &mut AppState, dest: Option<PathBuf>) {
    let Some(clip) = state.clipboard.clone() else {
        state.info("Clipboard is empty");
        return;
    };
    let Some(dest) = dest.or_else(|| state.pane().nav.current.clone()) else {
        state.error("Open a folder to paste into");
        return;
    };
    transfer(state, clip.paths, dest, clip.mode == ClipMode::Copy);
    if clip.mode == ClipMode::Cut {
        state.clipboard = None;
    }
}

fn drop_paths(state: &mut AppState, paths: Vec<PathBuf>, dest: PathBuf, copy: bool) {
    transfer(state, paths, dest, copy);
}

/// Destination folder for a copy/move to the other pane: its current folder, only in dual mode and
/// only when it differs from the source pane's folder.
pub fn other_pane_target(
    dual: bool,
    source: Option<&Path>,
    other: Option<&Path>,
) -> Result<PathBuf, &'static str> {
    if !dual {
        return Err("Turn on the second pane first (F3)");
    }
    let Some(other) = other else {
        return Err("The other pane has no folder open");
    };
    if source == Some(other) {
        return Err("Both panes show the same folder");
    }
    Ok(other.to_path_buf())
}

fn transfer_to_other_pane(state: &mut AppState, copy: bool) {
    let target = other_pane_target(
        state.settings.dual_pane,
        state.pane().nav.current.as_deref(),
        state.pane_at(state.other_index()).nav.current.as_deref(),
    );
    let dest = match target {
        Ok(d) => d,
        Err(msg) => {
            state.info(msg);
            return;
        }
    };
    let paths = target_members(state);
    if paths.is_empty() {
        state.info("Nothing selected");
        return;
    }
    transfer(state, paths, dest, copy);
}

fn toggle_dual_pane(state: &mut AppState) {
    let on = !state.settings.dual_pane;
    state.settings.dual_pane = on;
    if on {
        let start = state
            .settings
            .last_folder_secondary
            .clone()
            .filter(|p| p.is_dir())
            .or_else(|| state.panes[0].nav.current.clone());
        if state.panes[1].nav.current.is_none()
            && let Some(p) = start
        {
            set_current(state, 1, p, false);
        } else {
            refresh_view(state, 1, true);
        }
    } else {
        state.panes[1].scan_note = None;
        state.active = 0;
        // pane 1's last folder is remembered for the next time the pane is opened
        state.settings.last_folder_secondary = state.panes[1].nav.current.clone();
        // panes share the thumbnail queue; make pane 0 re-request its cards
        state.panes[0].view.reset_requests();
    }
    save_settings(state);
}

/// Queue a copy/move of `sources` into `dest`, skipping no-ops (move into own folder, folder into itself).
fn transfer(state: &mut AppState, sources: Vec<PathBuf>, dest: PathBuf, copy: bool) {
    if !dest.is_dir() {
        state.error(format!("Not a folder: {}", dest.display()));
        return;
    }
    let sources: Vec<PathBuf> = sources
        .into_iter()
        .filter(|s| !dest.starts_with(s) || !s.is_dir())
        .filter(|s| copy || s.parent() != Some(dest.as_path()))
        .collect();
    if sources.is_empty() {
        state.info("Nothing to do");
        return;
    }
    let conflict = ConflictPolicy::KeepBoth;
    let op = if copy {
        FileOp::Copy {
            sources,
            dest_dir: dest,
            conflict,
        }
    } else {
        FileOp::Move {
            sources,
            dest_dir: dest,
            conflict,
        }
    };
    submit(state, op);
}

fn valid_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    if name == "." || name == ".." {
        return Err("Invalid name".into());
    }
    if name.contains('/') || name.contains('\0') {
        return Err("Name cannot contain “/”".into());
    }
    if name.len() > 255 {
        return Err("Name is too long".into());
    }
    Ok(name.to_string())
}

fn begin_rename(state: &mut AppState) {
    let targets = target_entries(state);
    match targets.as_slice() {
        [] => state.info("Nothing selected"),
        [e] if e.sequence.is_some() => begin_batch_rename(state),
        [e] => state.dialogs.rename = Some((e.path.clone(), e.name.clone())),
        _ => state.error("Select exactly one item to rename (or use Batch Rename)"),
    }
}

fn rename(state: &mut AppState, from: PathBuf, new_name: &str) {
    let name = match valid_name(new_name) {
        Ok(n) => n,
        Err(e) => {
            state.error(e);
            return;
        }
    };
    let Some(parent) = from.parent() else {
        state.error("Cannot rename this item");
        return;
    };
    let to = parent.join(&name);
    if to == from {
        return;
    }
    if to.exists() {
        state.error(format!("“{name}” already exists"));
        return;
    }
    submit(state, FileOp::Rename { from, to });
}

fn begin_batch_rename(state: &mut AppState) {
    let files = target_members(state);
    if files.len() < 2 {
        state.error("Select at least two items for batch rename");
        return;
    }
    state.dialogs.batch_rename = Some(BatchRenameDialog::new(files));
}

fn new_folder(state: &mut AppState, name: &str) {
    let name = match valid_name(name) {
        Ok(n) => n,
        Err(e) => {
            state.error(e);
            return;
        }
    };
    let Some(parent) = state.pane().nav.current.clone() else {
        state.error("Open a folder first");
        return;
    };
    submit(state, FileOp::NewFolder { parent, name });
}

fn bump_library(state: &mut AppState) {
    state.library_generation = state.library_generation.wrapping_add(1);
}

/// Run a library mutation over `ids`; refresh when a library filter could change the result.
fn annotate(
    state: &mut AppState,
    ids: &[PathBuf],
    what: &str,
    f: impl FnOnce(
        &mut gallerybrowser_library::Library,
        &[PathBuf],
    ) -> gallerybrowser_library::Result<()>,
) {
    if ids.is_empty() {
        state.info("Nothing selected");
        return;
    }
    match f(&mut state.library, ids) {
        Ok(()) => state.info(format!(
            "{what} updated for {} item{}",
            ids.len(),
            if ids.len() == 1 { "" } else { "s" }
        )),
        Err(e) => state.error(format!("{what} update failed: {e}")),
    }
    bump_library(state);
    for i in 0..state.visible_panes() {
        if !state.pane_at(i).filters.query.is_empty() {
            refresh_view(state, i, true);
        }
    }
}

// ======================================================================================
// settings, fullscreen
// ======================================================================================

fn apply_settings(state: &mut AppState, ctx: &egui::Context, d: Settings) {
    let s = &mut state.settings;
    let prev_hidden = s.show_hidden;
    let prev_collapse = s.collapse_sequences;
    // merge the dialog-owned fields; favorites, recents, volume, view state stay untouched
    s.thumbnail_size = match d.thumbnail_size {
        0..=95 => 64,
        96..=191 => 128,
        192..=383 => 256,
        _ => 512,
    };
    s.show_hidden = d.show_hidden;
    s.video_preview = d.video_preview;
    s.thumbnail_threads = d.thumbnail_threads.clamp(1, 16);
    s.max_cache_mb = d.max_cache_mb.clamp(100, 100_000);
    s.ram_preview_mb = d.ram_preview_mb.clamp(256, 65_536);
    s.collapse_sequences = d.collapse_sequences;
    s.sequence_fps = if d.sequence_fps.is_finite() {
        d.sequence_fps.clamp(1.0, 240.0)
    } else {
        24.0
    };
    s.theme = d.theme;
    s.recents_max = d.recents_max.min(100);
    let max = s.recents_max;
    s.recents.truncate(max);

    save_settings(state);
    state
        .thumbs
        .reconfigure(thumb_config(&state.dirs, &state.settings));
    state
        .preview
        .set_cache_bytes(state.settings.ram_preview_mb.saturating_mul(MIB));
    theme::apply(ctx, state.settings.theme);
    state.tree_generation = state.tree_generation.wrapping_add(1);
    // thumbnail size / hidden files / sequence collapsing change what is displayed
    if prev_hidden != state.settings.show_hidden
        || prev_collapse != state.settings.collapse_sequences
    {
        refresh_all(state, true);
    } else {
        refresh_all(state, false);
    }
    // re-point the preview so a changed autoplay / fps setting takes effect
    let entry = state
        .synced_preview
        .as_deref()
        .and_then(|p| state.entry(p))
        .cloned();
    state.preview.set_entry(None, &state.settings);
    state.preview.set_entry(entry.as_ref(), &state.settings);
    state.info("Settings saved");
}

/// Persist settings; report (never panic) on failure.
pub fn save_settings(state: &mut AppState) {
    if let Err(e) = state.settings.save(&state.dirs) {
        state.error(format!("Could not save settings: {e}"));
    }
}

fn open_fullscreen(state: &mut AppState, p: PathBuf) {
    match state.entry(&p).map(|e| e.kind) {
        Some(MediaKind::Audio | MediaKind::Other) => {
            state.pending.push(Action::OpenWithDefaultApp(p));
        }
        Some(_) => {
            let sel = &mut state.pane_mut().selection;
            sel.items = BTreeSet::from([p.clone()]);
            sel.anchor = Some(p.clone());
            sel.focused = Some(p.clone());
            state.fullscreen = Some(FullscreenState::new(p));
        }
        None => state.error(format!(
            "Cannot open {}: not in the current view",
            p.display()
        )),
    }
}

fn fullscreen_step(state: &mut AppState, delta: isize) {
    let Some(cur) = state.fullscreen.as_ref().map(|f| f.path.clone()) else {
        return;
    };
    let viewable: Vec<&Entry> = state
        .pane()
        .entries
        .iter()
        .filter(|e| !matches!(e.kind, MediaKind::Audio | MediaKind::Other))
        .collect();
    if viewable.is_empty() {
        return;
    }
    let i = viewable.iter().position(|e| e.path == cur);
    let next = match i {
        Some(i) => (i as isize)
            .saturating_add(delta)
            .clamp(0, viewable.len() as isize - 1) as usize,
        None => 0,
    };
    let Some(path) = viewable.get(next).map(|e| e.path.clone()) else {
        return;
    };
    drop(viewable);
    if path == cur {
        return;
    }
    let sel = &mut state.pane_mut().selection;
    sel.items = BTreeSet::from([path.clone()]);
    sel.anchor = Some(path.clone());
    sel.focused = Some(path.clone());
    if let Some(f) = state.fullscreen.as_mut() {
        f.path = path;
    }
}

// ======================================================================================
// workers
// ======================================================================================

/// Drain fileops / thumbnail / metadata worker events once per frame (relink library on moves, etc.).
pub fn poll_workers(state: &mut AppState, ctx: &egui::Context) {
    // native folder dialog answer
    if let Some(rx) = &state.folder_dialog {
        match rx.try_recv() {
            Ok(Some(p)) => {
                state.folder_dialog = None;
                state.pending.push(Action::Navigate(p));
            }
            Ok(None) | Err(mpsc::TryRecvError::Disconnected) => state.folder_dialog = None,
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
        }
    }

    for ev in state.fileops.poll() {
        match ev {
            OpEvent::Progress { done, total, .. } => state.progress = Some((done, total)),
            OpEvent::Finished(outcome) => {
                apply_outcome(state, outcome);
            }
        }
    }
    if state.fileops.busy() {
        if state.progress.is_none() {
            state.progress = Some((0, 0));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    } else {
        state.progress = None;
    }

    if let Some(m) = state.metadata.poll()
        && state.pane().selection.focused.as_deref() == Some(m.path.as_path())
    {
        state.current_metadata = Some(m);
        ctx.request_repaint();
    }

    let results = state.thumbs.poll();
    if !results.is_empty() {
        // Hover-scrub frames get their own texture key and do not extend the thumbnail repaint window.
        if results.iter().any(|r| r.scrub.is_none()) {
            state.poll_until = Instant::now() + Duration::from_secs(1);
        }
        for r in results {
            let edge = match r.scrub {
                Some(s) => crate::textures::scrub_edge(r.max_edge, s.slot, s.slots),
                None => r.max_edge,
            };
            match r.image {
                Ok(img) => {
                    let _texture = state.textures.insert(ctx, r.path, edge, &img);
                }
                Err(e) => log::debug!("thumbnail failed for {}: {e}", r.path.display()),
            }
        }
        ctx.request_repaint();
    }
    if Instant::now() < state.poll_until {
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// Apply a finished file operation: library relink/forget, status text, view refresh.
fn apply_outcome(state: &mut AppState, o: OpOutcome) {
    let mut moved = o.moved.clone();
    moved.extend(sequence_pattern_moves(&o.moved));
    if !moved.is_empty()
        && let Err(e) = state.library.relink(&moved)
    {
        state.error(format!("Could not update the library after a move: {e}"));
    }
    if !o.removed.is_empty()
        && let Err(e) = state.library.forget(&o.removed)
    {
        state.error(format!("Could not update the library after a removal: {e}"));
    }
    bump_library(state);

    if let Some(first) = o.errors.first() {
        let more = o.errors.len().saturating_sub(1);
        let tail = if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        };
        state.error(format!("{}: {first}{tail}", o.label));
    } else {
        state.info(o.label.clone());
    }

    // a pane's folder may have been moved/renamed/removed
    for i in 0..state.panes.len() {
        recover_missing_folder(state, i, &o);
    }

    let folders = affected_folders(&o);
    for i in 0..state.visible_panes() {
        let p = state.pane_at_mut(i);
        let cur = p.nav.current.as_deref();
        let in_cur = |q: &PathBuf| q.parent() == cur;
        p.pending_select = match (o.created.as_slice(), o.moved.as_slice()) {
            ([c], _) if in_cur(c) => Some(c.clone()),
            ([], [(_, n)]) if in_cur(n) => Some(n.clone()),
            _ => None,
        };
    }
    state.tree_generation = state.tree_generation.wrapping_add(1);
    for i in 0..state.visible_panes() {
        let p = state.pane_at(i);
        let library = p.filters.scope == FilterScope::Library;
        if pane_affected(p.nav.current.as_deref(), library, &folders) {
            refresh_view(state, i, true);
        } else {
            state.pane_at_mut(i).pending_select = None;
        }
    }
}

/// Folders whose listing a finished operation may have changed: the parent of every path it moved
/// (old and new), created or removed.
pub fn affected_folders(o: &OpOutcome) -> BTreeSet<PathBuf> {
    let moved = o.moved.iter().flat_map(|(a, b)| [a, b]);
    moved
        .chain(o.created.iter())
        .chain(o.removed.iter())
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect()
}

/// Whether a pane showing `current` must be rescanned after an operation touching `folders`.
/// Library-scope panes always are (any member may have moved); so is a pane whose folder vanished.
pub fn pane_affected(current: Option<&Path>, library: bool, folders: &BTreeSet<PathBuf>) -> bool {
    library || current.is_some_and(|c| folders.contains(c) || !c.is_dir())
}

/// If pane `i`'s folder no longer exists, follow a move or fall back to the nearest existing ancestor.
fn recover_missing_folder(state: &mut AppState, i: usize, o: &OpOutcome) {
    let Some(cur) = state.pane_at(i).nav.current.clone() else {
        return;
    };
    if cur.is_dir() {
        return;
    }
    let remapped = o
        .moved
        .iter()
        .find_map(|(old, new)| cur.strip_prefix(old).ok().map(|rest| new.join(rest)))
        .filter(|p| p.is_dir());
    let target = remapped.or_else(|| {
        cur.ancestors()
            .skip(1)
            .find(|a| a.is_dir())
            .map(Path::to_path_buf)
    });
    let nav = &mut state.pane_at_mut(i).nav;
    nav.back.retain(|p| p.is_dir());
    nav.forward.retain(|p| p.is_dir());
    match target {
        Some(t) => set_current(state, i, t, false),
        None => state.pane_at_mut(i).nav.current = None,
    }
}

/// Library identities of sequences are pattern paths (`shot_####.exr`); derive them from frame moves so
/// annotations follow renamed/moved sequences. Both padding interpretations are emitted; pairs naming a
/// path the catalog does not know are harmless.
fn sequence_pattern_moves(moved: &[(PathBuf, PathBuf)]) -> Vec<(PathBuf, PathBuf)> {
    fn patterns(p: &Path) -> Vec<PathBuf> {
        let (Some(dir), Some(name)) = (
            p.parent(),
            p.file_name().map(|n| n.to_string_lossy().into_owned()),
        ) else {
            return Vec::new();
        };
        let Some((prefix, digits, suffix)) = split_numbered(&name) else {
            return Vec::new();
        };
        let mut widths = vec![digits.len().max(1)];
        if !digits.starts_with('0') {
            widths.push(1);
        }
        widths.dedup();
        widths
            .into_iter()
            .map(|w| dir.join(format!("{prefix}{}{suffix}", "#".repeat(w))))
            .collect()
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (old, new) in moved {
        let (po, pn) = (patterns(old), patterns(new));
        for (a, b) in po.into_iter().zip(pn) {
            if a != b && seen.insert((a.clone(), b.clone())) {
                out.push((a, b));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated() {
        assert!(valid_name("  ").is_err());
        assert!(valid_name("a/b").is_err());
        assert!(valid_name("..").is_err());
        assert_eq!(valid_name(" clip.mov ").as_deref(), Ok("clip.mov"));
    }

    #[test]
    fn sequence_moves_derive_patterns() {
        let moved = vec![
            (
                PathBuf::from("/d/a_0001.exr"),
                PathBuf::from("/d/b_0001.exr"),
            ),
            (
                PathBuf::from("/d/a_0002.exr"),
                PathBuf::from("/d/b_0002.exr"),
            ),
        ];
        let p = sequence_pattern_moves(&moved);
        assert_eq!(
            p,
            vec![(
                PathBuf::from("/d/a_####.exr"),
                PathBuf::from("/d/b_####.exr")
            )]
        );
    }
}

#[cfg(test)]
mod pane_tests {
    use super::*;

    #[test]
    fn other_pane_target_resolution() {
        let a = Path::new("/a");
        let b = Path::new("/b");
        assert_eq!(
            other_pane_target(true, Some(a), Some(b)),
            Ok(PathBuf::from("/b"))
        );
        assert!(other_pane_target(false, Some(a), Some(b)).is_err());
        assert!(other_pane_target(true, Some(a), None).is_err());
        assert!(other_pane_target(true, Some(a), Some(a)).is_err());
        assert_eq!(
            other_pane_target(true, None, Some(b)),
            Ok(PathBuf::from("/b"))
        );
    }

    #[test]
    fn outcome_affects_source_and_destination_folders() {
        let o = OpOutcome {
            moved: vec![(PathBuf::from("/src/a.png"), PathBuf::from("/dst/a.png"))],
            created: vec![PathBuf::from("/dst/b.png")],
            removed: vec![PathBuf::from("/gone/c.png")],
            ..OpOutcome::default()
        };
        let f = affected_folders(&o);
        assert_eq!(
            f,
            BTreeSet::from([
                PathBuf::from("/src"),
                PathBuf::from("/dst"),
                PathBuf::from("/gone")
            ])
        );
        assert!(pane_affected(Some(Path::new("/dst")), false, &f));
        assert!(pane_affected(Some(Path::new("/src")), false, &f));
        // missing folder or library scope always refresh; unrelated existing folder does not
        assert!(pane_affected(
            Some(Path::new("/definitely/missing")),
            false,
            &f
        ));
        assert!(pane_affected(None, true, &f));
        assert!(!pane_affected(Some(Path::new("/")), false, &f));
        assert!(!pane_affected(None, false, &f));
    }
}
