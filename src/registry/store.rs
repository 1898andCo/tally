//! Git-backed rule storage — CRUD operations for rules on the findings-data branch.

use std::path::Path;

use crate::error::{Result, TallyError};
use crate::storage::GitFindingsStore;

use super::rule::Rule;

/// Directory within the findings branch that holds rule JSON files.
pub const RULES_DIR: &str = "rules";

/// Rule store operations, delegating to `GitFindingsStore`.
pub struct RuleStore;

impl RuleStore {
    /// Save a rule as `rules/<id>.json` on the findings branch.
    ///
    /// # Errors
    ///
    /// Returns error if serialization or git operations fail.
    pub fn save_rule(store: &GitFindingsStore, rule: &Rule) -> Result<()> {
        let file_path = format!("{RULES_DIR}/{}.json", rule.id);
        let content = serde_json::to_string_pretty(rule).map_err(TallyError::Serialization)?;
        store.upsert_file_pub(
            &file_path,
            content.as_bytes(),
            &format!("Save rule {}", rule.id),
        )
    }

    /// Load a single rule by ID.
    ///
    /// # Errors
    ///
    /// Returns `TallyError::Git` if the file doesn't exist,
    /// or `TallyError::Serialization` if the JSON is malformed.
    pub fn load_rule(store: &GitFindingsStore, rule_id: &str) -> Result<Rule> {
        let file_path = format!("{RULES_DIR}/{rule_id}.json");
        let content = store.read_file_pub(&file_path)?;
        let rule: Rule = serde_json::from_slice(&content).map_err(TallyError::Serialization)?;
        Ok(rule)
    }

    /// Load all rules from the branch.
    ///
    /// Returns an empty vec if the `rules/` directory doesn't exist yet.
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail (except missing directory).
    pub fn load_all_rules(store: &GitFindingsStore) -> Result<Vec<Rule>> {
        let filenames = match store.list_directory_pub(RULES_DIR) {
            Ok(names) => names,
            Err(TallyError::Git(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };

        let mut rules = Vec::new();
        for name in &filenames {
            if name == ".gitkeep" || Path::new(name).extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            let file_path = format!("{RULES_DIR}/{name}");
            match store.read_file_pub(&file_path) {
                Ok(content) => match serde_json::from_slice::<Rule>(&content) {
                    Ok(rule) => rules.push(rule),
                    Err(e) => {
                        tracing::warn!(name, error = %e, "Skipping malformed rule");
                    }
                },
                Err(e) => {
                    tracing::warn!(name, error = %e, "Failed to read rule");
                }
            }
        }

        Ok(rules)
    }

    /// Delete a rule file from the branch.
    ///
    /// # Errors
    ///
    /// Returns error if git operations fail.
    pub fn delete_rule(store: &GitFindingsStore, rule_id: &str) -> Result<()> {
        let file_path = format!("{RULES_DIR}/{rule_id}.json");
        store.remove_file_pub(&file_path, &format!("Delete rule {rule_id}"))
    }
}
