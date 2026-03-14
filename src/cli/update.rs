//! Handler for `tally update`.

use chrono::Utc;

use crate::error::{Result, TallyError};
use crate::model::{LifecycleState, StateTransition};
use crate::storage::GitFindingsStore;

use super::common::{add_explicit_relationship, print_json, resolve_finding_id};

/// Arguments for updating a finding.
pub struct UpdateArgs<'a> {
    pub id: &'a str,
    pub status: &'a str,
    pub reason: Option<&'a str>,
    pub commit: Option<&'a str>,
    pub agent: &'a str,
    pub related_to: Option<&'a str>,
    pub relationship: &'a str,
}

/// Handle `tally update`.
///
/// # Errors
///
/// Returns error if finding not found, transition invalid, or storage fails.
#[tracing::instrument(skip_all, fields(id = args.id, status = args.status))]
pub fn handle_update(store: &GitFindingsStore, args: &UpdateArgs<'_>) -> Result<()> {
    let uuid = resolve_finding_id(store, args.id)?;
    let new_status: LifecycleState = args
        .status
        .parse()
        .map_err(|e: String| TallyError::InvalidInput(e))?;

    let mut finding = store.load_finding(&uuid)?;

    if !finding.status.can_transition_to(new_status) {
        return Err(TallyError::InvalidTransition {
            from: finding.status,
            to: new_status,
            valid: finding.status.allowed_transitions().to_vec(),
        });
    }

    finding.state_history.push(StateTransition {
        from: finding.status,
        to: new_status,
        timestamp: Utc::now(),
        agent_id: args.agent.to_string(),
        reason: args.reason.map(String::from),
        commit_sha: args.commit.map(String::from),
    });
    finding.status = new_status;
    finding.updated_at = Utc::now();

    store.save_finding(&finding)?;

    // Add explicit relationship if requested
    if let Some(related_id) = args.related_to {
        add_explicit_relationship(store, uuid, related_id, args.relationship)?;
    }

    print_json(&serde_json::json!({
        "uuid": uuid.to_string(),
        "status": finding.status.to_string(),
    }));

    Ok(())
}
