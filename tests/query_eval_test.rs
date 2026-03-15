//! Evaluator tests for `TallyQL` expressions against `Finding` objects.

use chrono::{TimeDelta, Utc};
use tally_ng::model::*;
use tally_ng::query::ast::{CompareOp, FilterExpr, StringOp, Value};
use tally_ng::query::evaluate;

// =============================================================================
// Test fixture: sample finding from story spec
// =============================================================================

fn fixture_finding() -> Finding {
    let created = Utc::now() - TimeDelta::try_days(5).expect("valid");
    let updated = Utc::now() - TimeDelta::try_days(3).expect("valid");
    Finding {
        schema_version: "1.1.0".to_string(),
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:test".to_string(),
        rule_id: "unsafe-unwrap".to_string(),
        locations: vec![
            Location {
                file_path: "src/api/handler.rs".to_string(),
                line_start: 42,
                line_end: 42,
                role: LocationRole::Primary,
                message: None,
            },
            Location {
                file_path: "src/api/routes.rs".to_string(),
                line_start: 15,
                line_end: 15,
                role: LocationRole::Secondary,
                message: None,
            },
        ],
        severity: Severity::Critical,
        category: "safety".to_string(),
        tags: vec![
            "story:5.1".to_string(),
            "security".to_string(),
            "check-drift".to_string(),
        ],
        title: "unwrap on Option in request handler".to_string(),
        description: "Using .unwrap() on user input that may be None".to_string(),
        suggested_fix: Some("Use .ok_or() with a proper error type".to_string()),
        evidence: Some("handler.rs:42: let val = input.get(\"key\").unwrap();".to_string()),
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "dclaude".to_string(),
            session_id: "sess-001".to_string(),
            detected_at: created,
            session_short_id: None,
        }],
        created_at: created,
        updated_at: updated,
        repo_id: String::new(),
        branch: None,
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
        notes: vec![],
        edit_history: vec![],
    }
}

/// Create a finding with None optional fields for edge case testing.
fn finding_no_optionals() -> Finding {
    let mut f = fixture_finding();
    f.uuid = uuid::Uuid::now_v7();
    f.suggested_fix = None;
    f.evidence = None;
    f.tags = vec![];
    f.discovered_by = vec![];
    f.category = String::new();
    f
}

// =============================================================================
// Story fixture assertions (from story spec table)
// =============================================================================

#[test]
fn severity_equals_critical_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::Eq,
        value: Value::Enum("critical".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn severity_equals_important_is_false() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::Eq,
        value: Value::Enum("important".into()),
    };
    assert!(!evaluate(&expr, &f));
}

#[test]
fn severity_greater_than_important_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::Gt,
        value: Value::Enum("important".into()),
    };
    assert!(evaluate(&expr, &f), "critical > important should be true");
}

#[test]
fn severity_less_than_important_is_false() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::Lt,
        value: Value::Enum("important".into()),
    };
    assert!(!evaluate(&expr, &f));
}

#[test]
fn status_equals_open_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "status".into(),
        op: CompareOp::Eq,
        value: Value::Enum("open".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn file_contains_api_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::StringMatch {
        field: "file".into(),
        op: StringOp::Contains,
        value: "api".into(),
    };
    assert!(evaluate(&expr, &f), "should match either location");
}

#[test]
fn file_contains_tests_is_false() {
    let f = fixture_finding();
    let expr = FilterExpr::StringMatch {
        field: "file".into(),
        op: StringOp::Contains,
        value: "tests".into(),
    };
    assert!(!evaluate(&expr, &f));
}

#[test]
fn rule_equals_unsafe_unwrap_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "rule".into(),
        op: CompareOp::Eq,
        value: Value::String("unsafe-unwrap".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn tag_equals_security_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "tag".into(),
        op: CompareOp::Eq,
        value: Value::Enum("security".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn tag_contains_story_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::StringMatch {
        field: "tag".into(),
        op: StringOp::Contains,
        value: "story".into(),
    };
    assert!(evaluate(&expr, &f), "should match 'story:5.1'");
}

#[test]
fn has_suggested_fix_is_true() {
    let f = fixture_finding();
    assert!(evaluate(&FilterExpr::Has("suggested_fix".into()), &f));
}

#[test]
fn missing_suggested_fix_is_false() {
    let f = fixture_finding();
    assert!(!evaluate(&FilterExpr::Missing("suggested_fix".into()), &f));
}

#[test]
fn missing_evidence_is_false() {
    let f = fixture_finding();
    assert!(!evaluate(&FilterExpr::Missing("evidence".into()), &f));
}

#[test]
fn agent_equals_dclaude_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "agent".into(),
        op: CompareOp::Eq,
        value: Value::Enum("dclaude".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn agent_equals_zclaude_is_false() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "agent".into(),
        op: CompareOp::Eq,
        value: Value::Enum("zclaude".into()),
    };
    assert!(!evaluate(&expr, &f));
}

#[test]
fn title_contains_unwrap_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::StringMatch {
        field: "title".into(),
        op: StringOp::Contains,
        value: "unwrap".into(),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn not_status_closed_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Not(Box::new(FilterExpr::Comparison {
        field: "status".into(),
        op: CompareOp::Eq,
        value: Value::Enum("closed".into()),
    }));
    assert!(evaluate(&expr, &f));
}

#[test]
fn severity_critical_and_file_contains_api_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::And(
        Box::new(FilterExpr::Comparison {
            field: "severity".into(),
            op: CompareOp::Eq,
            value: Value::Enum("critical".into()),
        }),
        Box::new(FilterExpr::StringMatch {
            field: "file".into(),
            op: StringOp::Contains,
            value: "api".into(),
        }),
    );
    assert!(evaluate(&expr, &f));
}

#[test]
fn severity_suggestion_or_status_closed_is_false() {
    let f = fixture_finding();
    let expr = FilterExpr::Or(
        Box::new(FilterExpr::Comparison {
            field: "severity".into(),
            op: CompareOp::Eq,
            value: Value::Enum("suggestion".into()),
        }),
        Box::new(FilterExpr::Comparison {
            field: "status".into(),
            op: CompareOp::Eq,
            value: Value::Enum("closed".into()),
        }),
    );
    assert!(!evaluate(&expr, &f));
}

// =============================================================================
// Date comparison with absolute date string
// =============================================================================

#[test]
fn created_at_after_past_date_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "created_at".into(),
        op: CompareOp::Gt,
        value: Value::String("2020-01-01".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn created_at_before_future_date_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "created_at".into(),
        op: CompareOp::Lt,
        value: Value::String("2099-12-31".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn created_at_with_rfc3339() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "created_at".into(),
        op: CompareOp::Gt,
        value: Value::String("2020-01-01T00:00:00Z".into()),
    };
    assert!(evaluate(&expr, &f));
}

// =============================================================================
// Date comparison with relative duration
// =============================================================================

#[test]
fn created_at_within_last_30_days() {
    let f = fixture_finding(); // created 5 days ago
    let expr = FilterExpr::Comparison {
        field: "created_at".into(),
        op: CompareOp::Gt,
        value: Value::Duration(std::time::Duration::from_secs(30 * 86400)),
    };
    // created 5 days ago > (now - 30 days) → true
    assert!(evaluate(&expr, &f));
}

#[test]
fn created_at_not_within_last_1_day() {
    let f = fixture_finding(); // created 5 days ago
    let expr = FilterExpr::Comparison {
        field: "created_at".into(),
        op: CompareOp::Gt,
        value: Value::Duration(std::time::Duration::from_secs(86400)),
    };
    // created 5 days ago > (now - 1 day) → false
    assert!(!evaluate(&expr, &f));
}

// =============================================================================
// IN list evaluation
// =============================================================================

#[test]
fn severity_in_list_matches() {
    let f = fixture_finding();
    let expr = FilterExpr::InList {
        field: "severity".into(),
        values: vec![
            Value::Enum("critical".into()),
            Value::Enum("important".into()),
        ],
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn severity_in_list_no_match() {
    let f = fixture_finding();
    let expr = FilterExpr::InList {
        field: "severity".into(),
        values: vec![
            Value::Enum("suggestion".into()),
            Value::Enum("tech_debt".into()),
        ],
    };
    assert!(!evaluate(&expr, &f));
}

#[test]
fn status_in_list_matches() {
    let f = fixture_finding();
    let expr = FilterExpr::InList {
        field: "status".into(),
        values: vec![
            Value::Enum("open".into()),
            Value::Enum("acknowledged".into()),
        ],
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn tag_in_list_matches() {
    let f = fixture_finding();
    let expr = FilterExpr::InList {
        field: "tag".into(),
        values: vec![
            Value::String("security".into()),
            Value::String("performance".into()),
        ],
    };
    assert!(evaluate(&expr, &f));
}

// =============================================================================
// Edge cases: None/empty fields
// =============================================================================

#[test]
fn has_suggested_fix_when_none_is_false() {
    let f = finding_no_optionals();
    assert!(!evaluate(&FilterExpr::Has("suggested_fix".into()), &f));
}

#[test]
fn missing_suggested_fix_when_none_is_true() {
    let f = finding_no_optionals();
    assert!(evaluate(&FilterExpr::Missing("suggested_fix".into()), &f));
}

#[test]
fn has_tag_when_empty_is_false() {
    let f = finding_no_optionals();
    assert!(!evaluate(&FilterExpr::Has("tag".into()), &f));
}

#[test]
fn missing_tag_when_empty_is_true() {
    let f = finding_no_optionals();
    assert!(evaluate(&FilterExpr::Missing("tag".into()), &f));
}

#[test]
fn has_agent_when_empty_is_false() {
    let f = finding_no_optionals();
    assert!(!evaluate(&FilterExpr::Has("agent".into()), &f));
}

#[test]
fn missing_agent_when_empty_is_true() {
    let f = finding_no_optionals();
    assert!(evaluate(&FilterExpr::Missing("agent".into()), &f));
}

#[test]
fn string_match_on_none_suggested_fix_is_false() {
    let f = finding_no_optionals();
    let expr = FilterExpr::StringMatch {
        field: "suggested_fix".into(),
        op: StringOp::Contains,
        value: "anything".into(),
    };
    assert!(!evaluate(&expr, &f));
}

#[test]
fn comparison_on_none_evidence_ne_is_true() {
    let f = finding_no_optionals();
    let expr = FilterExpr::Comparison {
        field: "evidence".into(),
        op: CompareOp::Ne,
        value: Value::String("anything".into()),
    };
    assert!(evaluate(&expr, &f), "None != anything should be true");
}

// =============================================================================
// String operations: STARTSWITH, ENDSWITH
// =============================================================================

#[test]
fn file_startswith_src() {
    let f = fixture_finding();
    let expr = FilterExpr::StringMatch {
        field: "file".into(),
        op: StringOp::StartsWith,
        value: "src/api".into(),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn file_endswith_rs() {
    let f = fixture_finding();
    let expr = FilterExpr::StringMatch {
        field: "file".into(),
        op: StringOp::EndsWith,
        value: ".rs".into(),
    };
    assert!(evaluate(&expr, &f));
}

// =============================================================================
// Case insensitivity in comparisons
// =============================================================================

#[test]
fn severity_comparison_is_case_insensitive() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::Eq,
        value: Value::Enum("CRITICAL".into()),
    };
    assert!(evaluate(&expr, &f));
}

#[test]
fn string_match_is_case_insensitive() {
    let f = fixture_finding();
    let expr = FilterExpr::StringMatch {
        field: "title".into(),
        op: StringOp::Contains,
        value: "UNWRAP".into(),
    };
    assert!(evaluate(&expr, &f));
}

// =============================================================================
// Severity ordering edge cases
// =============================================================================

#[test]
fn severity_gte_critical_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::GtEq,
        value: Value::Enum("critical".into()),
    };
    assert!(evaluate(&expr, &f), "critical >= critical");
}

#[test]
fn severity_lte_critical_is_true() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::LtEq,
        value: Value::Enum("critical".into()),
    };
    assert!(evaluate(&expr, &f), "critical <= critical");
}

// =============================================================================
// HAS/MISSING for always-present fields
// =============================================================================

#[test]
fn has_severity_always_true() {
    let f = fixture_finding();
    assert!(evaluate(&FilterExpr::Has("severity".into()), &f));
}

#[test]
fn has_created_at_always_true() {
    let f = fixture_finding();
    assert!(evaluate(&FilterExpr::Has("created_at".into()), &f));
}

// =============================================================================
// Invalid enum values
// =============================================================================

#[test]
fn invalid_severity_value_returns_false() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "severity".into(),
        op: CompareOp::Eq,
        value: Value::Enum("ultra".into()),
    };
    assert!(!evaluate(&expr, &f));
}

#[test]
fn invalid_status_value_returns_false() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "status".into(),
        op: CompareOp::Eq,
        value: Value::Enum("running".into()),
    };
    assert!(!evaluate(&expr, &f));
}

// =============================================================================
// Invalid date string returns false
// =============================================================================

#[test]
fn invalid_date_string_returns_false() {
    let f = fixture_finding();
    let expr = FilterExpr::Comparison {
        field: "created_at".into(),
        op: CompareOp::Gt,
        value: Value::String("not-a-date".into()),
    };
    assert!(!evaluate(&expr, &f));
}
