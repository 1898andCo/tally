//! Rule registry — centralized rule management with semantic matching.
//!
//! Provides a rule registry for normalizing, deduplicating, and managing
//! rule IDs across multiple AI agents. Rules are stored as individual JSON
//! files on the `findings-data` branch (`rules/<rule-id>.json`).

pub mod matcher;
pub mod normalize;
pub mod rule;
pub mod stopwords;
pub mod store;

pub use matcher::{MatchResult, RuleMatcher};
pub use normalize::{normalize_rule_id, validate_rule_id};
pub use rule::{Rule, RuleExample, RuleScope, RuleStatus};
pub use store::RuleStore;
