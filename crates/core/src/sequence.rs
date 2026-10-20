use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequenceFrame {
    /// Source frame number parsed from the file name (gaps allowed).
    pub number: i64,
    pub path: PathBuf,
}

/// A numbered image sequence in one directory: `prefix` + zero-padded number + `suffix` (incl. extension).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub dir: PathBuf,
    pub prefix: String,
    pub suffix: String,
    /// Digit count of the zero padding (0 = unpadded).
    pub padding: usize,
    /// Sorted by `number`, unique numbers.
    pub frames: Vec<SequenceFrame>,
}

impl Sequence {
    /// Stable identity: `dir/prefix####suffix` (`#` repeated `max(padding,1)` times).
    pub fn pattern_path(&self) -> PathBuf {
        self.dir.clone()
    }
    /// Display name, e.g. `shot_[1001-1100].exr` (missing frames do not change it).
    pub fn display_name(&self) -> String {
        String::new()
    }
    pub fn first(&self) -> Option<&SequenceFrame> {
        self.frames.first()
    }
    pub fn last(&self) -> Option<&SequenceFrame> {
        self.frames.last()
    }
    /// Frame used for the thumbnail (middle frame by index).
    pub fn representative(&self) -> Option<&SequenceFrame> {
        self.frames.get(self.frames.len() / 2)
    }
    pub fn paths(&self) -> Vec<PathBuf> {
        self.frames.iter().map(|f| f.path.clone()).collect()
    }
    /// Inclusive ranges of missing frame numbers between first and last.
    pub fn missing_ranges(&self) -> Vec<(i64, i64)> {
        Vec::new()
    }
}

/// Group image files into sequences (same dir, prefix, padding-compatible number, suffix; >= `min_len` frames).
/// Returns (sequences, leftover single files). Only image kinds form sequences; PSD/PSB never do.
pub fn detect_sequences(_files: &[PathBuf], _min_len: usize) -> (Vec<Sequence>, Vec<PathBuf>) {
    (Vec::new(), Vec::new())
}

/// Parse `name` into (prefix, digits, suffix) using the LAST digit run before the extension.
pub fn split_numbered(_name: &str) -> Option<(String, String, String)> {
    None
}

#[allow(dead_code)]
fn _unused(_: &Path) {}
