//! `TallyQL` field name registry.
//!
//! Maps `TallyQL` field names to `Finding` struct fields and their types.
//! Used by the parser for field validation and the evaluator for
//! type-appropriate comparison logic.

/// All valid field names in `TallyQL` expressions.
pub const KNOWN_FIELDS: &[&str] = &[
    "severity",
    "status",
    "file",
    "rule",
    "title",
    "description",
    "suggested_fix",
    "evidence",
    "category",
    "agent",
    "tag",
    "created_at",
    "updated_at",
];

/// Sortable field names (subset of [`KNOWN_FIELDS`]).
pub const SORTABLE_FIELDS: &[&str] = &[
    "severity",
    "status",
    "created_at",
    "updated_at",
    "file",
    "rule",
    "title",
];

/// Type classification for a `TallyQL` field, used by the evaluator
/// to determine comparison and existence-check semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Plain string: title, description, category, rule.
    StringField,
    /// Ordered enum: severity (with numeric ordering).
    OrderedEnumField,
    /// Unordered enum: status (equality/IN only, no >/<).
    EnumField,
    /// `DateTime<Utc>`: `created_at`, `updated_at`.
    DateTimeField,
    /// `Option<String>`: `suggested_fix`, evidence. Supports HAS/MISSING.
    OptionalStringField,
    /// `Vec<String>`: tags. Any-match semantics. Supports HAS/MISSING.
    ArrayStringField,
    /// `Vec<AgentRecord>`: `discovered_by`. Any-match on `agent_id`. Supports HAS/MISSING.
    AgentArrayField,
}

/// Returns the type classification for a known field name.
///
/// # Panics
///
/// Panics if `name` is not in [`KNOWN_FIELDS`]. Callers must validate first.
#[must_use]
pub fn field_type(name: &str) -> FieldType {
    match name {
        "severity" => FieldType::OrderedEnumField,
        "status" => FieldType::EnumField,
        "file" | "rule" | "title" | "description" | "category" => FieldType::StringField,
        "suggested_fix" | "evidence" => FieldType::OptionalStringField,
        "agent" => FieldType::AgentArrayField,
        "tag" => FieldType::ArrayStringField,
        "created_at" | "updated_at" => FieldType::DateTimeField,
        _ => unreachable!("field_type called with unvalidated field: {name}"),
    }
}

/// Validate a field name against the known fields list.
///
/// # Errors
///
/// Returns an error message if the field is unknown, with a typo suggestion
/// if a close match is found (Levenshtein distance <= 2).
pub fn validate_field(name: &str) -> std::result::Result<(), String> {
    if KNOWN_FIELDS.contains(&name) {
        return Ok(());
    }

    let suggestion = KNOWN_FIELDS
        .iter()
        .filter(|f| {
            f.contains(name) || name.contains(**f) || strsim::normalized_levenshtein(f, name) >= 0.6
        })
        .copied()
        .next();

    let mut msg = format!(
        "unknown field '{name}', expected one of: {}",
        KNOWN_FIELDS.join(", ")
    );
    if let Some(s) = suggestion {
        use std::fmt::Write;
        let _ = write!(msg, ". Did you mean '{s}'?");
    }
    Err(msg)
}

/// Validate a sort field name.
///
/// # Errors
///
/// Returns an error message if the field is not sortable.
pub fn validate_sort_field(name: &str) -> std::result::Result<(), String> {
    if SORTABLE_FIELDS.contains(&name) {
        Ok(())
    } else {
        Err(format!(
            "cannot sort by '{name}', sortable fields: {}",
            SORTABLE_FIELDS.join(", ")
        ))
    }
}
