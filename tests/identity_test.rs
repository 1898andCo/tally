//! Tests for content fingerprint computation and identity resolution.

use tally_ng::model::*;
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
    assert_ne!(
        fp_a, fp_b,
        "different files should produce different fingerprints"
    );
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
    assert_ne!(
        fp_a, fp_b,
        "different lines should produce different fingerprints"
    );
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
    assert_ne!(
        fp_a, fp_b,
        "different rules should produce different fingerprints"
    );
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
    assert!(
        fp.starts_with("sha256:"),
        "fingerprint should start with sha256: prefix"
    );
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
        schema_version: "1.0.0".to_string(),
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
        IdentityResolution::RelatedFinding { uuid, distance: 3 }
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

// =============================================================================
// Additional fingerprint tests
// =============================================================================

#[test]
fn fingerprint_empty_file_path() {
    let loc = Location {
        file_path: String::new(),
        line_start: 1,
        line_end: 1,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "some-rule");
    assert!(
        fp.starts_with("sha256:"),
        "empty file_path should still produce valid sha256 hash"
    );
    assert_eq!(fp.len(), 7 + 64);
}

#[test]
fn fingerprint_empty_rule_id() {
    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 1,
        line_end: 1,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "");
    assert!(
        fp.starts_with("sha256:"),
        "empty rule_id should still produce valid sha256 hash"
    );
    assert_eq!(fp.len(), 7 + 64);
}

#[test]
fn fingerprint_line_end_matters() {
    let loc_a = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 10,
        line_end: 10,
        role: LocationRole::Primary,
        message: None,
    };
    let loc_b = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 10,
        line_end: 20,
        role: LocationRole::Primary,
        message: None,
    };

    let fp_a = compute_fingerprint(&loc_a, "rule-x");
    let fp_b = compute_fingerprint(&loc_b, "rule-x");
    assert_ne!(
        fp_a, fp_b,
        "different line_end should produce different fingerprints"
    );
}

#[test]
fn fingerprint_max_line_numbers() {
    let loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: u32::MAX,
        line_end: u32::MAX,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "rule-x");
    assert!(
        fp.starts_with("sha256:"),
        "u32::MAX line numbers should still produce valid sha256 hash"
    );
    assert_eq!(fp.len(), 7 + 64);
}

// =============================================================================
// Additional identity resolution tests
// =============================================================================

#[test]
fn resolver_same_rule_different_files() {
    let uuid = Uuid::now_v7();
    let existing = make_finding(uuid, "src/main.rs", 42, "unsafe-unwrap");
    let resolver = FindingIdentityResolver::from_findings(&[existing]);

    // Same rule, close line, but different file — should NOT match proximity
    let loc = Location {
        file_path: "src/lib.rs".to_string(),
        line_start: 43,
        line_end: 43,
        role: LocationRole::Primary,
        message: None,
    };
    let fp = compute_fingerprint(&loc, "unsafe-unwrap");

    let result = resolver.resolve(&fp, "src/lib.rs", 43, "unsafe-unwrap", 5);
    assert_eq!(
        result,
        IdentityResolution::NewFinding,
        "proximity on wrong file should return NewFinding"
    );
}

#[test]
fn resolver_proximity_at_boundary() {
    let uuid = Uuid::now_v7();
    let existing = make_finding(uuid, "src/main.rs", 50, "unsafe-unwrap");
    let resolver = FindingIdentityResolver::from_findings(&[existing]);

    // Distance == threshold (5) → should be RelatedFinding
    let loc_at = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 55,
        line_end: 55,
        role: LocationRole::Primary,
        message: None,
    };
    let fp_at = compute_fingerprint(&loc_at, "unsafe-unwrap");
    let result_at = resolver.resolve(&fp_at, "src/main.rs", 55, "unsafe-unwrap", 5);
    assert_eq!(
        result_at,
        IdentityResolution::RelatedFinding { uuid, distance: 5 },
        "distance == threshold should be RelatedFinding"
    );

    // Distance == threshold+1 (6) → should be NewFinding
    let loc_beyond = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 56,
        line_end: 56,
        role: LocationRole::Primary,
        message: None,
    };
    let fp_beyond = compute_fingerprint(&loc_beyond, "unsafe-unwrap");
    let result_beyond = resolver.resolve(&fp_beyond, "src/main.rs", 56, "unsafe-unwrap", 5);
    assert_eq!(
        result_beyond,
        IdentityResolution::NewFinding,
        "distance == threshold+1 should be NewFinding"
    );
}

#[test]
fn resolver_secondary_location_not_indexed() {
    // Create a finding where primary is at line 100, secondary at line 10
    let uuid = Uuid::now_v7();
    let primary_loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 100,
        line_end: 100,
        role: LocationRole::Primary,
        message: None,
    };
    let secondary_loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 10,
        line_end: 10,
        role: LocationRole::Secondary,
        message: None,
    };
    let fp = compute_fingerprint(&primary_loc, "unsafe-unwrap");
    let finding = Finding {
        schema_version: "1.0.0".to_string(),
        uuid,
        content_fingerprint: fp,
        rule_id: "unsafe-unwrap".to_string(),
        locations: vec![primary_loc, secondary_loc],
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
    };

    let resolver = FindingIdentityResolver::from_findings(&[finding]);

    // A new finding at line 10 (where the secondary location was) should NOT match
    let new_loc = Location {
        file_path: "src/main.rs".to_string(),
        line_start: 10,
        line_end: 10,
        role: LocationRole::Primary,
        message: None,
    };
    let new_fp = compute_fingerprint(&new_loc, "unsafe-unwrap");
    let result = resolver.resolve(&new_fp, "src/main.rs", 10, "unsafe-unwrap", 5);
    assert_eq!(
        result,
        IdentityResolution::NewFinding,
        "secondary location should NOT be indexed for proximity matching"
    );
}
