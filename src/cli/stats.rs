//! Handler for `tally stats`.

use crate::error::Result;
use crate::model::{LifecycleState, Severity};
use crate::storage::GitFindingsStore;

/// Handle `tally stats`.
///
/// # Errors
///
/// Returns error if storage fails.
#[tracing::instrument(skip_all)]
pub fn handle_stats(store: &GitFindingsStore) -> Result<()> {
    let findings = store.load_all()?;

    let mut by_severity = std::collections::HashMap::new();
    let mut by_status = std::collections::HashMap::new();

    for finding in &findings {
        *by_severity.entry(finding.severity).or_insert(0u32) += 1;
        *by_status.entry(finding.status).or_insert(0u32) += 1;
    }

    println!("Findings Summary");
    for sev in [
        Severity::Critical,
        Severity::Important,
        Severity::Suggestion,
        Severity::TechDebt,
    ] {
        println!("  {sev:<12} {}", by_severity.get(&sev).unwrap_or(&0));
    }
    println!("  Total:       {}", findings.len());
    println!();

    for state in LifecycleState::all() {
        if let Some(&count) = by_status.get(state) {
            if count > 0 {
                println!("  {state:<15} {count}");
            }
        }
    }

    Ok(())
}
