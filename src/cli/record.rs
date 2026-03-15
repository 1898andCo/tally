//! Handler for `tally record`.

use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, TallyError};
use crate::model::{
    AgentRecord, Finding, FindingIdentityResolver, FindingRelationship, IdentityResolution,
    LifecycleState, Location, LocationRole, RelationshipType, Severity, compute_fingerprint,
    default_schema_version,
};
use crate::registry::store::RuleStore;
use crate::registry::{RuleMatcher, normalize_rule_id};
use crate::storage::GitFindingsStore;

use super::common::{add_explicit_relationship, parse_location_flag, parse_tags, print_json};

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
    pub extra_locations: &'a [String],
    pub related_to: Option<&'a str>,
    pub relationship: &'a str,
    pub category: &'a str,
    pub suggested_fix: Option<&'a str>,
    pub evidence: Option<&'a str>,
}

/// Handle `tally record`.
///
/// # Errors
///
/// Returns error if severity is invalid, storage fails, or branch doesn't exist.
#[tracing::instrument(skip_all, fields(file = args.file, rule = args.rule, severity = args.severity))]
pub fn handle_record(store: &GitFindingsStore, args: &RecordArgs<'_>) -> Result<()> {
    let severity: Severity = args
        .severity
        .parse()
        .map_err(|e: String| TallyError::InvalidSeverity(e))?;

    let primary_location = Location {
        file_path: args.file.to_string(),
        line_start: args.line,
        line_end: args.line_end.unwrap_or(args.line),
        role: LocationRole::Primary,
        message: None,
    };

    // Parse additional locations from --location flags
    let mut locations = vec![primary_location.clone()];
    for loc_str in args.extra_locations {
        locations.push(parse_location_flag(loc_str)?);
    }

    // Resolve rule ID through the registry matching pipeline
    let (canonical_rule_id, original_rule_id, match_result) =
        resolve_rule_id(store, args.rule, args.description)?;

    // Use canonical rule ID for fingerprint
    let fingerprint = compute_fingerprint(&primary_location, &canonical_rule_id);

    let existing = store.load_all().unwrap_or_default();
    let resolver = FindingIdentityResolver::from_findings(&existing);
    let resolution = resolver.resolve(&fingerprint, args.file, args.line, &canonical_rule_id, 5);

    match resolution {
        IdentityResolution::ExistingFinding { uuid } => {
            handle_dedup(store, uuid, args)?;
        }
        IdentityResolution::RelatedFinding { uuid, distance } => {
            let new_uuid = create_finding(
                store,
                severity,
                locations,
                fingerprint,
                args,
                &canonical_rule_id,
                original_rule_id.as_deref(),
            )?;
            add_relationship(store, new_uuid, uuid, distance)?;
            if let Some(related_id) = args.related_to {
                add_explicit_relationship(store, new_uuid, related_id, args.relationship)?;
            }
            let mut output = serde_json::json!({
                "status": "created",
                "uuid": new_uuid.to_string(),
                "related_to": uuid.to_string(),
                "distance": distance,
            });
            add_rule_info_to_output(&mut output, &match_result, original_rule_id.as_deref());
            print_json(&output);
        }
        IdentityResolution::NewFinding => {
            let new_uuid = create_finding(
                store,
                severity,
                locations,
                fingerprint,
                args,
                &canonical_rule_id,
                original_rule_id.as_deref(),
            )?;
            if let Some(related_id) = args.related_to {
                add_explicit_relationship(store, new_uuid, related_id, args.relationship)?;
            }
            let mut output = serde_json::json!({
                "status": "created",
                "uuid": new_uuid.to_string(),
            });
            add_rule_info_to_output(&mut output, &match_result, original_rule_id.as_deref());
            print_json(&output);
        }
    }

    Ok(())
}

/// Resolve a rule ID through the registry matching pipeline.
///
/// Returns (canonical, original if different, match result).
fn resolve_rule_id(
    store: &GitFindingsStore,
    input_rule: &str,
    description: &str,
) -> Result<(String, Option<String>, crate::registry::MatchResult)> {
    let rules = RuleStore::load_all_rules(store).unwrap_or_default();
    let matcher = RuleMatcher::new(rules);

    let desc = if description.is_empty() {
        None
    } else {
        Some(description)
    };

    let match_result = matcher.resolve(input_rule, None, desc)?;

    // Auto-register if no match found
    if match_result.method == "auto_registered" {
        let rule = crate::registry::Rule::new(
            match_result.canonical_id.clone(),
            match_result.canonical_id.clone(),
            description.to_string(),
        );
        let mut auto_rule = rule;
        auto_rule.status = crate::registry::RuleStatus::Experimental;
        auto_rule.created_by = "auto".to_string();
        // Best-effort save — don't fail the record if rule save fails
        if let Err(e) = RuleStore::save_rule(store, &auto_rule) {
            tracing::warn!(error = %e, "Failed to auto-register rule");
        }
    }

    // Determine if the rule ID was changed
    let normalized = normalize_rule_id(input_rule).unwrap_or_else(|_| input_rule.to_string());
    let original = (normalized != match_result.canonical_id).then(|| input_rule.to_string());

    Ok((match_result.canonical_id.clone(), original, match_result))
}

/// Add rule registry info to JSON output.
fn add_rule_info_to_output(
    output: &mut serde_json::Value,
    match_result: &crate::registry::MatchResult,
    original_rule_id: Option<&str>,
) {
    let obj = output.as_object_mut().expect("output should be object");
    obj.insert(
        "rule_id".to_string(),
        serde_json::Value::String(match_result.canonical_id.clone()),
    );
    if let Some(original) = original_rule_id {
        obj.insert(
            "original_rule_id".to_string(),
            serde_json::Value::String(original.to_string()),
        );
    }
    if match_result.method != "exact" {
        obj.insert(
            "normalized_by".to_string(),
            serde_json::Value::String(match_result.method.clone()),
        );
    }
    if !match_result.similar_rules.is_empty() {
        obj.insert(
            "similar_rules".to_string(),
            serde_json::to_value(&match_result.similar_rules).unwrap_or_default(),
        );
    }
}

fn handle_dedup(store: &GitFindingsStore, uuid: Uuid, args: &RecordArgs<'_>) -> Result<()> {
    let mut finding = store.load_finding(&uuid)?;
    let already_recorded = finding
        .discovered_by
        .iter()
        .any(|a| a.agent_id == args.agent && a.session_id == args.session);

    let mut changed = false;

    if !already_recorded {
        finding.discovered_by.push(AgentRecord {
            agent_id: args.agent.to_string(),
            session_id: args.session.to_string(),
            detected_at: Utc::now(),
            session_short_id: None,
        });
        changed = true;
    }

    // AC-8: Update primary location if the code moved
    let new_primary = Location {
        file_path: args.file.to_string(),
        line_start: args.line,
        line_end: args.line_end.unwrap_or(args.line),
        role: LocationRole::Primary,
        message: None,
    };
    let current_primary = finding
        .locations
        .iter()
        .find(|l| l.role == LocationRole::Primary)
        .or_else(|| finding.locations.first());
    if current_primary.is_none_or(|p| *p != new_primary) {
        if let Some(pos) = finding
            .locations
            .iter()
            .position(|l| l.role == LocationRole::Primary)
        {
            finding.locations[pos] = new_primary;
        } else if let Some(first) = finding.locations.first_mut() {
            *first = new_primary;
        } else {
            finding.locations.push(new_primary);
        }
        changed = true;
    }

    if changed {
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
    locations: Vec<Location>,
    fingerprint: String,
    args: &RecordArgs<'_>,
    canonical_rule_id: &str,
    original_rule_id: Option<&str>,
) -> Result<Uuid> {
    let new_uuid = Uuid::now_v7();
    let (repo_id, branch, commit_sha) = store.git_context();
    let finding = Finding {
        schema_version: default_schema_version(),
        uuid: new_uuid,
        content_fingerprint: fingerprint,
        rule_id: canonical_rule_id.to_string(),
        original_rule_id: original_rule_id.map(String::from),
        locations,
        severity,
        category: args.category.to_string(),
        tags: parse_tags(args.tags),
        title: args.title.to_string(),
        description: args.description.to_string(),
        suggested_fix: args.suggested_fix.map(String::from),
        evidence: args.evidence.map(String::from),
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
        repo_id,
        branch,
        pr_number: None,
        commit_sha,
        relationships: vec![],
        suppression: None,
        notes: vec![],
        edit_history: vec![],
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
