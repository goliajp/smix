//! Does the format survive a corpus with a real app's shape?
//!
//! The parser and the reconciler were written against examples chosen
//! to exercise them. This drives both from a corpus written by reading
//! a real two-platform product — its shape, not its text, which is
//! confidential and this crate is published.
//!
//! The step exists to be allowed to fail. If the format does not
//! survive real features, the answer is to change the format and let
//! the earlier tests go red, not to bend the corpus until it fits.
//!
//! What the reading found is recorded in the corpus README and is the
//! argument for the whole crate: the two native suites already write
//! the same sentence for the same requirement, word for word, and
//! nothing can join them because the sentence has no id.

use smix_contract::{parse_claims, parse_contracts, reconcile};

const EXPECTED: [&str; 2] = ["ios", "android"];

fn corpus() -> (Vec<smix_contract::Contract>, Vec<smix_contract::Claim>) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus");
    let contracts = parse_contracts(
        &std::fs::read_to_string(format!("{dir}/contracts.yaml")).expect("contracts.yaml"),
        "tests/corpus/contracts.yaml",
    )
    .expect("the corpus parses");
    let mut claims = parse_claims(
        &std::fs::read_to_string(format!("{dir}/claims-ios.yaml")).expect("claims-ios.yaml"),
        "tests/corpus/claims-ios.yaml",
    )
    .expect("ios claims parse");
    claims.extend(
        parse_claims(
            &std::fs::read_to_string(format!("{dir}/claims-android.yaml"))
                .expect("claims-android.yaml"),
            "tests/corpus/claims-android.yaml",
        )
        .expect("android claims parse"),
    );
    (contracts, claims)
}

#[test]
fn the_corpus_is_big_enough_to_be_worth_reconciling() {
    let (contracts, claims) = corpus();
    assert!(
        contracts.len() >= 5,
        "the point of this corpus is realistic size; got {}",
        contracts.len()
    );
    assert!(!claims.is_empty());
}

#[test]
fn a_real_one_sided_gap_comes_out_named() {
    let (contracts, claims) = corpus();
    let r = reconcile(&contracts, &claims, &EXPECTED).expect("the corpus reconciles");

    let gap = r
        .partially_claimed
        .iter()
        .find(|p| p.contract.id.starts_with("CTR-CALLOUT-"))
        .expect("the callout rules are covered on one platform only");
    assert_eq!(gap.claimed_by, vec!["ios"]);
    assert_eq!(gap.missing, vec!["android"]);
}

#[test]
fn the_corpus_exercises_all_three_answers() {
    // A corpus where everything is fully claimed leaves the other two
    // answers untested against realistic data — the same mistake as a
    // gate that only ever drives the easy subject.
    let (contracts, claims) = corpus();
    let r = reconcile(&contracts, &claims, &EXPECTED).expect("reconciles");
    assert!(
        !r.unclaimed.is_empty(),
        "no unclaimed contract: this corpus cannot show what an unclaimed \
         requirement looks like"
    );
    assert!(
        !r.partially_claimed.is_empty(),
        "no partially claimed contract"
    );
    assert!(!r.fully_claimed.is_empty(), "no fully claimed contract");
    assert_eq!(
        r.unclaimed.len() + r.partially_claimed.len() + r.fully_claimed.len(),
        contracts.len(),
        "every contract lands in exactly one set"
    );
}

#[test]
fn one_requirement_is_one_user_visible_outcome() {
    // The granularity convention, asserted rather than described.
    //
    // Coarser and two platforms can both claim an id while covering
    // different halves of it; finer and nobody writes them. The
    // mechanical stand-in is that a statement describes a single
    // outcome — no conjunction joining two of them, no semicolon
    // splicing a second sentence on.
    let (contracts, _) = corpus();
    for c in &contracts {
        assert!(
            !c.statement.contains(';'),
            "{}: a semicolon splices a second requirement onto the first — \
             give it its own id: {:?}",
            c.id,
            c.statement
        );
        assert!(
            !c.statement.contains(" and then "),
            "{}: `and then` is a sequence, which is two outcomes: {:?}",
            c.id,
            c.statement
        );
        assert!(
            c.statement.len() <= 120,
            "{}: {} characters. A requirement that needs a paragraph is more \
             than one requirement: {:?}",
            c.id,
            c.statement.len(),
            c.statement
        );
    }
}
