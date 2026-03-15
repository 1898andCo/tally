//! Tests for rule ID normalization and validation (spec task 9.3).

use tally_ng::registry::normalize_rule_id;

// =============================================================================
// Positive: Basic normalization transformations
// =============================================================================

#[test]
fn underscore_replaced_with_hyphen() {
    assert_eq!(
        normalize_rule_id("unsafe_unwrap").expect("normalize underscore to hyphen"),
        "unsafe-unwrap"
    );
}

#[test]
fn space_replaced_with_hyphen() {
    assert_eq!(
        normalize_rule_id("unsafe unwrap").expect("normalize space to hyphen"),
        "unsafe-unwrap"
    );
}

#[test]
fn agent_namespace_stripped() {
    assert_eq!(
        normalize_rule_id("dclaude:unsafe-unwrap").expect("strip agent namespace"),
        "unsafe-unwrap"
    );
}

#[test]
fn agent_namespace_stripped_with_mixed_case() {
    assert_eq!(
        normalize_rule_id("DClaude:Unsafe_Unwrap").expect("strip namespace and normalize case"),
        "unsafe-unwrap"
    );
}

#[test]
fn case_folded_to_lowercase() {
    assert_eq!(
        normalize_rule_id("Unsafe-Unwrap").expect("fold mixed case to lowercase"),
        "unsafe-unwrap"
    );
}

#[test]
fn all_uppercase_folded() {
    assert_eq!(
        normalize_rule_id("UNSAFE-UNWRAP").expect("fold all-uppercase to lowercase"),
        "unsafe-unwrap"
    );
}

#[test]
fn already_normalized_unchanged() {
    assert_eq!(
        normalize_rule_id("unsafe-unwrap").expect("already-normalized ID unchanged"),
        "unsafe-unwrap"
    );
}

// =============================================================================
// Positive: NO prefix stripping — semantic prefixes are preserved
// =============================================================================

#[test]
fn no_prefix_preserved() {
    assert_eq!(
        normalize_rule_id("no-unwrap").expect("preserve no- prefix"),
        "no-unwrap"
    );
}

#[test]
fn check_prefix_preserved() {
    assert_eq!(
        normalize_rule_id("check-sql").expect("preserve check- prefix"),
        "check-sql"
    );
}

#[test]
fn disallow_prefix_preserved() {
    assert_eq!(
        normalize_rule_id("disallow-eval").expect("preserve disallow- prefix"),
        "disallow-eval"
    );
}

// =============================================================================
// Positive: Consecutive hyphens collapsed
// =============================================================================

#[test]
fn consecutive_hyphens_collapsed() {
    assert_eq!(
        normalize_rule_id("foo--bar").expect("collapse consecutive hyphens"),
        "foo-bar"
    );
}

#[test]
fn triple_hyphens_collapsed() {
    assert_eq!(
        normalize_rule_id("foo---bar").expect("collapse triple hyphens"),
        "foo-bar"
    );
}

#[test]
fn underscores_and_spaces_combined_collapse() {
    // "foo_ _bar" -> "foo---bar" -> "foo-bar"
    assert_eq!(
        normalize_rule_id("foo_ _bar").expect("collapse mixed separator sequence"),
        "foo-bar"
    );
}

// =============================================================================
// Positive: Leading/trailing hyphens trimmed
// =============================================================================

#[test]
fn leading_hyphen_trimmed() {
    assert_eq!(
        normalize_rule_id("-foo-bar").expect("trim leading hyphen"),
        "foo-bar"
    );
}

#[test]
fn trailing_hyphen_trimmed() {
    assert_eq!(
        normalize_rule_id("foo-bar-").expect("trim trailing hyphen"),
        "foo-bar"
    );
}

#[test]
fn leading_and_trailing_hyphens_trimmed() {
    assert_eq!(
        normalize_rule_id("--foo-bar--").expect("trim leading and trailing hyphens"),
        "foo-bar"
    );
}

// =============================================================================
// Positive: Idempotent — normalize(normalize(x)) == normalize(x)
// =============================================================================

#[test]
fn idempotent_simple() {
    let input = "unsafe-unwrap";
    let once = normalize_rule_id(input).expect("first normalization pass");
    let twice = normalize_rule_id(&once).expect("second normalization pass");
    assert_eq!(once, twice);
}

#[test]
fn idempotent_with_namespace() {
    let input = "dclaude:Unsafe_Unwrap";
    let once = normalize_rule_id(input).expect("first normalization of namespaced ID");
    let twice = normalize_rule_id(&once).expect("second normalization of namespaced ID");
    assert_eq!(once, twice);
}

#[test]
fn idempotent_with_consecutive_hyphens() {
    let input = "foo--bar";
    let once = normalize_rule_id(input).expect("first normalization of consecutive hyphens");
    let twice = normalize_rule_id(&once).expect("second normalization of consecutive hyphens");
    assert_eq!(once, twice);
}

#[test]
fn idempotent_with_mixed_transforms() {
    let input = "Agent:FOO_ _BAR";
    let once = normalize_rule_id(input).expect("first normalization of mixed transforms");
    let twice = normalize_rule_id(&once).expect("second normalization of mixed transforms");
    assert_eq!(once, twice);
}

#[test]
fn idempotent_no_prefix() {
    let input = "no-unwrap";
    let once = normalize_rule_id(input).expect("first normalization of no- prefix ID");
    let twice = normalize_rule_id(&once).expect("second normalization of no- prefix ID");
    assert_eq!(once, twice);
}

// =============================================================================
// Negative: single char rejected (min length is 2)
// =============================================================================

#[test]
fn single_char_rejected() {
    assert!(
        normalize_rule_id("a").is_err(),
        "single char should be rejected (min 2)"
    );
}

// =============================================================================
// Negative: empty string rejected
// =============================================================================

#[test]
fn empty_string_rejected() {
    assert!(
        normalize_rule_id("").is_err(),
        "empty string should be rejected"
    );
}

// =============================================================================
// Negative: special characters rejected
// =============================================================================

#[test]
fn slash_rejected() {
    assert!(
        normalize_rule_id("foo/bar").is_err(),
        "slash should be rejected"
    );
}

#[test]
fn dot_rejected() {
    assert!(
        normalize_rule_id("foo.bar").is_err(),
        "dot should be rejected"
    );
}

#[test]
fn backslash_rejected() {
    assert!(
        normalize_rule_id("foo\\bar").is_err(),
        "backslash should be rejected"
    );
}

// =============================================================================
// Negative: too long rejected (65+ chars after normalization)
// =============================================================================

#[test]
fn sixty_five_chars_rejected() {
    let input = "a".repeat(65);
    assert!(
        normalize_rule_id(&input).is_err(),
        "65-char ID should be rejected (max 64)"
    );
}

#[test]
fn one_hundred_chars_rejected() {
    let input = "a".repeat(100);
    assert!(
        normalize_rule_id(&input).is_err(),
        "100-char ID should be rejected"
    );
}

// =============================================================================
// Negative: leading/trailing hyphens rejected (after normalization)
// Note: normalization trims hyphens, so these only fail if the result
// is too short or empty after trimming.
// =============================================================================

#[test]
fn only_hyphens_rejected() {
    assert!(
        normalize_rule_id("---").is_err(),
        "hyphens-only should be rejected (empty after trim)"
    );
}

#[test]
fn single_char_after_hyphen_trim_rejected() {
    // "-a-" trims to "a" which is 1 char < min 2
    assert!(
        normalize_rule_id("-a-").is_err(),
        "single char after trim should be rejected"
    );
}

// =============================================================================
// Boundary: exactly 2 chars (minimum valid)
// =============================================================================

#[test]
fn two_char_id_accepted() {
    assert_eq!(
        normalize_rule_id("ab").expect("accept minimum 2-char ID"),
        "ab"
    );
}

// =============================================================================
// Boundary: exactly 64 chars (maximum valid)
// =============================================================================

#[test]
fn sixty_four_char_id_accepted() {
    let input = "a".repeat(64);
    assert_eq!(
        normalize_rule_id(&input).expect("accept maximum 64-char ID"),
        input
    );
}

// =============================================================================
// Boundary: digits allowed
// =============================================================================

#[test]
fn numeric_id_accepted() {
    assert_eq!(
        normalize_rule_id("r2d2").expect("accept alphanumeric ID"),
        "r2d2"
    );
}

#[test]
fn all_digits_accepted() {
    assert_eq!(normalize_rule_id("42").expect("accept all-digit ID"), "42");
}

// =============================================================================
// Edge: namespace-only input (nothing after colon)
// =============================================================================

#[test]
fn namespace_only_rejected() {
    // "dclaude:" -> strip namespace -> "" -> too short
    assert!(
        normalize_rule_id("dclaude:").is_err(),
        "namespace-only input should be rejected"
    );
}

#[test]
fn namespace_with_single_char_rejected() {
    // "dclaude:a" -> "a" -> 1 char < min 2
    assert!(
        normalize_rule_id("dclaude:a").is_err(),
        "namespace with single char should be rejected"
    );
}
