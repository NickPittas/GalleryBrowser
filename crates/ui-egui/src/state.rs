use std::collections::BTreeSet;
use std::path::PathBuf;

use gallerybrowser_core::{AppDirs, Entry, FilterScope, MediaKind, Settings, SortKey, ViewMode};
use gallerybrowser_fileops::{FileOpsWorker, RenamePlan};
use gallerybrowser_library::{Library, LibraryQuery};
use gallerybrowser_media::{MediaMetadata, MetadataWorker, Thumbnailer};

use crate::batch_rename::BatchRenameDialog;
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
    TogglePreviewPane,
    AddFavorite(PathBuf),
    RemoveFavorite(PathBuf),
    // windows
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

pub struct AppState {
    pub dirs: AppDirs,
    pub settings: Settings,
    pub library: Library,
    pub fileops: FileOpsWorker,
    pub thumbs: Thumbnailer,
    pub metadata: MetadataWorker,
    /// Latest metadata for `selection.focused`.
    pub current_metadata: Option<MediaMetadata>,
    pub nav: Navigation,
    /// Unfiltered scan of `nav.current` (cached between filter changes).
    pub scanned: Vec<Entry>,
    /// Visible, filtered, sorted.
    pub entries: Vec<Entry>,
    pub selection: Selection,
    pub filters: Filters,
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
    /// Select this entry after the next refresh (new/renamed/opened item).
    pub pending_select: Option<PathBuf>,
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
    /// Entries of the Library scope could not be fully loaded etc. — human readable scan note.
    pub scan_note: Option<String>,
}

impl AppState {
    pub fn push(&mut self, a: Action) {
        self.pending.push(a);
    }
    pub fn entry(&self, path: &std::path::Path) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }
    /// Selected entries in visible order.
    pub fn selected_entries(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| self.selection.items.contains(&e.path))
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

/// egui drag-and-drop payload for internal drags (file pane → folder tree): member paths.
#[derive(Clone, Debug)]
pub struct DragPaths(pub Vec<PathBuf>);
