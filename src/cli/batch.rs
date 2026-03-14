//! Handler for `tally record-batch`.

use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, TallyError};
use crate::model::{
    AgentRecord, Finding, FindingIdentityResolver, IdentityResolution, LifecycleState, Location,
    LocationRole, Severity, compute_fingerprint, default_schema_version,
};
use crate::storage::GitFindingsStore;

use super::common::print_json;

/// Handle `tally record-batch`.
///
/// # Errors
///
/// Returns error if storage fails. Individual finding errors are reported
/// per-item (partial success).
#[tracing::instrument(skip_all, fields(input = input_path))]
pub fn handle_record_batch(store: &GitFindingsStore, input_path: &str, agent: &str) -> Result<()> {
    use std::io::{self, BufRead};

    let reader: Box<dyn BufRead> = if input_path == "-" {
        Box::new(io::stdin().lock())
    } else {
        let file = std::fs::File::open(input_path).map_err(TallyError::Io)?;
        Box::new(io::BufReader::new(file))
    };

    let existing = store.load_all().unwrap_or_default();
    let resolver = FindingIdentityResolver::from_findings(&existing);

    let mut total = 0u32;
    let mut succeeded = 0u32;
    let mut failed = 0u32;
    let mut results: Vec<serde_json::Value> = Vec::new();

    for (idx, line) in reader.lines().enumerate() {
        let line = line.map_err(TallyError::Io)?;
        if line.trim().is_empty() {
            continue;
        }
        total += 1;

        match process_batch_line(&line, store, &resolver, agent) {
            Ok(result) => {
                succeeded += 1;
                results.push(serde_json::json!({"index": idx, "status": "ok", "result": result}));
            }
            Err(e) => {
                failed += 1;
                results.push(serde_json::json!({
                    "index": idx,
                    "status": "error",
                    "error": e.to_string(),
                }));
            }
        }
    }

    let output = serde_json::json!({
        "total": total,
        "succeeded": succeeded,
        "failed": failed,
        "results": results,
    });
    print_json(&output);

    Ok(())
}

/// Process one line of batch JSONL input.
fn process_batch_line(
    line: &str,
    store: &GitFindingsStore,
    resolver: &FindingIdentityResolver,
    agent: &str,
) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    struct BatchEntry {
        file_path: String,
        line_start: u32,
        line_end: Option<u32>,
        severity: String,
        title: String,
        rule_id: String,
        description: Option<String>,
    }

    let entry: BatchEntry = serde_json::from_str(line).map_err(TallyError::Serialization)?;

    let severity: Severity = entry
        .severity
        .parse()
        .map_err(|e: String| TallyError::InvalidSeverity(e))?;

    let location = Location {
        file_path: entry.file_path.clone(),
        line_start: entry.line_start,
        line_end: entry.line_end.unwrap_or(entry.line_start),
        role: LocationRole::Primary,
        message: None,
    };

    let fingerprint = compute_fingerprint(&location, &entry.rule_id);
    let resolution = resolver.resolve(
        &fingerprint,
        &entry.file_path,
        entry.line_start,
        &entry.rule_id,
        5,
    );

    match resolution {
        IdentityResolution::ExistingFinding { uuid } => {
            Ok(serde_json::json!({"status": "deduplicated", "uuid": uuid.to_string()}))
        }
        IdentityResolution::NewFinding | IdentityResolution::RelatedFinding { .. } => {
            let new_uuid = Uuid::now_v7();
            let finding = Finding {
                schema_version: default_schema_version(),
                uuid: new_uuid,
                content_fingerprint: fingerprint,
                rule_id: entry.rule_id,
                locations: vec![location],
                severity,
                category: String::new(),
                tags: vec![],
                title: entry.title,
                description: entry.description.unwrap_or_default(),
                suggested_fix: None,
                evidence: None,
                status: LifecycleState::Open,
                state_history: vec![],
                discovered_by: vec![AgentRecord {
                    agent_id: agent.to_string(),
                    session_id: String::new(),
                    detected_at: Utc::now(),
                    session_short_id: None,
                }],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                repo_id: String::new(),
                branch: None,
                pr_number: None,
                commit_sha: None,
                relationships: vec![],
                suppression: None,
                notes: vec![],
                edit_history: vec![],
            };
            store.save_finding(&finding)?;
            Ok(serde_json::json!({"status": "created", "uuid": new_uuid.to_string()}))
        }
    }
}
