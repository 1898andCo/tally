//! Handler for `tally update-fields`.

use crate::error::{Result, TallyError};
use crate::storage::GitFindingsStore;

use super::OutputFormat;
use super::common::{print_json, resolve_finding_id};

/// Handle `tally update-fields`.
///
/// # Errors
///
/// Returns error if finding not found, field invalid, or no fields specified.
#[tracing::instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
pub fn handle_update_fields(
    store: &GitFindingsStore,
    id_str: &str,
    title: Option<&str>,
    description: Option<&str>,
    suggested_fix: Option<&str>,
    evidence: Option<&str>,
    severity: Option<&str>,
    category: Option<&str>,
    tags: Option<&str>,
    agent: &str,
    format: OutputFormat,
) -> Result<()> {
    let uuid = resolve_finding_id(store, id_str)?;
    let mut finding = store.load_finding(&uuid)?;

    let mut edits = 0u32;

    if let Some(v) = title {
        finding.edit_field("title", serde_json::json!(v), agent)?;
        edits += 1;
    }
    if let Some(v) = description {
        finding.edit_field("description", serde_json::json!(v), agent)?;
        edits += 1;
    }
    if let Some(v) = suggested_fix {
        finding.edit_field("suggested_fix", serde_json::json!(v), agent)?;
        edits += 1;
    }
    if let Some(v) = evidence {
        finding.edit_field("evidence", serde_json::json!(v), agent)?;
        edits += 1;
    }
    if let Some(v) = severity {
        finding.edit_field("severity", serde_json::json!(v), agent)?;
        edits += 1;
    }
    if let Some(v) = category {
        finding.edit_field("category", serde_json::json!(v), agent)?;
        edits += 1;
    }
    if let Some(v) = tags {
        // Parse comma-separated tags into array
        let tag_array: Vec<&str> = v
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect();
        finding.edit_field("tags", serde_json::json!(tag_array), agent)?;
        edits += 1;
    }

    if edits == 0 {
        return Err(TallyError::InvalidInput(
            "at least one field must be specified (--title, --description, --suggested-fix, --evidence, --severity, --category, --tags)".to_string(),
        ));
    }

    store.save_finding(&finding)?;

    match format {
        OutputFormat::Json => print_json(&finding),
        OutputFormat::Table | OutputFormat::Summary => {
            println!("Updated {edits} field(s) on {uuid}");
        }
    }

    Ok(())
}
