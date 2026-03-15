//! Tests for the rule matcher pipeline (spec tasks 9.2, 9.3b, 9.4).
//!
//! Covers all 7 stages: normalize, exact match, alias lookup, CWE cross-reference,
//! Jaro-Winkler similarity, token Jaccard on descriptions, and auto-registration.

use tally_ng::registry::{Rule, RuleMatcher, RuleStatus};

// =============================================================================
// Test fixture
// =============================================================================

/// Build 4 test rules from the spec fixture.
fn test_rules() -> Vec<Rule> {
    let mut unsafe_unwrap = Rule::new(
        "unsafe-unwrap".to_string(),
        "Unsafe Unwrap".to_string(),
        "Using unwrap on Result or Option in production code without error handling".to_string(),
    );
    unsafe_unwrap.aliases = vec!["unwrap-usage".to_string(), "no-unwrap".to_string()];
    unsafe_unwrap.cwe_ids = vec!["CWE-252".to_string()];
    unsafe_unwrap.category = "safety".to_string();
    unsafe_unwrap.severity_hint = "high".to_string();
    unsafe_unwrap.status = RuleStatus::Active;

    let mut sql_injection = Rule::new(
        "sql-injection".to_string(),
        "SQL Injection".to_string(),
        "Unsanitized user input concatenated into SQL query strings".to_string(),
    );
    sql_injection.aliases = vec!["sqli".to_string()];
    sql_injection.cwe_ids = vec!["CWE-89".to_string()];
    sql_injection.category = "security".to_string();
    sql_injection.severity_hint = "critical".to_string();
    sql_injection.status = RuleStatus::Active;

    let mut spec_drift = Rule::new(
        "spec-drift".to_string(),
        "Spec Drift".to_string(),
        "Implementation deviates from the specification document".to_string(),
    );
    spec_drift.category = "spec-compliance".to_string();
    spec_drift.severity_hint = "medium".to_string();
    spec_drift.status = RuleStatus::Active;

    let mut resource_leak = Rule::new(
        "resource-leak".to_string(),
        "Resource Leak".to_string(),
        "File handle or database connection not closed after use".to_string(),
    );
    resource_leak.cwe_ids = vec!["CWE-404".to_string()];
    resource_leak.category = "reliability".to_string();
    resource_leak.severity_hint = "high".to_string();
    resource_leak.status = RuleStatus::Active;

    vec![unsafe_unwrap, sql_injection, spec_drift, resource_leak]
}

/// Build a matcher from the standard test fixture.
fn test_matcher() -> RuleMatcher {
    RuleMatcher::new(test_rules())
}

// =============================================================================
// Positive tests — each pipeline stage
// =============================================================================

#[test]
fn exact_match_returns_confidence_1() {
    let matcher = test_matcher();
    let result = matcher
        .resolve("unsafe-unwrap", None, None)
        .expect("exact match on canonical ID should resolve");

    assert_eq!(result.canonical_id, "unsafe-unwrap");
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    assert_eq!(result.method, "exact");
}

#[test]
fn alias_maps_to_canonical() {
    let matcher = test_matcher();
    let result = matcher
        .resolve("unwrap-usage", None, None)
        .expect("alias should resolve to canonical rule");

    assert_eq!(result.canonical_id, "unsafe-unwrap");
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    assert_eq!(result.method, "alias");
}

#[test]
fn normalize_then_exact() {
    let matcher = test_matcher();
    // Mixed case and underscore should normalize to "unsafe-unwrap"
    let result = matcher
        .resolve("Unsafe_Unwrap", None, None)
        .expect("normalized mixed-case underscore ID should resolve");

    assert_eq!(result.canonical_id, "unsafe-unwrap");
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    assert_eq!(result.method, "exact");
}

#[test]
fn agent_namespace_stripped_then_exact() {
    let matcher = test_matcher();
    // Agent namespace prefix "dclaude:" should be stripped before matching
    let result = matcher
        .resolve("dclaude:unsafe-unwrap", None, None)
        .expect("agent namespace prefix should be stripped before matching");

    assert_eq!(result.canonical_id, "unsafe-unwrap");
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    assert_eq!(result.method, "exact");
}

#[test]
fn cwe_match_adds_suggestion() {
    let matcher = test_matcher();
    // Unknown rule ID but shares CWE-252 with "unsafe-unwrap"
    let cwe_ids = vec!["CWE-252".to_string()];
    let result = matcher
        .resolve("unknown-cwe-rule", Some(&cwe_ids), None)
        .expect("unknown rule with CWE hint should auto-register");

    // Should auto-register (not auto-match via CWE)
    assert_eq!(result.method, "auto_registered");
    assert!((result.confidence - 0.0).abs() < f64::EPSILON);

    // But CWE match should appear in suggestions
    let cwe_suggestion = result
        .similar_rules
        .iter()
        .find(|s| s.method == "cwe")
        .expect("CWE suggestion should be present");
    assert_eq!(cwe_suggestion.id, "unsafe-unwrap");
    assert!((cwe_suggestion.confidence - 0.7).abs() < f64::EPSILON);
}

#[test]
fn jaro_winkler_adds_suggestion() {
    let matcher = test_matcher();
    // Typo in rule ID — JW should suggest the correct one
    let result = matcher
        .resolve("unsaf-unwrap", None, None)
        .expect("typo rule ID should auto-register with JW suggestions");

    // Should auto-register, not auto-match
    assert_eq!(result.method, "auto_registered");
    assert!((result.confidence - 0.0).abs() < f64::EPSILON);

    // JW suggestion should be present
    let jw_suggestion = result
        .similar_rules
        .iter()
        .find(|s| s.method == "jaro_winkler")
        .expect("Jaro-Winkler suggestion should be present");
    assert_eq!(jw_suggestion.id, "unsafe-unwrap");
    assert!(
        jw_suggestion.confidence >= 0.6,
        "JW score should meet the 0.6 threshold, got {}",
        jw_suggestion.confidence
    );
}

#[test]
fn token_jaccard_adds_suggestion() {
    // Use a dedicated matcher where "spec-drift" has a known description,
    // and the input rule ID is very different (low JW) but description overlaps.
    let rule = Rule::new(
        "spec-drift".to_string(),
        "Spec Drift".to_string(),
        "implementation deviates specification document".to_string(),
    );
    let matcher = RuleMatcher::new(vec![rule]);

    // Input ID is completely unrelated (low JW to "spec-drift") but description
    // shares tokens with the rule's description.
    let result = matcher
        .resolve(
            "zz-check-ab",
            None,
            Some("implementation deviates specification document"),
        )
        .expect("unrelated ID with overlapping description should auto-register");

    assert_eq!(result.method, "auto_registered");

    // Jaccard suggestion should be present (identical description tokens)
    let jaccard_suggestion = result
        .similar_rules
        .iter()
        .find(|s| s.method == "token_jaccard");
    assert!(
        jaccard_suggestion.is_some(),
        "Token Jaccard suggestion should be present for overlapping description; \
         similar_rules: {:?}",
        result.similar_rules
    );
}

#[test]
fn auto_registration_for_unknown() {
    let matcher = test_matcher();
    let result = matcher
        .resolve("completely-new-rule", None, None)
        .expect("unknown rule should auto-register successfully");

    assert_eq!(result.canonical_id, "completely-new-rule");
    assert!((result.confidence - 0.0).abs() < f64::EPSILON);
    assert_eq!(result.method, "auto_registered");
}

// =============================================================================
// Negative / boundary tests
// =============================================================================

#[test]
fn alias_shadows_canonical_rejected() {
    let matcher = test_matcher();
    // Alias "sqli" exists on sql-injection; a new rule with canonical "sqli" should conflict
    let result = matcher.check_id_namespace("new-rule", &["sql-injection".to_string()]);
    assert!(
        result.is_err(),
        "alias matching an existing canonical ID should be rejected"
    );
    let err_msg = result
        .expect_err("alias shadowing a canonical ID should be rejected")
        .to_string();
    assert!(
        err_msg.contains("conflicts with canonical rule"),
        "error should mention canonical conflict, got: {err_msg}"
    );
}

#[test]
fn canonical_shadows_alias_rejected() {
    let matcher = test_matcher();
    // "unwrap-usage" is an alias on "unsafe-unwrap"; using it as a canonical ID should fail
    let result = matcher.check_id_namespace("unwrap-usage", &[]);
    assert!(
        result.is_err(),
        "canonical ID matching an existing alias should be rejected"
    );
    let err_msg = result
        .expect_err("canonical ID shadowing an existing alias should be rejected")
        .to_string();
    assert!(
        err_msg.contains("conflicts with alias"),
        "error should mention alias conflict, got: {err_msg}"
    );
}

#[test]
fn alias_claimed_by_other_rule_rejected() {
    let matcher = test_matcher();
    // "sqli" is an alias on sql-injection; another rule trying to claim it should fail
    let result = matcher.check_id_namespace("new-rule", &["sqli".to_string()]);
    assert!(
        result.is_err(),
        "alias already owned by another rule should be rejected"
    );
    let err_msg = result
        .expect_err("alias already owned by another rule should be rejected")
        .to_string();
    assert!(
        err_msg.contains("already claimed"),
        "error should mention alias claimed, got: {err_msg}"
    );
}

#[test]
fn empty_registry_auto_registers() {
    let matcher = RuleMatcher::new(vec![]);
    let result = matcher
        .resolve("brand-new-rule", None, None)
        .expect("empty registry should auto-register any valid rule");

    assert_eq!(result.canonical_id, "brand-new-rule");
    assert!((result.confidence - 0.0).abs() < f64::EPSILON);
    assert_eq!(result.method, "auto_registered");
    assert!(
        result.similar_rules.is_empty(),
        "empty registry should produce no suggestions"
    );
}

#[test]
fn invalid_rule_id_returns_error() {
    let matcher = test_matcher();
    // "/" is not a valid rule ID character — normalization should reject it
    let result = matcher.resolve("/", None, None);
    assert!(
        result.is_err(),
        "rule ID containing only '/' should be rejected by normalization"
    );
}

#[test]
fn single_char_rule_id_returns_error() {
    let matcher = test_matcher();
    // Single-char IDs are below the 2-char minimum
    let result = matcher.resolve("x", None, None);
    assert!(
        result.is_err(),
        "single-char rule ID should be rejected (minimum 2 chars)"
    );
}

// =============================================================================
// Combined pipeline tests
// =============================================================================

#[test]
fn full_pipeline_exact_match_short_circuits() {
    let matcher = test_matcher();
    let result = matcher
        .resolve(
            "unsafe-unwrap",
            Some(&["CWE-252".to_string()]),
            Some("unwrap on Result without error handling"),
        )
        .expect("exact match with extra hints should still resolve");

    // Exact match should short-circuit — no further stages run
    assert_eq!(result.method, "exact");
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    assert!(
        result.similar_rules.is_empty(),
        "exact match should not populate similar_rules"
    );
}

#[test]
fn full_pipeline_alias_short_circuits() {
    let matcher = test_matcher();
    let result = matcher
        .resolve(
            "unwrap-usage",
            Some(&["CWE-252".to_string()]),
            Some("unwrap on Result without error handling"),
        )
        .expect("alias match with extra hints should still resolve");

    // Alias match should short-circuit — no CWE/JW/Jaccard stages
    assert_eq!(result.method, "alias");
    assert_eq!(result.canonical_id, "unsafe-unwrap");
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    assert!(
        result.similar_rules.is_empty(),
        "alias match should not populate similar_rules"
    );
}

#[test]
fn jw_does_not_auto_match() {
    // Two very similar rule IDs that JW would score highly on
    let rule1 = Rule::new(
        "rule-crit1".to_string(),
        "Critical Rule 1".to_string(),
        "First critical rule for testing".to_string(),
    );
    let rule2 = Rule::new(
        "rule-crit2".to_string(),
        "Critical Rule 2".to_string(),
        "Second critical rule for testing".to_string(),
    );
    let matcher = RuleMatcher::new(vec![rule1, rule2]);

    // "rule-crit1" is an exact match — verify baseline
    let exact = matcher
        .resolve("rule-crit1", None, None)
        .expect("exact match baseline should resolve");
    assert_eq!(exact.method, "exact");

    // "rule-crit3" is close to both but should NOT auto-match
    let result = matcher
        .resolve("rule-crit3", None, None)
        .expect("similar but non-matching ID should auto-register");
    assert_eq!(
        result.method, "auto_registered",
        "high JW score must NOT auto-match — should auto-register instead"
    );
    assert!((result.confidence - 0.0).abs() < f64::EPSILON);

    // But similar_rules should be populated
    assert!(
        !result.similar_rules.is_empty(),
        "similar_rules should contain JW suggestions for close matches"
    );
    assert!(
        result
            .similar_rules
            .iter()
            .any(|s| s.method == "jaro_winkler"),
        "at least one suggestion should use jaro_winkler method"
    );
}

#[test]
fn cwe_multiple_rules_all_suggested() {
    // Two rules sharing the same CWE
    let mut rule_a = Rule::new(
        "rule-alpha".to_string(),
        "Alpha Rule".to_string(),
        "Alpha description".to_string(),
    );
    rule_a.cwe_ids = vec!["CWE-79".to_string()];

    let mut rule_b = Rule::new(
        "rule-beta".to_string(),
        "Beta Rule".to_string(),
        "Beta description".to_string(),
    );
    rule_b.cwe_ids = vec!["CWE-79".to_string()];

    let matcher = RuleMatcher::new(vec![rule_a, rule_b]);
    let cwe_ids = vec!["CWE-79".to_string()];
    let result = matcher
        .resolve("unknown-xss-rule", Some(&cwe_ids), None)
        .expect("unknown rule with shared CWE should auto-register");

    let cwe_suggestions: Vec<_> = result
        .similar_rules
        .iter()
        .filter(|s| s.method == "cwe")
        .collect();
    assert_eq!(
        cwe_suggestions.len(),
        2,
        "both rules sharing CWE-79 should appear as suggestions"
    );
}

#[test]
fn normalize_underscore_and_mixed_case_for_alias() {
    let matcher = test_matcher();
    // "Unwrap_Usage" should normalize to "unwrap-usage" which is an alias
    let result = matcher
        .resolve("Unwrap_Usage", None, None)
        .expect("mixed-case underscore alias should normalize and resolve");

    assert_eq!(result.canonical_id, "unsafe-unwrap");
    assert_eq!(result.method, "alias");
    assert!((result.confidence - 1.0).abs() < f64::EPSILON);
}

#[test]
fn agent_namespace_stripped_then_alias() {
    let matcher = test_matcher();
    // Agent namespace + alias: "sonnet:unwrap-usage" → strip → "unwrap-usage" → alias
    let result = matcher
        .resolve("sonnet:unwrap-usage", None, None)
        .expect("agent namespace stripped then alias should resolve");

    assert_eq!(result.canonical_id, "unsafe-unwrap");
    assert_eq!(result.method, "alias");
}

#[test]
fn exists_returns_true_for_canonical() {
    let matcher = test_matcher();
    assert!(matcher.exists("unsafe-unwrap"));
    assert!(matcher.exists("sql-injection"));
}

#[test]
fn exists_returns_true_for_alias() {
    let matcher = test_matcher();
    assert!(matcher.exists("unwrap-usage"));
    assert!(matcher.exists("sqli"));
}

#[test]
fn exists_returns_false_for_unknown() {
    let matcher = test_matcher();
    assert!(!matcher.exists("nonexistent-rule"));
}

#[test]
fn get_rule_returns_rule_for_canonical() {
    let matcher = test_matcher();
    let rule = matcher.get_rule("unsafe-unwrap");
    assert!(rule.is_some());
    assert_eq!(
        rule.expect("get_rule should return rule for canonical ID")
            .name,
        "Unsafe Unwrap"
    );
}

#[test]
fn get_rule_returns_none_for_alias() {
    let matcher = test_matcher();
    // get_rule only works with canonical IDs, not aliases
    assert!(matcher.get_rule("unwrap-usage").is_none());
}

#[test]
fn check_id_namespace_accepts_valid_new_rule() {
    let matcher = test_matcher();
    // A completely new rule with new aliases should pass
    let result = matcher.check_id_namespace(
        "brand-new-rule",
        &["new-alias-1".to_string(), "new-alias-2".to_string()],
    );
    assert!(
        result.is_ok(),
        "valid new rule should pass namespace check: {:?}",
        result.err()
    );
}

#[test]
fn check_id_namespace_allows_own_aliases() {
    let matcher = test_matcher();
    // A rule re-declaring its own aliases should be OK
    let result = matcher.check_id_namespace(
        "unsafe-unwrap",
        &["unwrap-usage".to_string(), "no-unwrap".to_string()],
    );
    assert!(
        result.is_ok(),
        "rule re-declaring its own aliases should pass: {:?}",
        result.err()
    );
}
