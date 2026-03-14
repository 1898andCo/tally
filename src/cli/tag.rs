//! Handler for `tally tag`.

use chrono::Utc;

use crate::error::{Result, TallyError};
use crate::model::FieldEdit;
use crate::storage::GitFindingsStore;

use super::common::{print_json, resolve_finding_id};

/// Handle `tally tag`.
///
/// # Errors
///
/// Returns error if finding not found or no --add/--remove specified.
#[tracing::instrument(skip_all)]
pub fn handle_manage_tags(
    store: &GitFindingsStore,
    id_str: &str,
    add: &[String],
    remove: &[String],
    agent: &str,
) -> Result<()> {
    if add.is_empty() && remove.is_empty() {
        return Err(TallyError::InvalidInput(
            "at least one --add or --remove must be specified".to_string(),
        ));
    }

    let uuid = resolve_finding_id(store, id_str)?;
    let mut finding = store.load_finding(&uuid)?;
    let old_tags = finding.tags.clone();

    // Add tags (dedup)
    for tag in add {
        if !finding.tags.contains(tag) {
            finding.tags.push(tag.clone());
        }
    }

    // Remove tags
    finding.tags.retain(|t| !remove.contains(t));

    finding.edit_history.push(FieldEdit {
        field: "tags".to_string(),
        old_value: serde_json::to_value(&old_tags).unwrap_or_default(),
        new_value: serde_json::to_value(&finding.tags).unwrap_or_default(),
        timestamp: Utc::now(),
        agent_id: agent.to_string(),
    });
    finding.updated_at = Utc::now();

    store.save_finding(&finding)?;

    print_json(&serde_json::json!({
        "uuid": uuid.to_string(),
        "tags": finding.tags,
    }));

    Ok(())
}
