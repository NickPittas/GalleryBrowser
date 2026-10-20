use std::path::{Path, PathBuf};

use crate::{Entry, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default)]
pub enum SortKey {
    #[default]
    Name,
    Modified,
    Size,
    Kind,
}

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub show_hidden: bool,
    pub collapse_sequences: bool,
    /// Minimum frames to form a sequence (Python: 2).
    pub min_sequence_len: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { show_hidden: false, collapse_sequences: true, min_sequence_len: 2 }
    }
}

/// Media entries directly inside `dir` (non-recursive, no directories). Unreadable single files are skipped
/// (logged); an unreadable `dir` is an error.
pub fn scan_folder(_dir: &Path, _opts: &ScanOptions) -> Result<Vec<Entry>> {
    Ok(Vec::new())
}

/// Immediate subdirectories, naturally sorted; hidden ones only if `show_hidden`.
pub fn list_subfolders(_dir: &Path, _show_hidden: bool) -> Result<Vec<PathBuf>> {
    Ok(Vec::new())
}

/// Stable sort; ties broken by natural name order.
pub fn sort_entries(_entries: &mut [Entry], _key: SortKey, _ascending: bool) {}
