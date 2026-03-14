//! CLI command handler implementations.

use chrono::Utc;
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use uuid::Uuid;

use crate::error::{Result, TallyError};
use crate::model::{
    AgentRecord, Finding, FindingIdentityResolver, FindingRelationship, IdentityResolution,
    LifecycleState, Location, LocationRole, RelationshipType, Severity, StateTransition,
    Suppression, SuppressionType, compute_fingerprint,
};
use crate::session::SessionIdMapper;
use crate::storage::GitFindingsStore;

use super::OutputFormat;

/// Handle `tally init`.
///
/// # Errors
///
/// Returns error if branch creation fails.
pub fn handle_init(store: &GitFindingsStore) -> Result<()> {
    store.init()?;
    eprintln!("Initialized findings-data branch.");
    Ok(())
}

/// Arguments for recording a finding.
pub struct RecordArgs<'a> {
    pub file: &'a str,
    pub line: u32,
    pub line_end: Option<u32>,
    pub severity: &'a str,
    pub title: &'a str,
    pub rule: &'a str,
    pub description: &'a str,
    pub tags: &'a str,
    pub agent: &'a str,
    pub session: &'a str,
}

/// Handle `tally record`.
///
/// # Errors
///
/// Returns error if severity is invalid, storage fails, or branch doesn't exist.
pub fn handle_record(store: &GitFindingsStore, args: &RecordArgs<'_>) -> Result<()> {
    let severity: Severity = args
        .severity
        .parse()
        .map_err(|e: String| TallyError::InvalidSeverity(e))?;

    let location = Location {
        file_path: args.file.to_string(),
        line_start: args.line,
        line_end: args.line_end.unwrap_or(args.line),
        role: LocationRole::Primary,
        message: None,
    };

    let fingerprint = compute_fingerprint(&location, args.rule);

    let existing = store.load_all().unwrap_or_default();
    let resolver = FindingIdentityResolver::from_findings(&existing);
    let resolution = resolver.resolve(&fingerprint, args.file, args.line, args.rule, 5);

    match resolution {
        IdentityResolution::ExistingFinding { uuid } => {
            handle_dedup(store, uuid, args)?;
        }
        IdentityResolution::RelatedFinding { uuid, distance } => {
            let new_uuid = create_finding(store, severity, location, fingerprint, args)?;
            add_relationship(store, new_uuid, uuid, distance)?;
            print_json(&serde_json::json!({
                "status": "created",
                "uuid": new_uuid.to_string(),
                "related_to": uuid.to_string(),
                "distance": distance,
            }));
        }
        IdentityResolution::NewFinding => {
            let new_uuid = create_finding(store, severity, location, fingerprint, args)?;
            print_json(&serde_json::json!({
                "status": "created",
                "uuid": new_uuid.to_string(),
            }));
        }
    }

    Ok(())
}

/// Handle `tally query`.
///
/// # Errors
///
/// Returns error if storage fails or branch doesn't exist.
pub fn handle_query(
    store: &GitFindingsStore,
    status_filter: Option<&str>,
    severity_filter: Option<&str>,
    file_filter: Option<&str>,
    rule_filter: Option<&str>,
    format: OutputFormat,
    limit: usize,
) -> Result<()> {
    let mut findings = store.load_all()?;

    if let Some(s) = status_filter {
        if let Ok(status) = s.parse::<LifecycleState>() {
            findings.retain(|f| f.status == status);
        }
    }
    if let Some(s) = severity_filter {
        if let Ok(severity) = s.parse::<Severity>() {
            findings.retain(|f| f.severity == severity);
        }
    }
    if let Some(pat) = file_filter {
        findings.retain(|f| f.locations.iter().any(|l| l.file_path.contains(pat)));
    }
    if let Some(rule) = rule_filter {
        findings.retain(|f| f.rule_id == rule);
    }

    findings.truncate(limit);

    // Assign session short IDs for display
    let mut mapper = SessionIdMapper::new();
    for finding in &findings {
        mapper.assign(finding.uuid, finding.severity);
    }

    match format {
        OutputFormat::Json => print_json_with_short_ids(&findings, &mapper),
        OutputFormat::Table => print_table(&findings, &mapper),
        OutputFormat::Summary => print_summary(&findings),
    }

    Ok(())
}

/// Handle `tally update`.
///
/// # Errors
///
/// Returns error if finding not found, transition invalid, or storage fails.
pub fn handle_update(
    store: &GitFindingsStore,
    id_str: &str,
    status_str: &str,
    reason: Option<&str>,
    commit_sha: Option<&str>,
    agent: &str,
) -> Result<()> {
    let uuid = resolve_finding_id(store, id_str)?;
    let new_status: LifecycleState = status_str
        .parse()
        .map_err(|e: String| TallyError::InvalidSeverity(e))?;

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
        agent_id: agent.to_string(),
        reason: reason.map(String::from),
        commit_sha: commit_sha.map(String::from),
    });
    finding.status = new_status;
    finding.updated_at = Utc::now();

    store.save_finding(&finding)?;

    print_json(&serde_json::json!({
        "uuid": uuid.to_string(),
        "status": finding.status.to_string(),
    }));

    Ok(())
}

/// Handle `tally suppress`.
///
/// # Errors
///
/// Returns error if finding not found, transition invalid, or storage fails.
pub fn handle_suppress(
    store: &GitFindingsStore,
    id_str: &str,
    reason: &str,
    expires: Option<&str>,
    agent: &str,
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
                .map_err(|e| TallyError::InvalidSeverity(format!("invalid date: {e}")))
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
    finding.suppression = Some(Suppression {
        suppressed_at: Utc::now(),
        reason: reason.to_string(),
        expires_at,
        suppression_type: SuppressionType::Global,
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

/// Handle `tally stats`.
///
/// # Errors
///
/// Returns error if storage fails.
pub fn handle_stats(store: &GitFindingsStore) -> Result<()> {
    let findings = store.load_all()?;

    let mut by_severity = std::collections::HashMap::new();
    let mut by_status = std::collections::HashMap::new();

    for finding in &findings {
        *by_severity.entry(finding.severity).or_insert(0u32) += 1;
        *by_status.entry(finding.status).or_insert(0u32) += 1;
    }

    println!("Findings Summary");
    for sev in [Severity::Critical, Severity::Important, Severity::Suggestion, Severity::TechDebt] {
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

/// Handle `tally record-batch`.
///
/// # Errors
///
/// Returns error if storage fails. Individual finding errors are reported
/// per-item (partial success).
pub fn handle_record_batch(
    store: &GitFindingsStore,
    input_path: &str,
    agent: &str,
) -> Result<()> {
    use std::io::{self, BufRead};

    let reader: Box<dyn BufRead> = if input_path == "-" {
        Box::new(io::stdin().lock())
    } else {
        let file = std::fs::File::open(input_path)
            .map_err(TallyError::Io)?;
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

/// Handle `tally export`.
///
/// # Errors
///
/// Returns error if storage or serialization fails.
pub fn handle_export(
    store: &GitFindingsStore,
    format: super::ExportFormat,
    output_path: Option<&str>,
) -> Result<()> {
    let findings = store.load_all()?;

    let content = match format {
        super::ExportFormat::Json => {
            serde_json::to_string_pretty(&findings).map_err(TallyError::Serialization)?
        }
        super::ExportFormat::Csv => export_csv(&findings),
        super::ExportFormat::Sarif => export_sarif(&findings),
    };

    match output_path {
        Some(path) => {
            std::fs::write(path, &content).map_err(TallyError::Io)?;
            eprintln!("Exported {} findings to {path}", findings.len());
        }
        None => println!("{content}"),
    }

    Ok(())
}

/// Handle `tally sync`.
///
/// # Errors
///
/// Returns error if remote operations fail.
pub fn handle_sync(store: &GitFindingsStore, remote: &str) -> Result<()> {
    let result = store.sync(remote)?;
    eprintln!(
        "Sync complete: fetched={}, merged={}, pushed={}",
        result.fetched, result.merged, result.pushed
    );
    Ok(())
}

/// Handle `tally import` — import findings from dclaude/zclaude JSON state files.
///
/// # Errors
///
/// Returns error if the file cannot be read or parsed.
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
        eprintln!("No findings found in state file. Expected dclaude or zclaude format.");
        return Ok(());
    };

    for entry in findings {
        match import_single_finding(entry, store) {
            Ok(uuid) => {
                imported += 1;
                eprintln!("  Imported: {uuid}");
            }
            Err(e) => {
                skipped += 1;
                eprintln!("  Skipped: {e}");
            }
        }
    }

    eprintln!("Import complete: {imported} imported, {skipped} skipped");
    Ok(())
}

// =============================================================================
// Private helpers
// =============================================================================

/// Resolve a finding ID that can be either a UUID or a session short ID (C1, I2, etc.).
///
/// Loads all findings to build a session mapper if the input isn't a valid UUID.
fn resolve_finding_id(store: &GitFindingsStore, id_str: &str) -> Result<Uuid> {
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

fn parse_tags(tags_str: &str) -> Vec<String> {
    tags_str
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn handle_dedup(store: &GitFindingsStore, uuid: Uuid, args: &RecordArgs<'_>) -> Result<()> {
    let mut finding = store.load_finding(&uuid)?;
    let already_recorded = finding
        .discovered_by
        .iter()
        .any(|a| a.agent_id == args.agent && a.session_id == args.session);

    if !already_recorded {
        finding.discovered_by.push(AgentRecord {
            agent_id: args.agent.to_string(),
            session_id: args.session.to_string(),
            detected_at: Utc::now(),
            session_short_id: None,
        });
        finding.updated_at = Utc::now();
        store.save_finding(&finding)?;
    }

    print_json(&serde_json::json!({
        "status": "deduplicated",
        "uuid": uuid.to_string(),
    }));
    Ok(())
}

fn create_finding(
    store: &GitFindingsStore,
    severity: Severity,
    location: Location,
    fingerprint: String,
    args: &RecordArgs<'_>,
) -> Result<Uuid> {
    let new_uuid = Uuid::now_v7();
    let finding = Finding {
        uuid: new_uuid,
        content_fingerprint: fingerprint,
        rule_id: args.rule.to_string(),
        locations: vec![location],
        severity,
        category: String::new(),
        tags: parse_tags(args.tags),
        title: args.title.to_string(),
        description: args.description.to_string(),
        suggested_fix: None,
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![AgentRecord {
            agent_id: args.agent.to_string(),
            session_id: args.session.to_string(),
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
    };

    store.save_finding(&finding)?;
    Ok(new_uuid)
}

fn add_relationship(
    store: &GitFindingsStore,
    finding_uuid: Uuid,
    related_uuid: Uuid,
    distance: u32,
) -> Result<()> {
    let mut finding = store.load_finding(&finding_uuid)?;
    finding.relationships.push(FindingRelationship {
        related_finding_id: related_uuid,
        relationship_type: RelationshipType::RelatedTo,
        reason: Some(format!("Same rule within {distance} lines")),
        created_at: Utc::now(),
    });
    finding.updated_at = Utc::now();
    store.save_finding(&finding)
}

fn print_json(value: &impl serde::Serialize) {
    if let Ok(json) = serde_json::to_string_pretty(value) {
        println!("{json}");
    }
}

fn print_json_with_short_ids(findings: &[Finding], mapper: &SessionIdMapper) {
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

fn print_table(findings: &[Finding], mapper: &SessionIdMapper) {
    if findings.is_empty() {
        println!("No findings.");
        return;
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(vec!["ID", "UUID", "Severity", "Status", "File", "Line", "Title"]);

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

fn print_summary(findings: &[Finding]) {
    let mut by_severity = std::collections::HashMap::new();
    for finding in findings {
        *by_severity.entry(finding.severity).or_insert(0u32) += 1;
    }

    println!("Query Results: {} findings", findings.len());
    for sev in [Severity::Critical, Severity::Important, Severity::Suggestion, Severity::TechDebt] {
        if let Some(&count) = by_severity.get(&sev) {
            if count > 0 {
                println!("  {sev}: {count}");
            }
        }
    }
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

    let entry: BatchEntry =
        serde_json::from_str(line).map_err(TallyError::Serialization)?;

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
            };
            store.save_finding(&finding)?;
            Ok(serde_json::json!({"status": "created", "uuid": new_uuid.to_string()}))
        }
    }
}

fn export_csv(findings: &[Finding]) -> String {
    use std::fmt::Write;
    let mut out = String::from(
        "uuid,severity,status,rule_id,file_path,line_start,line_end,title,created_at\n",
    );
    for f in findings {
        let (file, ls, le) = f
            .locations
            .first()
            .map_or(("", 0, 0), |l| {
                (l.file_path.as_str(), l.line_start, l.line_end)
            });
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{}",
            f.uuid,
            f.severity,
            f.status,
            f.rule_id,
            file,
            ls,
            le,
            f.title.replace(',', ";"),
            f.created_at.to_rfc3339(),
        );
    }
    out
}

fn export_sarif(findings: &[Finding]) -> String {
    let rules: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| &f.rule_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(|rule| {
            serde_json::json!({
                "id": rule,
                "shortDescription": {"text": rule},
            })
        })
        .collect();

    let rule_ids: Vec<&str> = findings
        .iter()
        .map(|f| f.rule_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let results: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            let locations: Vec<serde_json::Value> = f
                .locations
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "physicalLocation": {
                            "artifactLocation": {"uri": l.file_path},
                            "region": {
                                "startLine": l.line_start,
                                "endLine": l.line_end,
                            }
                        }
                    })
                })
                .collect();

            serde_json::json!({
                "ruleId": f.rule_id,
                "level": f.severity.to_sarif_level(),
                "message": {"text": f.title},
                "locations": locations,
                "ruleIndex": rule_ids.iter().position(|r| *r == f.rule_id).unwrap_or(0),
            })
        })
        .collect();

    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "tally",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/1898andCo/tally",
                    "rules": rules,
                }
            },
            "results": results,
        }]
    });

    serde_json::to_string_pretty(&sarif).unwrap_or_default()
}

/// Import a single finding from dclaude/zclaude JSON format.
fn import_single_finding(
    entry: &serde_json::Value,
    store: &GitFindingsStore,
) -> Result<Uuid> {
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
