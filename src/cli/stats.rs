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
    let mut with_notes = 0u32;
    let mut with_edits = 0u32;
    let mut tag_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    for finding in &findings {
        *by_severity.entry(finding.severity).or_insert(0u32) += 1;
        *by_status.entry(finding.status).or_insert(0u32) += 1;
        if !finding.notes.is_empty() {
            with_notes += 1;
        }
        if !finding.edit_history.is_empty() {
            with_edits += 1;
        }
        for tag in &finding.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
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

    if with_notes > 0 || with_edits > 0 {
        println!();
        println!("  Findings with notes: {with_notes}");
        println!("  Findings with edits: {with_edits}");
    }

    if !tag_counts.is_empty() {
        println!();
        println!("  Top tags:");
        let mut sorted: Vec<_> = tag_counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        for (tag, count) in sorted.into_iter().take(5) {
            println!("    {tag:<30} {count}");
        }
    }

    // Doctor check: warn if findings-data has no upstream tracking branch
    if !findings.is_empty() && !store.has_remote_branch() {
        println!();
        println!(
            "  Warning: findings-data branch is not pushed to remote — \
             findings are local-only. Run `tally sync` to push."
        );
    }

    Ok(())
}
