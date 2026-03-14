//! Finding data model — the core struct representing an issue discovered by an AI agent.
//!
//! Identity is a hybrid of UUID (stable reference), content fingerprint
//! (deduplication), and rule ID (grouping). Modeled after `SonarQube`
//! (content hash + rule), `CodeClimate` (UUID + remapping), and git-bug
//! (content-addressed).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::state_machine::{LifecycleState, StateTransition};

/// Default schema version for new findings.
#[must_use]
pub fn default_schema_version() -> String {
    "1.0.0".to_string()
}

/// Default datetime for deserialization of legacy files.
pub(crate) fn default_datetime() -> DateTime<Utc> {
    Utc::now()
}

/// A finding represents a single issue discovered in code by an AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    // --- Schema ---
    /// Schema version for forward/backward compatibility.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,

    // --- Identity ---
    /// Stable UUID v7 (time-ordered). Assigned on first creation, never changes.
    #[serde(default)]
    pub uuid: Uuid,
    /// SHA-256 of (`file_path` + `line_range` + `rule_id`). For deduplication and
    /// re-matching after refactoring.
    #[serde(default)]
    pub content_fingerprint: String,
    /// Grouping key: "unsafe-unwrap", "sql-injection", "missing-test", etc.
    /// Enables "show all instances of this rule" queries.
    #[serde(default)]
    pub rule_id: String,

    // --- Locations (multi-file supported) ---
    /// Primary location + optional secondary locations (cross-file findings).
    /// Maps to SARIF multi-location representation.
    #[serde(default)]
    pub locations: Vec<Location>,

    // --- Classification ---
    #[serde(default)]
    pub severity: Severity,
    #[serde(default)]
    pub category: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    // --- Description ---
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,

    // --- Lifecycle ---
    #[serde(default)]
    pub status: LifecycleState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_history: Vec<StateTransition>,

    // --- Provenance ---
    #[serde(default)]
    pub discovered_by: Vec<AgentRecord>,
    #[serde(default = "default_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_datetime")]
    pub updated_at: DateTime<Utc>,

    // --- Context ---
    #[serde(default)]
    pub repo_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,

    // --- Relationships ---
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<FindingRelationship>,

    // --- Suppression ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppression: Option<Suppression>,
}

/// A code location — file path + line range + role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub role: LocationRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Role of a location within a multi-location finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LocationRole {
    /// The main issue location.
    Primary,
    /// Supporting evidence in another file.
    Secondary,
    /// Additional context (not the issue itself).
    Context,
}

/// 4-tier severity matching dclaude/zclaude conventions.
/// Maps to SARIF on export: Critical->error, Important->warning,
/// Suggestion->note, TechDebt->none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Severity {
    Critical,
    Important,
    #[default]
    Suggestion,
    TechDebt,
}

impl Severity {
    /// Short ID prefix for session-scoped display (C1, I2, S3, TD4).
    #[must_use]
    pub fn short_prefix(&self) -> &'static str {
        match self {
            Self::Critical => "C",
            Self::Important => "I",
            Self::Suggestion => "S",
            Self::TechDebt => "TD",
        }
    }

    /// Map to SARIF level string.
    #[must_use]
    pub fn to_sarif_level(&self) -> &'static str {
        match self {
            Self::Critical => "error",
            Self::Important => "warning",
            Self::Suggestion => "note",
            Self::TechDebt => "none",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::Important => write!(f, "IMPORTANT"),
            Self::Suggestion => write!(f, "SUGGESTION"),
            Self::TechDebt => write!(f, "TECH_DEBT"),
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "critical" => Ok(Self::Critical),
            "important" => Ok(Self::Important),
            "suggestion" => Ok(Self::Suggestion),
            "tech_debt" | "tech-debt" | "techdebt" => Ok(Self::TechDebt),
            other => Err(format!(
                "invalid severity: '{other}' (valid: critical, important, suggestion, tech_debt)"
            )),
        }
    }
}

/// Record of which agent discovered this finding and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default = "default_datetime")]
    pub detected_at: DateTime<Utc>,
    /// Session-scoped short ID for display (e.g., "C1", "I2").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_short_id: Option<String>,
}

/// A typed link between two findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingRelationship {
    pub related_finding_id: Uuid,
    pub relationship_type: RelationshipType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Types of relationships between findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RelationshipType {
    /// This finding is a duplicate of the related finding.
    DuplicateOf,
    /// This finding blocks resolution of the related finding.
    Blocks,
    /// This finding is related but neither blocks nor duplicates.
    RelatedTo,
    /// Fixing this finding may resolve the related finding.
    Causes,
    /// This finding was discovered while fixing the related finding.
    DiscoveredWhileFixing,
    /// This finding supersedes (replaces) the related finding.
    Supersedes,
}

impl std::fmt::Display for RelationshipType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateOf => write!(f, "duplicate_of"),
            Self::Blocks => write!(f, "blocks"),
            Self::RelatedTo => write!(f, "related_to"),
            Self::Causes => write!(f, "causes"),
            Self::DiscoveredWhileFixing => write!(f, "discovered_while_fixing"),
            Self::Supersedes => write!(f, "supersedes"),
        }
    }
}

impl std::str::FromStr for RelationshipType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace('-', "_").as_str() {
            "duplicate_of" | "duplicate" => Ok(Self::DuplicateOf),
            "blocks" => Ok(Self::Blocks),
            "related_to" | "related" => Ok(Self::RelatedTo),
            "causes" => Ok(Self::Causes),
            "discovered_while_fixing" | "discovered-while-fixing" => {
                Ok(Self::DiscoveredWhileFixing)
            }
            "supersedes" => Ok(Self::Supersedes),
            other => Err(format!(
                "invalid relationship type: '{other}' (valid: duplicate_of, blocks, related_to, \
                 causes, discovered_while_fixing, supersedes)"
            )),
        }
    }
}

/// Suppression metadata for findings that should not be re-reported.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suppression {
    pub suppressed_at: DateTime<Utc>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub suppression_type: SuppressionType,
}

/// How the finding is suppressed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SuppressionType {
    /// Global suppression (applies everywhere).
    Global,
    /// File-level suppression.
    FileLevel,
    /// Inline comment suppression (e.g., `// tally:suppress unsafe-unwrap`).
    InlineComment { pattern: String },
}
