//! Handler for `tally query`.

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use uuid::Uuid;

use crate::error::{Result, TallyError};
use crate::model::{LifecycleState, Severity};
use crate::query::ast::SortSpec;
use crate::query::eval::{apply_filters, apply_sort};
use crate::query::fields::validate_sort_field;
use crate::query::parse_tallyql;
use crate::session::SessionIdMapper;
use crate::storage::GitFindingsStore;

use super::OutputFormat;
use super::common::{
    check_expiry_and_reopen, print_json_with_short_ids, print_summary, print_table,
};

/// Handle `tally query`.
///
/// # Errors
///
/// Returns error if storage fails, branch doesn't exist, or filter/sort
/// parameters are invalid.
#[tracing::instrument(skip_all, fields(format = ?format))]
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn handle_query(
    store: &GitFindingsStore,
    status_filter: Option<&str>,
    severity_filter: Option<&str>,
    file_filter: Option<&str>,
    rule_filter: Option<&str>,
    related_to_filter: Option<&str>,
    tag_filter: Option<&str>,
    filter_expr: Option<&str>,
    since: Option<&str>,
    before: Option<&str>,
    agent: Option<&str>,
    category: Option<&str>,
    not_status: Option<&str>,
    text_search: Option<&str>,
    sort_fields: &[String],
    sort_dir: Option<&str>,
    format: OutputFormat,
    limit: usize,
) -> Result<()> {
    let mut findings = store.load_all()?;

    // Check for expired suppressions and reopen them
    check_expiry_and_reopen(store, &mut findings);

    // --- Existing basic filters (backward compatible) ---

    // Multi-value status filter (comma-separated)
    if let Some(s) = status_filter {
        let statuses: Vec<LifecycleState> = s
            .split(',')
            .filter_map(|v| v.trim().parse::<LifecycleState>().ok())
            .collect();
        if !statuses.is_empty() {
            findings.retain(|f| statuses.contains(&f.status));
        }
    }

    // Multi-value severity filter (comma-separated)
    if let Some(s) = severity_filter {
        let severities: Vec<Severity> = s
            .split(',')
            .filter_map(|v| v.trim().parse::<Severity>().ok())
            .collect();
        if !severities.is_empty() {
            findings.retain(|f| severities.contains(&f.severity));
        }
    }

    if let Some(pat) = file_filter {
        findings.retain(|f| f.locations.iter().any(|l| l.file_path.contains(pat)));
    }
    if let Some(rule) = rule_filter {
        findings.retain(|f| f.rule_id == rule);
    }
    if let Some(related_id) = related_to_filter {
        if let Ok(related_uuid) = Uuid::parse_str(related_id) {
            findings.retain(|f| {
                f.relationships
                    .iter()
                    .any(|r| r.related_finding_id == related_uuid)
            });
        }
    }
    if let Some(tag) = tag_filter {
        findings.retain(|f| f.tags.iter().any(|t| t.contains(tag)));
    }

    // --- TallyQL expression filter ---

    let parsed_expr = if let Some(expr_str) = filter_expr {
        let expr = parse_tallyql(expr_str).map_err(|errs| {
            let msg = errs
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            TallyError::InvalidInput(format!("TallyQL parse error: {msg}"))
        })?;
        Some(expr)
    } else {
        None
    };

    // --- Enhanced filters via apply_filters ---

    let since_dt = since.map(parse_datetime).transpose()?;
    let before_dt = before.map(parse_datetime).transpose()?;
    let not_status_parsed = not_status
        .map(|s| {
            s.parse::<LifecycleState>()
                .map_err(TallyError::InvalidInput)
        })
        .transpose()?;

    apply_filters(
        &mut findings,
        parsed_expr.as_ref(),
        since_dt,
        before_dt,
        agent,
        category,
        not_status_parsed,
        text_search,
    );

    // --- Sorting ---

    let sort_specs = build_sort_specs(sort_fields, sort_dir)?;
    apply_sort(&mut findings, &sort_specs);

    // --- Output ---

    findings.truncate(limit);

    let mut mapper = SessionIdMapper::new();
    for finding in &findings {
        mapper.assign(finding.uuid, finding.severity);
    }

    match format {
        OutputFormat::Json => print_json_with_short_ids(&findings, &mapper),
        OutputFormat::Table => print_table(&findings, &mapper),
        OutputFormat::Summary => print_summary(&findings),
    }

    Ok(())
}

/// Parse a datetime string: accepts relative durations (`7d`, `24h`) or
/// ISO 8601 dates (`2026-03-01`, `2026-03-01T12:00:00Z`).
fn parse_datetime(input: &str) -> Result<DateTime<Utc>> {
    // Try relative duration first (via humantime)
    if let Ok(duration) = humantime::parse_duration(input) {
        let delta = TimeDelta::from_std(duration)
            .map_err(|_| TallyError::InvalidInput(format!("duration '{input}' out of range")))?;
        return Ok(Utc::now() - delta);
    }

    // Try RFC 3339 timestamp
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try ISO 8601 date (YYYY-MM-DD)
    if let Ok(date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let dt = date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always valid")
            .and_utc();
        return Ok(dt);
    }

    Err(TallyError::InvalidInput(format!(
        "invalid date/duration: '{input}'. Expected: 7d, 24h, 2026-03-01, or 2026-03-01T12:00:00Z"
    )))
}

/// Build sort specifications from CLI flags.
fn build_sort_specs(fields: &[String], dir: Option<&str>) -> Result<Vec<SortSpec>> {
    let descending = match dir {
        Some("desc") => true,
        Some("asc") | None => false,
        Some(other) => {
            return Err(TallyError::InvalidInput(format!(
                "invalid sort direction: '{other}'. Expected: asc or desc"
            )));
        }
    };

    fields
        .iter()
        .map(|f| {
            validate_sort_field(f).map_err(TallyError::InvalidInput)?;
            // Default: date fields sort desc, others asc
            let field_desc = if dir.is_none() {
                matches!(f.as_str(), "created_at" | "updated_at")
            } else {
                descending
            };
            Ok(SortSpec {
                field: f.clone(),
                descending: field_desc,
            })
        })
        .collect()
}
