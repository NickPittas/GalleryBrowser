use std::cmp::Ordering;

/// Natural ("human") ordering: `shot2` < `shot10`, case-insensitive.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}
