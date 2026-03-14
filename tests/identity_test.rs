//! Tests for content fingerprint computation and identity resolution.

use tally::model::*;
use uuid::Uuid;

// =============================================================================
// Task 2.8: Fingerprint determinism
// =============================================================================

#[test]
fn fingerprint_deterministic_for_same_input() {
    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };

    let fp1 = compute_fingerprint(&loc, "unsafe-unwrap");
    let fp2 = compute_fingerprint(&loc, "unsafe-unwrap");
    assert_eq!(fp1, fp2, "same input should produce same fingerprint");
}

#[test]
fn fingerprint_changes_with_file() {
    let loc_a = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };
    let loc_b = Location {
        file_path: "src/lib.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };

    let fp_a = compute_fingerprint(&loc_a, "unsafe-unwrap");
    let fp_b = compute_fingerprint(&loc_b, "unsafe-unwrap");
    assert_ne!(fp_a, fp_b, "different files should produce different fingerprints");
}

#[test]
fn fingerprint_changes_with_line() {
    let loc_a = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };
    let loc_b = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 100,
        line_end: 100,
        role: LocationRole::Primary,
        message: None,
    };

    let fp_a = compute_fingerprint(&loc_a, "unsafe-unwrap");
    let fp_b = compute_fingerprint(&loc_b, "unsafe-unwrap");
    assert_ne!(fp_a, fp_b, "different lines should produce different fingerprints");
}

#[test]
fn fingerprint_changes_with_rule() {
    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };

    let fp_a = compute_fingerprint(&loc, "unsafe-unwrap");
    let fp_b = compute_fingerprint(&loc, "sql-injection");
    assert_ne!(fp_a, fp_b, "different rules should produce different fingerprints");
}

#[test]
fn fingerprint_starts_with_sha256_prefix() {
    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };

    let fp = compute_fingerprint(&loc, "unsafe-unwrap");
    assert!(fp.starts_with("sha256:"), "fingerprint should start with sha256: prefix");
    assert_eq!(fp.len(), 7 + 64, "sha256: prefix + 64 hex chars");
}

// =============================================================================
// Task 2.9: Identity resolution
// =============================================================================

fn make_finding(uuid: Uuid, file: &str, line: u32, rule: &str) -> Finding {
    let loc = Location {
        file_path: file.to_string(),
        line_start: line,
        line_end: line,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, rule);
    Finding {
        uuid,
        content_fingerprint: fp,
        rule_id: rule.to_string(),
        locations: vec![loc],
        severity: Severity::Important,
        category: "test".to_string(),
        tags: vec![],
        title: "test finding".to_string(),
        description: "test".to_string(),
        suggested_fix: None,
        evidence: None,
        status: LifecycleState::Open,
        state_history: vec![],
        discovered_by: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        repo_id: "test/repo".to_string(),
        branch: None,
        pr_number: None,
        commit_sha: None,
        relationships: vec![],
        suppression: None,
    }
}

#[test]
fn resolve_existing_by_fingerprint() {
    let uuid = Uuid::now_v7();
    let existing = make_finding(uuid, "src/main.rs", 42, "unsafe-unwrap");
    let resolver = FindingIdentityResolver::from_findings(&[existing]);

    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "unsafe-unwrap");

    let result = resolver.resolve(&fp, "src/main.rs", 42, "unsafe-unwrap", 5);
    assert_eq!(result, IdentityResolution::ExistingFinding { uuid });
}

#[test]
fn resolve_related_by_proximity() {
    let uuid = Uuid::now_v7();
    let existing = make_finding(uuid, "src/main.rs", 42, "unsafe-unwrap");
    let resolver = FindingIdentityResolver::from_findings(&[existing]);

    // Same file, same rule, 3 lines away — different fingerprint but within threshold
    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 45,
        line_end: 45,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "unsafe-unwrap");

    let result = resolver.resolve(&fp, "src/main.rs", 45, "unsafe-unwrap", 5);
    assert_eq!(
        result,
        IdentityResolution::RelatedFinding {
            uuid,
            distance: 3
        }
    );
}

#[test]
fn resolve_new_when_distant() {
    let uuid = Uuid::now_v7();
    let existing = make_finding(uuid, "src/main.rs", 42, "unsafe-unwrap");
    let resolver = FindingIdentityResolver::from_findings(&[existing]);

    // Same file, same rule, but 100 lines away — beyond threshold
    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 142,
        line_end: 142,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "unsafe-unwrap");

    let result = resolver.resolve(&fp, "src/main.rs", 142, "unsafe-unwrap", 5);
    assert_eq!(result, IdentityResolution::NewFinding);
}

#[test]
fn resolve_new_when_different_file() {
    let uuid = Uuid::now_v7();
    let existing = make_finding(uuid, "src/main.rs", 42, "unsafe-unwrap");
    let resolver = FindingIdentityResolver::from_findings(&[existing]);

    let loc = Location {
        file_path: "src/lib.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "unsafe-unwrap");

    let result = resolver.resolve(&fp, "src/lib.rs", 42, "unsafe-unwrap", 5);
    assert_eq!(result, IdentityResolution::NewFinding);
}

#[test]
fn resolve_new_when_different_rule() {
    let uuid = Uuid::now_v7();
    let existing = make_finding(uuid, "src/main.rs", 42, "unsafe-unwrap");
    let resolver = FindingIdentityResolver::from_findings(&[existing]);

    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "sql-injection");

    let result = resolver.resolve(&fp, "src/main.rs", 42, "sql-injection", 5);
    assert_eq!(result, IdentityResolution::NewFinding);
}

#[test]
fn resolve_empty_resolver_returns_new() {
    let resolver = FindingIdentityResolver::from_findings(&[]);

    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 42,
        line_end: 42,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "unsafe-unwrap");

    let result = resolver.resolve(&fp, "src/main.rs", 42, "unsafe-unwrap", 5);
    assert_eq!(result, IdentityResolution::NewFinding);
}

// =============================================================================
// Primary location extraction
// =============================================================================

#[test]
fn primary_location_finds_primary() {
    let locations = vec![
        Location {
            file_path: "secondary.rs".to_string(),
            line_start: 1,
            line_end: 1,
            role: LocationRole::Secondary,
            message: None,
        },
        Location {
            file_path: "primary.rs".to_string(),
            line_start: 42,
            line_end: 42,
            role: LocationRole::Primary,
            message: None,
        },
    ];

    let primary = primary_location(&locations).expect("should find primary");
    assert_eq!(primary.file_path, "primary.rs");
}

#[test]
fn primary_location_falls_back_to_first() {
    let locations = vec![Location {
        file_path: "only.rs".to_string(),
        line_start: 1,
        line_end: 1,
        role: LocationRole::Secondary,
        message: None,
    }];

    let primary = primary_location(&locations).expect("should fall back to first");
    assert_eq!(primary.file_path, "only.rs");
}

#[test]
fn primary_location_empty_returns_none() {
    assert!(primary_location(&[]).is_none());
}
