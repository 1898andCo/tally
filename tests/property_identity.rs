//! Property-based tests for identity and model types.

use proptest::prelude::*;
use tally_ng::model::*;

// =============================================================================
// Fingerprint property tests
// =============================================================================

proptest! {
    #[test]
    fn proptest_fingerprint_special_chars_in_path(file_path in ".*") {
        let loc = Location {
            file_path,
            line_start: 1,
            line_end: 1,
            role: LocationRole::Primary,
            message: None,
        };
        let fp = compute_fingerprint(&loc, "rule-x");
        prop_assert!(fp.starts_with("sha256:"), "fingerprint should start with sha256: prefix");
        prop_assert_eq!(fp.len(), 7 + 64, "fingerprint should be sha256: + 64 hex chars");
    }
}

// =============================================================================
// LifecycleState property tests
// =============================================================================

fn arb_lifecycle_state() -> impl Strategy<Value = LifecycleState> {
    prop_oneof![
        Just(LifecycleState::Open),
        Just(LifecycleState::Acknowledged),
        Just(LifecycleState::InProgress),
        Just(LifecycleState::Resolved),
        Just(LifecycleState::FalsePositive),
        Just(LifecycleState::WontFix),
        Just(LifecycleState::Deferred),
        Just(LifecycleState::Suppressed),
        Just(LifecycleState::Reopened),
        Just(LifecycleState::Closed),
    ]
}

proptest! {
    #[test]
    fn proptest_lifecycle_display_roundtrip(state in arb_lifecycle_state()) {
        let display = state.to_string();
        let parsed: LifecycleState = display.parse().map_err(|e: String| {
            TestCaseError::Fail(e.into())
        })?;
        prop_assert_eq!(state, parsed);
    }
}

// =============================================================================
// Severity property tests
// =============================================================================

fn arb_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Critical),
        Just(Severity::Important),
        Just(Severity::Suggestion),
        Just(Severity::TechDebt),
    ]
}

proptest! {
    #[test]
    fn proptest_severity_display_roundtrip(sev in arb_severity()) {
        let display = sev.to_string();
        // Display is uppercase (e.g. "CRITICAL") but FromStr accepts lowercase
        let parsed: Severity = display.to_ascii_lowercase().parse().map_err(|e: String| {
            TestCaseError::Fail(e.into())
        })?;
        prop_assert_eq!(sev, parsed);
    }
}
