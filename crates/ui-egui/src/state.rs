use std::collections::BTreeSet;
use std::path::PathBuf;

use gallerybrowser_core::{AppDirs, Entry, FilterScope, MediaKind, Settings, SortKey, ViewMode};
use gallerybrowser_fileops::{FileOpsWorker, RenamePlan};
use gallerybrowser_library::{Library, LibraryQuery};
use gallerybrowser_media::{MediaMetadata, MetadataWorker, Thumbnailer};

use crate::batch_rename::BatchRenameDialog;
use crate::file_pane::FilePaneState;
use crate::fullscreen::FullscreenState;
use crate::preview::PreviewState;
use crate::settings_dialog::SettingsDialog;
use crate::textures::TextureCache;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    /// Ctrl-click.
    Toggle,
    /// Shift-click: from `Selection::anchor` to target over the CURRENT `AppState::entries`.
    Range,
}

#[derive(Clone, Debug, Default)]
pub struct Selection {
    /// `Entry::path`s; always a subset of the visible entries (reconciled by refresh_view).
    pub items: BTreeSet<PathBuf>,
    pub anchor: Option<PathBuf>,
    /// Keyboard focus / previewed item.
    pub focused: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipMode {
    Copy,
    Cut,
}

#[derive(Clone, Debug)]
pub struct Clipboard {
    pub mode: ClipMode,
    /// Member paths (sequences expanded).
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct Filters {
    pub search: String,
    /// Empty = all kinds.
    pub kinds: BTreeSet<MediaKind>,
    pub scope: FilterScope,
    pub query: LibraryQuery,
}

#[derive(Clone, Debug, Default)]
pub struct Navigation {
    pub current: Option<PathBuf>,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub kind: StatusKind,
    pub text: String,
}

/// Modal/dialog state. `None` = closed.
#[derive(Default)]
pub struct Dialogs {
    pub settings: Option<SettingsDialog>,
    pub batch_rename: Option<BatchRenameDialog>,
    /// Single rename: (entry path, edited name).
    pub rename: Option<(PathBuf, String)>,
    pub new_folder: Option<String>,
    /// Confirm permanent delete of these member paths.
    pub confirm_delete: Option<Vec<PathBuf>>,
    pub about: bool,
    pub shortcuts: bool,
    /// New tag / new collection name prompts.
    pub new_tag: Option<String>,
    pub new_collection: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Action {
    // navigation
    Navigate(PathBuf),
    OpenFolderDialog,
    Back,
    Forward,
    Up,
    Refresh,
    // selection
    Select {
        path: PathBuf,
        mode: SelectMode,
    },
    SelectAll,
    ClearSelection,
    /// Move focus by delta in the visible list (arrows; grid passes ±columns for up/down), extend = Shift.
    MoveFocus {
        delta: isize,
        extend: bool,
    },
    /// Enter/double-click: folder → navigate; media → fullscreen viewer.
    Activate(PathBuf),
    // file ops (act on selection, sequences expanded via Entry::member_paths)
    Copy,
    Cut,
    /// Paste clipboard into `dest` (None = current folder).
    Paste {
        dest: Option<PathBuf>,
    },
    /// External or internal drop: copy (Ctrl) or move.
    Drop {
        paths: Vec<PathBuf>,
        dest: PathBuf,
        copy: bool,
    },
    Trash,
    RequestDeletePermanently,
    DeletePermanently(Vec<PathBuf>),
    BeginRename,
    Rename {
        from: PathBuf,
        new_name: String,
    },
    BeginBatchRename,
    BatchRename(RenamePlan),
    BeginNewFolder,
    NewFolder(String),
    Undo,
    Redo,
    RevealInFileManager(PathBuf),
    OpenWithDefaultApp(PathBuf),
    CopyPathToClipboard(PathBuf),
    // organization (act on selection)
    SetRating(u8),
    AddTag(String),
    RemoveTag(String),
    CreateTag(String),
    DeleteTag(String),
    AddToCollection(String),
    RemoveFromCollection(String),
    CreateCollection(String),
    DeleteCollection(String),
    // view / filters
    SetScope(FilterScope),
    SetQuery(LibraryQuery),
    SetSearch(String),
    SetKinds(BTreeSet<MediaKind>),
    SetSort {
        key: SortKey,
        ascending: bool,
    },
    SetViewMode(ViewMode),
    SetThumbnailSize(u32),
    SetCollapseSequences(bool),
    AddFavorite(PathBuf),
    RemoveFavorite(PathBuf),
    TogglePreviewPane,
    /// Make pane `i` the active one (no-op when dual pane is off).
    FocusPane(usize),
    ToggleDualPane,
    SetShowAllFiles(bool),
    /// Copy / move the active pane's selection into the other pane's folder.
    CopyToOtherPane,
    MoveToOtherPane,
    OpenSettings,
    ApplySettings(Settings),
    OpenFullscreen(PathBuf),
    CloseFullscreen,
    /// Fullscreen next/previous media entry.
    FullscreenStep(isize),
    ShowAbout,
    ShowShortcuts,
    Quit,
    /// Paths received from the command line / a second instance.
    OpenPaths(Vec<PathBuf>),
}

/// Everything one file pane owns: its folder history, scan, view filters, selection and view state.
#[derive(Default)]
pub struct PaneState {
    pub nav: Navigation,
    /// Unfiltered scan of `nav.current` (cached between filter changes).
    pub scanned: Vec<Entry>,
    /// Visible, filtered, sorted.
    pub entries: Vec<Entry>,
    pub selection: Selection,
    pub filters: Filters,
    /// Entries of the Library scope could not be fully loaded etc. — human readable scan note.
    pub scan_note: Option<String>,
    /// Select this entry after the next refresh (new/renamed/opened item).
    pub pending_select: Option<PathBuf>,
    /// Widget-level view state (scroll, thumbnail request bookkeeping).
    pub view: FilePaneState,
}

pub struct AppState {
    pub dirs: AppDirs,
    pub settings: Settings,
    pub library: Library,
    pub fileops: FileOpsWorker,
    pub thumbs: Thumbnailer,
    pub metadata: MetadataWorker,
    /// Latest metadata for the active pane's `selection.focused`.
    pub current_metadata: Option<MediaMetadata>,
    /// Pane 0 (left) and pane 1 (right, shown when `settings.dual_pane`).
    pub panes: [PaneState; 2],
    /// Index of the active pane (0 unless dual pane is on).
    pub active: usize,
    pub clipboard: Option<Clipboard>,
    pub status: Option<Status>,
    pub dialogs: Dialogs,
    pub textures: TextureCache,
    pub preview: PreviewState,
    pub fullscreen: Option<FullscreenState>,
    /// Actions queued by panels this frame; drained by `actions::dispatch_pending`.
    pub pending: Vec<Action>,
    pub quit_requested: bool,
    // --- Shell additions ---
    /// Receiver of the native "Open Folder" dialog running on its own thread.
    pub folder_dialog: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,
    /// (done, total) of the running file operation.
    pub progress: Option<(u64, u64)>,
    /// Open this entry fullscreen after the next refresh (file given on the command line).
    pub pending_open: Option<PathBuf>,
    /// Bumped whenever the folder tree caches must be dropped (file ops, refresh, settings).
    pub tree_generation: u64,
    /// Bumped on every library mutation (tag/collection lists are cached per generation).
    pub library_generation: u64,
    /// Keep polling workers (thumbnails) until this instant.
    pub poll_until: std::time::Instant,
    /// Focus last pushed to the preview pane / metadata worker.
    pub synced_preview: Option<PathBuf>,
    pub synced_meta: Option<PathBuf>,
}

impl AppState {
    pub fn push(&mut self, a: Action) {
        self.pending.push(a);
    }
    /// Active pane (always valid: `active` is clamped by [`AppState::set_active`]).
    pub fn pane(&self) -> &PaneState {
        self.pane_at(self.active)
    }
    pub fn pane_mut(&mut self) -> &mut PaneState {
        let i = self.active;
        self.pane_at_mut(i)
    }
    pub fn pane_at(&self, i: usize) -> &PaneState {
        self.panes.get(i).unwrap_or(&self.panes[0])
    }
    pub fn pane_at_mut(&mut self, i: usize) -> &mut PaneState {
        let i = if i < self.panes.len() { i } else { 0 };
        &mut self.panes[i]
    }
    /// Index of the inactive pane.
    pub fn other_index(&self) -> usize {
        other_pane(self.active)
    }
    /// Number of panes currently shown.
    pub fn visible_panes(&self) -> usize {
        if self.settings.dual_pane { 2 } else { 1 }
    }
    /// Make pane `i` active; forced to 0 when only one pane is shown.
    pub fn set_active(&mut self, i: usize) {
        self.active = if self.settings.dual_pane && i < self.panes.len() {
            i
        } else {
            0
        };
    }
    /// Active pane's entry for `path`.
    pub fn entry(&self, path: &std::path::Path) -> Option<&Entry> {
        self.pane().entries.iter().find(|e| e.path == path)
    }
    /// Selected entries of the active pane in visible order.
    pub fn selected_entries(&self) -> Vec<&Entry> {
        let p = self.pane();
        p.entries
            .iter()
            .filter(|e| p.selection.items.contains(&e.path))
            .collect()
    }
    /// Member paths of all selected entries (sequences expanded).
    pub fn selected_member_paths(&self) -> Vec<PathBuf> {
        self.selected_entries()
            .iter()
            .flat_map(|e| e.member_paths())
            .collect()
    }
    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            kind: StatusKind::Info,
            text: text.into(),
        });
    }
    pub fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        log::error!("{text}");
        self.status = Some(Status {
            kind: StatusKind::Error,
            text,
        });
    }
}

/// Index of the pane that is not `i` (two panes).
pub fn other_pane(i: usize) -> usize {
    if i == 0 { 1 } else { 0 }
}

/// egui drag-and-drop payload for internal drags (file pane → folder tree): member paths.
#[derive(Clone, Debug)]
pub struct DragPaths(pub Vec<PathBuf>);
