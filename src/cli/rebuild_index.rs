//! Handler for `tally rebuild-index`.

use crate::error::Result;
use crate::storage::GitFindingsStore;

/// Handle `tally rebuild-index`.
///
/// # Errors
///
/// Returns error if storage fails.
#[tracing::instrument(skip_all)]
pub fn handle_rebuild_index(store: &GitFindingsStore) -> Result<()> {
    store.rebuild_index()?;
    tracing::info!("Index rebuilt");
    Ok(())
}
