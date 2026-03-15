//! Tests for the rule registry data model: `Rule`, `RuleStatus`, serialization.

use tally_ng::registry::{Rule, RuleExample, RuleScope, RuleStatus};

// =============================================================================
// Positive: Rule::new() helper
// =============================================================================

#[test]
fn rule_new_sets_required_fields() {
    let rule = Rule::new(
        "unsafe-unwrap".to_string(),
        "Unsafe Unwrap".to_string(),
        "Detects unwrap() calls in production code".to_string(),
    );
    assert_eq!(rule.id, "unsafe-unwrap");
    assert_eq!(rule.name, "Unsafe Unwrap");
    assert_eq!(
        rule.description,
        "Detects unwrap() calls in production code"
    );
}

#[test]
fn rule_new_defaults_status_to_active() {
    let rule = Rule::new("ab".into(), "n".into(), "d".into());
    assert_eq!(rule.status, RuleStatus::Active);
}

#[test]
fn rule_new_defaults_finding_count_to_zero() {
    let rule = Rule::new("ab".into(), "n".into(), "d".into());
    assert_eq!(rule.finding_count, 0);
}

#[test]
fn rule_new_defaults_collections_empty() {
    let rule = Rule::new("ab".into(), "n".into(), "d".into());
    assert!(rule.tags.is_empty());
    assert!(rule.cwe_ids.is_empty());
    assert!(rule.aliases.is_empty());
    assert!(rule.examples.is_empty());
    assert!(rule.references.is_empty());
    assert!(rule.related_rules.is_empty());
}

#[test]
fn rule_new_defaults_optional_fields_to_none() {
    let rule = Rule::new("ab".into(), "n".into(), "d".into());
    assert!(rule.scope.is_none());
    assert!(rule.suggested_fix_pattern.is_none());
    assert!(rule.embedding.is_none());
    assert!(rule.embedding_model.is_none());
}

#[test]
fn rule_new_sets_timestamps() {
    let before = chrono::Utc::now();
    let rule = Rule::new("ab".into(), "n".into(), "d".into());
    let after = chrono::Utc::now();
    assert!(rule.created_at >= before && rule.created_at <= after);
    assert!(rule.updated_at >= before && rule.updated_at <= after);
}

// =============================================================================
// Positive: Serialization roundtrip (all fields populated)
// =============================================================================

#[test]
fn serialization_roundtrip_all_fields() {
    let rule = Rule {
        id: "unsafe-unwrap".to_string(),
        name: "Unsafe Unwrap".to_string(),
        description: "Detects unwrap() in production code".to_string(),
        category: "safety".to_string(),
        severity_hint: "high".to_string(),
        tags: vec!["rust".to_string(), "safety".to_string()],
        cwe_ids: vec!["CWE-252".to_string()],
        aliases: vec!["no-unwrap".to_string()],
        scope: Some(RuleScope {
            include: vec!["**/*.rs".to_string()],
            exclude: vec!["tests/**".to_string()],
        }),
        examples: vec![RuleExample {
            example_type: "bad".to_string(),
            language: "rust".to_string(),
            code: "let v = opt.unwrap();".to_string(),
            explanation: "Panics on None".to_string(),
        }],
        suggested_fix_pattern: Some("Replace with ? or expect()".to_string()),
        references: vec!["https://example.com/safety".to_string()],
        related_rules: vec!["unsafe-expect".to_string()],
        created_by: "dclaude".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        status: RuleStatus::Experimental,
        finding_count: 42,
        embedding: Some(vec![0.1, 0.2, 0.3]),
        embedding_model: Some("text-embedding-3-small".to_string()),
    };

    let json = serde_json::to_string_pretty(&rule).expect("serialize should succeed");
    let deserialized: Rule = serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(deserialized.id, rule.id);
    assert_eq!(deserialized.name, rule.name);
    assert_eq!(deserialized.description, rule.description);
    assert_eq!(deserialized.category, rule.category);
    assert_eq!(deserialized.severity_hint, rule.severity_hint);
    assert_eq!(deserialized.tags, rule.tags);
    assert_eq!(deserialized.cwe_ids, rule.cwe_ids);
    assert_eq!(deserialized.aliases, rule.aliases);
    assert_eq!(deserialized.status, rule.status);
    assert_eq!(deserialized.finding_count, rule.finding_count);
    assert_eq!(deserialized.created_by, rule.created_by);
    assert_eq!(deserialized.embedding, rule.embedding);
    assert_eq!(deserialized.embedding_model, rule.embedding_model);
    assert_eq!(
        deserialized.suggested_fix_pattern,
        rule.suggested_fix_pattern
    );
    assert_eq!(deserialized.references, rule.references);
    assert_eq!(deserialized.related_rules, rule.related_rules);

    // Scope
    let scope = deserialized.scope.expect("scope should be present");
    assert_eq!(scope.include, vec!["**/*.rs"]);
    assert_eq!(scope.exclude, vec!["tests/**"]);

    // Examples
    assert_eq!(deserialized.examples.len(), 1);
    assert_eq!(deserialized.examples[0].example_type, "bad");
    assert_eq!(deserialized.examples[0].language, "rust");
}

// =============================================================================
// Positive: Deserialization with defaults (minimal JSON)
// =============================================================================

#[test]
fn deserialization_minimal_json_uses_defaults() {
    let json = r#"{
        "id": "check-sql",
        "name": "SQL Check",
        "description": "Checks SQL queries"
    }"#;

    let rule: Rule = serde_json::from_str(json).expect("minimal JSON should deserialize");

    assert_eq!(rule.id, "check-sql");
    assert_eq!(rule.name, "SQL Check");
    assert_eq!(rule.description, "Checks SQL queries");
    assert_eq!(rule.category, "");
    assert_eq!(rule.severity_hint, "");
    assert!(rule.tags.is_empty());
    assert!(rule.cwe_ids.is_empty());
    assert!(rule.aliases.is_empty());
    assert!(rule.scope.is_none());
    assert!(rule.examples.is_empty());
    assert!(rule.suggested_fix_pattern.is_none());
    assert!(rule.references.is_empty());
    assert!(rule.related_rules.is_empty());
    assert_eq!(rule.created_by, "");
    assert_eq!(rule.status, RuleStatus::Active);
    assert_eq!(rule.finding_count, 0);
    assert!(rule.embedding.is_none());
    assert!(rule.embedding_model.is_none());
}

// =============================================================================
// Positive: RuleStatus variants
// =============================================================================

#[test]
fn rule_status_from_str_active() {
    assert_eq!(
        "active"
            .parse::<RuleStatus>()
            .expect("parse 'active' as RuleStatus"),
        RuleStatus::Active
    );
}

#[test]
fn rule_status_from_str_deprecated() {
    assert_eq!(
        "deprecated"
            .parse::<RuleStatus>()
            .expect("parse 'deprecated' as RuleStatus"),
        RuleStatus::Deprecated
    );
}

#[test]
fn rule_status_from_str_experimental() {
    assert_eq!(
        "experimental"
            .parse::<RuleStatus>()
            .expect("parse 'experimental' as RuleStatus"),
        RuleStatus::Experimental
    );
}

#[test]
fn rule_status_from_str_case_insensitive() {
    assert_eq!(
        "ACTIVE"
            .parse::<RuleStatus>()
            .expect("parse 'ACTIVE' case-insensitively"),
        RuleStatus::Active
    );
    assert_eq!(
        "Deprecated"
            .parse::<RuleStatus>()
            .expect("parse 'Deprecated' case-insensitively"),
        RuleStatus::Deprecated
    );
    assert_eq!(
        "EXPERIMENTAL"
            .parse::<RuleStatus>()
            .expect("parse 'EXPERIMENTAL' case-insensitively"),
        RuleStatus::Experimental
    );
}

#[test]
fn rule_status_display() {
    assert_eq!(RuleStatus::Active.to_string(), "active");
    assert_eq!(RuleStatus::Deprecated.to_string(), "deprecated");
    assert_eq!(RuleStatus::Experimental.to_string(), "experimental");
}

#[test]
fn rule_status_promotion_rank_ordering() {
    assert!(RuleStatus::Active.promotion_rank() > RuleStatus::Experimental.promotion_rank());
    assert!(RuleStatus::Experimental.promotion_rank() > RuleStatus::Deprecated.promotion_rank());
}

// =============================================================================
// Negative: RuleStatus::from_str rejects invalid values
// =============================================================================

#[test]
fn rule_status_from_str_rejects_invalid() {
    let result = "invalid".parse::<RuleStatus>();
    assert!(result.is_err());
    let err = result.expect_err("parsing 'invalid' should fail");
    assert!(
        err.contains("invalid rule status"),
        "error should mention 'invalid rule status', got: {err}"
    );
}

#[test]
fn rule_status_from_str_rejects_empty() {
    assert!("".parse::<RuleStatus>().is_err());
}

#[test]
fn rule_status_from_str_rejects_typo() {
    assert!("actve".parse::<RuleStatus>().is_err());
}

// =============================================================================
// Negative: Deserialization with unknown status
// =============================================================================

#[test]
fn deserialization_unknown_status_fails() {
    let json = r#"{
        "id": "test-rule",
        "name": "Test",
        "description": "Test",
        "status": "unknown_status"
    }"#;
    let result = serde_json::from_str::<Rule>(json);
    assert!(
        result.is_err(),
        "unknown status should fail deserialization"
    );
}

// =============================================================================
// Boundary: Empty strings for name/description
// =============================================================================

#[test]
fn rule_new_allows_empty_name() {
    let rule = Rule::new("ab".into(), String::new(), "desc".into());
    assert_eq!(rule.name, "");
}

#[test]
fn rule_new_allows_empty_description() {
    let rule = Rule::new("ab".into(), "name".into(), String::new());
    assert_eq!(rule.description, "");
}

// =============================================================================
// Boundary: Max-length ID (64 chars)
// =============================================================================

#[test]
fn rule_new_accepts_64_char_id() {
    let id = "a".repeat(64);
    let rule = Rule::new(id.clone(), "name".into(), "desc".into());
    assert_eq!(rule.id, id);
}

#[test]
fn serialization_roundtrip_64_char_id() {
    let id = "a".repeat(64);
    let rule = Rule::new(id.clone(), "name".into(), "desc".into());
    let json = serde_json::to_string(&rule).expect("serialize");
    let deser: Rule = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deser.id, id);
}

// =============================================================================
// RuleStatus serde roundtrip
// =============================================================================

#[test]
fn rule_status_serde_roundtrip() {
    for status in [
        RuleStatus::Active,
        RuleStatus::Deprecated,
        RuleStatus::Experimental,
    ] {
        let json = serde_json::to_string(&status).expect("serialize status");
        let deser: RuleStatus = serde_json::from_str(&json).expect("deserialize status");
        assert_eq!(deser, status, "roundtrip failed for {status}");
    }
}

#[test]
fn rule_status_serializes_as_snake_case() {
    assert_eq!(
        serde_json::to_string(&RuleStatus::Active).expect("serialize Active"),
        "\"active\""
    );
    assert_eq!(
        serde_json::to_string(&RuleStatus::Deprecated).expect("serialize Deprecated"),
        "\"deprecated\""
    );
    assert_eq!(
        serde_json::to_string(&RuleStatus::Experimental).expect("serialize Experimental"),
        "\"experimental\""
    );
}

// =============================================================================
// Skip-serializing-if behavior
// =============================================================================

#[test]
fn empty_collections_omitted_from_json() {
    let rule = Rule::new("ab".into(), "n".into(), "d".into());
    let json = serde_json::to_string(&rule).expect("serialize");

    // These fields should be absent when empty/None
    assert!(
        !json.contains("\"tags\""),
        "empty tags should be omitted from JSON"
    );
    assert!(
        !json.contains("\"cwe_ids\""),
        "empty cwe_ids should be omitted from JSON"
    );
    assert!(
        !json.contains("\"aliases\""),
        "empty aliases should be omitted from JSON"
    );
    assert!(
        !json.contains("\"scope\""),
        "None scope should be omitted from JSON"
    );
    assert!(
        !json.contains("\"embedding\""),
        "None embedding should be omitted from JSON"
    );
}
