//! Handler for `tally suppress`.

use chrono::Utc;

use crate::error::{Result, TallyError};
use crate::model::{LifecycleState, StateTransition, Suppression, SuppressionType};
use crate::storage::GitFindingsStore;

use super::common::{print_json, resolve_finding_id};

/// Handle `tally suppress`.
///
/// # Errors
///
/// Returns error if finding not found, transition invalid, or storage fails.
#[tracing::instrument(skip_all, fields(id = id_str))]
pub fn handle_suppress(
    store: &GitFindingsStore,
    id_str: &str,
    reason: &str,
    expires: Option<&str>,
    agent: &str,
    suppression_type_str: &str,
    suppression_pattern: Option<&str>,
) -> Result<()> {
    let uuid = resolve_finding_id(store, id_str)?;
    let mut finding = store.load_finding(&uuid)?;

    if !finding.status.can_transition_to(LifecycleState::Suppressed) {
        return Err(TallyError::InvalidTransition {
            from: finding.status,
            to: LifecycleState::Suppressed,
            valid: finding.status.allowed_transitions().to_vec(),
        });
    }

    let expires_at = expires
        .map(|s| {
            s.parse::<chrono::DateTime<Utc>>()
                .map_err(|e| TallyError::InvalidInput(format!("invalid date: {e}")))
        })
        .transpose()?;

    finding.state_history.push(StateTransition {
        from: finding.status,
        to: LifecycleState::Suppressed,
        timestamp: Utc::now(),
        agent_id: agent.to_string(),
        reason: Some(reason.to_string()),
        commit_sha: None,
    });
    finding.status = LifecycleState::Suppressed;
    let parsed_suppression_type =
        parse_suppression_type(suppression_type_str, suppression_pattern)?;
    finding.suppression = Some(Suppression {
        suppressed_at: Utc::now(),
        reason: reason.to_string(),
        expires_at,
        suppression_type: parsed_suppression_type,
    });
    finding.updated_at = Utc::now();

    store.save_finding(&finding)?;

    print_json(&serde_json::json!({
        "uuid": uuid.to_string(),
        "status": "suppressed",
        "expires_at": expires_at.map(|d| d.to_rfc3339()),
    }));

    Ok(())
}

/// Parse a suppression type string into a `SuppressionType`.
fn parse_suppression_type(type_str: &str, pattern: Option<&str>) -> Result<SuppressionType> {
    match type_str.to_ascii_lowercase().as_str() {
        "global" => Ok(SuppressionType::Global),
        "file" | "file_level" => Ok(SuppressionType::FileLevel),
        "inline" | "inline_comment" => {
            let p = pattern.ok_or_else(|| {
                TallyError::InvalidInput(
                    "inline suppression requires --suppression-pattern".to_string(),
                )
            })?;
            Ok(SuppressionType::InlineComment {
                pattern: p.to_string(),
            })
        }
        other => Err(TallyError::InvalidInput(format!(
            "invalid suppression type: '{other}' (valid: global, file, inline)"
        ))),
    }
}
