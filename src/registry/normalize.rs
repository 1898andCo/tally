//! Rule ID normalization and validation.
//!
//! Canonical normalization: lowercase, `_` → `-`, spaces → `-`,
//! strip agent namespace prefix (e.g., `dclaude:`), trim leading/trailing hyphens.
//! NO prefix stripping of `no-`/`disallow-`/`check-` — these carry semantic meaning.

use crate::error::{Result, TallyError};

/// Regex pattern for valid rule IDs after normalization.
/// 2-64 chars, lowercase alphanumeric with hyphens, no leading/trailing hyphens.
const RULE_ID_PATTERN: &str = r"^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$";

/// Normalize a rule ID to canonical form.
///
/// Steps:
/// 1. Lowercase
/// 2. Replace `_` with `-`
/// 3. Replace spaces with `-`
/// 4. Strip agent namespace prefix (e.g., `dclaude:unsafe-unwrap` → `unsafe-unwrap`)
/// 5. Trim leading/trailing hyphens
/// 6. Collapse consecutive hyphens
///
/// # Errors
///
/// Returns error if the normalized ID doesn't match the validation pattern.
pub fn normalize_rule_id(input: &str) -> Result<String> {
    let mut id = input.to_ascii_lowercase();

    // Replace underscores and spaces with hyphens
    id = id.replace('_', "-");
    id = id.replace(' ', "-");

    // Strip agent namespace prefix (anything before the first `:`)
    if let Some(pos) = id.find(':') {
        id = id[pos + 1..].to_string();
    }

    // Trim leading/trailing hyphens
    id = id.trim_matches('-').to_string();

    // Collapse consecutive hyphens
    while id.contains("--") {
        id = id.replace("--", "-");
    }

    validate_rule_id(&id)?;
    Ok(id)
}

/// Validate a rule ID against the canonical format.
///
/// Must be 2-64 chars, lowercase alphanumeric with hyphens,
/// no leading/trailing hyphens.
///
/// # Errors
///
/// Returns `InvalidInput` if the ID doesn't match.
pub fn validate_rule_id(id: &str) -> Result<()> {
    // Quick checks before regex
    if id.len() < 2 || id.len() > 64 {
        return Err(TallyError::InvalidInput(format!(
            "invalid rule ID '{id}' — must be 2-64 chars, \
             lowercase alphanumeric with hyphens"
        )));
    }

    // Check pattern: ^[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$
    let chars: Vec<char> = id.chars().collect();

    // First char must be alphanumeric
    if !chars[0].is_ascii_lowercase() && !chars[0].is_ascii_digit() {
        return Err(TallyError::InvalidInput(format!(
            "invalid rule ID '{id}' after normalization — \
             must be 2-64 chars, lowercase alphanumeric with hyphens"
        )));
    }

    // Last char must be alphanumeric
    if let Some(&last) = chars.last() {
        if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
            return Err(TallyError::InvalidInput(format!(
                "invalid rule ID '{id}' after normalization — \
                 must be 2-64 chars, lowercase alphanumeric with hyphens"
            )));
        }
    }

    // Middle chars must be alphanumeric or hyphen
    for &c in &chars[1..chars.len() - 1] {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return Err(TallyError::InvalidInput(format!(
                "invalid rule ID '{id}' after normalization — \
                 must be 2-64 chars, lowercase alphanumeric with hyphens"
            )));
        }
    }

    // Verify against regex pattern for documentation clarity
    debug_assert!(
        regex_matches(id),
        "ID '{id}' passed manual checks but not regex {RULE_ID_PATTERN}"
    );

    Ok(())
}

/// Manual regex check (no regex crate dependency).
fn regex_matches(id: &str) -> bool {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() < 2 || chars.len() > 64 {
        return false;
    }
    if !chars[0].is_ascii_lowercase() && !chars[0].is_ascii_digit() {
        return false;
    }
    if let Some(&last) = chars.last() {
        if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
            return false;
        }
    }
    for &c in &chars[1..chars.len() - 1] {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return false;
        }
    }
    true
}
