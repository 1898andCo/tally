//! Scope enforcement tests — spec task 9.5 fixture cases.
//!
//! Validates glob-based file path matching for rule applicability.
//! All 8 fixture cases from the spec are covered.

use tally_ng::registry::{RuleScope, check_scope};

fn scope_with(include: &[&str], exclude: &[&str]) -> RuleScope {
    RuleScope {
        include: include.iter().map(|s| (*s).to_string()).collect(),
        exclude: exclude.iter().map(|s| (*s).to_string()).collect(),
    }
}

#[test]
fn include_matches_file_in_scope() {
    let scope = scope_with(&["src/**/*.rs"], &[]);
    let result = check_scope(Some(&scope), "test-rule", "src/api/handler.rs");
    assert!(
        result.is_none(),
        "File matching include pattern should be in scope"
    );
}

#[test]
fn include_rejects_file_outside_scope() {
    let scope = scope_with(&["src/**/*.rs"], &[]);
    let result = check_scope(Some(&scope), "test-rule", "tests/api_test.rs");
    assert!(
        result.is_some(),
        "File not matching include pattern should be out of scope"
    );
    let warning = result.expect("out-of-scope file should produce a warning");
    assert!(
        warning.contains("test-rule"),
        "Warning should mention the rule ID: {warning}"
    );
    assert!(
        warning.contains("tests/api_test.rs"),
        "Warning should mention the file path: {warning}"
    );
}

#[test]
fn exclude_overrides_include() {
    let scope = scope_with(&["src/**/*.rs"], &["src/generated/**"]);
    let result = check_scope(Some(&scope), "test-rule", "src/generated/proto.rs");
    assert!(
        result.is_some(),
        "Excluded file should be out of scope even if it matches include"
    );
    let warning = result.expect("excluded file should produce a warning");
    assert!(
        warning.contains("excludes"),
        "Warning should indicate exclusion: {warning}"
    );
}

#[test]
fn no_scope_matches_everything() {
    let result = check_scope(None, "test-rule", "literally/any/file.txt");
    assert!(result.is_none(), "No scope (None) should match every file");
}

#[test]
fn include_rejects_outside_prefix() {
    let scope = scope_with(&["src/api/**"], &[]);
    let result = check_scope(Some(&scope), "test-rule", "src/lib.rs");
    assert!(
        result.is_some(),
        "File outside the include prefix should be rejected"
    );
    let warning = result.expect("file outside include prefix should produce a warning");
    assert!(
        warning.contains("src/lib.rs"),
        "Warning should mention the rejected file: {warning}"
    );
}

#[test]
fn exclude_wins_over_include() {
    let scope = scope_with(&["src/**/*.rs"], &["src/gen/**"]);
    let result = check_scope(Some(&scope), "test-rule", "src/gen/auto.rs");
    assert!(
        result.is_some(),
        "Exclude should win over include for matching file"
    );
}

#[test]
fn no_include_with_exclude_passes_non_excluded() {
    let scope = scope_with(&[], &["vendor/**"]);
    let result = check_scope(Some(&scope), "test-rule", "src/api.rs");
    assert!(
        result.is_none(),
        "File not matching exclude-only scope should pass"
    );
}

#[test]
fn exclude_only_rejects_excluded() {
    let scope = scope_with(&[], &["vendor/**"]);
    let result = check_scope(Some(&scope), "test-rule", "vendor/lib.rs");
    assert!(
        result.is_some(),
        "File matching exclude pattern should be rejected"
    );
    let warning = result.expect("exclude-only match should produce a warning");
    assert!(
        warning.contains("vendor/lib.rs"),
        "Warning should mention the excluded file: {warning}"
    );
}
