//! Organization metadata (tags = catalog keywords, collections = catalog albums, ratings 0–5) keyed by
//! `Entry::path`, persisted with `lightcraft-catalog` (journal + snapshots, crash-safe) under
//! `AppDirs::library_dir()`. Files enter the catalog only when annotated. Path changes from file ops
//! MUST be applied with [`Library::relink`]; deletions with [`Library::forget`].

use std::path::{Path, PathBuf};

use gallerybrowser_core::Entry;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("catalog error: {0}")]
    Catalog(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    Exists(String),
    #[error("legacy import failed: {0}")]
    Legacy(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedCount {
    pub name: String,
    /// Number of files carrying it.
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatingFilter {
    Exactly(u8),
    AtLeast(u8),
    /// Rating 0 or not in the catalog at all.
    Unrated,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryQuery {
    /// Files must carry ALL of these tags.
    pub tags: Vec<String>,
    pub collection: Option<String>,
    pub rating: Option<RatingFilter>,
}

impl LibraryQuery {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.collection.is_none() && self.rating.is_none()
    }
}

/// Annotations of one path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotations {
    pub rating: u8,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
}

pub struct Library {}

impl Library {
    /// Open or create the catalog in `dir`. If `legacy_sqlite` is given and the catalog is new,
    /// import tags/collections/ratings from the Python-era SQLite DB once.
    pub fn open(_dir: &Path, _legacy_sqlite: Option<&Path>) -> Result<Self> {
        Err(LibraryError::Catalog("stub".into()))
    }
    /// In-memory catalog (tests, and fallback when the on-disk catalog cannot be opened).
    pub fn in_memory() -> Self {
        Self {}
    }

    pub fn tags(&self) -> Vec<NamedCount> {
        Vec::new()
    }
    pub fn create_tag(&mut self, _name: &str) -> Result<()> { Ok(()) }
    pub fn rename_tag(&mut self, _old: &str, _new: &str) -> Result<()> { Ok(()) }
    pub fn delete_tag(&mut self, _name: &str) -> Result<()> { Ok(()) }
    pub fn add_tag(&mut self, _paths: &[PathBuf], _tag: &str) -> Result<()> { Ok(()) }
    pub fn remove_tag(&mut self, _paths: &[PathBuf], _tag: &str) -> Result<()> { Ok(()) }

    pub fn collections(&self) -> Vec<NamedCount> {
        Vec::new()
    }
    pub fn create_collection(&mut self, _name: &str) -> Result<()> { Ok(()) }
    pub fn rename_collection(&mut self, _old: &str, _new: &str) -> Result<()> { Ok(()) }
    pub fn delete_collection(&mut self, _name: &str) -> Result<()> { Ok(()) }
    pub fn add_to_collection(&mut self, _paths: &[PathBuf], _name: &str) -> Result<()> { Ok(()) }
    pub fn remove_from_collection(&mut self, _paths: &[PathBuf], _name: &str) -> Result<()> { Ok(()) }

    /// 0 clears the rating. Values > 5 are an error.
    pub fn set_rating(&mut self, _paths: &[PathBuf], _rating: u8) -> Result<()> { Ok(()) }
    pub fn annotations(&self, _path: &Path) -> Annotations { Annotations::default() }

    /// Library scope: every catalog path matching `q` that still exists on disk.
    pub fn query(&self, _q: &LibraryQuery) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Folder scope: keep scanned entries matching `q` (Unrated keeps entries absent from the catalog).
    pub fn filter_entries(&self, entries: Vec<Entry>, _q: &LibraryQuery) -> Vec<Entry> { entries }

    /// Apply path changes (files and directories: a moved directory relinks everything below it).
    pub fn relink(&mut self, _moved: &[(PathBuf, PathBuf)]) -> Result<()> { Ok(()) }
    /// Drop annotations of permanently removed/trashed paths (and everything below removed directories).
    pub fn forget(&mut self, _removed: &[PathBuf]) -> Result<()> { Ok(()) }
}
