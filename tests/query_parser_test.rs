//! Parser tests for `TallyQL` expressions.

use std::time::Duration;
use tally_ng::query::ast::{CompareOp, FilterExpr, StringOp, Value};
use tally_ng::query::parse_tallyql;

// =============================================================================
// Positive: simple comparisons
// =============================================================================

#[test]
fn severity_equals_critical() {
    let expr = parse_tallyql("severity = critical").expect("should parse");
    assert_eq!(
        expr,
        FilterExpr::Comparison {
            field: "severity".to_string(),
            op: CompareOp::Eq,
            value: Value::Enum("critical".to_string()),
        }
    );
}

#[test]
fn status_not_equals_closed() {
    let expr = parse_tallyql("status != closed").expect("should parse");
    assert_eq!(
        expr,
        FilterExpr::Comparison {
            field: "status".to_string(),
            op: CompareOp::Ne,
            value: Value::Enum("closed".to_string()),
        }
    );
}

#[test]
fn created_at_greater_than_duration() {
    let expr = parse_tallyql("created_at > 7d").expect("should parse");
    assert_eq!(
        expr,
        FilterExpr::Comparison {
            field: "created_at".to_string(),
            op: CompareOp::Gt,
            value: Value::Duration(Duration::from_secs(7 * 86400)),
        }
    );
}

#[test]
fn title_contains_string() {
    let expr = parse_tallyql(r#"title CONTAINS "unwrap""#).expect("should parse");
    assert_eq!(
        expr,
        FilterExpr::StringMatch {
            field: "title".to_string(),
            op: StringOp::Contains,
            value: "unwrap".to_string(),
        }
    );
}

#[test]
fn file_startswith_string() {
    let expr = parse_tallyql(r#"file STARTSWITH "src/api""#).expect("should parse");
    assert_eq!(
        expr,
        FilterExpr::StringMatch {
            field: "file".to_string(),
            op: StringOp::StartsWith,
            value: "src/api".to_string(),
        }
    );
}

#[test]
fn rule_endswith_string() {
    let expr = parse_tallyql(r#"rule ENDSWITH "unwrap""#).expect("should parse");
    assert_eq!(
        expr,
        FilterExpr::StringMatch {
            field: "rule".to_string(),
            op: StringOp::EndsWith,
            value: "unwrap".to_string(),
        }
    );
}

// =============================================================================
// Positive: existence operators
// =============================================================================

#[test]
fn has_suggested_fix() {
    let expr = parse_tallyql("HAS suggested_fix").expect("should parse");
    assert_eq!(expr, FilterExpr::Has("suggested_fix".to_string()));
}

#[test]
fn missing_evidence() {
    let expr = parse_tallyql("MISSING evidence").expect("should parse");
    assert_eq!(expr, FilterExpr::Missing("evidence".to_string()));
}

// =============================================================================
// Positive: IN lists
// =============================================================================

#[test]
fn severity_in_list() {
    let expr = parse_tallyql("severity IN (critical, important)").expect("should parse");
    assert_eq!(
        expr,
        FilterExpr::InList {
            field: "severity".to_string(),
            values: vec![
                Value::Enum("critical".to_string()),
                Value::Enum("important".to_string()),
            ],
        }
    );
}

// =============================================================================
// Positive: boolean operators
// =============================================================================

#[test]
fn and_operator() {
    let expr =
        parse_tallyql(r#"severity = critical AND file CONTAINS "api""#).expect("should parse");
    match expr {
        FilterExpr::And(left, right) => {
            assert!(matches!(*left, FilterExpr::Comparison { .. }));
            assert!(matches!(*right, FilterExpr::StringMatch { .. }));
        }
        other => panic!("expected And, got {other:?}"),
    }
}

#[test]
fn or_operator() {
    let expr = parse_tallyql("severity = critical OR status = open").expect("should parse");
    match expr {
        FilterExpr::Or(left, right) => {
            assert!(matches!(*left, FilterExpr::Comparison { .. }));
            assert!(matches!(*right, FilterExpr::Comparison { .. }));
        }
        other => panic!("expected Or, got {other:?}"),
    }
}

#[test]
fn not_operator() {
    let expr = parse_tallyql("NOT status = closed").expect("should parse");
    match expr {
        FilterExpr::Not(inner) => {
            assert!(matches!(*inner, FilterExpr::Comparison { .. }));
        }
        other => panic!("expected Not, got {other:?}"),
    }
}

// =============================================================================
// Positive: precedence
// =============================================================================

#[test]
fn and_binds_tighter_than_or() {
    // a AND b OR c  →  Or(And(a, b), c)
    let expr = parse_tallyql("severity = critical AND status = open OR rule = \"test\"")
        .expect("should parse");
    match expr {
        FilterExpr::Or(left, right) => {
            assert!(
                matches!(*left, FilterExpr::And(..)),
                "left should be And, got {left:?}"
            );
            assert!(
                matches!(*right, FilterExpr::Comparison { .. }),
                "right should be Comparison, got {right:?}"
            );
        }
        other => panic!("expected Or(And(..), ..), got {other:?}"),
    }
}

#[test]
fn parentheses_override_precedence() {
    // a AND (b OR c)  →  And(a, Or(b, c))
    let expr = parse_tallyql("severity = critical AND (status = open OR rule = \"test\")")
        .expect("should parse");
    match expr {
        FilterExpr::And(left, right) => {
            assert!(matches!(*left, FilterExpr::Comparison { .. }));
            assert!(
                matches!(*right, FilterExpr::Or(..)),
                "right should be Or, got {right:?}"
            );
        }
        other => panic!("expected And(.., Or(..)), got {other:?}"),
    }
}

// =============================================================================
// Positive: case insensitivity
// =============================================================================

#[test]
fn keywords_are_case_insensitive() {
    parse_tallyql("severity = critical and status = open").expect("lowercase and");
    parse_tallyql("severity = critical AND status = open").expect("uppercase AND");
    parse_tallyql("severity = critical And status = open").expect("mixed case And");
}

#[test]
fn string_ops_are_case_insensitive() {
    parse_tallyql(r#"title contains "x""#).expect("lowercase contains");
    parse_tallyql(r#"title CONTAINS "x""#).expect("uppercase CONTAINS");
    parse_tallyql(r#"title Contains "x""#).expect("mixed case Contains");
}

#[test]
fn has_missing_case_insensitive() {
    parse_tallyql("has suggested_fix").expect("lowercase has");
    parse_tallyql("HAS suggested_fix").expect("uppercase HAS");
    parse_tallyql("missing evidence").expect("lowercase missing");
    parse_tallyql("MISSING evidence").expect("uppercase MISSING");
}

// =============================================================================
// Positive: operator aliases
// =============================================================================

#[test]
fn double_ampersand_is_and() {
    let expr = parse_tallyql("severity = critical && status = open").expect("should parse");
    assert!(matches!(expr, FilterExpr::And(..)));
}

#[test]
fn double_pipe_is_or() {
    let expr = parse_tallyql("severity = critical || status = open").expect("should parse");
    assert!(matches!(expr, FilterExpr::Or(..)));
}

#[test]
fn bang_is_not() {
    let expr = parse_tallyql("!status = closed").expect("should parse");
    assert!(matches!(expr, FilterExpr::Not(..)));
}

#[test]
fn double_equals_is_eq() {
    let expr = parse_tallyql("severity == critical").expect("should parse");
    assert!(matches!(
        expr,
        FilterExpr::Comparison {
            op: CompareOp::Eq,
            ..
        }
    ));
}

// =============================================================================
// Positive: duration parsing
// =============================================================================

#[test]
fn duration_seconds() {
    let expr = parse_tallyql("created_at > 60s").expect("should parse");
    if let FilterExpr::Comparison { value, .. } = expr {
        assert_eq!(value, Value::Duration(Duration::from_secs(60)));
    } else {
        panic!("expected Comparison");
    }
}

#[test]
fn duration_minutes() {
    let expr = parse_tallyql("created_at > 30m").expect("should parse");
    if let FilterExpr::Comparison { value, .. } = expr {
        assert_eq!(value, Value::Duration(Duration::from_secs(30 * 60)));
    } else {
        panic!("expected Comparison");
    }
}

#[test]
fn duration_hours() {
    let expr = parse_tallyql("created_at > 24h").expect("should parse");
    if let FilterExpr::Comparison { value, .. } = expr {
        assert_eq!(value, Value::Duration(Duration::from_secs(24 * 3600)));
    } else {
        panic!("expected Comparison");
    }
}

// =============================================================================
// Positive: quoted strings with escapes
// =============================================================================

#[test]
fn quoted_string_with_escape() {
    let expr = parse_tallyql(r#"title CONTAINS "hello \"world\"""#).expect("should parse");
    if let FilterExpr::StringMatch { value, .. } = expr {
        assert_eq!(value, r#"hello "world""#);
    } else {
        panic!("expected StringMatch");
    }
}

#[test]
fn quoted_string_with_newline_escape() {
    let expr = parse_tallyql(r#"title CONTAINS "line1\nline2""#).expect("should parse");
    if let FilterExpr::StringMatch { value, .. } = expr {
        assert_eq!(value, "line1\nline2");
    } else {
        panic!("expected StringMatch");
    }
}

// =============================================================================
// Positive: comments
// =============================================================================

#[test]
fn hash_comment_stripped() {
    let expr = parse_tallyql("severity = critical # only critical").expect("should parse");
    assert!(matches!(expr, FilterExpr::Comparison { .. }));
}

#[test]
fn double_slash_comment_stripped() {
    let expr = parse_tallyql("severity = critical // only critical").expect("should parse");
    assert!(matches!(expr, FilterExpr::Comparison { .. }));
}

// =============================================================================
// Positive: complex expressions from story fixtures
// =============================================================================

#[test]
fn story_fixture_not_with_and() {
    let expr = parse_tallyql("NOT (status = closed) AND created_at > 7d").expect("should parse");
    assert!(matches!(expr, FilterExpr::And(..)));
}

#[test]
fn story_fixture_has_with_and() {
    let expr =
        parse_tallyql(r#"HAS suggested_fix AND tag CONTAINS "security""#).expect("should parse");
    assert!(matches!(expr, FilterExpr::And(..)));
}

#[test]
fn story_fixture_in_list_with_or() {
    let expr =
        parse_tallyql("severity IN (critical, important) OR status = open").expect("should parse");
    assert!(matches!(expr, FilterExpr::Or(..)));
}

// =============================================================================
// Positive: integer values
// =============================================================================

#[test]
fn negative_integer() {
    let expr = parse_tallyql("created_at > -1").expect("should parse");
    if let FilterExpr::Comparison { value, .. } = expr {
        assert_eq!(value, Value::Integer(-1));
    } else {
        panic!("expected Comparison");
    }
}

// =============================================================================
// Positive: comparison operators
// =============================================================================

#[test]
fn all_comparison_operators_parse() {
    parse_tallyql("severity = critical").expect("eq");
    parse_tallyql("severity != critical").expect("ne");
    parse_tallyql("severity > critical").expect("gt");
    parse_tallyql("severity < critical").expect("lt");
    parse_tallyql("severity >= critical").expect("gte");
    parse_tallyql("severity <= critical").expect("lte");
}

// =============================================================================
// Negative: unknown field
// =============================================================================

#[test]
fn unknown_field_returns_error() {
    let errs = parse_tallyql("foo = bar").expect_err("should fail");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: empty input
// =============================================================================

#[test]
fn empty_input_returns_error() {
    let errs = parse_tallyql("").expect_err("should fail");
    assert!(!errs.is_empty());
    let msg = errs[0].to_string();
    assert!(msg.contains("expected"), "got: {msg}");
}

#[test]
fn whitespace_only_returns_error() {
    let errs = parse_tallyql("   ").expect_err("should fail");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: oversized input
// =============================================================================

#[test]
fn oversized_input_returns_error() {
    let big = "a".repeat(MAX_QUERY_LENGTH + 1);
    let errs = parse_tallyql(&big).expect_err("should fail");
    assert!(!errs.is_empty());
    let msg = errs[0].to_string();
    assert!(msg.contains("8KB") || msg.contains("bytes"), "got: {msg}");
}

const MAX_QUERY_LENGTH: usize = 8192;

// =============================================================================
// Negative: syntax errors
// =============================================================================

#[test]
fn missing_value_after_operator() {
    let errs = parse_tallyql("severity =").expect_err("should fail");
    assert!(!errs.is_empty());
}

#[test]
fn missing_closing_paren() {
    let errs = parse_tallyql("(severity = critical").expect_err("should fail");
    assert!(!errs.is_empty());
}

#[test]
fn dangling_and() {
    let errs = parse_tallyql("severity = critical AND").expect_err("should fail");
    assert!(!errs.is_empty());
}
