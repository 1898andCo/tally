//! Handlers for `tally rule` subcommands.

use chrono::Utc;

use crate::error::{Result, TallyError};
use crate::registry::matcher::RuleMatcher;
use crate::registry::normalize::validate_rule_id;
use crate::registry::rule::{Rule, RuleExample, RuleScope, RuleStatus};
use crate::registry::store::RuleStore;
use crate::storage::GitFindingsStore;

use super::OutputFormat;
use super::common::print_json;

/// Handle `tally rule create`.
///
/// # Errors
///
/// Returns error if rule ID is invalid, already exists, or storage fails.
#[allow(clippy::too_many_arguments)]
pub fn handle_rule_create(
    store: &GitFindingsStore,
    id: &str,
    name: &str,
    description: &str,
    category: Option<&str>,
    severity_hint: Option<&str>,
    aliases: &[String],
    cwe_ids: &[String],
    scope_include: &[String],
    scope_exclude: &[String],
    tags: &[String],
) -> Result<()> {
    validate_rule_id(id)?;

    // Check if rule already exists
    if RuleStore::load_rule(store, id).is_ok() {
        return Err(TallyError::InvalidInput(format!(
            "Rule '{id}' already exists. Use 'tally rule update' to modify."
        )));
    }

    // Bidirectional namespace check
    let all_rules = RuleStore::load_all_rules(store).unwrap_or_default();
    let matcher = RuleMatcher::new(all_rules);
    matcher.check_id_namespace(id, aliases)?;

    let scope = if scope_include.is_empty() && scope_exclude.is_empty() {
        None
    } else {
        Some(RuleScope {
            include: scope_include.to_vec(),
            exclude: scope_exclude.to_vec(),
        })
    };

    let now = Utc::now();
    let rule = Rule {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        category: category.unwrap_or_default().to_string(),
        severity_hint: severity_hint.unwrap_or_default().to_string(),
        tags: tags.to_vec(),
        cwe_ids: cwe_ids.to_vec(),
        aliases: aliases.to_vec(),
        scope,
        examples: vec![],
        suggested_fix_pattern: None,
        references: vec![],
        related_rules: vec![],
        created_by: "cli".to_string(),
        created_at: now,
        updated_at: now,
        status: RuleStatus::Active,
        finding_count: 0,
        embedding: None,
        embedding_model: None,
    };

    RuleStore::save_rule(store, &rule)?;

    println!("Created rule: {id} (active, {} aliases)", aliases.len());
    Ok(())
}

/// Handle `tally rule get`.
///
/// # Errors
///
/// Returns error if rule doesn't exist or storage fails.
pub fn handle_rule_get(store: &GitFindingsStore, id: &str) -> Result<()> {
    let rule = RuleStore::load_rule(store, id)?;
    print_json(&serde_json::to_value(&rule).unwrap_or_default());
    Ok(())
}

/// Handle `tally rule list`.
///
/// # Errors
///
/// Returns error if storage fails.
#[allow(clippy::too_many_lines)]
pub fn handle_rule_list(
    store: &GitFindingsStore,
    category: Option<&str>,
    status: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let mut rules = RuleStore::load_all_rules(store)?;

    // Filter
    if let Some(cat) = category {
        rules.retain(|r| r.category == cat);
    }
    if let Some(st) = status {
        let target: RuleStatus = st
            .parse()
            .map_err(|e: String| TallyError::InvalidInput(e))?;
        rules.retain(|r| r.status == target);
    }

    // Sort by ID
    rules.sort_by(|a, b| a.id.cmp(&b.id));

    match format {
        OutputFormat::Json => {
            print_json(&serde_json::to_value(&rules).unwrap_or_default());
        }
        OutputFormat::Table | OutputFormat::Summary => {
            if rules.is_empty() {
                println!("No rules found.");
                return Ok(());
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec![
                "ID", "Name", "Category", "Status", "Severity", "Aliases", "Findings",
            ]);
            for rule in &rules {
                table.add_row(vec![
                    &rule.id,
                    &rule.name,
                    &rule.category,
                    &rule.status.to_string(),
                    &rule.severity_hint,
                    &rule.aliases.join(", "),
                    &rule.finding_count.to_string(),
                ]);
            }
            println!("{table}");
        }
    }

    Ok(())
}

/// Handle `tally rule search`.
///
/// # Errors
///
/// Returns error if storage fails.
pub fn handle_rule_search(store: &GitFindingsStore, query: &str, limit: usize) -> Result<()> {
    let rules = RuleStore::load_all_rules(store)?;
    let matcher = RuleMatcher::new(rules.clone());

    let mut results: Vec<SearchResult> = Vec::new();

    // Pre-check: exact ID or alias match
    if let Some(rule) = matcher.get_rule(query) {
        results.push(SearchResult {
            id: rule.id.clone(),
            confidence: 1.0,
            method: "exact".to_string(),
            name: rule.name.clone(),
            description: rule.description.clone(),
            category: rule.category.clone(),
            status: rule.status.to_string(),
            finding_count: rule.finding_count,
            aliases: rule.aliases.clone(),
        });
    }

    // Fuzzy search: JW on IDs + Token Jaccard on descriptions
    let query_lower = query.to_ascii_lowercase();
    for rule in &rules {
        if results.iter().any(|r| r.id == rule.id) {
            continue;
        }

        // JW on rule ID
        let id_score = strsim::jaro_winkler(&query_lower, &rule.id);

        // Also check aliases
        let alias_score = rule
            .aliases
            .iter()
            .map(|a| strsim::jaro_winkler(&query_lower, a))
            .fold(0.0_f64, f64::max);

        let best_score = id_score.max(alias_score);

        // Token match on description
        let desc_score = if rule.description.is_empty() {
            0.0
        } else {
            let query_tokens: Vec<&str> = query_lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .collect();
            let desc_lower = rule.description.to_ascii_lowercase();
            let rule_words: Vec<String> = desc_lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();

            let query_set: std::collections::HashSet<&str> = query_tokens.iter().copied().collect();
            let rule_set: std::collections::HashSet<&str> =
                rule_words.iter().map(String::as_str).collect();
            let overlap = query_set.iter().filter(|q| rule_set.contains(**q)).count();
            #[allow(clippy::cast_precision_loss)]
            if query_set.is_empty() {
                0.0
            } else {
                overlap as f64 / query_set.len() as f64
            }
        };

        let final_score = best_score.max(desc_score);
        if final_score >= 0.3 {
            let method = if alias_score > id_score && alias_score >= 0.3 {
                "alias_match"
            } else if desc_score > best_score {
                "description"
            } else {
                "jaro_winkler"
            };
            results.push(SearchResult {
                id: rule.id.clone(),
                confidence: final_score,
                method: method.to_string(),
                name: rule.name.clone(),
                description: rule.description.clone(),
                category: rule.category.clone(),
                status: rule.status.to_string(),
                finding_count: rule.finding_count,
                aliases: rule.aliases.clone(),
            });
        }
    }

    // Sort by confidence descending
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    if results.is_empty() {
        println!("No matching rules found for '{query}'.");
    } else {
        print_json(&serde_json::to_value(&results).unwrap_or_default());
    }
    Ok(())
}

/// Handle `tally rule update`.
///
/// # Errors
///
/// Returns error if rule doesn't exist, alias conflicts, or storage fails.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn handle_rule_update(
    store: &GitFindingsStore,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    status: Option<&str>,
    add_aliases: &[String],
    remove_aliases: &[String],
    add_cwe: &[String],
    scope_include: &[String],
    scope_exclude: &[String],
) -> Result<()> {
    let mut rule = RuleStore::load_rule(store, id)?;

    // Validate new aliases against namespace
    if !add_aliases.is_empty() {
        let all_rules = RuleStore::load_all_rules(store).unwrap_or_default();
        let matcher = RuleMatcher::new(all_rules);
        matcher.check_id_namespace(id, add_aliases)?;
    }

    if let Some(n) = name {
        rule.name = n.to_string();
    }
    if let Some(d) = description {
        rule.description = d.to_string();
        // Invalidate embedding when description changes
        rule.embedding = None;
        rule.embedding_model = None;
    }
    if let Some(s) = status {
        rule.status = s.parse().map_err(|e: String| TallyError::InvalidInput(e))?;
    }

    for alias in add_aliases {
        if !rule.aliases.contains(alias) {
            rule.aliases.push(alias.clone());
        }
    }
    for alias in remove_aliases {
        rule.aliases.retain(|a| a != alias);
    }
    for cwe in add_cwe {
        if !rule.cwe_ids.contains(cwe) {
            rule.cwe_ids.push(cwe.clone());
        }
    }

    if !scope_include.is_empty() || !scope_exclude.is_empty() {
        let existing_scope = rule.scope.take().unwrap_or(RuleScope {
            include: vec![],
            exclude: vec![],
        });
        rule.scope = Some(RuleScope {
            include: if scope_include.is_empty() {
                existing_scope.include
            } else {
                scope_include.to_vec()
            },
            exclude: if scope_exclude.is_empty() {
                existing_scope.exclude
            } else {
                scope_exclude.to_vec()
            },
        });
    }

    rule.updated_at = Utc::now();
    RuleStore::save_rule(store, &rule)?;

    println!("Updated rule: {id}");
    Ok(())
}

/// Handle `tally rule delete` (deprecate, not actually delete).
///
/// # Errors
///
/// Returns error if rule doesn't exist or storage fails.
pub fn handle_rule_delete(store: &GitFindingsStore, id: &str, reason: &str) -> Result<()> {
    let mut rule = RuleStore::load_rule(store, id)?;
    rule.status = RuleStatus::Deprecated;
    rule.updated_at = Utc::now();
    // Store reason in references as a deprecation note
    rule.references.push(format!("deprecated: {reason}"));
    RuleStore::save_rule(store, &rule)?;

    println!("Deprecated rule: {id} (reason: {reason})");
    Ok(())
}

/// Handle `tally rule add-example`.
///
/// # Errors
///
/// Returns error if rule doesn't exist or storage fails.
pub fn handle_rule_add_example(
    store: &GitFindingsStore,
    id: &str,
    example_type: &str,
    language: &str,
    code: &str,
    explanation: &str,
) -> Result<()> {
    let mut rule = RuleStore::load_rule(store, id)?;
    rule.examples.push(RuleExample {
        example_type: example_type.to_string(),
        language: language.to_string(),
        code: code.to_string(),
        explanation: explanation.to_string(),
    });
    rule.updated_at = Utc::now();
    RuleStore::save_rule(store, &rule)?;

    println!(
        "Added {} example to rule: {id} ({} total)",
        example_type,
        rule.examples.len()
    );
    Ok(())
}

/// Handle `tally rule migrate`.
///
/// # Errors
///
/// Returns error if storage fails.
pub fn handle_rule_migrate(store: &GitFindingsStore) -> Result<()> {
    let findings = store.load_all()?;
    let existing_rules = RuleStore::load_all_rules(store).unwrap_or_default();
    let existing_ids: std::collections::HashSet<String> =
        existing_rules.iter().map(|r| r.id.clone()).collect();

    // Gather stats per unique rule_id
    let mut rule_stats: std::collections::HashMap<String, MigrationInfo> =
        std::collections::HashMap::new();

    for finding in &findings {
        let entry = rule_stats
            .entry(finding.rule_id.clone())
            .or_insert_with(|| MigrationInfo {
                count: 0,
                severities: std::collections::HashSet::new(),
                categories: std::collections::HashSet::new(),
                longest_description: String::new(),
            });
        entry.count += 1;
        entry.severities.insert(format!("{}", finding.severity));
        if !finding.category.is_empty() {
            entry.categories.insert(finding.category.clone());
        }
        if finding.description.len() > entry.longest_description.len() {
            entry.longest_description.clone_from(&finding.description);
        }
    }

    println!("Scanning {} findings...", findings.len());
    println!("Found {} unique rule IDs:", rule_stats.len());

    let mut registered = 0;
    let mut skipped = 0;
    for (rule_id, info) in &rule_stats {
        println!(
            "  {rule_id:30} ({} finding{})",
            info.count,
            if info.count == 1 { "" } else { "s" }
        );

        if existing_ids.contains(rule_id) {
            skipped += 1;
            continue;
        }

        // Validate rule ID
        if validate_rule_id(rule_id).is_err() {
            println!("  [skip] Invalid rule ID format: {rule_id}");
            skipped += 1;
            continue;
        }

        let severity_hint = highest_severity(&info.severities);
        let category = info.categories.iter().next().cloned().unwrap_or_default();

        let now = Utc::now();
        let rule = Rule {
            id: rule_id.clone(),
            name: rule_id.clone(),
            description: info.longest_description.clone(),
            category,
            severity_hint,
            tags: vec![],
            cwe_ids: vec![],
            aliases: vec![],
            scope: None,
            examples: vec![],
            suggested_fix_pattern: None,
            references: vec![],
            related_rules: vec![],
            created_by: "tally:migrate".to_string(),
            created_at: now,
            updated_at: now,
            status: RuleStatus::Experimental,
            finding_count: info.count,
            embedding: None,
            embedding_model: None,
        };

        if let Err(e) = RuleStore::save_rule(store, &rule) {
            eprintln!("  [error] Failed to register {rule_id}: {e}");
        } else {
            registered += 1;
        }
    }

    println!();
    println!("Registered {registered} rules with status: experimental");
    if skipped > 0 {
        println!("Skipped {skipped} (already registered or invalid format)");
    }

    // Detect related rules by shared prefix
    let rule_ids: Vec<&String> = rule_stats.keys().collect();
    detect_related_prefixes(&rule_ids);

    Ok(())
}

struct MigrationInfo {
    count: u64,
    severities: std::collections::HashSet<String>,
    categories: std::collections::HashSet<String>,
    longest_description: String,
}

fn highest_severity(severities: &std::collections::HashSet<String>) -> String {
    for s in &[
        "critical",
        "CRITICAL",
        "important",
        "IMPORTANT",
        "suggestion",
        "SUGGESTION",
    ] {
        if severities.contains(*s) || severities.contains(&s.to_ascii_lowercase()) {
            return s.to_ascii_lowercase();
        }
    }
    "suggestion".to_string()
}

fn detect_related_prefixes(rule_ids: &[&String]) {
    let mut prefix_groups: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for id in rule_ids {
        // Extract prefix: everything up to the last hyphen segment
        if let Some(pos) = id.rfind('-') {
            let prefix = &id[..pos];
            if prefix.len() >= 3 {
                prefix_groups
                    .entry(prefix.to_string())
                    .or_default()
                    .push((*id).clone());
            }
        }
    }

    let groups: Vec<_> = prefix_groups
        .iter()
        .filter(|(_, ids)| ids.len() > 1)
        .collect();

    if !groups.is_empty() {
        println!("Related rules detected:");
        for (prefix, ids) in &groups {
            println!("  {prefix}-* share prefix: {}", ids.join(", "));
            println!("  Consider adding aliases or consolidating.");
        }
    }
}

#[derive(serde::Serialize)]
struct SearchResult {
    id: String,
    confidence: f64,
    method: String,
    name: String,
    description: String,
    category: String,
    status: String,
    finding_count: u64,
    aliases: Vec<String>,
}
