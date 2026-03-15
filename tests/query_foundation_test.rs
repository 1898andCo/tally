//! Tests for `TallyQL` foundation: field registry, error types, AST display,
//! `apply_filters`, and `apply_sort`.

use chrono::{TimeDelta, Utc};
use tally_ng::model::*;
use tally_ng::query::ast::{CompareOp, SortSpec, StringOp, Value};
use tally_ng::query::error::TallyQLError;
use tally_ng::query::eval::{apply_filters, apply_sort};
use tally_ng::query::fields::{
    FieldType, KNOWN_FIELDS, SORTABLE_FIELDS, field_type, validate_field, validate_sort_field,
};

// =============================================================================
// Helpers
// =============================================================================

fn make_finding() -> Finding {
    Finding {
        schema_version: "1.1.0".to_string(),
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:test".to_string(),
        rule_id: "unsafe-unwrap".to_string(),
        original_rule_id: None,
        locations: vec![Location {
            file_path: "src/api/handler.rs".to_string(),
            line_start: 42,
            line_end: 42,
            role: LocationRole::Primary,
            message: None,
        }],
        severity: Severity::Critical,
        category: "safety".to_string(),
        tags: vec!["story:5.1".to_string(), "security".to_string()],
        title: "unwrap on Option in request handler".to_string(),
        description: "Using .unwrap() on user input that may be None".to_string(),
        suggested_fix: Some("Use .ok_or() with a proper error type".to_string()),
        evidence: Some("handler.rs:42: let val = input.get(\"key\").unwrap();".to_string()),
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "dclaude".to_string(),
            session_id: "sess-001".to_string(),
            detected_at: Utc::now(),
            session_short_id: None,
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
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

fn make_finding_with(
    severity: Severity,
    status: LifecycleState,
    title: &str,
    agent: &str,
    category: &str,
    created_at: chrono::DateTime<Utc>,
) -> Finding {
    let mut f = make_finding();
    f.uuid = uuid::Uuid::now_v7();
    f.severity = severity;
    f.status = status;
    f.title = title.to_string();
    f.description = String::new();
    f.suggested_fix = None;
    f.evidence = None;
    f.category = category.to_string();
    f.created_at = created_at;
    f.discovered_by = vec![AgentRecord {
        agent_id: agent.to_string(),
        session_id: "sess".to_string(),
        detected_at: created_at,
        session_short_id: None,
    }];
    f
}

// =============================================================================
// Field registry: validate_field — positive
// =============================================================================

#[test]
fn validate_field_accepts_all_known_fields() {
    for field in KNOWN_FIELDS {
        assert!(
            validate_field(field).is_ok(),
            "field '{field}' should be valid"
        );
    }
}

#[test]
fn validate_field_count_is_thirteen() {
    assert_eq!(KNOWN_FIELDS.len(), 13);
}

// =============================================================================
// Field registry: validate_field — negative
// =============================================================================

#[test]
fn validate_field_rejects_unknown_field() {
    let err = validate_field("foo").expect_err("unknown field should be rejected");
    assert!(err.contains("unknown field 'foo'"), "got: {err}");
    assert!(err.contains("expected one of:"), "got: {err}");
}

#[test]
fn validate_field_suggests_typo_correction() {
    let err = validate_field("svearity").expect_err("typo should be rejected");
    assert!(
        err.contains("Did you mean 'severity'?"),
        "should suggest severity, got: {err}"
    );
}

#[test]
fn validate_field_suggests_substring_match() {
    let err = validate_field("stat").expect_err("substring should be rejected");
    assert!(
        err.contains("Did you mean 'status'?"),
        "should suggest status, got: {err}"
    );
}

#[test]
fn validate_field_no_suggestion_for_completely_wrong() {
    let err = validate_field("xyzzy").expect_err("completely wrong should be rejected");
    assert!(
        !err.contains("Did you mean"),
        "should not suggest anything, got: {err}"
    );
}

// =============================================================================
// Field registry: field_type — positive
// =============================================================================

#[test]
fn field_type_severity_is_ordered_enum() {
    assert_eq!(field_type("severity"), FieldType::OrderedEnumField);
}

#[test]
fn field_type_status_is_enum() {
    assert_eq!(field_type("status"), FieldType::EnumField);
}

#[test]
fn field_type_string_fields() {
    for f in &["file", "rule", "title", "description", "category"] {
        assert_eq!(field_type(f), FieldType::StringField, "field: {f}");
    }
}

#[test]
fn field_type_optional_string_fields() {
    assert_eq!(field_type("suggested_fix"), FieldType::OptionalStringField);
    assert_eq!(field_type("evidence"), FieldType::OptionalStringField);
}

#[test]
fn field_type_datetime_fields() {
    assert_eq!(field_type("created_at"), FieldType::DateTimeField);
    assert_eq!(field_type("updated_at"), FieldType::DateTimeField);
}

#[test]
fn field_type_agent_is_agent_array() {
    assert_eq!(field_type("agent"), FieldType::AgentArrayField);
}

#[test]
fn field_type_tag_is_array_string() {
    assert_eq!(field_type("tag"), FieldType::ArrayStringField);
}

// =============================================================================
// Field registry: validate_sort_field
// =============================================================================

#[test]
fn validate_sort_field_accepts_sortable() {
    for f in SORTABLE_FIELDS {
        assert!(
            validate_sort_field(f).is_ok(),
            "field '{f}' should be sortable"
        );
    }
}

#[test]
fn validate_sort_field_rejects_non_sortable() {
    let err = validate_sort_field("description").expect_err("description is not sortable");
    assert!(err.contains("cannot sort by 'description'"), "got: {err}");
    assert!(err.contains("sortable fields:"), "got: {err}");
}

#[test]
fn validate_sort_field_rejects_unknown() {
    let err = validate_sort_field("foo").expect_err("unknown field is not sortable");
    assert!(err.contains("cannot sort by 'foo'"), "got: {err}");
}

// =============================================================================
// AST Display impls
// =============================================================================

#[test]
fn compare_op_display() {
    assert_eq!(CompareOp::Eq.to_string(), "=");
    assert_eq!(CompareOp::Ne.to_string(), "!=");
    assert_eq!(CompareOp::Gt.to_string(), ">");
    assert_eq!(CompareOp::Lt.to_string(), "<");
    assert_eq!(CompareOp::GtEq.to_string(), ">=");
    assert_eq!(CompareOp::LtEq.to_string(), "<=");
}

#[test]
fn string_op_display() {
    assert_eq!(StringOp::Contains.to_string(), "CONTAINS");
    assert_eq!(StringOp::StartsWith.to_string(), "STARTSWITH");
    assert_eq!(StringOp::EndsWith.to_string(), "ENDSWITH");
}

#[test]
fn value_display_string() {
    let v = Value::String("hello".to_string());
    assert_eq!(v.to_string(), "\"hello\"");
}

#[test]
fn value_display_integer() {
    assert_eq!(Value::Integer(42).to_string(), "42");
    assert_eq!(Value::Integer(-1).to_string(), "-1");
}

#[test]
fn value_display_duration() {
    let v = Value::Duration(std::time::Duration::from_secs(86400));
    assert_eq!(v.to_string(), "86400s");
}

#[test]
fn value_display_enum() {
    let v = Value::Enum("critical".to_string());
    assert_eq!(v.to_string(), "critical");
}

// =============================================================================
// Error constructors
// =============================================================================

#[test]
fn error_unexpected_token_has_span_and_found() {
    let err = TallyQLError::unexpected_token(5..10, "field name", "123");
    assert_eq!(*err.span(), 5..10);
    let msg = err.to_string();
    assert!(msg.contains("field name"), "got: {msg}");
    assert!(msg.contains("123"), "got: {msg}");
}

#[test]
fn error_unexpected_eof_shows_end_of_input() {
    let err = TallyQLError::unexpected_eof(0..0, "expression");
    let msg = err.to_string();
    assert!(msg.contains("end of input"), "got: {msg}");
    assert!(msg.contains("expression"), "got: {msg}");
}

#[test]
fn error_with_hint_stores_hint() {
    let err =
        TallyQLError::unexpected_token(0..3, "field", "foo").with_hint("did you mean 'file'?");
    assert_eq!(err.hint(), Some("did you mean 'file'?"));
}

#[test]
fn error_without_hint_returns_none() {
    let err = TallyQLError::unexpected_eof(0..0, "query");
    assert!(err.hint().is_none());
}

// =============================================================================
// apply_filters — since/before
// =============================================================================

#[test]
fn apply_filters_since_keeps_recent() {
    let now = Utc::now();
    let old = now - TimeDelta::try_days(10).expect("valid");
    let recent = now - TimeDelta::try_hours(1).expect("valid");
    let cutoff = now - TimeDelta::try_days(7).expect("valid");

    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "old",
            "cli",
            "",
            old,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Open,
            "recent",
            "cli",
            "",
            recent,
        ),
    ];

    apply_filters(
        &mut findings,
        None,
        Some(cutoff),
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "recent");
}

#[test]
fn apply_filters_before_keeps_old() {
    let now = Utc::now();
    let old = now - TimeDelta::try_days(10).expect("valid");
    let recent = now - TimeDelta::try_hours(1).expect("valid");
    let cutoff = now - TimeDelta::try_days(3).expect("valid");

    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "old",
            "cli",
            "",
            old,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Open,
            "recent",
            "cli",
            "",
            recent,
        ),
    ];

    apply_filters(
        &mut findings,
        None,
        None,
        Some(cutoff),
        None,
        None,
        None,
        None,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "old");
}

// =============================================================================
// apply_filters — agent
// =============================================================================

#[test]
fn apply_filters_agent_matches() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "a",
            "dclaude",
            "",
            now,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Open,
            "b",
            "zclaude",
            "",
            now,
        ),
    ];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        Some("dclaude"),
        None,
        None,
        None,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "a");
}

#[test]
fn apply_filters_agent_no_match_returns_empty() {
    let now = Utc::now();
    let mut findings = vec![make_finding_with(
        Severity::Critical,
        LifecycleState::Open,
        "a",
        "dclaude",
        "",
        now,
    )];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        Some("cursor"),
        None,
        None,
        None,
    );
    assert!(findings.is_empty());
}

// =============================================================================
// apply_filters — category
// =============================================================================

#[test]
fn apply_filters_category_exact_match() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "a",
            "cli",
            "safety",
            now,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Open,
            "b",
            "cli",
            "injection",
            now,
        ),
    ];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        Some("safety"),
        None,
        None,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "a");
}

// =============================================================================
// apply_filters — not_status
// =============================================================================

#[test]
fn apply_filters_not_status_excludes_matching() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "open",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Closed,
            "closed",
            "cli",
            "",
            now,
        ),
    ];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        Some(LifecycleState::Closed),
        None,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "open");
}

#[test]
fn apply_filters_not_status_keeps_all_when_none_match() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "a",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Open,
            "b",
            "cli",
            "",
            now,
        ),
    ];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        Some(LifecycleState::Closed),
        None,
    );
    assert_eq!(findings.len(), 2);
}

// =============================================================================
// apply_filters — text search
// =============================================================================

#[test]
fn apply_filters_text_searches_title() {
    let now = Utc::now();
    let mut f1 = make_finding_with(
        Severity::Critical,
        LifecycleState::Open,
        "unwrap on Option",
        "cli",
        "",
        now,
    );
    f1.description = "some description".to_string();
    f1.suggested_fix = None;
    f1.evidence = None;
    let mut f2 = make_finding_with(
        Severity::Important,
        LifecycleState::Open,
        "missing test",
        "cli",
        "",
        now,
    );
    f2.description = "some other description".to_string();
    f2.suggested_fix = None;
    f2.evidence = None;
    let mut findings = vec![f1, f2];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("unwrap"),
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "unwrap on Option");
}

#[test]
fn apply_filters_text_is_case_insensitive() {
    let now = Utc::now();
    let mut findings = vec![make_finding_with(
        Severity::Critical,
        LifecycleState::Open,
        "SQL Injection Risk",
        "cli",
        "",
        now,
    )];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("sql injection"),
    );
    assert_eq!(findings.len(), 1);
}

#[test]
fn apply_filters_text_searches_description() {
    let mut f = make_finding();
    f.description = "This uses unsafe unwrap calls".to_string();
    f.title = "something else".to_string();
    let mut findings = vec![f];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("unwrap"),
    );
    assert_eq!(findings.len(), 1);
}

#[test]
fn apply_filters_text_searches_suggested_fix() {
    let mut f = make_finding();
    f.title = "no match here".to_string();
    f.description = "no match here either".to_string();
    f.suggested_fix = Some("Use ok_or instead of unwrap".to_string());
    let mut findings = vec![f];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("unwrap"),
    );
    assert_eq!(findings.len(), 1);
}

#[test]
fn apply_filters_text_searches_evidence() {
    let mut f = make_finding();
    f.title = "no match".to_string();
    f.description = "no match".to_string();
    f.suggested_fix = None;
    f.evidence = Some("line 42: input.unwrap()".to_string());
    let mut findings = vec![f];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("unwrap"),
    );
    assert_eq!(findings.len(), 1);
}

#[test]
fn apply_filters_text_none_fields_dont_crash() {
    let mut f = make_finding();
    f.title = "no match".to_string();
    f.description = "no match".to_string();
    f.suggested_fix = None;
    f.evidence = None;
    let mut findings = vec![f];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("unwrap"),
    );
    assert!(findings.is_empty());
}

#[test]
fn apply_filters_text_no_match_returns_empty() {
    let mut findings = vec![make_finding()];
    apply_filters(
        &mut findings,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("xyzzy_nonexistent"),
    );
    assert!(findings.is_empty());
}

// =============================================================================
// apply_filters — combined
// =============================================================================

#[test]
fn apply_filters_combined_agent_and_not_status() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "a",
            "dclaude",
            "",
            now,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Closed,
            "b",
            "dclaude",
            "",
            now,
        ),
        make_finding_with(
            Severity::Suggestion,
            LifecycleState::Open,
            "c",
            "zclaude",
            "",
            now,
        ),
    ];

    apply_filters(
        &mut findings,
        None,
        None,
        None,
        Some("dclaude"),
        None,
        Some(LifecycleState::Closed),
        None,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "a");
}

#[test]
fn apply_filters_all_filters_at_once() {
    let now = Utc::now();
    let recent = now - TimeDelta::try_hours(1).expect("valid");

    let mut f = make_finding_with(
        Severity::Critical,
        LifecycleState::Open,
        "unwrap bug",
        "dclaude",
        "safety",
        recent,
    );
    f.description = "found an unwrap in production code".to_string();
    let mut findings = vec![f];

    apply_filters(
        &mut findings,
        None,
        Some(now - TimeDelta::try_days(1).expect("valid")), // since 1 day ago
        None,
        Some("dclaude"),
        Some("safety"),
        Some(LifecycleState::Closed),
        Some("unwrap"),
    );
    assert_eq!(findings.len(), 1);
}

// =============================================================================
// apply_sort
// =============================================================================

#[test]
fn apply_sort_empty_specs_is_noop() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Suggestion,
            LifecycleState::Open,
            "z",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "a",
            "cli",
            "",
            now,
        ),
    ];
    let original_order: Vec<String> = findings.iter().map(|f| f.title.clone()).collect();

    apply_sort(&mut findings, &[]);
    let after: Vec<String> = findings.iter().map(|f| f.title.clone()).collect();
    assert_eq!(original_order, after);
}

#[test]
fn apply_sort_by_severity_ascending() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "crit",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::TechDebt,
            LifecycleState::Open,
            "td",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Open,
            "imp",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Suggestion,
            LifecycleState::Open,
            "sug",
            "cli",
            "",
            now,
        ),
    ];

    apply_sort(
        &mut findings,
        &[SortSpec {
            field: "severity".to_string(),
            descending: false,
        }],
    );
    let titles: Vec<&str> = findings.iter().map(|f| f.title.as_str()).collect();
    assert_eq!(titles, vec!["td", "sug", "imp", "crit"]);
}

#[test]
fn apply_sort_by_severity_descending() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::TechDebt,
            LifecycleState::Open,
            "td",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "crit",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Suggestion,
            LifecycleState::Open,
            "sug",
            "cli",
            "",
            now,
        ),
    ];

    apply_sort(
        &mut findings,
        &[SortSpec {
            field: "severity".to_string(),
            descending: true,
        }],
    );
    let titles: Vec<&str> = findings.iter().map(|f| f.title.as_str()).collect();
    assert_eq!(titles, vec!["crit", "sug", "td"]);
}

#[test]
fn apply_sort_by_title_ascending() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "zulu",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "alpha",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "mike",
            "cli",
            "",
            now,
        ),
    ];

    apply_sort(
        &mut findings,
        &[SortSpec {
            field: "title".to_string(),
            descending: false,
        }],
    );
    let titles: Vec<&str> = findings.iter().map(|f| f.title.as_str()).collect();
    assert_eq!(titles, vec!["alpha", "mike", "zulu"]);
}

#[test]
fn apply_sort_by_created_at_descending() {
    let now = Utc::now();
    let old = now - TimeDelta::try_days(5).expect("valid");
    let mid = now - TimeDelta::try_days(2).expect("valid");

    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "old",
            "cli",
            "",
            old,
        ),
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "new",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "mid",
            "cli",
            "",
            mid,
        ),
    ];

    apply_sort(
        &mut findings,
        &[SortSpec {
            field: "created_at".to_string(),
            descending: true,
        }],
    );
    let titles: Vec<&str> = findings.iter().map(|f| f.title.as_str()).collect();
    assert_eq!(titles, vec!["new", "mid", "old"]);
}

#[test]
fn apply_sort_multi_field_severity_then_title() {
    let now = Utc::now();
    let mut findings = vec![
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "beta",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Important,
            LifecycleState::Open,
            "alpha",
            "cli",
            "",
            now,
        ),
        make_finding_with(
            Severity::Critical,
            LifecycleState::Open,
            "alpha",
            "cli",
            "",
            now,
        ),
    ];

    apply_sort(
        &mut findings,
        &[
            SortSpec {
                field: "severity".to_string(),
                descending: true,
            },
            SortSpec {
                field: "title".to_string(),
                descending: false,
            },
        ],
    );
    let labels: Vec<String> = findings
        .iter()
        .map(|f| format!("{:?}:{}", f.severity, f.title))
        .collect();
    assert_eq!(
        labels,
        vec!["Critical:alpha", "Critical:beta", "Important:alpha"]
    );
}
