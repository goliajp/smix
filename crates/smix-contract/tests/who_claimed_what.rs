//! Who claimed what, and what nobody claimed.
//!
//! Three sets come out of a reconciliation: nobody claims this, some
//! platforms claim it, every expected platform claims it. The middle
//! one is the interesting one — it is the answer to "we verify this on
//! iOS and nowhere else" that nothing could give before.
//!
//! The names are `partially_claimed` rather than `one_sided` because
//! the expected platforms are an argument, not a pair baked into a
//! type: smix drives iOS, Android and the web, and a set named for two
//! would have to be renamed the moment a third is asked for. What the
//! middle set carries is WHICH expected platforms are missing, which is
//! the part a reader acts on.

use smix_contract::{ContractError, parse_claims, parse_contracts, reconcile};

const CONTRACTS: &str = "\
- id: CTR-0001
  statement: Pausing notifications from the camera card, and taking it back
- id: CTR-0002
  statement: Wiping a camera's history asks for the account's own address
- id: CTR-0003
  statement: A camera that has gone offline says so on its own card
";

fn contracts() -> Vec<smix_contract::Contract> {
    parse_contracts(CONTRACTS, "contracts.yaml").expect("fixture parses")
}

const EXPECTED: [&str; 2] = ["ios", "android"];

#[test]
fn a_contract_nobody_claims_is_named() {
    let claims = parse_claims(
        "- contract: CTR-0001\n  platform: ios\n- contract: CTR-0001\n  platform: android\n",
        "claims.yaml",
    )
    .unwrap();
    let r = reconcile(&contracts(), &claims, &EXPECTED).expect("reconciles");
    let unclaimed: Vec<&str> = r.unclaimed.iter().map(|c| c.id.as_str()).collect();
    assert!(unclaimed.contains(&"CTR-0002"), "got {unclaimed:?}");
    assert!(unclaimed.contains(&"CTR-0003"), "got {unclaimed:?}");
}

#[test]
fn a_contract_only_one_platform_claims_says_which_is_missing() {
    let claims = parse_claims("- contract: CTR-0002\n  platform: ios\n", "claims.yaml").unwrap();
    let r = reconcile(&contracts(), &claims, &EXPECTED).expect("reconciles");
    let partial = r
        .partially_claimed
        .iter()
        .find(|p| p.contract.id == "CTR-0002")
        .expect("CTR-0002 should be partially claimed");
    // Naming the missing side is the whole point. "Partially claimed"
    // without it sends the reader to two test suites to work out which.
    assert_eq!(partial.missing, vec!["android"]);
    assert_eq!(partial.claimed_by, vec!["ios"]);
}

#[test]
fn a_contract_every_platform_claims_is_fully_claimed() {
    let claims = parse_claims(
        "- contract: CTR-0003\n  platform: ios\n- contract: CTR-0003\n  platform: android\n",
        "claims.yaml",
    )
    .unwrap();
    let r = reconcile(&contracts(), &claims, &EXPECTED).expect("reconciles");
    let ids: Vec<&str> = r.fully_claimed.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["CTR-0003"]);
}

#[test]
fn a_claim_on_an_id_no_contract_carries_is_an_error() {
    // Dropping it silently leaves the requirement the author MEANT
    // looking unclaimed, while the author believes they are covering
    // it. The two halves of that are each invisible on their own.
    let claims = parse_claims("- contract: CTR-9999\n  platform: ios\n", "claims.yaml").unwrap();
    let err = reconcile(&contracts(), &claims, &EXPECTED).expect_err("should refuse");
    let said = err.to_string();
    assert!(said.contains("CTR-9999"), "said: {said}");
    assert!(said.contains("claims.yaml"), "said: {said}");
    assert!(matches!(err, ContractError::UnknownContract { .. }));
}

#[test]
fn a_claim_naming_a_platform_nobody_expects_is_an_error() {
    // Same shape as a mistyped id: `androd` claims nothing, and the
    // platform it meant goes on looking unclaimed.
    let claims = parse_claims("- contract: CTR-0001\n  platform: androd\n", "claims.yaml").unwrap();
    let err = reconcile(&contracts(), &claims, &EXPECTED).expect_err("should refuse");
    let said = err.to_string();
    assert!(said.contains("androd"), "said: {said}");
    assert!(
        said.contains("ios") && said.contains("android"),
        "the refusal must say which platforms ARE expected, said: {said}"
    );
}

#[test]
fn nothing_to_reconcile_is_refused_rather_than_agreed_with() {
    // Three empty sets on an empty corpus is the shape of perfect
    // coverage. A predicate that is true on empty input is not a
    // predicate (project rule gate/no-empty-predicate), so this
    // refuses rather than handing back an agreement nobody earned.
    let err = reconcile(&[], &[], &EXPECTED).expect_err("should refuse");
    assert!(matches!(err, ContractError::NothingToReconcile { .. }));
    assert!(err.to_string().contains("no contracts"), "said: {err}");
}

#[test]
fn no_expected_platforms_is_refused_too() {
    // With nothing expected, every contract is trivially fully claimed
    // — the other way the same vacuous agreement appears.
    let err = reconcile(&contracts(), &[], &[]).expect_err("should refuse");
    assert!(matches!(err, ContractError::NothingToReconcile { .. }));
}

#[test]
fn the_same_claim_twice_is_not_two_platforms() {
    // Two claims from ios must not add up to "both sides". This is the
    // arithmetic equivalent of counting a repeated reading as
    // corroboration.
    let claims = parse_claims(
        "- contract: CTR-0001\n  platform: ios\n- contract: CTR-0001\n  platform: ios\n",
        "claims.yaml",
    )
    .unwrap();
    let r = reconcile(&contracts(), &claims, &EXPECTED).expect("reconciles");
    let partial = r
        .partially_claimed
        .iter()
        .find(|p| p.contract.id == "CTR-0001")
        .expect("still only one platform");
    // `missing` alone does not bite this: two ios claims leave android
    // missing either way, so the assertion passed with the dedup
    // removed. What the repetition changes is the OTHER side — found by
    // taking the rule out and watching nothing go red.
    assert_eq!(partial.claimed_by, vec!["ios"]);
    assert_eq!(partial.missing, vec!["android"]);
}
