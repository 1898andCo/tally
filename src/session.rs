//! Session-scoped short ID mapping (C1, I2, S3, TD4).
//!
//! Maps stable UUIDs to human-friendly short IDs for the duration of a session.
//! Short IDs use severity-based prefixes matching dclaude/zclaude conventions:
//! - C = Critical, I = Important, S = Suggestion, TD = Tech Debt
//!
//! Short IDs reset each session — they are for display only, not persistence.

use std::collections::HashMap;
use uuid::Uuid;

use crate::model::Severity;

/// Maps UUIDs to session-scoped short IDs and vice versa.
pub struct SessionIdMapper {
    /// UUID -> short ID
    uuid_to_short: HashMap<Uuid, String>,
    /// short ID (uppercase) -> UUID
    short_to_uuid: HashMap<String, Uuid>,
    /// Next counter per severity prefix
    counters: HashMap<&'static str, u32>,
}

impl SessionIdMapper {
    /// Create a new empty mapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            uuid_to_short: HashMap::new(),
            short_to_uuid: HashMap::new(),
            counters: HashMap::new(),
        }
    }

    /// Assign a short ID to a finding UUID based on its severity.
    ///
    /// If the UUID already has a short ID, returns the existing one.
    pub fn assign(&mut self, uuid: Uuid, severity: Severity) -> &str {
        if self.uuid_to_short.contains_key(&uuid) {
            return &self.uuid_to_short[&uuid];
        }

        let prefix = severity.short_prefix();
        let counter = self.counters.entry(prefix).or_insert(0);
        *counter += 1;
        let short_id = format!("{prefix}{counter}");

        self.short_to_uuid.insert(short_id.to_uppercase(), uuid);
        self.uuid_to_short.insert(uuid, short_id);
        &self.uuid_to_short[&uuid]
    }

    /// Look up a UUID by short ID (case-insensitive).
    #[must_use]
    pub fn resolve(&self, short_id: &str) -> Option<Uuid> {
        self.short_to_uuid.get(&short_id.to_uppercase()).copied()
    }

    /// Look up a short ID by UUID.
    #[must_use]
    pub fn short_id(&self, uuid: &Uuid) -> Option<&str> {
        self.uuid_to_short.get(uuid).map(String::as_str)
    }

    /// Resolve an input that could be either a UUID string or a short ID.
    ///
    /// Tries UUID parse first, then short ID lookup.
    #[must_use]
    pub fn resolve_id(&self, input: &str) -> Option<Uuid> {
        // Try UUID first
        if let Ok(uuid) = Uuid::parse_str(input) {
            return Some(uuid);
        }
        // Try short ID
        self.resolve(input)
    }

    /// Number of assigned IDs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.uuid_to_short.len()
    }

    /// Whether any IDs have been assigned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.uuid_to_short.is_empty()
    }
}

impl Default for SessionIdMapper {
    fn default() -> Self {
        Self::new()
    }
}
