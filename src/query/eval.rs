//! `TallyQL` expression evaluator.
//!
//! Evaluates a [`FilterExpr`] AST against a [`Finding`], returning `bool`.
//! Also provides shared filter and sort functions used by both CLI and MCP.

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};

use crate::model::{Finding, LifecycleState, Severity};
use crate::query::ast::{CompareOp, FilterExpr, SortSpec, StringOp, Value};

/// Evaluate a filter expression against a finding.
///
/// Returns `true` if the finding matches the expression.
#[must_use]
pub fn evaluate(expr: &FilterExpr, finding: &Finding) -> bool {
    match expr {
        FilterExpr::And(left, right) => evaluate(left, finding) && evaluate(right, finding),
        FilterExpr::Or(left, right) => evaluate(left, finding) || evaluate(right, finding),
        FilterExpr::Not(inner) => !evaluate(inner, finding),
        FilterExpr::Has(field) => eval_has(field, finding),
        FilterExpr::Missing(field) => !eval_has(field, finding),
        FilterExpr::Comparison { field, op, value } => eval_comparison(field, *op, value, finding),
        FilterExpr::StringMatch { field, op, value } => {
            eval_string_match(field, *op, value, finding)
        }
        FilterExpr::InList { field, values } => eval_in_list(field, values, finding),
    }
}

/// Check if a field "exists" (is non-empty, non-None).
fn eval_has(field: &str, finding: &Finding) -> bool {
    match field {
        "suggested_fix" => finding.suggested_fix.is_some(),
        "evidence" => finding.evidence.is_some(),
        "tag" => !finding.tags.is_empty(),
        "agent" => !finding.discovered_by.is_empty(),
        // String fields: HAS means non-empty
        "title" => !finding.title.is_empty(),
        "description" => !finding.description.is_empty(),
        "category" => !finding.category.is_empty(),
        "rule" => !finding.rule_id.is_empty(),
        "file" => !finding.locations.is_empty(),
        // Enum and DateTime fields: always present (non-optional)
        "severity" | "status" | "created_at" | "updated_at" => true,
        _ => false,
    }
}

/// Evaluate a comparison: `field op value`.
fn eval_comparison(field: &str, op: CompareOp, value: &Value, finding: &Finding) -> bool {
    match field {
        "severity" => eval_severity_comparison(op, value, finding.severity),
        "status" => eval_status_comparison(op, value, finding.status),
        "created_at" => eval_datetime_comparison(op, value, finding.created_at),
        "updated_at" => eval_datetime_comparison(op, value, finding.updated_at),
        "file" => eval_multi_string_comparison(
            op,
            value,
            &finding
                .locations
                .iter()
                .map(|l| l.file_path.as_str())
                .collect::<Vec<_>>(),
        ),
        "agent" => eval_multi_string_comparison(
            op,
            value,
            &finding
                .discovered_by
                .iter()
                .map(|a| a.agent_id.as_str())
                .collect::<Vec<_>>(),
        ),
        "tag" => eval_multi_string_comparison(
            op,
            value,
            &finding.tags.iter().map(String::as_str).collect::<Vec<_>>(),
        ),
        "rule" => eval_single_string_comparison(op, value, &finding.rule_id),
        "title" => eval_single_string_comparison(op, value, &finding.title),
        "description" => eval_single_string_comparison(op, value, &finding.description),
        "category" => eval_single_string_comparison(op, value, &finding.category),
        "suggested_fix" => match &finding.suggested_fix {
            Some(s) => eval_single_string_comparison(op, value, s),
            None => op == CompareOp::Ne, // None != anything is true
        },
        "evidence" => match &finding.evidence {
            Some(s) => eval_single_string_comparison(op, value, s),
            None => op == CompareOp::Ne,
        },
        _ => false,
    }
}

/// Compare severity with ordering: critical=3 > important=2 > suggestion=1 > `tech_debt`=0.
fn eval_severity_comparison(op: CompareOp, value: &Value, actual: Severity) -> bool {
    let value_str = match value {
        Value::Enum(s) | Value::String(s) => s.as_str(),
        _ => return false,
    };
    let Ok(target) = value_str.parse::<Severity>() else {
        return false;
    };
    let actual_ord = severity_ordinal(actual);
    let target_ord = severity_ordinal(target);
    apply_ord_op(op, actual_ord.cmp(&target_ord))
}

/// Compare status (equality only, no ordering).
fn eval_status_comparison(op: CompareOp, value: &Value, actual: LifecycleState) -> bool {
    let value_str = match value {
        Value::Enum(s) | Value::String(s) => s.as_str(),
        _ => return false,
    };
    let Ok(target) = value_str.parse::<LifecycleState>() else {
        return false;
    };
    match op {
        CompareOp::Eq => actual == target,
        CompareOp::Ne => actual != target,
        // Status has no meaningful ordering
        _ => false,
    }
}

/// Compare a `DateTime<Utc>` field against a value.
///
/// Supports:
/// - `Duration` values: resolved relative to `Utc::now()` (e.g., `created_at > 7d` means
///   "created more than 7 days ago" — the duration is subtracted from now)
/// - `String` values: parsed as ISO 8601 date or RFC 3339 timestamp
fn eval_datetime_comparison(op: CompareOp, value: &Value, actual: DateTime<Utc>) -> bool {
    let target = match value {
        Value::Duration(d) => {
            let Ok(delta) = TimeDelta::from_std(*d) else {
                return false;
            };
            Utc::now() - delta
        }
        Value::String(s) => {
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                dt.with_timezone(&Utc)
            } else if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                date.and_hms_opt(0, 0, 0)
                    .expect("midnight is always valid")
                    .and_utc()
            } else {
                return false;
            }
        }
        _ => return false,
    };
    apply_ord_op(op, actual.cmp(&target))
}

/// Compare a single string field.
fn eval_single_string_comparison(op: CompareOp, value: &Value, actual: &str) -> bool {
    let target = value_to_string(value);
    match op {
        CompareOp::Eq => actual.eq_ignore_ascii_case(&target),
        CompareOp::Ne => !actual.eq_ignore_ascii_case(&target),
        CompareOp::Gt => actual > target.as_str(),
        CompareOp::Lt => actual < target.as_str(),
        CompareOp::GtEq => actual >= target.as_str(),
        CompareOp::LtEq => actual <= target.as_str(),
    }
}

/// Compare against a multi-value field (any-match semantics).
///
/// For `=`: true if ANY value matches.
/// For `!=`: true if NO value matches.
fn eval_multi_string_comparison(op: CompareOp, value: &Value, actuals: &[&str]) -> bool {
    let target = value_to_string(value);
    match op {
        CompareOp::Eq => actuals.iter().any(|a| a.eq_ignore_ascii_case(&target)),
        CompareOp::Ne => actuals.iter().all(|a| !a.eq_ignore_ascii_case(&target)),
        // Ordering on multi-value fields: compare against first
        _ => actuals
            .first()
            .is_some_and(|a| apply_ord_op(op, (*a).cmp(target.as_str()))),
    }
}

/// Evaluate a string match operation (case-insensitive).
fn eval_string_match(field: &str, op: StringOp, pattern: &str, finding: &Finding) -> bool {
    let pattern_lower = pattern.to_lowercase();
    match field {
        // Multi-value: any-match semantics
        "file" => finding
            .locations
            .iter()
            .any(|l| string_op_match(op, &l.file_path.to_lowercase(), &pattern_lower)),
        "agent" => finding
            .discovered_by
            .iter()
            .any(|a| string_op_match(op, &a.agent_id.to_lowercase(), &pattern_lower)),
        "tag" => finding
            .tags
            .iter()
            .any(|t| string_op_match(op, &t.to_lowercase(), &pattern_lower)),
        // Optional fields
        "suggested_fix" => finding
            .suggested_fix
            .as_deref()
            .is_some_and(|s| string_op_match(op, &s.to_lowercase(), &pattern_lower)),
        "evidence" => finding
            .evidence
            .as_deref()
            .is_some_and(|s| string_op_match(op, &s.to_lowercase(), &pattern_lower)),
        // Single string fields
        _ => {
            let actual = single_string_field(field, finding);
            string_op_match(op, &actual.to_lowercase(), &pattern_lower)
        }
    }
}

/// Apply a string operation.
fn string_op_match(op: StringOp, haystack: &str, needle: &str) -> bool {
    match op {
        StringOp::Contains => haystack.contains(needle),
        StringOp::StartsWith => haystack.starts_with(needle),
        StringOp::EndsWith => haystack.ends_with(needle),
    }
}

/// Evaluate an IN list: true if field value matches any value in the list.
fn eval_in_list(field: &str, values: &[Value], finding: &Finding) -> bool {
    match field {
        "severity" => {
            let actual = finding.severity;
            values.iter().any(|v| {
                let s = match v {
                    Value::Enum(s) | Value::String(s) => s.as_str(),
                    _ => return false,
                };
                s.parse::<Severity>().is_ok_and(|target| actual == target)
            })
        }
        "status" => {
            let actual = finding.status;
            values.iter().any(|v| {
                let s = match v {
                    Value::Enum(s) | Value::String(s) => s.as_str(),
                    _ => return false,
                };
                s.parse::<LifecycleState>()
                    .is_ok_and(|target| actual == target)
            })
        }
        // String/array fields: any value in list matches any field value
        "tag" => {
            let targets: Vec<String> = values.iter().map(value_to_string).collect();
            targets
                .iter()
                .any(|target| finding.tags.iter().any(|t| t.eq_ignore_ascii_case(target)))
        }
        "agent" => {
            let targets: Vec<String> = values.iter().map(value_to_string).collect();
            targets.iter().any(|target| {
                finding
                    .discovered_by
                    .iter()
                    .any(|a| a.agent_id.eq_ignore_ascii_case(target))
            })
        }
        "file" => {
            let targets: Vec<String> = values.iter().map(value_to_string).collect();
            targets.iter().any(|target| {
                finding
                    .locations
                    .iter()
                    .any(|l| l.file_path.eq_ignore_ascii_case(target))
            })
        }
        _ => {
            let actual = single_string_field(field, finding);
            let targets: Vec<String> = values.iter().map(value_to_string).collect();
            targets.iter().any(|t| actual.eq_ignore_ascii_case(t))
        }
    }
}

/// Get a single string field value from a finding.
fn single_string_field(field: &str, finding: &Finding) -> String {
    match field {
        "title" => finding.title.clone(),
        "description" => finding.description.clone(),
        "category" => finding.category.clone(),
        "rule" => finding.rule_id.clone(),
        "severity" => finding.severity.to_string().to_lowercase(),
        "status" => finding.status.to_string(),
        _ => String::new(),
    }
}

/// Extract the string representation from a Value.
fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) | Value::Enum(s) => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Duration(d) => format!("{}s", d.as_secs()),
    }
}

/// Apply a comparison operator to an `Ordering`.
fn apply_ord_op(op: CompareOp, ord: std::cmp::Ordering) -> bool {
    match op {
        CompareOp::Eq => ord == std::cmp::Ordering::Equal,
        CompareOp::Ne => ord != std::cmp::Ordering::Equal,
        CompareOp::Gt => ord == std::cmp::Ordering::Greater,
        CompareOp::Lt => ord == std::cmp::Ordering::Less,
        CompareOp::GtEq => ord != std::cmp::Ordering::Less,
        CompareOp::LtEq => ord != std::cmp::Ordering::Greater,
    }
}

/// Apply `TallyQL` and enhanced filters to a findings list (mutates in place).
///
/// Used by both CLI `handle_query()` and MCP `query_findings()` to share
/// filter logic and prevent divergence.
#[allow(clippy::too_many_arguments)]
pub fn apply_filters(
    findings: &mut Vec<Finding>,
    filter_expr: Option<&FilterExpr>,
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
    agent: Option<&str>,
    category: Option<&str>,
    not_status: Option<LifecycleState>,
    text_search: Option<&str>,
) {
    if let Some(expr) = filter_expr {
        findings.retain(|f| evaluate(expr, f));
    }
    if let Some(since) = since {
        findings.retain(|f| f.created_at >= since);
    }
    if let Some(before) = before {
        findings.retain(|f| f.created_at < before);
    }
    if let Some(agent_id) = agent {
        findings.retain(|f| f.discovered_by.iter().any(|a| a.agent_id == agent_id));
    }
    if let Some(cat) = category {
        findings.retain(|f| f.category == cat);
    }
    if let Some(excluded) = not_status {
        findings.retain(|f| f.status != excluded);
    }
    if let Some(search) = text_search {
        let search_lower = search.to_lowercase();
        findings.retain(|f| {
            f.title.to_lowercase().contains(&search_lower)
                || f.description.to_lowercase().contains(&search_lower)
                || f.suggested_fix
                    .as_deref()
                    .is_some_and(|s| s.to_lowercase().contains(&search_lower))
                || f.evidence
                    .as_deref()
                    .is_some_and(|s| s.to_lowercase().contains(&search_lower))
        });
    }
}

/// Sort findings by the given sort specifications.
///
/// Each [`SortSpec`] specifies a field and direction. Earlier specs have
/// higher priority (primary sort key first).
pub fn apply_sort(findings: &mut [Finding], sort_specs: &[SortSpec]) {
    if sort_specs.is_empty() {
        return;
    }
    findings.sort_by(|a, b| {
        for spec in sort_specs {
            let ord = compare_field(a, b, &spec.field);
            let ord = if spec.descending { ord.reverse() } else { ord };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    });
}

/// Compare two findings by a single field name.
fn compare_field(a: &Finding, b: &Finding, field: &str) -> std::cmp::Ordering {
    match field {
        "severity" => severity_ordinal(a.severity).cmp(&severity_ordinal(b.severity)),
        "status" => format!("{:?}", a.status).cmp(&format!("{:?}", b.status)),
        "created_at" => a.created_at.cmp(&b.created_at),
        "updated_at" => a.updated_at.cmp(&b.updated_at),
        "title" => a.title.cmp(&b.title),
        "rule" => a.rule_id.cmp(&b.rule_id),
        "file" => {
            let a_file = a.locations.first().map_or("", |l| l.file_path.as_str());
            let b_file = b.locations.first().map_or("", |l| l.file_path.as_str());
            a_file.cmp(b_file)
        }
        _ => std::cmp::Ordering::Equal,
    }
}

/// Map severity to ordinal for ordering: critical=3 > important=2 > suggestion=1 > `tech_debt`=0.
const fn severity_ordinal(severity: Severity) -> u8 {
    match severity {
        Severity::Critical => 3,
        Severity::Important => 2,
        Severity::Suggestion => 1,
        Severity::TechDebt => 0,
    }
}
