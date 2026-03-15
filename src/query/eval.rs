//! `TallyQL` expression evaluator.
//!
//! Evaluates a [`FilterExpr`] AST against a [`Finding`], returning `bool`.
//! Also provides shared filter and sort functions used by both CLI and MCP.

use chrono::{DateTime, Utc};

use crate::model::{Finding, LifecycleState, Severity};
use crate::query::ast::{FilterExpr, SortSpec};

// Placeholder — full implementation in commit 3

/// Evaluate a filter expression against a finding.
///
/// Returns `true` if the finding matches the expression.
#[must_use]
pub fn evaluate(_expr: &FilterExpr, _finding: &Finding) -> bool {
    // Stub — returns true until commit 3 implements evaluation
    true
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
