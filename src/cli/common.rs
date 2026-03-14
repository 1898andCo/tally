//! Shared helpers used across CLI handler modules.

use chrono::Utc;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED};
use uuid::Uuid;

use crate::error::{Result, TallyError};
use crate::model::{
    Finding, FindingRelationship, LifecycleState, Location, LocationRole, RelationshipType,
    Severity, StateTransition,
};
use crate::session::SessionIdMapper;
use crate::storage::GitFindingsStore;

/// Resolve a finding ID that can be either a UUID or a session short ID (C1, I2, etc.).
///
/// Loads all findings to build a session mapper if the input isn't a valid UUID.
pub(crate) fn resolve_finding_id(store: &GitFindingsStore, id_str: &str) -> Result<Uuid> {
    // Try UUID first
    if let Ok(uuid) = Uuid::parse_str(id_str) {
        return Ok(uuid);
    }

    // Try short ID — need to load all findings to build the mapper
    let findings = store.load_all()?;
    let mut mapper = SessionIdMapper::new();
    for finding in &findings {
        mapper.assign(finding.uuid, finding.severity);
    }

    mapper.resolve(id_str).ok_or_else(|| TallyError::NotFound {
        uuid: id_str.to_string(),
    })
}

/// Check for expired suppressions and reopen them.
///
/// Iterates findings: if `status == Suppressed` and `suppression.expires_at < now`,
/// transitions to `Open`, clears suppression, and saves.
pub(crate) fn check_expiry_and_reopen(store: &GitFindingsStore, findings: &mut [Finding]) {
    let now = Utc::now();
    for finding in findings.iter_mut() {
        if finding.status != LifecycleState::Suppressed {
            continue;
        }
        let expired = finding
            .suppression
            .as_ref()
            .and_then(|s| s.expires_at)
            .is_some_and(|exp| exp < now);
        if expired {
            finding.state_history.push(StateTransition {
                from: LifecycleState::Suppressed,
                to: LifecycleState::Open,
                timestamp: now,
                agent_id: "system".to_string(),
                reason: Some("Suppression expired".to_string()),
                commit_sha: None,
            });
            finding.status = LifecycleState::Open;
            finding.suppression = None;
            finding.updated_at = now;
            // Best-effort save; don't fail the query if this errors
            let _ = store.save_finding(finding);
        }
    }
}

/// Parse a comma-separated tags string into a `Vec<String>`.
pub(crate) fn parse_tags(tags_str: &str) -> Vec<String> {
    tags_str
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Parse a `--location` flag value: `file:line_start:line_end:role` or `file:line:role`.
pub(crate) fn parse_location_flag(s: &str) -> Result<Location> {
    let parts: Vec<&str> = s.splitn(4, ':').collect();
    match parts.len() {
        3 => {
            // file:line:role
            let line: u32 = parts[1].parse().map_err(|_| {
                TallyError::InvalidInput(format!("invalid line number in location: {s}"))
            })?;
            let role = parse_location_role(parts[2])?;
            Ok(Location {
                file_path: parts[0].to_string(),
                line_start: line,
                line_end: line,
                role,
                message: None,
            })
        }
        4 => {
            // file:line_start:line_end:role
            let line_start: u32 = parts[1].parse().map_err(|_| {
                TallyError::InvalidInput(format!("invalid line_start in location: {s}"))
            })?;
            let line_end: u32 = parts[2].parse().map_err(|_| {
                TallyError::InvalidInput(format!("invalid line_end in location: {s}"))
            })?;
            let role = parse_location_role(parts[3])?;
            Ok(Location {
                file_path: parts[0].to_string(),
                line_start,
                line_end,
                role,
                message: None,
            })
        }
        _ => Err(TallyError::InvalidInput(format!(
            "invalid location format: '{s}' (expected file:line:role or file:line_start:line_end:role)"
        ))),
    }
}

/// Parse a location role string.
pub(crate) fn parse_location_role(s: &str) -> Result<LocationRole> {
    match s.to_ascii_lowercase().as_str() {
        "primary" => Ok(LocationRole::Primary),
        "secondary" => Ok(LocationRole::Secondary),
        "context" => Ok(LocationRole::Context),
        other => Err(TallyError::InvalidInput(format!(
            "invalid location role: '{other}' (valid: primary, secondary, context)"
        ))),
    }
}

/// Add an explicit user-specified relationship between findings.
pub(crate) fn add_explicit_relationship(
    store: &GitFindingsStore,
    finding_uuid: Uuid,
    related_id_str: &str,
    relationship_str: &str,
) -> Result<()> {
    let related_uuid = resolve_finding_id(store, related_id_str)?;
    let rel_type: RelationshipType = relationship_str
        .parse()
        .map_err(|e: String| TallyError::InvalidInput(e))?;

    let mut finding = store.load_finding(&finding_uuid)?;
    finding.relationships.push(FindingRelationship {
        related_finding_id: related_uuid,
        relationship_type: rel_type,
        reason: None,
        created_at: Utc::now(),
    });
    finding.updated_at = Utc::now();
    store.save_finding(&finding)
}

pub(crate) fn print_json(value: &impl serde::Serialize) {
    if let Ok(json) = serde_json::to_string_pretty(value) {
        println!("{json}");
    }
}

pub(crate) fn print_json_with_short_ids(findings: &[Finding], mapper: &SessionIdMapper) {
    #[derive(serde::Serialize)]
    struct FindingWithShortId<'a> {
        short_id: &'a str,
        #[serde(flatten)]
        finding: &'a Finding,
    }

    let enriched: Vec<FindingWithShortId<'_>> = findings
        .iter()
        .map(|f| FindingWithShortId {
            short_id: mapper.short_id(&f.uuid).unwrap_or("?"),
            finding: f,
        })
        .collect();

    print_json(&enriched);
}

pub(crate) fn print_table(findings: &[Finding], mapper: &SessionIdMapper) {
    if findings.is_empty() {
        println!("No findings.");
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(vec![
        "ID", "UUID", "Severity", "Status", "File", "Line", "Title",
    ]);

    for finding in findings {
        let short = mapper.short_id(&finding.uuid).unwrap_or("?");
        let (file, line) = finding
            .locations
            .first()
            .map_or(("?", "?".to_string()), |l| {
                (l.file_path.as_str(), l.line_start.to_string())
            });

        table.add_row(vec![
            short,
            &finding.uuid.to_string()[..8],
            &finding.severity.to_string(),
            &finding.status.to_string(),
            file,
            &line,
            &finding.title,
        ]);
    }

    println!("{table}");
}

pub(crate) fn print_summary(findings: &[Finding]) {
    let mut by_severity = std::collections::HashMap::new();
    for finding in findings {
        *by_severity.entry(finding.severity).or_insert(0u32) += 1;
    }

    println!("Query Results: {} findings", findings.len());
    for sev in [
        Severity::Critical,
        Severity::Important,
        Severity::Suggestion,
        Severity::TechDebt,
    ] {
        if let Some(&count) = by_severity.get(&sev) {
            if count > 0 {
                println!("  {sev}: {count}");
            }
        }
    }
}
