//! Handler for `tally rebuild-index`.

use crate::error::Result;
use crate::storage::GitFindingsStore;

/// Handle `tally rebuild-index`.
///
/// # Errors
///
/// Returns error if storage fails.
#[tracing::instrument(skip_all)]
pub fn handle_rebuild_index(store: &GitFindingsStore, include_rules: bool) -> Result<()> {
    store.rebuild_index()?;
    tracing::info!("Index rebuilt");

    if include_rules {
        store.rebuild_rule_counts()?;
        tracing::info!("Rule finding counts rebuilt");
    }

    Ok(())
}
