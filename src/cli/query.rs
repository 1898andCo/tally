//! Handler for `tally query`.

use uuid::Uuid;

use crate::error::Result;
use crate::model::{LifecycleState, Severity};
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
/// Returns error if storage fails or branch doesn't exist.
#[tracing::instrument(skip_all, fields(format = ?format))]
#[allow(clippy::too_many_arguments)]
pub fn handle_query(
    store: &GitFindingsStore,
    status_filter: Option<&str>,
    severity_filter: Option<&str>,
    file_filter: Option<&str>,
    rule_filter: Option<&str>,
    related_to_filter: Option<&str>,
    format: OutputFormat,
    limit: usize,
) -> Result<()> {
    let mut findings = store.load_all()?;

    // Check for expired suppressions and reopen them
    check_expiry_and_reopen(store, &mut findings);

    if let Some(s) = status_filter {
        if let Ok(status) = s.parse::<LifecycleState>() {
            findings.retain(|f| f.status == status);
        }
    }
    if let Some(s) = severity_filter {
        if let Ok(severity) = s.parse::<Severity>() {
            findings.retain(|f| f.severity == severity);
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

    findings.truncate(limit);

    // Assign session short IDs for display
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
