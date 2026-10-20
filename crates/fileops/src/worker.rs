use std::path::PathBuf;

use crate::{FileOp, OpError};

#[derive(Clone, Debug, Default)]
pub struct OpOutcome {
    pub id: u64,
    /// Human label for status bar / undo menu, e.g. "Move 3 items".
    pub label: String,
    /// (old, new) for every file/dir that changed path (move, rename, batch rename, undo of those).
    pub moved: Vec<(PathBuf, PathBuf)>,
    pub created: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
    pub errors: Vec<OpError>,
}

#[derive(Clone, Debug)]
pub enum OpEvent {
    Progress { id: u64, done: u64, total: u64, current: Option<PathBuf> },
    Finished(OpOutcome),
}

/// Owns the worker thread and the undo/redo history (history mutated only on the worker, serialized).
pub struct FileOpsWorker {}

impl FileOpsWorker {
    pub fn new() -> Self {
        Self {}
    }
    /// Queue an operation; returns its id immediately.
    pub fn submit(&self, _op: FileOp) -> u64 {
        0
    }
    /// Undo the most recent undoable finished op (queued like any op). `None` if nothing to undo.
    pub fn undo(&self) -> Option<u64> {
        None
    }
    pub fn redo(&self) -> Option<u64> {
        None
    }
    pub fn can_undo(&self) -> bool {
        false
    }
    pub fn can_redo(&self) -> bool {
        false
    }
    /// Label of the op `undo` would revert.
    pub fn undo_label(&self) -> Option<String> {
        None
    }
    /// Non-blocking: drain events since last call.
    pub fn poll(&self) -> Vec<OpEvent> {
        Vec::new()
    }
    /// True while any op is queued or running.
    pub fn busy(&self) -> bool {
        false
    }
}

impl Default for FileOpsWorker {
    fn default() -> Self {
        Self::new()
    }
}
