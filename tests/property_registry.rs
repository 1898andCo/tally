//! Property-based tests for rule registry normalization and matching.

use proptest::prelude::*;
use tally_ng::registry::{RuleMatcher, normalize_rule_id, validate_rule_id};

// =============================================================================
// Strategy: valid rule ID strings (2-64 chars, [a-z0-9] bookends, [a-z0-9-] middle)
// =============================================================================

fn arb_valid_rule_id() -> impl Strategy<Value = String> {
    // First char: [a-z0-9], middle: [a-z0-9-]{0,62}, last char: [a-z0-9]
    // Total length: 2-64
    let alnum = prop::char::ranges(vec!['a'..='z', '0'..='9'].into());
    let middle_char = prop::char::ranges(vec!['a'..='z', '0'..='9', '-'..='-'].into());

    (
        alnum.clone(),
        prop::collection::vec(middle_char, 0..62),
        alnum,
    )
        .prop_map(|(first, middle, last)| {
            let mut s = String::with_capacity(2 + middle.len());
            s.push(first);
            for c in middle {
                s.push(c);
            }
            s.push(last);
            s
        })
}

// =============================================================================
// Property 1: normalize is idempotent — normalize(normalize(x)) == normalize(x)
// =============================================================================

proptest! {
    #[test]
    fn normalize_is_idempotent(id in arb_valid_rule_id()) {
        let first = normalize_rule_id(&id);
        prop_assume!(first.is_ok());
        let first = first.expect("first normalize succeeded");

        let second = normalize_rule_id(&first).expect("second normalize must succeed on valid output");
        prop_assert_eq!(
            first,
            second,
            "normalize must be idempotent"
        );
    }
}

// =============================================================================
// Property 2: RuleMatcher::resolve never panics on arbitrary input
// =============================================================================

proptest! {
    #[test]
    fn matcher_never_panics(input in any::<String>()) {
        let matcher = RuleMatcher::new(vec![]);
        // We only care that this does not panic — Ok or Err are both fine
        let _ = matcher.resolve(&input, None, None);
    }

    #[test]
    fn matcher_never_panics_with_cwe_and_desc(
        input in any::<String>(),
        cwe in any::<String>(),
        desc in any::<String>(),
    ) {
        let matcher = RuleMatcher::new(vec![]);
        let cwe_ids = vec![cwe];
        let _ = matcher.resolve(&input, Some(&cwe_ids), Some(&desc));
    }
}

// =============================================================================
// Property 3: if validate rejects, normalize also rejects or produces a
// different string (normalization never "validates" an invalid raw input
// by returning it unchanged)
// =============================================================================

proptest! {
    #[test]
    fn validate_rejects_then_normalize_rejects_or_transforms(
        input in "[a-z0-9 _:A-Z\\-]{1,80}"
    ) {
        if validate_rule_id(&input).is_err() {
            match normalize_rule_id(&input) {
                Err(_) => {
                    // Good — normalize also rejects
                }
                Ok(normalized) => {
                    // Normalize succeeded, but the result must differ from the raw input
                    // (normalization transformed the invalid input into a valid one)
                    prop_assert_ne!(
                        normalized,
                        input,
                        "validate rejects input but normalize returns it unchanged"
                    );
                }
            }
        }
    }
}
