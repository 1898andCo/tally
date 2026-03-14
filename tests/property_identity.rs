//! Task 2.10: Property-based tests for fingerprint determinism.

use proptest::prelude::*;
use tally::model::{Location, LocationRole, compute_fingerprint};

proptest! {
    /// Same (file, line_start, line_end, rule) always produces the same fingerprint.
    #[test]
    fn fingerprint_is_deterministic(
        file in "[a-z_/]{1,30}\\.rs",
        line_start in 1u32..10000,
        line_end in 1u32..10000,
        rule in "[a-z_-]{3,20}",
    ) {
        let loc = Location {
            file_path: file,
            line_start,
            line_end,
            role: LocationRole::Primary,
            message: None,
        };

        let fp1 = compute_fingerprint(&loc, &rule);
        let fp2 = compute_fingerprint(&loc, &rule);
        prop_assert_eq!(&fp1, &fp2);
        prop_assert!(fp1.starts_with("sha256:"));
    }

    /// Different inputs produce different fingerprints (collision resistance).
    #[test]
    fn fingerprint_changes_with_any_input_change(
        file_a in "[a-z]{1,10}\\.rs",
        file_b in "[a-z]{1,10}\\.rs",
        line_a in 1u32..5000,
        line_b in 5001u32..10000,
        rule in "[a-z_-]{3,20}",
    ) {
        let loc_a = Location {
            file_path: file_a.clone(),
            line_start: line_a,
            line_end: line_a,
            role: LocationRole::Primary,
            message: None,
        };
        let loc_b = Location {
            file_path: file_b.clone(),
            line_start: line_b,
            line_end: line_b,
            role: LocationRole::Primary,
            message: None,
        };

        // At minimum the line ranges differ, so fingerprints should differ
        let fp_a = compute_fingerprint(&loc_a, &rule);
        let fp_b = compute_fingerprint(&loc_b, &rule);

        // Only assert different if inputs actually differ
        if file_a != file_b || line_a != line_b {
            prop_assert_ne!(fp_a, fp_b);
        }
    }
}
