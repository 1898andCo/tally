//! Finding lifecycle state machine with validated transitions.
//!
//! 10 states modeled after `SonarQube` + `CodeClimate` + Semgrep.
//! All transitions are recorded with timestamp, agent, reason, and optional commit.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Finding lifecycle state.
///
/// Transitions are validated — see [`LifecycleState::allowed_transitions`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleState {
    #[default]
    Open,
    Acknowledged,
    InProgress,
    Resolved,
    FalsePositive,
    WontFix,
    Deferred,
    Suppressed,
    Reopened,
    Closed,
}

impl LifecycleState {
    /// Valid transitions from this state.
    ///
    /// Any transition not in this list is rejected with an error.
    #[must_use]
    #[allow(clippy::match_same_arms)] // Each state is semantically distinct
    pub fn allowed_transitions(&self) -> &'static [LifecycleState] {
        match self {
            Self::Open => &[
                Self::Acknowledged,
                Self::InProgress,
                Self::FalsePositive,
                Self::Deferred,
                Self::Suppressed,
            ],
            Self::Acknowledged => &[
                Self::InProgress,
                Self::FalsePositive,
                Self::WontFix,
                Self::Deferred,
            ],
            Self::InProgress => &[Self::Resolved, Self::WontFix, Self::Deferred],
            Self::Resolved => &[Self::Reopened, Self::Closed],
            Self::FalsePositive => &[Self::Reopened, Self::Closed],
            Self::WontFix => &[Self::Reopened, Self::Closed],
            Self::Deferred => &[Self::Open, Self::Closed],
            Self::Suppressed => &[Self::Open, Self::Closed],
            Self::Reopened => &[Self::Acknowledged, Self::InProgress],
            Self::Closed => &[], // Terminal — no transitions
        }
    }

    /// Check if transitioning to `target` is valid from this state.
    #[must_use]
    pub fn can_transition_to(&self, target: Self) -> bool {
        self.allowed_transitions().contains(&target)
    }

    /// All possible lifecycle states.
    #[must_use]
    pub fn all() -> &'static [LifecycleState] {
        &[
            Self::Open,
            Self::Acknowledged,
            Self::InProgress,
            Self::Resolved,
            Self::FalsePositive,
            Self::WontFix,
            Self::Deferred,
            Self::Suppressed,
            Self::Reopened,
            Self::Closed,
        ]
    }
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Acknowledged => write!(f, "acknowledged"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Resolved => write!(f, "resolved"),
            Self::FalsePositive => write!(f, "false_positive"),
            Self::WontFix => write!(f, "wont_fix"),
            Self::Deferred => write!(f, "deferred"),
            Self::Suppressed => write!(f, "suppressed"),
            Self::Reopened => write!(f, "reopened"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

impl std::str::FromStr for LifecycleState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().replace('-', "_").as_str() {
            "open" => Ok(Self::Open),
            "acknowledged" => Ok(Self::Acknowledged),
            "in_progress" => Ok(Self::InProgress),
            "resolved" => Ok(Self::Resolved),
            "false_positive" => Ok(Self::FalsePositive),
            "wont_fix" => Ok(Self::WontFix),
            "deferred" => Ok(Self::Deferred),
            "suppressed" => Ok(Self::Suppressed),
            "reopened" => Ok(Self::Reopened),
            "closed" => Ok(Self::Closed),
            other => Err(format!(
                "invalid lifecycle state: '{other}' (valid: open, acknowledged, in_progress, \
                 resolved, false_positive, wont_fix, deferred, suppressed, reopened, closed)"
            )),
        }
    }
}

/// A recorded state transition with full provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    #[serde(default)]
    pub from: LifecycleState,
    #[serde(default)]
    pub to: LifecycleState,
    #[serde(default = "crate::model::finding::default_datetime")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}
