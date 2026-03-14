//! Tests for the tally data model: state machine, finding types, serialization.

use tally_ng::model::*;

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
        schema_version: "1.0.0".to_string(),
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
        notes: vec![],
        edit_history: vec![],
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

// =============================================================================
// Finding: additional positive tests
// =============================================================================

#[test]
fn finding_with_empty_locations() {
    let finding = Finding {
        schema_version: "1.0.0".to_string(),
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:def456".to_string(),
        rule_id: "missing-test".to_string(),
        locations: vec![],
        severity: Severity::Suggestion,
        category: "coverage".to_string(),
        tags: vec![],
        title: "No test coverage".to_string(),
        description: "Module has no tests.".to_string(),
        suggested_fix: None,
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "claude-code".to_string(),
            session_id: "sess_empty".to_string(),
            detected_at: chrono::Utc::now(),
            session_short_id: None,
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        repo_id: "1898andCo/tally".to_string(),
        branch: None,
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
        notes: vec![],
        edit_history: vec![],
    };

    let json = serde_json::to_string_pretty(&finding).expect("serialize");
    let deserialized: Finding = serde_json::from_str(&json).expect("deserialize");
    assert!(deserialized.locations.is_empty());
}

#[test]
fn finding_with_all_fields_populated() {
    let now = chrono::Utc::now();
    let related_uuid = uuid::Uuid::now_v7();
    let finding = Finding {
        schema_version: "1.0.0".to_string(),
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:all_fields".to_string(),
        rule_id: "sql-injection".to_string(),
        locations: vec![
            Location {
                file_path: "src/db.rs".to_string(),
                line_start: 10,
                line_end: 15,
                role: LocationRole::Primary,
                message: Some("SQL query built from user input".to_string()),
            },
            Location {
                file_path: "src/handler.rs".to_string(),
                line_start: 42,
                line_end: 42,
                role: LocationRole::Secondary,
                message: Some("User input originates here".to_string()),
            },
        ],
        severity: Severity::Critical,
        category: "security".to_string(),
        tags: vec!["owasp-top10".to_string(), "injection".to_string()],
        title: "SQL injection vulnerability".to_string(),
        description: "User input is concatenated into SQL query.".to_string(),
        suggested_fix: Some("Use parameterized queries.".to_string()),
        evidence: Some(
            "Line 12: format!(\"SELECT * FROM users WHERE id = {}\", user_input)".to_string(),
        ),
        status: LifecycleState::Acknowledged,
        state_history: vec![StateTransition {
            from: LifecycleState::Open,
            to: LifecycleState::Acknowledged,
            timestamp: now,
            agent_id: "claude-code".to_string(),
            reason: Some("Confirmed by developer".to_string()),
            commit_sha: Some("abc123".to_string()),
        }],
        discovered_by: vec![AgentRecord {
            agent_id: "claude-code".to_string(),
            session_id: "sess_full".to_string(),
            detected_at: now,
            session_short_id: Some("C1".to_string()),
        }],
        created_at: now,
        updated_at: now,
        repo_id: "1898andCo/tally".to_string(),
        branch: Some("feature/auth".to_string()),
        pr_number: Some(42),
        commit_sha: Some("deadbeef".to_string()),
        relationships: vec![FindingRelationship {
            related_finding_id: related_uuid,
            relationship_type: RelationshipType::Causes,
            reason: Some("Root cause of data leak".to_string()),
            created_at: now,
        }],
        suppression: Some(Suppression {
            suppressed_at: now,
            reason: "False positive in test code".to_string(),
            expires_at: Some(now + chrono::Duration::days(30)),
            suppression_type: SuppressionType::InlineComment {
                pattern: "tally:suppress sql-injection".to_string(),
            },
        }),
        notes: vec![],
        edit_history: vec![],
    };

    let json = serde_json::to_string_pretty(&finding).expect("serialize");
    let deserialized: Finding = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.uuid, finding.uuid);
    assert_eq!(deserialized.locations.len(), 2);
    assert_eq!(deserialized.tags.len(), 2);
    assert_eq!(deserialized.state_history.len(), 1);
    assert_eq!(deserialized.relationships.len(), 1);
    assert!(deserialized.suppression.is_some());
    assert!(deserialized.evidence.is_some());
    assert_eq!(deserialized.pr_number, Some(42));
    assert_eq!(deserialized.commit_sha.as_deref(), Some("deadbeef"));
}

// =============================================================================
// Finding: negative deserialization tests
// =============================================================================

#[test]
fn finding_deserialize_missing_fields_uses_defaults() {
    // JSON missing "uuid" and other fields — should deserialize with defaults
    let json = r#"{
        "content_fingerprint": "sha256:abc",
        "rule_id": "test",
        "locations": [],
        "severity": "critical",
        "category": "test",
        "title": "test",
        "description": "test",
        "status": "open",
        "discovered_by": [],
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z",
        "repo_id": "test/repo"
    }"#;
    let result = serde_json::from_str::<Finding>(json);
    assert!(
        result.is_ok(),
        "missing uuid should deserialize with nil default"
    );
    let finding = result.expect("should deserialize");
    assert!(finding.uuid.is_nil(), "default uuid should be nil");
    assert_eq!(
        finding.schema_version, "1.0.0",
        "schema_version should default to 1.0.0"
    );
}

#[test]
fn finding_deserialize_invalid_severity() {
    let json = r#"{
        "uuid": "01938a6e-7c3b-7000-8000-000000000001",
        "content_fingerprint": "sha256:abc",
        "rule_id": "test",
        "locations": [],
        "severity": "ultra",
        "category": "test",
        "title": "test",
        "description": "test",
        "status": "open",
        "discovered_by": [],
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z",
        "repo_id": "test/repo"
    }"#;
    let result = serde_json::from_str::<Finding>(json);
    assert!(
        result.is_err(),
        "invalid severity 'ultra' should fail deserialization"
    );
}

#[test]
fn finding_deserialize_invalid_status() {
    let json = r#"{
        "uuid": "01938a6e-7c3b-7000-8000-000000000001",
        "content_fingerprint": "sha256:abc",
        "rule_id": "test",
        "locations": [],
        "severity": "critical",
        "category": "test",
        "title": "test",
        "description": "test",
        "status": "deleted",
        "discovered_by": [],
        "created_at": "2025-01-01T00:00:00Z",
        "updated_at": "2025-01-01T00:00:00Z",
        "repo_id": "test/repo"
    }"#;
    let result = serde_json::from_str::<Finding>(json);
    assert!(
        result.is_err(),
        "invalid status 'deleted' should fail deserialization"
    );
}

// =============================================================================
// LocationRole tests
// =============================================================================

#[test]
fn location_role_display_primary() {
    let json = serde_json::to_string(&LocationRole::Primary).expect("serialize");
    assert_eq!(json, "\"primary\"");
}

#[test]
fn location_role_display_secondary() {
    let json = serde_json::to_string(&LocationRole::Secondary).expect("serialize");
    assert_eq!(json, "\"secondary\"");
}

#[test]
fn location_role_display_context() {
    let json = serde_json::to_string(&LocationRole::Context).expect("serialize");
    assert_eq!(json, "\"context\"");
}

#[test]
fn location_role_serde_roundtrip() {
    for role in [
        LocationRole::Primary,
        LocationRole::Secondary,
        LocationRole::Context,
    ] {
        let json = serde_json::to_string(&role).expect("serialize");
        let deserialized: LocationRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, role, "roundtrip failed for {json}");
    }
}

// =============================================================================
// RelationshipType tests
// =============================================================================

#[test]
fn relationship_type_display_all() {
    assert_eq!(RelationshipType::DuplicateOf.to_string(), "duplicate_of");
    assert_eq!(RelationshipType::Blocks.to_string(), "blocks");
    assert_eq!(RelationshipType::RelatedTo.to_string(), "related_to");
    assert_eq!(RelationshipType::Causes.to_string(), "causes");
    assert_eq!(
        RelationshipType::DiscoveredWhileFixing.to_string(),
        "discovered_while_fixing"
    );
    assert_eq!(RelationshipType::Supersedes.to_string(), "supersedes");
}

#[test]
fn relationship_type_from_str_all() {
    assert_eq!(
        "duplicate_of".parse::<RelationshipType>().expect("parse"),
        RelationshipType::DuplicateOf
    );
    assert_eq!(
        "blocks".parse::<RelationshipType>().expect("parse"),
        RelationshipType::Blocks
    );
    assert_eq!(
        "related_to".parse::<RelationshipType>().expect("parse"),
        RelationshipType::RelatedTo
    );
    assert_eq!(
        "causes".parse::<RelationshipType>().expect("parse"),
        RelationshipType::Causes
    );
    assert_eq!(
        "discovered_while_fixing"
            .parse::<RelationshipType>()
            .expect("parse"),
        RelationshipType::DiscoveredWhileFixing
    );
    assert_eq!(
        "supersedes".parse::<RelationshipType>().expect("parse"),
        RelationshipType::Supersedes
    );
}

#[test]
fn relationship_type_from_str_with_dashes() {
    assert_eq!(
        "discovered-while-fixing"
            .parse::<RelationshipType>()
            .expect("parse"),
        RelationshipType::DiscoveredWhileFixing
    );
}

#[test]
fn relationship_type_from_str_shorthand() {
    assert_eq!(
        "duplicate".parse::<RelationshipType>().expect("parse"),
        RelationshipType::DuplicateOf
    );
    assert_eq!(
        "related".parse::<RelationshipType>().expect("parse"),
        RelationshipType::RelatedTo
    );
}

#[test]
fn relationship_type_from_str_invalid() {
    assert!(
        "depends".parse::<RelationshipType>().is_err(),
        "'depends' should not parse"
    );
    assert!(
        "".parse::<RelationshipType>().is_err(),
        "empty should not parse"
    );
    assert!(
        "orphan".parse::<RelationshipType>().is_err(),
        "'orphan' should not parse"
    );
}

#[test]
fn relationship_type_serde_roundtrip() {
    let all_types = [
        RelationshipType::DuplicateOf,
        RelationshipType::Blocks,
        RelationshipType::RelatedTo,
        RelationshipType::Causes,
        RelationshipType::DiscoveredWhileFixing,
        RelationshipType::Supersedes,
    ];
    for rt in all_types {
        let json = serde_json::to_string(&rt).expect("serialize");
        let deserialized: RelationshipType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, rt, "roundtrip failed for {json}");
    }
}

// =============================================================================
// SuppressionType tests
// =============================================================================

#[test]
fn suppression_type_serde_global() {
    let st = SuppressionType::Global;
    let json = serde_json::to_string(&st).expect("serialize");
    let deserialized: SuppressionType = serde_json::from_str(&json).expect("deserialize");
    // Verify by re-serializing (SuppressionType doesn't derive PartialEq)
    let json2 = serde_json::to_string(&deserialized).expect("re-serialize");
    assert_eq!(json, json2);
}

#[test]
fn suppression_type_serde_file_level() {
    let st = SuppressionType::FileLevel;
    let json = serde_json::to_string(&st).expect("serialize");
    let deserialized: SuppressionType = serde_json::from_str(&json).expect("deserialize");
    let json2 = serde_json::to_string(&deserialized).expect("re-serialize");
    assert_eq!(json, json2);
}

#[test]
fn suppression_type_serde_inline_comment() {
    let st = SuppressionType::InlineComment {
        pattern: "tally:suppress".to_string(),
    };
    let json = serde_json::to_string(&st).expect("serialize");
    let deserialized: SuppressionType = serde_json::from_str(&json).expect("deserialize");
    let json2 = serde_json::to_string(&deserialized).expect("re-serialize");
    assert_eq!(json, json2);
}

#[test]
fn suppression_type_inline_empty_pattern() {
    let st = SuppressionType::InlineComment {
        pattern: String::new(),
    };
    let json = serde_json::to_string(&st).expect("serialize");
    let deserialized: SuppressionType = serde_json::from_str(&json).expect("deserialize");
    let json2 = serde_json::to_string(&deserialized).expect("re-serialize");
    assert_eq!(json, json2);
}

// =============================================================================
// LifecycleState additional tests
// =============================================================================

#[test]
fn lifecycle_from_str_numeric() {
    assert!(
        "123".parse::<LifecycleState>().is_err(),
        "numeric string should not parse as LifecycleState"
    );
}

#[test]
fn lifecycle_from_str_hyphen_normalization() {
    assert_eq!(
        "false-positive".parse::<LifecycleState>().expect("parse"),
        LifecycleState::FalsePositive
    );
    assert_eq!(
        "wont-fix".parse::<LifecycleState>().expect("parse"),
        LifecycleState::WontFix
    );
    assert_eq!(
        "in-progress".parse::<LifecycleState>().expect("parse"),
        LifecycleState::InProgress
    );
}

#[test]
fn lifecycle_from_str_error_includes_valid_list() {
    let err = "bogus"
        .parse::<LifecycleState>()
        .expect_err("should fail to parse 'bogus'");
    assert!(
        err.contains("valid:"),
        "error message should list valid states, got: {err}"
    );
}

#[test]
fn lifecycle_display_roundtrip_all() {
    for state in LifecycleState::all() {
        let display = state.to_string();
        let parsed: LifecycleState = display.parse().unwrap_or_else(|e| {
            panic!("Display→FromStr roundtrip failed for {state}: {e}");
        });
        assert_eq!(*state, parsed, "roundtrip failed for {state}");
    }
}

#[test]
fn lifecycle_closed_has_no_transitions() {
    assert!(
        LifecycleState::Closed.allowed_transitions().is_empty(),
        "Closed should have no valid transitions"
    );
}

// =============================================================================
// Severity additional tests
// =============================================================================

#[test]
fn severity_from_str_whitespace_rejects() {
    assert!(
        " critical".parse::<Severity>().is_err(),
        "leading space should be rejected"
    );
    assert!(
        "critical ".parse::<Severity>().is_err(),
        "trailing space should be rejected"
    );
}

#[test]
fn severity_from_str_unicode_rejects() {
    assert!(
        "cr\u{00EF}tical".parse::<Severity>().is_err(),
        "unicode characters should be rejected"
    );
}

#[test]
fn severity_from_str_mixed_case_valid() {
    // All-caps
    assert_eq!(
        "CRITICAL".parse::<Severity>().expect("parse CRITICAL"),
        Severity::Critical
    );
    // Title case
    assert_eq!(
        "Critical".parse::<Severity>().expect("parse Critical"),
        Severity::Critical
    );
    // Mixed case
    assert_eq!(
        "cRiTiCaL".parse::<Severity>().expect("parse cRiTiCaL"),
        Severity::Critical
    );
    assert_eq!(
        "Suggestion".parse::<Severity>().expect("parse Suggestion"),
        Severity::Suggestion
    );
    assert_eq!(
        "TECH_DEBT".parse::<Severity>().expect("parse TECH_DEBT"),
        Severity::TechDebt
    );
}

#[test]
fn severity_display_roundtrip_all() {
    let all_severities = [
        Severity::Critical,
        Severity::Important,
        Severity::Suggestion,
        Severity::TechDebt,
    ];
    for sev in all_severities {
        let display = sev.to_string();
        let parsed: Severity = display.to_ascii_lowercase().parse().unwrap_or_else(|e| {
            panic!("Display→FromStr roundtrip failed for {sev}: {e}");
        });
        assert_eq!(sev, parsed, "roundtrip failed for {sev}");
    }
}

// =============================================================================
// Phase 1: Notes & Edit History (v0.5.0)
// =============================================================================

/// Helper to create a minimal finding for edit/note tests.
fn make_test_finding() -> Finding {
    Finding {
        schema_version: "1.0.0".to_string(),
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:test".to_string(),
        rule_id: "spec-drift".to_string(),
        locations: vec![Location {
            file_path: "src/main.rs".to_string(),
            line_start: 42,
            line_end: 42,
            role: LocationRole::Primary,
            message: None,
        }],
        severity: Severity::Important,
        category: String::new(),
        tags: vec![],
        title: "unwrap on Option".to_string(),
        description: "Line 42 calls .unwrap() on an Option.".to_string(),
        suggested_fix: Some("Use ? or expect()".to_string()),
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "test".to_string(),
            session_id: String::new(),
            detected_at: chrono::Utc::now(),
            session_short_id: None,
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
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

#[test]
fn v040_finding_deserializes_without_notes_or_edit_history() {
    let json = r#"{
      "schema_version": "1.0.0",
      "uuid": "019cebd0-5c2a-7b73-8fb9-bdc551dce811",
      "content_fingerprint": "sha256:e7f50c20ae2afdc066f9839ff3b481429bcac80b85a5308f7f3e74d6fa906488",
      "rule_id": "spec-drift",
      "locations": [{"file_path": "src/main.rs", "line_start": 42, "line_end": 42, "role": "primary"}],
      "severity": "important",
      "category": "",
      "title": "unwrap on Option",
      "description": "Line 42 calls .unwrap() on an Option.",
      "suggested_fix": "Use ? or expect()",
      "evidence": null,
      "status": "open",
      "state_history": [],
      "discovered_by": [{"agent_id": "test", "session_id": "", "detected_at": "2026-03-14T10:00:00Z"}],
      "created_at": "2026-03-14T10:00:00Z",
      "updated_at": "2026-03-14T10:00:00Z",
      "repo_id": ""
    }"#;

    let finding: Finding = serde_json::from_str(json).expect("deserialize v0.4.0 finding");
    assert!(finding.notes.is_empty(), "notes should default to empty");
    assert!(
        finding.edit_history.is_empty(),
        "edit_history should default to empty"
    );
    assert_eq!(finding.title, "unwrap on Option");
    assert_eq!(finding.severity, Severity::Important);
    assert_eq!(finding.status, LifecycleState::Open);
}

#[test]
fn finding_with_notes_serializes_correctly() {
    let mut finding = make_test_finding();
    finding.add_note("Covered by Story 1.21 AC-2", "dclaude:check-drift");

    let json = serde_json::to_string_pretty(&finding).expect("serialize");
    assert!(json.contains("\"notes\""), "JSON should contain notes");
    assert!(
        json.contains("Covered by Story 1.21 AC-2"),
        "note text in JSON"
    );
}

#[test]
fn finding_with_empty_notes_omits_from_json() {
    let finding = make_test_finding();
    let json = serde_json::to_string_pretty(&finding).expect("serialize");
    assert!(
        !json.contains("\"notes\""),
        "empty notes should be omitted from JSON"
    );
    assert!(
        !json.contains("\"edit_history\""),
        "empty edit_history should be omitted from JSON"
    );
}

#[test]
fn edit_field_captures_old_and_new_value() {
    let mut finding = make_test_finding();
    finding
        .edit_field(
            "suggested_fix",
            serde_json::Value::String("Support both =~ and MATCHES".to_string()),
            "dclaude:check-drift",
        )
        .expect("edit");

    assert_eq!(finding.edit_history.len(), 1);
    let edit = &finding.edit_history[0];
    assert_eq!(edit.field, "suggested_fix");
    assert_eq!(edit.old_value, serde_json::json!("Use ? or expect()"));
    assert_eq!(
        edit.new_value,
        serde_json::json!("Support both =~ and MATCHES")
    );
    assert_eq!(edit.agent_id, "dclaude:check-drift");
}

#[test]
fn edit_field_title_updates_correctly() {
    let mut finding = make_test_finding();
    finding
        .edit_field("title", serde_json::json!("new title"), "test")
        .expect("edit");
    assert_eq!(finding.title, "new title");
}

#[test]
fn edit_field_description_updates_correctly() {
    let mut finding = make_test_finding();
    finding
        .edit_field(
            "description",
            serde_json::json!("updated description"),
            "test",
        )
        .expect("edit");
    assert_eq!(finding.description, "updated description");
}

#[test]
fn edit_field_suggested_fix_updates_correctly() {
    let mut finding = make_test_finding();
    finding
        .edit_field("suggested_fix", serde_json::json!("new fix"), "test")
        .expect("edit");
    assert_eq!(finding.suggested_fix.as_deref(), Some("new fix"));
}

#[test]
fn edit_field_evidence_updates_correctly() {
    let mut finding = make_test_finding();
    finding
        .edit_field("evidence", serde_json::json!("new evidence"), "test")
        .expect("edit");
    assert_eq!(finding.evidence.as_deref(), Some("new evidence"));
}

#[test]
fn edit_field_severity_updates_correctly() {
    let mut finding = make_test_finding();
    assert_eq!(finding.severity, Severity::Important);
    finding
        .edit_field("severity", serde_json::json!("critical"), "test")
        .expect("edit");
    assert_eq!(finding.severity, Severity::Critical);
}

#[test]
fn edit_field_category_updates_correctly() {
    let mut finding = make_test_finding();
    finding
        .edit_field("category", serde_json::json!("security"), "test")
        .expect("edit");
    assert_eq!(finding.category, "security");
}

#[test]
fn edit_field_tags_replaces_array() {
    let mut finding = make_test_finding();
    finding.tags = vec!["old-tag".to_string()];
    finding
        .edit_field("tags", serde_json::json!(["new-tag", "another"]), "test")
        .expect("edit");
    assert_eq!(finding.tags, vec!["new-tag", "another"]);
}

#[test]
fn add_note_appends_without_status_change() {
    let mut finding = make_test_finding();
    let original_status = finding.status;
    finding.add_note("some context", "cli");
    assert_eq!(finding.notes.len(), 1);
    assert_eq!(finding.notes[0].text, "some context");
    assert_eq!(finding.status, original_status);
}

#[test]
fn multiple_edits_grow_history_sequentially() {
    let mut finding = make_test_finding();
    finding
        .edit_field("title", serde_json::json!("v2"), "agent1")
        .expect("edit 1");
    finding
        .edit_field("title", serde_json::json!("v3"), "agent2")
        .expect("edit 2");
    finding
        .edit_field("description", serde_json::json!("new desc"), "agent3")
        .expect("edit 3");

    assert_eq!(finding.edit_history.len(), 3);
    assert_eq!(finding.edit_history[0].new_value, serde_json::json!("v2"));
    assert_eq!(finding.edit_history[1].old_value, serde_json::json!("v2"));
    assert_eq!(finding.edit_history[1].new_value, serde_json::json!("v3"));
    assert_eq!(finding.edit_history[2].field, "description");
}

#[test]
fn multiple_notes_grow_sequentially() {
    let mut finding = make_test_finding();
    finding.add_note("first", "agent1");
    finding.add_note("second", "agent2");
    assert_eq!(finding.notes.len(), 2);
    assert_eq!(finding.notes[0].text, "first");
    assert_eq!(finding.notes[1].text, "second");
}

#[test]
fn edit_field_updates_updated_at_timestamp() {
    let mut finding = make_test_finding();
    let before = finding.updated_at;
    std::thread::sleep(std::time::Duration::from_millis(2));
    finding
        .edit_field("title", serde_json::json!("new"), "test")
        .expect("edit");
    assert!(finding.updated_at > before);
}

// --- Negative tests ---

#[test]
fn edit_field_rejects_immutable_field_uuid() {
    let mut finding = make_test_finding();
    let result = finding.edit_field("uuid", serde_json::json!("new-uuid"), "test");
    assert!(result.is_err());
}

#[test]
fn edit_field_rejects_immutable_field_fingerprint() {
    let mut finding = make_test_finding();
    let result = finding.edit_field("content_fingerprint", serde_json::json!("new"), "test");
    assert!(result.is_err());
}

#[test]
fn edit_field_rejects_immutable_field_rule_id() {
    let mut finding = make_test_finding();
    let result = finding.edit_field("rule_id", serde_json::json!("new"), "test");
    assert!(result.is_err());
}

#[test]
fn edit_field_rejects_immutable_field_status() {
    let mut finding = make_test_finding();
    let result = finding.edit_field("status", serde_json::json!("closed"), "test");
    assert!(result.is_err());
}

#[test]
fn edit_field_rejects_immutable_field_created_at() {
    let mut finding = make_test_finding();
    let result = finding.edit_field("created_at", serde_json::json!("2026-01-01"), "test");
    assert!(result.is_err());
}

#[test]
fn edit_field_rejects_unknown_field() {
    let mut finding = make_test_finding();
    let result = finding.edit_field("nonexistent_field", serde_json::json!("x"), "test");
    assert!(result.is_err());
}

#[test]
fn edit_field_severity_rejects_invalid_value() {
    let mut finding = make_test_finding();
    let result = finding.edit_field("severity", serde_json::json!("ultra"), "test");
    assert!(result.is_err());
}

#[test]
fn edit_field_error_lists_editable_fields() {
    let mut finding = make_test_finding();
    let err = finding
        .edit_field("uuid", serde_json::json!("x"), "test")
        .expect_err("uuid should be immutable");
    let msg = err.to_string();
    for field in &[
        "title",
        "description",
        "suggested_fix",
        "evidence",
        "severity",
        "category",
        "tags",
    ] {
        assert!(
            msg.contains(field),
            "error should list '{field}' as editable, got: {msg}"
        );
    }
}
