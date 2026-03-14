//! Handler for `tally note`.

use crate::error::{Result, TallyError};
use crate::storage::GitFindingsStore;

use super::common::{print_json, resolve_finding_id};

/// Handle `tally note`.
///
/// # Errors
///
/// Returns error if finding not found or note text is empty.
#[tracing::instrument(skip_all)]
pub fn handle_add_note(
    store: &GitFindingsStore,
    id_str: &str,
    text: &str,
    agent: &str,
) -> Result<()> {
    if text.is_empty() {
        return Err(TallyError::InvalidInput(
            "note text cannot be empty".to_string(),
        ));
    }

    let uuid = resolve_finding_id(store, id_str)?;
    let mut finding = store.load_finding(&uuid)?;

    finding.add_note(text, agent);
    store.save_finding(&finding)?;

    print_json(&serde_json::json!({
        "uuid": uuid.to_string(),
        "status": finding.status.to_string(),
        "notes_count": finding.notes.len(),
    }));

    Ok(())
}
