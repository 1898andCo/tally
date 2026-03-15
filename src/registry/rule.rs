//! Rule data model — the core struct representing a rule in the registry.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A rule in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Canonical rule ID (lowercase, hyphens, 2-64 chars).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Detailed description of what this rule checks.
    pub description: String,
    /// Domain category (e.g., "safety", "security", "spec-compliance").
    #[serde(default)]
    pub category: String,
    /// Suggested severity for findings matching this rule.
    #[serde(default)]
    pub severity_hint: String,
    /// Searchable tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// CWE identifiers associated with this rule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cwe_ids: Vec<String>,
    /// Alternative names that map to this canonical ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// File scope restrictions (glob patterns).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<RuleScope>,
    /// Code examples (bad/good patterns).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<RuleExample>,
    /// Suggested fix pattern (regex or template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_fix_pattern: Option<String>,
    /// External references (URLs, SARIF rule descriptors).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
    /// IDs of related rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_rules: Vec<String>,
    /// Who created this rule (agent ID or "cli" or "tally:migrate").
    #[serde(default)]
    pub created_by: String,
    /// When this rule was first created.
    #[serde(default = "default_datetime")]
    pub created_at: DateTime<Utc>,
    /// Last modification time.
    #[serde(default = "default_datetime")]
    pub updated_at: DateTime<Utc>,
    /// Lifecycle status.
    #[serde(default)]
    pub status: RuleStatus,
    /// Cached count of findings using this rule (approximate).
    #[serde(default)]
    pub finding_count: u64,
    /// Cached embedding vector for semantic search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Model used to generate the embedding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

fn default_datetime() -> DateTime<Utc> {
    Utc::now()
}

/// Rule lifecycle status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuleStatus {
    #[default]
    Active,
    Deprecated,
    Experimental,
}

impl RuleStatus {
    /// Promotion rank (higher = more promoted).
    #[must_use]
    pub fn promotion_rank(self) -> u8 {
        match self {
            Self::Deprecated => 0,
            Self::Experimental => 1,
            Self::Active => 2,
        }
    }
}

impl std::fmt::Display for RuleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Deprecated => write!(f, "deprecated"),
            Self::Experimental => write!(f, "experimental"),
        }
    }
}

impl std::str::FromStr for RuleStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "deprecated" => Ok(Self::Deprecated),
            "experimental" => Ok(Self::Experimental),
            other => Err(format!(
                "invalid rule status: '{other}' (valid: active, deprecated, experimental)"
            )),
        }
    }
}

/// File scope restrictions using glob patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleScope {
    /// Include patterns — rule applies only to files matching these globs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    /// Exclude patterns — rule does not apply to files matching these globs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

/// A code example attached to a rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleExample {
    /// Example type: "bad" or "good".
    #[serde(rename = "type")]
    pub example_type: String,
    /// Programming language.
    pub language: String,
    /// Code snippet.
    pub code: String,
    /// Explanation of why this is bad/good.
    pub explanation: String,
}

impl Rule {
    /// Create a new rule with required fields and defaults.
    #[must_use]
    pub fn new(id: String, name: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            description,
            category: String::new(),
            severity_hint: String::new(),
            tags: Vec::new(),
            cwe_ids: Vec::new(),
            aliases: Vec::new(),
            scope: None,
            examples: Vec::new(),
            suggested_fix_pattern: None,
            references: Vec::new(),
            related_rules: Vec::new(),
            created_by: String::new(),
            created_at: now,
            updated_at: now,
            status: RuleStatus::Active,
            finding_count: 0,
            embedding: None,
            embedding_model: None,
        }
    }
}
