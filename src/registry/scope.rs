//! Scope enforcement — glob-based file path matching for rule applicability.
//!
//! Rules can define `scope.include` and `scope.exclude` glob patterns.
//! Scope violations produce advisory warnings, never block recording.

use globset::{Glob, GlobSet, GlobSetBuilder};

use super::rule::RuleScope;

/// Check if a file path is in scope for a rule.
///
/// Returns `None` if the file is in scope (or rule has no scope),
/// or `Some(warning_message)` if the file is out of scope.
#[must_use]
pub fn check_scope(scope: Option<&RuleScope>, rule_id: &str, file_path: &str) -> Option<String> {
    let scope = scope?; // No scope = applies everywhere

    // Check exclude first (exclude wins over include)
    if !scope.exclude.is_empty() {
        if let Ok(exclude_set) = build_glob_set(&scope.exclude) {
            if exclude_set.is_match(file_path) {
                return Some(format!(
                    "Rule '{rule_id}' excludes [{}] and finding is in {file_path}",
                    scope.exclude.join(", ")
                ));
            }
        }
    }

    // Check include (if set, file must match at least one pattern)
    if !scope.include.is_empty() {
        if let Ok(include_set) = build_glob_set(&scope.include) {
            if !include_set.is_match(file_path) {
                return Some(format!(
                    "Rule '{rule_id}' is scoped to [{}] but finding is in {file_path}",
                    scope.include.join(", ")
                ));
            }
        }
    }

    None
}

/// Build a `GlobSet` from a list of glob pattern strings.
fn build_glob_set(patterns: &[String]) -> Result<GlobSet, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    builder.build()
}
