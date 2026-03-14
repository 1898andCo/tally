//! Error types for tally.

use crate::model::state_machine::LifecycleState;

/// Tally error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TallyError {
    #[error("finding not found: {uuid}")]
    NotFound { uuid: String },

    #[error(
        "invalid state transition: {from} -> {to} (valid targets from {from}: {})",
        valid.iter().map(std::string::ToString::to_string).collect::<Vec<_>>().join(", ")
    )]
    InvalidTransition {
        from: LifecycleState,
        to: LifecycleState,
        valid: Vec<LifecycleState>,
    },

    #[error("git storage error: {0}")]
    Git(#[from] git2::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("findings branch '{branch}' not found — run `tally init`")]
    BranchNotFound { branch: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid severity: {0}")]
    InvalidSeverity(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("no primary location — at least one location required")]
    NoLocation,
}

/// Crate-level Result alias.
pub type Result<T> = std::result::Result<T, TallyError>;
