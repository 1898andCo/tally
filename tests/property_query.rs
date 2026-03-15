//! Property tests for `TallyQL` parser and evaluator.

use proptest::prelude::*;
use tally_ng::model::*;
use tally_ng::query::ast::{CompareOp, FilterExpr, StringOp, Value};
use tally_ng::query::evaluate;

/// Create a minimal finding for property tests.
fn make_finding() -> Finding {
    Finding {
        schema_version: "1.1.0".to_string(),
        uuid: uuid::Uuid::now_v7(),
        content_fingerprint: "sha256:prop".to_string(),
        rule_id: "test-rule".to_string(),
        locations: vec![Location {
            file_path: "src/lib.rs".to_string(),
            line_start: 1,
            line_end: 1,
            role: LocationRole::Primary,
            message: None,
        }],
        severity: Severity::Suggestion,
        category: String::new(),
        tags: vec![],
        title: "prop test".to_string(),
        description: "desc".to_string(),
        suggested_fix: None,
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![],
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

/// Strategy for generating leaf `FilterExpr` values.
fn leaf_expr() -> impl Strategy<Value = FilterExpr> {
    prop_oneof![
        // Severity comparison
        prop::sample::select(&["critical", "important", "suggestion", "tech_debt",]).prop_map(
            |sev| FilterExpr::Comparison {
                field: "severity".to_string(),
                op: CompareOp::Eq,
                value: Value::Enum(sev.to_string()),
            }
        ),
        // Status comparison
        prop::sample::select(&["open", "closed", "acknowledged",]).prop_map(|status| {
            FilterExpr::Comparison {
                field: "status".to_string(),
                op: CompareOp::Eq,
                value: Value::Enum(status.to_string()),
            }
        }),
        // Title contains
        "[a-z]{1,10}".prop_map(|s| FilterExpr::StringMatch {
            field: "title".to_string(),
            op: StringOp::Contains,
            value: s,
        }),
        // HAS / MISSING
        prop::sample::select(&["suggested_fix", "evidence", "tag", "agent",])
            .prop_map(|f| FilterExpr::Has(f.to_string())),
        prop::sample::select(&["suggested_fix", "evidence", "tag", "agent",])
            .prop_map(|f| FilterExpr::Missing(f.to_string())),
    ]
}

/// Strategy for generating `FilterExpr` trees up to a given depth.
fn filter_expr_strategy(max_depth: u32) -> impl Strategy<Value = FilterExpr> {
    leaf_expr().prop_recursive(max_depth, 64, 3, |inner| {
        prop_oneof![
            // AND
            (inner.clone(), inner.clone())
                .prop_map(|(a, b)| FilterExpr::And(Box::new(a), Box::new(b))),
            // OR
            (inner.clone(), inner.clone())
                .prop_map(|(a, b)| FilterExpr::Or(Box::new(a), Box::new(b))),
            // NOT
            inner.prop_map(|a| FilterExpr::Not(Box::new(a))),
        ]
    })
}

proptest! {
    /// Evaluating any generated expression against any finding never panics.
    #[test]
    fn evaluate_never_panics(expr in filter_expr_strategy(4)) {
        let finding = make_finding();
        // Just call evaluate — if it panics, the test fails
        let _ = evaluate(&expr, &finding);
    }

    /// AND is conjunction: evaluate(And(a, b)) == evaluate(a) && evaluate(b).
    #[test]
    fn and_is_conjunction(a in leaf_expr(), b in leaf_expr()) {
        let finding = make_finding();
        let combined = FilterExpr::And(Box::new(a.clone()), Box::new(b.clone()));
        let expected = evaluate(&a, &finding) && evaluate(&b, &finding);
        prop_assert_eq!(evaluate(&combined, &finding), expected);
    }

    /// OR is disjunction: evaluate(Or(a, b)) == evaluate(a) || evaluate(b).
    #[test]
    fn or_is_disjunction(a in leaf_expr(), b in leaf_expr()) {
        let finding = make_finding();
        let combined = FilterExpr::Or(Box::new(a.clone()), Box::new(b.clone()));
        let expected = evaluate(&a, &finding) || evaluate(&b, &finding);
        prop_assert_eq!(evaluate(&combined, &finding), expected);
    }

    /// NOT is negation: evaluate(Not(a)) == !evaluate(a).
    #[test]
    fn not_is_negation(a in leaf_expr()) {
        let finding = make_finding();
        let negated = FilterExpr::Not(Box::new(a.clone()));
        prop_assert_eq!(evaluate(&negated, &finding), !evaluate(&a, &finding));
    }

    /// Double negation: evaluate(Not(Not(a))) == evaluate(a).
    #[test]
    fn double_negation_is_identity(a in leaf_expr()) {
        let finding = make_finding();
        let double_neg = FilterExpr::Not(Box::new(FilterExpr::Not(Box::new(a.clone()))));
        prop_assert_eq!(evaluate(&double_neg, &finding), evaluate(&a, &finding));
    }
}
