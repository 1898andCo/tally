//! Content fingerprint computation and finding identity resolution.
//!
//! Identity scheme: UUID v7 (stable) + content fingerprint (dedup) + rule ID (grouping).
//! Resolution priority: fingerprint match > nearby location (5 lines) > new finding.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

use super::finding::{Finding, Location, LocationRole};

/// Compute a deterministic content fingerprint for deduplication.
///
/// Formula: `SHA-256(file_path + ":" + line_start + "-" + line_end + ":" + rule_id)`
///
/// Uses only the primary location. Does NOT include description, title, or severity —
/// those can change without creating a new finding.
#[must_use]
pub fn compute_fingerprint(primary_location: &Location, rule_id: &str) -> String {
    let input = format!(
        "{}:{}-{}:{}",
        primary_location.file_path, primary_location.line_start, primary_location.line_end, rule_id
    );
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

/// Extract the primary location from a finding's location list.
///
/// Returns the first location with `role == Primary`, or the first location if
/// none is explicitly marked as primary.
#[must_use]
pub fn primary_location(locations: &[Location]) -> Option<&Location> {
    locations
        .iter()
        .find(|l| l.role == LocationRole::Primary)
        .or_else(|| locations.first())
}

/// Result of resolving a new finding's identity against existing findings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityResolution {
    /// Exact fingerprint match — this is the same finding (possibly relocated).
    ExistingFinding { uuid: Uuid },
    /// No fingerprint match, but a finding with the same rule is nearby (within threshold).
    RelatedFinding { uuid: Uuid, distance: u32 },
    /// No match — this is a genuinely new finding.
    NewFinding,
}

/// Resolves new findings against existing ones for deduplication.
///
/// Uses three indexes:
/// 1. Fingerprint index — exact match (highest confidence)
/// 2. Location index — same rule within N lines (medium confidence)
/// 3. Rule index — same rule anywhere (for grouping, not dedup)
pub struct FindingIdentityResolver {
    /// Map: fingerprint -> UUID
    by_fingerprint: HashMap<String, Uuid>,
    /// Map: (`file_path`, `rule_id`) -> Vec<(`line_start`, UUID)>
    by_location: HashMap<(String, String), Vec<(u32, Uuid)>>,
}

impl FindingIdentityResolver {
    /// Build resolver from existing findings.
    #[must_use]
    pub fn from_findings(findings: &[Finding]) -> Self {
        let mut by_fingerprint = HashMap::new();
        let mut by_location: HashMap<(String, String), Vec<(u32, Uuid)>> = HashMap::new();

        for finding in findings {
            by_fingerprint.insert(finding.content_fingerprint.clone(), finding.uuid);

            if let Some(loc) = primary_location(&finding.locations) {
                let key = (loc.file_path.clone(), finding.rule_id.clone());
                by_location
                    .entry(key)
                    .or_default()
                    .push((loc.line_start, finding.uuid));
            }
        }

        Self {
            by_fingerprint,
            by_location,
        }
    }

    /// Resolve a new finding's identity.
    ///
    /// Returns `ExistingFinding` if fingerprint matches, `RelatedFinding` if same
    /// rule is nearby (within `proximity_threshold` lines), or `NewFinding`.
    #[must_use]
    pub fn resolve(
        &self,
        fingerprint: &str,
        file_path: &str,
        line_start: u32,
        rule_id: &str,
        proximity_threshold: u32,
    ) -> IdentityResolution {
        // Priority 1: Exact fingerprint match
        if let Some(&uuid) = self.by_fingerprint.get(fingerprint) {
            return IdentityResolution::ExistingFinding { uuid };
        }

        // Priority 2: Same rule, nearby location
        let key = (file_path.to_string(), rule_id.to_string());
        if let Some(entries) = self.by_location.get(&key) {
            for &(existing_line, uuid) in entries {
                let distance = line_start.abs_diff(existing_line);
                if distance <= proximity_threshold {
                    return IdentityResolution::RelatedFinding { uuid, distance };
                }
            }
        }

        // Priority 3: No match
        IdentityResolution::NewFinding
    }
}
