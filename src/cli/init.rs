//! Handler for `tally init`.

use crate::error::Result;
use crate::storage::GitFindingsStore;

/// Handle `tally init`.
///
/// # Errors
///
/// Returns error if branch creation fails.
#[tracing::instrument(skip_all)]
pub fn handle_init(store: &GitFindingsStore) -> Result<()> {
    store.init()?;
    tracing::info!("Initialized findings-data branch");
    Ok(())
}
