//! Rule matching pipeline — 7-stage resolution from input rule ID to canonical.
//!
//! Stages:
//! 1. Normalize input
//! 2. Exact match (`HashMap` lookup)
//! 3. Alias lookup
//! 4. CWE cross-reference (suggestion only, confidence 0.7)
//! 5. Jaro-Winkler similarity on rule IDs (suggestion only)
//! 6. Token Jaccard on descriptions (suggestion only)
//! 7. Semantic embedding (feature-gated, deferred)
//!
//! Deep research (Mar 2026) confirmed: production tools (Semgrep, `SonarQube`,
//! SARIF v2.1) use deterministic matching for rule IDs — exact match or
//! explicit aliases only. Fuzzy matching (JW, Levenshtein) populates
//! `similar_rules` suggestions but never auto-normalizes. This prevents
//! false positives like "rule-crit1" ≠ "rule-crit2" (JW=0.97, Lev=1).
//! Auto-registration creates a new experimental rule for unknown IDs.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::normalize::normalize_rule_id;
use super::rule::Rule;
use super::stopwords::remove_stopwords;

/// Minimum confidence to include as a suggestion.
const SUGGEST_THRESHOLD: f64 = 0.6;

/// Result of matching an input rule ID against the registry.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// The canonical rule ID (either matched or the normalized input for auto-registration).
    pub canonical_id: String,
    /// Confidence score (1.0 = exact/alias, 0.0 = auto-registered new rule).
    pub confidence: f64,
    /// Method that produced the match.
    pub method: String,
    /// Similar rules found during matching (confidence 0.6-0.84 range).
    pub similar_rules: Vec<SimilarRule>,
}

/// A similar rule suggestion.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SimilarRule {
    pub id: String,
    pub confidence: f64,
    pub method: String,
}

/// The rule matcher holds a loaded registry and provides resolution.
pub struct RuleMatcher {
    /// Rules indexed by canonical ID.
    rules: HashMap<String, Rule>,
    /// Reverse lookup: alias → canonical ID.
    alias_index: HashMap<String, String>,
    /// Reverse lookup: CWE ID → list of canonical rule IDs.
    cwe_index: HashMap<String, Vec<String>>,
}

impl RuleMatcher {
    /// Build a matcher from a list of rules.
    #[must_use]
    pub fn new(rules: Vec<Rule>) -> Self {
        let mut alias_index = HashMap::new();
        let mut cwe_index: HashMap<String, Vec<String>> = HashMap::new();
        let mut rule_map = HashMap::new();

        for rule in rules {
            for alias in &rule.aliases {
                alias_index.insert(alias.clone(), rule.id.clone());
            }
            for cwe in &rule.cwe_ids {
                cwe_index
                    .entry(cwe.clone())
                    .or_default()
                    .push(rule.id.clone());
            }
            rule_map.insert(rule.id.clone(), rule);
        }

        Self {
            rules: rule_map,
            alias_index,
            cwe_index,
        }
    }

    /// Resolve an input rule ID to a canonical ID through the matching pipeline.
    ///
    /// Only exact matches and alias lookups auto-resolve. All fuzzy matches
    /// (JW, Levenshtein, CWE, Jaccard) populate `similar_rules` as suggestions.
    /// Unknown rule IDs are auto-registered with status `experimental`.
    ///
    /// # Arguments
    /// * `input` — raw rule ID from the agent
    /// * `cwe_ids` — optional CWE IDs provided by the agent
    /// * `description` — optional description for token matching
    ///
    /// # Errors
    ///
    /// Returns error only if the input cannot be normalized to a valid rule ID.
    pub fn resolve(
        &self,
        input: &str,
        cwe_ids: Option<&[String]>,
        description: Option<&str>,
    ) -> crate::error::Result<MatchResult> {
        let mut similar_rules = Vec::new();

        // Stage 1: Normalize
        let normalized = normalize_rule_id(input)?;

        // Stage 2: Exact match
        if self.rules.contains_key(&normalized) {
            return Ok(MatchResult {
                canonical_id: normalized,
                confidence: 1.0,
                method: "exact".to_string(),
                similar_rules,
            });
        }

        // Stage 3: Alias lookup
        if let Some(canonical) = self.alias_index.get(&normalized) {
            return Ok(MatchResult {
                canonical_id: canonical.clone(),
                confidence: 1.0,
                method: "alias".to_string(),
                similar_rules,
            });
        }

        // --- From here, all matches are SUGGESTIONS only (never auto-normalize) ---

        // Stage 4: CWE cross-reference
        if let Some(cwe_ids) = cwe_ids {
            for cwe in cwe_ids {
                if let Some(rule_ids) = self.cwe_index.get(cwe) {
                    for rule_id in rule_ids {
                        similar_rules.push(SimilarRule {
                            id: rule_id.clone(),
                            confidence: 0.7,
                            method: "cwe".to_string(),
                        });
                    }
                }
            }
        }

        // Stage 5: Jaro-Winkler on rule IDs (suggestion only)
        let mut best_jw_score = 0.0_f64;
        let mut best_jw_id = String::new();

        for rule_id in self.rules.keys() {
            let score = strsim::jaro_winkler(&normalized, rule_id);
            if score > best_jw_score {
                best_jw_score = score;
                best_jw_id.clone_from(rule_id);
            }
        }

        // Also check aliases for JW
        for (alias, canonical) in &self.alias_index {
            let score = strsim::jaro_winkler(&normalized, alias);
            if score > best_jw_score {
                best_jw_score = score;
                best_jw_id.clone_from(canonical);
            }
        }

        if best_jw_score >= SUGGEST_THRESHOLD && !similar_rules.iter().any(|s| s.id == best_jw_id) {
            similar_rules.push(SimilarRule {
                id: best_jw_id,
                confidence: best_jw_score,
                method: "jaro_winkler".to_string(),
            });
        }

        // Stage 6: Token Jaccard on descriptions (suggestion only)
        if let Some(desc) = description {
            let query_tokens = tokenize(desc);
            let query_filtered = remove_stopwords(&query_tokens);

            if !query_filtered.is_empty() {
                for (rule_id, rule) in &self.rules {
                    if rule.description.is_empty() {
                        continue;
                    }
                    let rule_tokens = tokenize(&rule.description);
                    let rule_filtered = remove_stopwords(&rule_tokens);

                    let jaccard = jaccard_similarity(&query_filtered, &rule_filtered);
                    if jaccard >= 0.5 && !similar_rules.iter().any(|s| s.id == *rule_id) {
                        similar_rules.push(SimilarRule {
                            id: rule_id.clone(),
                            confidence: jaccard,
                            method: "token_jaccard".to_string(),
                        });
                    }
                }
            }
        }

        // Stage 7: Semantic embedding — deferred to feature-gated module

        // No match found — auto-register as new rule
        Ok(MatchResult {
            canonical_id: normalized,
            confidence: 0.0,
            method: "auto_registered".to_string(),
            similar_rules,
        })
    }

    /// Check if a rule ID exists in the registry (canonical or alias).
    #[must_use]
    pub fn exists(&self, id: &str) -> bool {
        self.rules.contains_key(id) || self.alias_index.contains_key(id)
    }

    /// Get a reference to a rule by canonical ID.
    #[must_use]
    pub fn get_rule(&self, id: &str) -> Option<&Rule> {
        self.rules.get(id)
    }

    /// Get all rules.
    #[must_use]
    pub fn rules(&self) -> &HashMap<String, Rule> {
        &self.rules
    }

    /// Check bidirectional ID namespace: alias must not match any canonical ID,
    /// and canonical ID must not match any existing alias.
    ///
    /// # Errors
    ///
    /// Returns error describing the conflict if one is found.
    pub fn check_id_namespace(
        &self,
        canonical_id: &str,
        aliases: &[String],
    ) -> crate::error::Result<()> {
        // Check if canonical ID conflicts with an existing alias
        if let Some(owner) = self.alias_index.get(canonical_id) {
            return Err(crate::error::TallyError::InvalidInput(format!(
                "rule ID '{canonical_id}' conflicts with alias on rule '{owner}'"
            )));
        }

        for alias in aliases {
            // Check if alias matches an existing canonical ID
            if self.rules.contains_key(alias) {
                return Err(crate::error::TallyError::InvalidInput(format!(
                    "alias '{alias}' conflicts with canonical rule '{alias}'"
                )));
            }

            // Check if alias is claimed by another rule
            if let Some(owner) = self.alias_index.get(alias) {
                if owner != canonical_id {
                    return Err(crate::error::TallyError::InvalidInput(format!(
                        "alias '{alias}' is already claimed by rule '{owner}'"
                    )));
                }
            }
        }

        Ok(())
    }
}

/// Tokenize text into lowercase words.
fn tokenize(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Compute Jaccard similarity between two token sets.
#[allow(clippy::cast_precision_loss)] // Token counts are small (<1000), well within f64 range
fn jaccard_similarity(a: &[&str], b: &[&str]) -> f64 {
    use std::collections::HashSet;

    if a.is_empty() && b.is_empty() {
        return 0.0;
    }

    let set_a: HashSet<&str> = a.iter().copied().collect();
    let set_b: HashSet<&str> = b.iter().copied().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}
