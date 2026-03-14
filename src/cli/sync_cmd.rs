//! Handler for `tally sync`.

use crate::error::Result;
use crate::storage::GitFindingsStore;

/// Handle `tally sync`.
///
/// # Errors
///
/// Returns error if remote operations fail.
#[tracing::instrument(skip_all, fields(remote = remote))]
pub fn handle_sync(store: &GitFindingsStore, remote: &str) -> Result<()> {
    let result = store.sync(remote)?;
    tracing::info!(
        fetched = result.fetched,
        merged = result.merged,
        pushed = result.pushed,
        "Sync complete"
    );
    Ok(())
}
