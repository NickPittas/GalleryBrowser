use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CaseMode { #[default] Keep, Lower, Upper, Title }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NumberMode {
    #[default]
    None,
    Prefix,
    Suffix,
    /// The whole stem becomes the number.
    Replace,
}

/// Batch-rename rule. Field set mirrors the Python dialog (see behavior spec): template tokens,
/// find/replace (optionally regex, case-sensitive), case conversion, numbering (start, step, padding,
/// separator), prefix/suffix, extension change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenameRule {
    /// Template with tokens, e.g. `{name}_{n}`; empty = keep stem. Tokens: {name} {ext} {n} {date} {time}
    /// {parent} {width} {height}(if known else empty) — exact list per Python spec.
    pub template: String,
    pub find: String,
    pub replace: String,
    pub use_regex: bool,
    pub case_sensitive: bool,
    pub case_mode: CaseMode,
    pub number_mode: NumberMode,
    pub start: i64,
    pub step: i64,
    pub padding: usize,
    pub separator: String,
    pub prefix: String,
    pub suffix: String,
    /// `None` keep extension; `Some("")` remove; `Some("jpg")` change.
    pub new_extension: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameStatus {
    Ok,
    Unchanged,
    /// Empty name, `/`, NUL, `.`/`..`, invalid regex, name too long.
    Invalid(String),
    /// Two outputs equal, or output equals an existing file that is NOT itself being renamed away.
    Collision(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenameItem {
    pub from: PathBuf,
    pub to: PathBuf,
    pub status: RenameStatus,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenamePlan {
    pub items: Vec<RenameItem>,
}

impl RenamePlan {
    /// Executable: no Invalid/Collision items and at least one Ok item.
    pub fn is_valid(&self) -> bool {
        false
    }
}

/// Pure planning (preview); checks the filesystem for collisions with unrelated existing files.
pub fn plan_renames(_rule: &RenameRule, _files: &[PathBuf]) -> RenamePlan {
    RenamePlan::default()
}
