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
            if let Some(prev) = state.nav.back.pop() {
                if let Some(cur) = state.nav.current.clone() {
                    state.nav.forward.push(cur);
                }
                set_current(state, prev, false);
            }
        }
        Action::Forward => {
            if let Some(next) = state.nav.forward.pop() {
                if let Some(cur) = state.nav.current.clone() {
                    state.nav.back.push(cur);
                }
                set_current(state, next, false);
            }
        }
        Action::Up => {
            let parent = state
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
            refresh_view(state, true);
        }

        // ---- selection ----
        Action::Select { path, mode } => select(state, path, mode),
        Action::SelectAll => {
            state.selection.items = state.entries.iter().map(|e| e.path.clone()).collect();
            if state.selection.focused.is_none() {
                state.selection.focused = state.entries.first().map(|e| e.path.clone());
            }
            if state.selection.anchor.is_none() {
                state.selection.anchor = state.selection.focused.clone();
            }
        }
        Action::ClearSelection => {
            state.selection.items.clear();
            state.selection.anchor = None;
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
            if state.nav.current.is_some() && state.filters.scope == FilterScope::Folder {
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
            let dir = if p.is_dir() {
                Some(p.as_path())
            } else {
                p.parent()
            };
            match dir {
                Some(d) => {
                    if let Err(e) = open::that_detached(d) {
                        state.error(format!(
                            "Could not open file manager for {}: {e}",
                            d.display()
                        ));
                    }
                }
                None => state.error(format!("No containing folder for {}", p.display())),
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
            if state.filters.query.tags.contains(&name) {
                state.filters.query.tags.retain(|t| *t != name);
                refresh_view(state, true);
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
            if state.filters.query.collection.as_deref() == Some(name.as_str()) {
                state.filters.query.collection = None;
                refresh_view(state, true);
            }
        }

        // ---- view / filters ----
        Action::SetScope(scope) => {
            if state.filters.scope != scope {
                state.filters.scope = scope;
                refresh_view(state, true);
            }
        }
        Action::SetQuery(q) => {
            state.filters.query = q;
            refresh_view(state, true);
        }
        Action::SetSearch(s) => {
            state.filters.search = s;
            refresh_view(state, false);
        }
        Action::SetKinds(k) => {
            state.filters.kinds = k;
            refresh_view(state, false);
        }
        Action::SetSort { key, ascending } => {
            state.settings.sort_key = key;
            state.settings.sort_ascending = ascending;
            refresh_view(state, false);
        }
        Action::SetViewMode(m) => state.settings.view_mode = m,
        Action::SetThumbnailSize(s) => {
            state.settings.thumbnail_size = s.clamp(64, 512);
            refresh_view(state, false);
        }
        Action::SetCollapseSequences(b) => {
            // Goes through the one pipeline so active library filters still apply (Python bug #3).
            state.settings.collapse_sequences = b;
            refresh_view(state, true);
        }
        Action::TogglePreviewPane => state.settings.show_preview = !state.settings.show_preview,
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

// ======================================================================================
// refresh pipeline
// ======================================================================================

/// Rescan (if `rescan`) and rebuild `state.entries`; reconcile selection; request thumbnails.
pub fn refresh_view(state: &mut AppState, rescan: bool) {
    if rescan {
        state.scan_note = None;
        state.scanned = match state.filters.scope {
            FilterScope::Folder => scan_current(state),
            FilterScope::Library => library_entries(state),
        };
    }

    let mut entries: Vec<Entry> = state.scanned.clone();
    if state.filters.scope == FilterScope::Folder && !state.filters.query.is_empty() {
        entries = state.library.filter_entries(entries, &state.filters.query);
    }
    if !state.filters.kinds.is_empty() {
        entries.retain(|e| state.filters.kinds.contains(&e.kind));
    }
    let needle = state.filters.search.trim().to_lowercase();
    if !needle.is_empty() {
        entries.retain(|e| e.name.to_lowercase().contains(&needle));
    }
    sort_entries(
        &mut entries,
        state.settings.sort_key,
        state.settings.sort_ascending,
    );
    state.entries = entries;

    // reconcile selection / anchor / focus with what is visible
    let visible: HashSet<&PathBuf> = state.entries.iter().map(|e| &e.path).collect();
    state.selection.items.retain(|p| visible.contains(p));
    if state
        .selection
        .anchor
        .as_ref()
        .is_some_and(|p| !visible.contains(p))
    {
        state.selection.anchor = None;
    }
    if state
        .selection
        .focused
        .as_ref()
        .is_some_and(|p| !visible.contains(p))
    {
        state.selection.focused = None;
    }
    drop(visible);

    if let Some(p) = state.pending_select.take()
        && state.entries.iter().any(|e| e.path == p)
    {
        state.selection.items = BTreeSet::from([p.clone()]);
        state.selection.anchor = Some(p.clone());
        state.selection.focused = Some(p);
    }

    state.thumbs.begin_batch();
    {
        let keep: HashSet<&Path> = state.entries.iter().map(|e| e.path.as_path()).collect();
        state.textures.retain(&|p: &Path| keep.contains(p));
    }
    state.poll_until = Instant::now() + Duration::from_secs(5);

    if let Some(p) = state.pending_open.take() {
        open_fullscreen(state, p);
    }
    sync_focus(state);
}

fn scan_options(state: &AppState) -> ScanOptions {
    ScanOptions {
        show_hidden: state.settings.show_hidden,
        collapse_sequences: state.settings.collapse_sequences,
        ..ScanOptions::default()
    }
}

fn scan_current(state: &mut AppState) -> Vec<Entry> {
    let Some(dir) = state.nav.current.clone() else {
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
fn library_entries(state: &mut AppState) -> Vec<Entry> {
    let paths = state.library.query(&state.filters.query);
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
        state.scan_note = Some(format!(
            "{skipped} catalog item(s) are missing or unsupported"
        ));
    }
    out
}

/// Push the focused entry to the preview pane and metadata worker when it changed.
fn sync_focus(state: &mut AppState) {
    let focused = state.selection.focused.clone();

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
    if p.is_file() {
        if let Some(parent) = p.parent().map(Path::to_path_buf) {
            state.pending_select = Some(p);
            navigate(state, parent);
        }
        return;
    }
    if !p.is_dir() {
        state.error(format!("Not a folder: {}", p.display()));
        return;
    }
    if state.nav.current.as_deref() == Some(p.as_path()) {
        refresh_view(state, true);
        return;
    }
    if let Some(cur) = state.nav.current.clone() {
        state.nav.back.push(cur);
    }
    state.nav.forward.clear();
    set_current(state, p, true);
}

/// Make `p` the current folder (history already adjusted by the caller).
fn set_current(state: &mut AppState, p: PathBuf, record: bool) {
    if !p.is_dir() {
        state.error(format!("Folder no longer exists: {}", p.display()));
        return;
    }
    state.nav.current = Some(p.clone());
    state.selection.items.clear();
    state.selection.anchor = None;
    state.selection.focused = None;
    if record {
        state.settings.add_recent(p.clone());
    }
    state.settings.last_folder = Some(p);
    refresh_view(state, true);
}

fn open_folder_dialog(state: &mut AppState, ctx: &egui::Context) {
    if state.folder_dialog.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let start = state.nav.current.clone();
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
            state.pending_select = Some(p.clone());
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
    state.entries.iter().position(|e| e.path == p)
}

fn select(state: &mut AppState, path: PathBuf, mode: SelectMode) {
    let Some(ti) = index_of(state, &path) else {
        return;
    };
    match mode {
        SelectMode::Replace => {
            state.selection.items = BTreeSet::from([path.clone()]);
            state.selection.anchor = Some(path.clone());
        }
        SelectMode::Toggle => {
            if !state.selection.items.remove(&path) {
                state.selection.items.insert(path.clone());
            }
            state.selection.anchor = Some(path.clone());
        }
        SelectMode::Range => {
            let ai = state
                .selection
                .anchor
                .as_deref()
                .and_then(|a| index_of(state, a))
                .or_else(|| {
                    state
                        .selection
                        .focused
                        .as_deref()
                        .and_then(|a| index_of(state, a))
                })
                .unwrap_or(ti);
            if state.selection.anchor.is_none() {
                state.selection.anchor = state.entries.get(ai).map(|e| e.path.clone());
            }
            let (lo, hi) = (ai.min(ti), ai.max(ti));
            state.selection.items = state
                .entries
                .iter()
                .skip(lo)
                .take(hi - lo + 1)
                .map(|e| e.path.clone())
                .collect();
        }
    }
    state.selection.focused = Some(path);
}

fn move_focus(state: &mut AppState, delta: isize, extend: bool) {
    let len = state.entries.len();
    if len == 0 {
        return;
    }
    let cur = state
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
    let Some(path) = state.entries.get(target).map(|e| e.path.clone()) else {
        return;
    };
    let mode = if extend {
        SelectMode::Range
    } else {
        SelectMode::Replace
    };
    if extend && state.selection.anchor.is_none() {
        state.selection.anchor = state.selection.focused.clone();
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
        MediaKind::Audio => {
            state.pending.push(Action::OpenWithDefaultApp(p));
        }
        _ => open_fullscreen(state, p),
    }
}

/// Entries an operation applies to: the selection, else the focused entry.
fn target_entries(state: &AppState) -> Vec<Entry> {
    let sel: Vec<Entry> = state.selected_entries().into_iter().cloned().collect();
    if !sel.is_empty() {
        return sel;
    }
    state
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
    let Some(dest) = dest.or_else(|| state.nav.current.clone()) else {
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
    let Some(parent) = state.nav.current.clone() else {
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
    if !state.filters.query.is_empty() {
        refresh_view(state, true);
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
        refresh_view(state, true);
    } else {
        refresh_view(state, false);
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
        Some(MediaKind::Audio) => state.pending.push(Action::OpenWithDefaultApp(p)),
        Some(_) => {
            state.selection.items = BTreeSet::from([p.clone()]);
            state.selection.anchor = Some(p.clone());
            state.selection.focused = Some(p.clone());
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
        .entries
        .iter()
        .filter(|e| e.kind != MediaKind::Audio)
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
    state.selection.items = BTreeSet::from([path.clone()]);
    state.selection.anchor = Some(path.clone());
    state.selection.focused = Some(path.clone());
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
        && state.selection.focused.as_deref() == Some(m.path.as_path())
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

    // the current folder may have been moved/renamed/removed
    if let Some(cur) = state.nav.current.clone()
        && !cur.is_dir()
    {
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
        state.nav.back.retain(|p| p.is_dir());
        state.nav.forward.retain(|p| p.is_dir());
        match target {
            Some(t) => set_current(state, t, false),
            None => state.nav.current = None,
        }
    }

    let cur = state.nav.current.as_deref();
    let in_cur = |p: &PathBuf| p.parent() == cur;
    state.pending_select = match (o.created.as_slice(), o.moved.as_slice()) {
        ([c], _) if in_cur(c) => Some(c.clone()),
        ([], [(_, n)]) if in_cur(n) => Some(n.clone()),
        _ => None,
    };
    state.tree_generation = state.tree_generation.wrapping_add(1);
    refresh_view(state, true);
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
