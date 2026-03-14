//! Tests for the tally data model: state machine, finding types, serialization.

use tally::model::*;

// =============================================================================
// Task 2.6: All 24 valid state transitions (positive)
// =============================================================================

/// Helper to assert a transition is valid.
fn assert_valid(from: LifecycleState, to: LifecycleState) {
    assert!(from.can_transition_to(to), "{from} -> {to} should be valid");
}

#[test]
fn open_to_acknowledged() {
    assert_valid(LifecycleState::Open, LifecycleState::Acknowledged);
}

#[test]
fn open_to_in_progress() {
    assert_valid(LifecycleState::Open, LifecycleState::InProgress);
}

#[test]
fn open_to_false_positive() {
    assert_valid(LifecycleState::Open, LifecycleState::FalsePositive);
}

#[test]
fn open_to_deferred() {
    assert_valid(LifecycleState::Open, LifecycleState::Deferred);
}

#[test]
fn open_to_suppressed() {
    assert_valid(LifecycleState::Open, LifecycleState::Suppressed);
}

#[test]
fn acknowledged_to_in_progress() {
    assert_valid(LifecycleState::Acknowledged, LifecycleState::InProgress);
}

#[test]
fn acknowledged_to_false_positive() {
    assert_valid(LifecycleState::Acknowledged, LifecycleState::FalsePositive);
}

#[test]
fn acknowledged_to_wont_fix() {
    assert_valid(LifecycleState::Acknowledged, LifecycleState::WontFix);
}

#[test]
fn acknowledged_to_deferred() {
    assert_valid(LifecycleState::Acknowledged, LifecycleState::Deferred);
}

#[test]
fn in_progress_to_resolved() {
    assert_valid(LifecycleState::InProgress, LifecycleState::Resolved);
}

#[test]
fn in_progress_to_wont_fix() {
    assert_valid(LifecycleState::InProgress, LifecycleState::WontFix);
}

#[test]
fn in_progress_to_deferred() {
    assert_valid(LifecycleState::InProgress, LifecycleState::Deferred);
}

#[test]
fn resolved_to_reopened() {
    assert_valid(LifecycleState::Resolved, LifecycleState::Reopened);
}

#[test]
fn resolved_to_closed() {
    assert_valid(LifecycleState::Resolved, LifecycleState::Closed);
}

#[test]
fn false_positive_to_reopened() {
    assert_valid(LifecycleState::FalsePositive, LifecycleState::Reopened);
}

#[test]
fn false_positive_to_closed() {
    assert_valid(LifecycleState::FalsePositive, LifecycleState::Closed);
}

#[test]
fn wont_fix_to_reopened() {
    assert_valid(LifecycleState::WontFix, LifecycleState::Reopened);
}

#[test]
fn wont_fix_to_closed() {
    assert_valid(LifecycleState::WontFix, LifecycleState::Closed);
}

#[test]
fn deferred_to_open() {
    assert_valid(LifecycleState::Deferred, LifecycleState::Open);
}

#[test]
fn deferred_to_closed() {
    assert_valid(LifecycleState::Deferred, LifecycleState::Closed);
}

#[test]
fn suppressed_to_open() {
    assert_valid(LifecycleState::Suppressed, LifecycleState::Open);
}

#[test]
fn suppressed_to_closed() {
    assert_valid(LifecycleState::Suppressed, LifecycleState::Closed);
}

#[test]
fn reopened_to_acknowledged() {
    assert_valid(LifecycleState::Reopened, LifecycleState::Acknowledged);
}

#[test]
fn reopened_to_in_progress() {
    assert_valid(LifecycleState::Reopened, LifecycleState::InProgress);
}

// =============================================================================
// Task 2.7: Invalid transitions (negative)
// =============================================================================

/// Helper to assert a transition is invalid.
fn assert_invalid(from: LifecycleState, to: LifecycleState) {
    assert!(
        !from.can_transition_to(to),
        "{from} -> {to} should be INVALID"
    );
}

#[test]
fn open_to_closed_invalid() {
    assert_invalid(LifecycleState::Open, LifecycleState::Closed);
}

#[test]
fn open_to_resolved_invalid() {
    assert_invalid(LifecycleState::Open, LifecycleState::Resolved);
}

#[test]
fn open_to_wont_fix_invalid() {
    assert_invalid(LifecycleState::Open, LifecycleState::WontFix);
}

#[test]
fn open_to_reopened_invalid() {
    assert_invalid(LifecycleState::Open, LifecycleState::Reopened);
}

#[test]
fn closed_to_anything_invalid() {
    for state in LifecycleState::all() {
        assert_invalid(LifecycleState::Closed, *state);
    }
}

#[test]
fn in_progress_to_open_invalid() {
    assert_invalid(LifecycleState::InProgress, LifecycleState::Open);
}

#[test]
fn resolved_to_open_invalid() {
    assert_invalid(LifecycleState::Resolved, LifecycleState::Open);
}

#[test]
fn reopened_to_closed_invalid() {
    assert_invalid(LifecycleState::Reopened, LifecycleState::Closed);
}

#[test]
fn self_transition_invalid() {
    for state in LifecycleState::all() {
        assert_invalid(*state, *state);
    }
}

// =============================================================================
// Severity tests
// =============================================================================

#[test]
fn severity_short_prefix() {
    assert_eq!(Severity::Critical.short_prefix(), "C");
    assert_eq!(Severity::Important.short_prefix(), "I");
    assert_eq!(Severity::Suggestion.short_prefix(), "S");
    assert_eq!(Severity::TechDebt.short_prefix(), "TD");
}

#[test]
fn severity_sarif_mapping() {
    assert_eq!(Severity::Critical.to_sarif_level(), "error");
    assert_eq!(Severity::Important.to_sarif_level(), "warning");
    assert_eq!(Severity::Suggestion.to_sarif_level(), "note");
    assert_eq!(Severity::TechDebt.to_sarif_level(), "none");
}

#[test]
fn severity_from_str_valid() {
    assert_eq!(
        "critical".parse::<Severity>().expect("parse"),
        Severity::Critical
    );
    assert_eq!(
        "IMPORTANT".parse::<Severity>().expect("parse"),
        Severity::Important
    );
    assert_eq!(
        "tech-debt".parse::<Severity>().expect("parse"),
        Severity::TechDebt
    );
    assert_eq!(
        "tech_debt".parse::<Severity>().expect("parse"),
        Severity::TechDebt
    );
    assert_eq!(
        "techdebt".parse::<Severity>().expect("parse"),
        Severity::TechDebt
    );
}

#[test]
fn severity_from_str_invalid() {
    assert!("high".parse::<Severity>().is_err());
    assert!("low".parse::<Severity>().is_err());
    assert!("".parse::<Severity>().is_err());
}

#[test]
fn lifecycle_from_str_valid() {
    assert_eq!(
        "open".parse::<LifecycleState>().expect("parse"),
        LifecycleState::Open
    );
    assert_eq!(
        "in-progress".parse::<LifecycleState>().expect("parse"),
        LifecycleState::InProgress
    );
    assert_eq!(
        "in_progress".parse::<LifecycleState>().expect("parse"),
        LifecycleState::InProgress
    );
    assert_eq!(
        "false_positive".parse::<LifecycleState>().expect("parse"),
        LifecycleState::FalsePositive
    );
    assert_eq!(
        "WONT_FIX".parse::<LifecycleState>().expect("parse"),
        LifecycleState::WontFix
    );
}

#[test]
fn lifecycle_from_str_invalid() {
    assert!("pending".parse::<LifecycleState>().is_err());
    assert!("verified".parse::<LifecycleState>().is_err());
    assert!("".parse::<LifecycleState>().is_err());
}

// =============================================================================
// Serialization round-trip
// =============================================================================

#[test]
fn finding_serialization_roundtrip() {
    let finding = Finding {
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:abc123".to_string(),
        rule_id: "unsafe-unwrap".to_string(),
        locations: vec![Location {
            file_path: "src/main.rs".to_string(),
            line_start: 42,
            line_end: 42,
            role: LocationRole::Primary,
            message: None,
        }],
        severity: Severity::Critical,
        category: "correctness".to_string(),
        tags: vec!["panic-safety".to_string()],
        title: "unwrap on Option".to_string(),
        description: "Line 42 calls .unwrap() without error handling.".to_string(),
        suggested_fix: Some("Use ? or unwrap_or_default()".to_string()),
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "claude-code".to_string(),
            session_id: "sess_abc123".to_string(),
            detected_at: chrono::Utc::now(),
            session_short_id: Some("C1".to_string()),
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        repo_id: "1898andCo/tally".to_string(),
        branch: Some("develop".to_string()),
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
    };

    let json = serde_json::to_string_pretty(&finding).expect("serialize");
    let deserialized: Finding = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.uuid, finding.uuid);
    assert_eq!(deserialized.rule_id, finding.rule_id);
    assert_eq!(deserialized.severity, finding.severity);
    assert_eq!(deserialized.status, finding.status);
    assert_eq!(deserialized.locations.len(), 1);
    assert_eq!(deserialized.locations[0].file_path, "src/main.rs");
}

#[test]
fn severity_serialization_roundtrip() {
    let json = serde_json::to_string(&Severity::TechDebt).expect("serialize");
    assert_eq!(json, "\"tech_debt\"");
    let deserialized: Severity = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized, Severity::TechDebt);
}

#[test]
fn lifecycle_serialization_roundtrip() {
    let json = serde_json::to_string(&LifecycleState::FalsePositive).expect("serialize");
    assert_eq!(json, "\"false_positive\"");
    let deserialized: LifecycleState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized, LifecycleState::FalsePositive);
}
