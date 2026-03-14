//! Handler for `tally import`.

use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, TallyError};
use crate::model::{
    AgentRecord, Finding, LifecycleState, Location, LocationRole, Severity, compute_fingerprint,
    default_schema_version,
};
use crate::storage::GitFindingsStore;

/// Handle `tally import` — import findings from dclaude/zclaude JSON state files.
///
/// # Errors
///
/// Returns error if the file cannot be read or parsed.
#[tracing::instrument(skip_all, fields(path = path))]
pub fn handle_import(store: &GitFindingsStore, path: &str) -> Result<()> {
    let content = std::fs::read_to_string(path).map_err(TallyError::Io)?;
    let state: serde_json::Value =
        serde_json::from_str(&content).map_err(TallyError::Serialization)?;

    let mut imported = 0u32;
    let mut skipped = 0u32;

    // Try dclaude format: { "active_cycle": { "findings": [...] } }
    let findings_arr = state
        .get("active_cycle")
        .and_then(|c| c.get("findings"))
        .and_then(|f| f.as_array())
        // Try zclaude format: { "reviews": [{ "findings": [...] }] }
        .or_else(|| {
            state
                .get("reviews")
                .and_then(|r| r.as_array())
                .and_then(|reviews| reviews.last())
                .and_then(|r| r.get("findings"))
                .and_then(|f| f.as_array())
        });

    let Some(findings) = findings_arr else {
        tracing::warn!("No findings found in state file, expected dclaude or zclaude format");
        return Ok(());
    };

    for entry in findings {
        match import_single_finding(entry, store) {
            Ok(uuid) => {
                imported += 1;
                tracing::info!(%uuid, "Imported finding");
            }
            Err(e) => {
                skipped += 1;
                tracing::warn!(error = %e, "Skipped finding");
            }
        }
    }

    tracing::info!(imported, skipped, "Import complete");
    Ok(())
}

/// Import a single finding from dclaude/zclaude JSON format.
fn import_single_finding(entry: &serde_json::Value, store: &GitFindingsStore) -> Result<Uuid> {
    let title = entry
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported finding");
    let file = entry
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let line: u32 = entry
        .get("lines")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1);

    // Map dclaude severity: C/I/S/TD id prefix -> tally severity
    let severity = match entry.get("severity").and_then(|v| v.as_str()) {
        Some("critical") => Severity::Critical,
        Some("important") => Severity::Important,
        Some("suggestion") => Severity::Suggestion,
        Some("tech_debt") => Severity::TechDebt,
        _ => {
            // Infer from ID prefix
            let id = entry.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.starts_with('C') {
                Severity::Critical
            } else if id.starts_with('I') {
                Severity::Important
            } else {
                // S and TD both default to Suggestion for simplicity
                Severity::Suggestion
            }
        }
    };

    // Map dclaude status
    let status = match entry.get("status").and_then(|v| v.as_str()) {
        Some("verified") => LifecycleState::Resolved,
        Some("skipped") => LifecycleState::Deferred,
        Some("wont_fix") => LifecycleState::WontFix,
        _ => LifecycleState::Open,
    };

    let category = entry
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let location = Location {
        file_path: file.to_string(),
        line_start: line,
        line_end: line,
        role: LocationRole::Primary,
        message: None,
    };

    let rule_id = if category.is_empty() {
        "imported".to_string()
    } else {
        category.clone()
    };

    let fingerprint = compute_fingerprint(&location, &rule_id);
    let new_uuid = Uuid::now_v7();

    let finding = Finding {
        schema_version: default_schema_version(),
        uuid: new_uuid,
        content_fingerprint: fingerprint,
        rule_id,
        locations: vec![location],
        severity,
        category,
        tags: vec!["imported".to_string()],
        title: title.to_string(),
        description: String::new(),
        suggested_fix: None,
        evidence: None,
        status,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: "import".to_string(),
            session_id: String::new(),
            detected_at: Utc::now(),
            session_short_id: entry.get("id").and_then(|v| v.as_str()).map(String::from),
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        repo_id: String::new(),
        branch: None,
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
    };

    store.save_finding(&finding)?;
    Ok(new_uuid)
}
