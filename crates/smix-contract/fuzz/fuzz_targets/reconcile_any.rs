#![no_main]
//! Any pair of parsed corpora reconciles or refuses, and when it
//! reconciles the three sets partition the contracts exactly once.
//!
//! The arithmetic is small enough to look obviously right and is the
//! part everything downstream reads. What this pins is the property a
//! reader assumes without checking: every contract lands in exactly one
//! answer. A contract in two sets, or in none, would let a coverage
//! report both name a gap and not name it.

use libfuzzer_sys::fuzz_target;
use smix_contract::{parse_claims, parse_contracts, reconcile};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // Split the input into the two files rather than fuzzing one: the
    // interesting states are disagreements between them.
    //
    // On a char boundary, found rather than assumed: `split_at` at
    // len/2 panics in the middle of a multi-byte character, and the
    // first fifteen seconds of fuzzing found exactly that — a crash in
    // the harness, which reads precisely like a crash in the subject.
    let mid = (0..=text.len())
        .rev()
        .find(|i| text.is_char_boundary(*i) && *i <= text.len() / 2)
        .unwrap_or(0);
    let (a, b) = text.split_at(mid);
    let Ok(contracts) = parse_contracts(a, "fuzz-contracts") else {
        return;
    };
    let Ok(claims) = parse_claims(b, "fuzz-claims") else {
        return;
    };

    for expected in [&[][..], &["ios"][..], &["ios", "android"][..]] {
        let Ok(r) = reconcile(&contracts, &claims, expected) else {
            continue;
        };
        let total = r.unclaimed.len() + r.partially_claimed.len() + r.fully_claimed.len();
        assert_eq!(
            total,
            contracts.len(),
            "the three sets must partition the contracts exactly once"
        );
        for p in &r.partially_claimed {
            assert!(
                !p.claimed_by.is_empty() && !p.missing.is_empty(),
                "partially claimed means some and not all — an empty side on \
                 either makes it one of the other two answers"
            );
            assert_eq!(p.claimed_by.len() + p.missing.len(), expected.len());
        }
        for c in &r.fully_claimed {
            assert!(!r.unclaimed.iter().any(|u| u.id == c.id));
        }
    }
});
