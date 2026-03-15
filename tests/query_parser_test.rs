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

// =============================================================================
// Positive: IN list variants
// =============================================================================

#[test]
fn in_list_with_quoted_strings() {
    let expr =
        parse_tallyql(r#"rule IN ("unsafe-unwrap", "sql-injection")"#).expect("should parse");
    if let FilterExpr::InList { field, values } = expr {
        assert_eq!(field, "rule");
        assert_eq!(values.len(), 2);
        assert_eq!(values[0], Value::String("unsafe-unwrap".to_string()));
        assert_eq!(values[1], Value::String("sql-injection".to_string()));
    } else {
        panic!("expected InList, got {expr:?}");
    }
}

#[test]
fn in_list_single_value() {
    let expr = parse_tallyql("severity IN (critical)").expect("should parse");
    if let FilterExpr::InList { values, .. } = expr {
        assert_eq!(values.len(), 1);
    } else {
        panic!("expected InList, got {expr:?}");
    }
}

// =============================================================================
// Positive: chained boolean operators
// =============================================================================

#[test]
fn multiple_and_chain() {
    // a AND b AND c → And(And(a, b), c) — left-associative
    let expr = parse_tallyql("severity = critical AND status = open AND rule = \"test\"")
        .expect("should parse");
    match expr {
        FilterExpr::And(left, right) => {
            assert!(
                matches!(*left, FilterExpr::And(..)),
                "left should be And (left-assoc), got {left:?}"
            );
            assert!(matches!(*right, FilterExpr::Comparison { .. }));
        }
        other => panic!("expected And(And(..), ..), got {other:?}"),
    }
}

#[test]
fn multiple_or_chain() {
    // a OR b OR c → Or(Or(a, b), c) — left-associative
    let expr = parse_tallyql("severity = critical OR status = open OR rule = \"test\"")
        .expect("should parse");
    match expr {
        FilterExpr::Or(left, right) => {
            assert!(
                matches!(*left, FilterExpr::Or(..)),
                "left should be Or (left-assoc), got {left:?}"
            );
            assert!(matches!(*right, FilterExpr::Comparison { .. }));
        }
        other => panic!("expected Or(Or(..), ..), got {other:?}"),
    }
}

#[test]
fn mixed_not_and_or_precedence() {
    // NOT a AND b OR c → Or(And(Not(a), b), c)
    let expr = parse_tallyql("NOT severity = critical AND status = open OR rule = \"test\"")
        .expect("should parse");
    match expr {
        FilterExpr::Or(left, _right) => match *left {
            FilterExpr::And(inner_left, _) => {
                assert!(
                    matches!(*inner_left, FilterExpr::Not(..)),
                    "inner left should be Not, got {inner_left:?}"
                );
            }
            other => panic!("expected And(Not(..), ..), got {other:?}"),
        },
        other => panic!("expected Or(And(Not(..), ..), ..), got {other:?}"),
    }
}

// =============================================================================
// Positive: nested NOT
// =============================================================================

#[test]
fn double_not_with_parens() {
    // NOT (NOT status = closed) — parens needed to disambiguate nested NOT
    let expr = parse_tallyql("NOT (NOT status = closed)").expect("should parse");
    match expr {
        FilterExpr::Not(inner) => {
            assert!(
                matches!(*inner, FilterExpr::Not(..)),
                "should be Not(Not(..)), got {inner:?}"
            );
        }
        other => panic!("expected Not(Not(..)), got {other:?}"),
    }
}

// =============================================================================
// Positive: comparison with quoted string value
// =============================================================================

#[test]
fn comparison_with_quoted_string_value() {
    let expr = parse_tallyql(r#"title = "hello world""#).expect("should parse");
    if let FilterExpr::Comparison { field, op, value } = expr {
        assert_eq!(field, "title");
        assert_eq!(op, CompareOp::Eq);
        assert_eq!(value, Value::String("hello world".to_string()));
    } else {
        panic!("expected Comparison, got {expr:?}");
    }
}

#[test]
fn comparison_with_quoted_date_string() {
    let expr = parse_tallyql(r#"created_at > "2026-03-01""#).expect("should parse");
    if let FilterExpr::Comparison { value, .. } = expr {
        assert_eq!(value, Value::String("2026-03-01".to_string()));
    } else {
        panic!("expected Comparison, got {expr:?}");
    }
}

#[test]
fn comparison_with_rfc3339_date() {
    let expr = parse_tallyql(r#"created_at > "2026-03-01T12:00:00Z""#).expect("should parse");
    if let FilterExpr::Comparison { value, .. } = expr {
        assert_eq!(value, Value::String("2026-03-01T12:00:00Z".to_string()));
    } else {
        panic!("expected Comparison, got {expr:?}");
    }
}

// =============================================================================
// Positive: HAS/MISSING for array and agent fields
// =============================================================================

#[test]
fn has_tag() {
    let expr = parse_tallyql("HAS tag").expect("should parse");
    assert_eq!(expr, FilterExpr::Has("tag".to_string()));
}

#[test]
fn missing_agent() {
    let expr = parse_tallyql("MISSING agent").expect("should parse");
    assert_eq!(expr, FilterExpr::Missing("agent".to_string()));
}

// =============================================================================
// Positive: deeply nested within limit
// =============================================================================

#[test]
fn deeply_nested_within_limit_parses() {
    // 10 levels of nesting — well within 64 limit
    let mut expr = "severity = critical".to_string();
    for _ in 0..10 {
        expr = format!("({expr})");
    }
    parse_tallyql(&expr).expect("10 levels should parse fine");
}

// =============================================================================
// Positive: whitespace tolerance
// =============================================================================

#[test]
fn extra_whitespace_between_tokens() {
    parse_tallyql("  severity   =   critical  ").expect("should parse with extra whitespace");
}

#[test]
fn tabs_between_tokens() {
    parse_tallyql("severity\t=\tcritical").expect("should parse with tabs");
}

// =============================================================================
// Positive: duration days standalone
// =============================================================================

#[test]
fn duration_days() {
    let expr = parse_tallyql("created_at > 14d").expect("should parse");
    if let FilterExpr::Comparison { value, .. } = expr {
        assert_eq!(value, Value::Duration(Duration::from_secs(14 * 86400)));
    } else {
        panic!("expected Comparison, got {expr:?}");
    }
}

// =============================================================================
// Negative: depth exceeded
// =============================================================================

#[test]
fn depth_exceeded_returns_error() {
    // 65 levels of nesting — exceeds max of 64
    let mut expr = "severity = critical".to_string();
    for _ in 0..65 {
        expr = format!("({expr})");
    }
    let errs = parse_tallyql(&expr).expect_err("should fail at depth 65");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: invalid string operator
// =============================================================================

#[test]
fn unknown_string_operator_returns_error() {
    let errs = parse_tallyql(r#"title LIKE "x""#).expect_err("LIKE is not a valid operator");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: IN list syntax errors
// =============================================================================

#[test]
fn in_list_missing_closing_paren() {
    let errs =
        parse_tallyql("severity IN (critical, important").expect_err("missing closing paren");
    assert!(!errs.is_empty());
}

#[test]
fn in_list_empty() {
    let errs = parse_tallyql("severity IN ()").expect_err("empty IN list");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: quoted string errors
// =============================================================================

#[test]
fn unclosed_quoted_string() {
    let errs = parse_tallyql(r#"title CONTAINS "hello"#).expect_err("unclosed quote");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: dangling operators
// =============================================================================

#[test]
fn dangling_or() {
    let errs = parse_tallyql("severity = critical OR").expect_err("dangling OR");
    assert!(!errs.is_empty());
}

#[test]
fn dangling_not_at_end() {
    let errs = parse_tallyql("NOT").expect_err("dangling NOT");
    assert!(!errs.is_empty());
}

#[test]
fn just_operator_no_operands() {
    let errs = parse_tallyql("AND").expect_err("just AND");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: invalid values
// =============================================================================

#[test]
fn trailing_garbage_after_expression() {
    let errs = parse_tallyql("severity = critical garbage").expect_err("trailing garbage");
    assert!(!errs.is_empty());
}

// =============================================================================
// Negative: HAS/MISSING with unknown field
// =============================================================================

#[test]
fn has_unknown_field_returns_error() {
    let errs = parse_tallyql("HAS nonexistent").expect_err("unknown field");
    assert!(!errs.is_empty());
}

#[test]
fn missing_unknown_field_returns_error() {
    let errs = parse_tallyql("MISSING nonexistent").expect_err("unknown field");
    assert!(!errs.is_empty());
}
