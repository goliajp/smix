//! Reconciling a whole tree: contracts, hand-written claims, and the
//! claims written in the source beside the tests.
//!
//! One answer from three notations, because a team has all three at
//! once during the years it takes to move from one to another.

use smix_contract::{ContractError, reconcile_tree};
use std::fs;
use std::path::PathBuf;

/// A throwaway tree, in a directory nothing else will pick.
///
/// The first version named the directory after a hash of the file
/// list, and two cases with the same shape collided: cargo runs tests
/// in parallel, so one was writing into the directory another had just
/// deleted. Three tests failed inside the helper and none of them was
/// about the helper. A counter is enough — the name only has to be
/// unique, not meaningful.
fn tree(files: &[(&str, &str)]) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "smix-contract-tree-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    for (rel, body) in files {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    dir
}

const CONTRACTS: &str = "\
- id: CTR-0001
  statement: The first thing the app owes
- id: CTR-0002
  statement: The second thing the app owes
";

#[test]
fn three_notations_reach_one_answer() {
    let root = tree(&[
        ("app.contracts.yaml", CONTRACTS),
        // hand-written, the notation a team starts with
        (
            "qa/legacy.claims.yaml",
            "- contract: CTR-0001\n  platform: android\n",
        ),
        // in the source, the notation it ends up with
        (
            "ios/Tests/MenuTests.swift",
            "// covers: CTR-0001\n@Test func a() {}\n",
        ),
        (
            "android/test/MenuTest.kt",
            "// covers: CTR-0002\n@Test fun b() {}\n",
        ),
    ]);
    let r = reconcile_tree(&root, &["ios", "android"]).expect("reconciles");

    let both: Vec<&str> = r.fully_claimed.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(both, vec!["CTR-0001"], "ios from source, android by hand");

    let partial: Vec<(&str, &str)> = r
        .partially_claimed
        .iter()
        .map(|p| (p.contract.id.as_str(), p.missing[0].as_str()))
        .collect();
    assert_eq!(partial, vec![("CTR-0002", "ios")]);
}

#[test]
fn the_platform_comes_from_the_path() {
    // A source file's platform is a property of where it lives. It is
    // read rather than asked for because nobody will keep a list of
    // which directories are which in step with the directories.
    let root = tree(&[
        ("app.contracts.yaml", CONTRACTS),
        ("platforms/ios/T.swift", "// covers: CTR-0001\n"),
        ("platforms/android/T.kt", "// covers: CTR-0001\n"),
    ]);
    let r = reconcile_tree(&root, &["ios", "android"]).expect("reconciles");
    assert_eq!(
        r.fully_claimed
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["CTR-0001"]
    );
}

#[test]
fn a_source_claim_whose_platform_cannot_be_read_is_refused() {
    // Guessing is the dangerous answer here. Guess wrong and a
    // requirement covered on one platform reads as covered on both —
    // the exact fact this layer exists to surface, inverted.
    let root = tree(&[
        ("app.contracts.yaml", CONTRACTS),
        ("somewhere/T.swift", "// covers: CTR-0001\n"),
    ]);
    let err = reconcile_tree(&root, &["ios", "android"]).expect_err("refused");
    let said = err.to_string();
    assert!(said.contains("somewhere/T.swift"), "said: {said}");
    assert!(
        said.contains("ios") && said.contains("android"),
        "the refusal must say which platforms it looked for: {said}"
    );
}

#[test]
fn a_tree_with_no_contracts_is_refused() {
    // Same rule as the reconciler's: three empty sets over nothing is
    // the shape of perfect coverage.
    let root = tree(&[("ios/T.swift", "// covers: CTR-0001\n")]);
    let err = reconcile_tree(&root, &["ios", "android"]).expect_err("refused");
    assert!(matches!(err, ContractError::NothingToReconcile { .. }));
}

#[test]
fn the_same_id_in_two_contract_files_is_refused_by_both_names() {
    // Two files each defining CTR-0001 makes every claim on it
    // ambiguous, and the reader needs to know which two files.
    let root = tree(&[
        (
            "a.contracts.yaml",
            "- id: CTR-0001\n  statement: One thing\n",
        ),
        (
            "b.contracts.yaml",
            "- id: CTR-0001\n  statement: A different thing\n",
        ),
        ("ios/T.swift", "// covers: CTR-0001\n"),
    ]);
    let err = reconcile_tree(&root, &["ios", "android"]).expect_err("refused");
    let said = err.to_string();
    assert!(said.contains("a.contracts.yaml"), "said: {said}");
    assert!(said.contains("b.contracts.yaml"), "said: {said}");
}

#[test]
fn build_output_is_not_scanned() {
    // target/ holds copies of source and generated files; scanning it
    // would double every claim and invent platforms out of path
    // fragments.
    let root = tree(&[
        ("app.contracts.yaml", CONTRACTS),
        ("ios/T.swift", "// covers: CTR-0001\n"),
        ("target/debug/ios/T.swift", "// covers: CTR-0002\n"),
        (".git/objects/ios/x.swift", "// covers: CTR-0002\n"),
    ]);
    let r = reconcile_tree(&root, &["ios", "android"]).expect("reconciles");
    assert_eq!(
        r.unclaimed
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["CTR-0002"],
        "the copy under target/ must not have claimed it"
    );
}

// ---- the ratchet ------------------------------------------------------

use smix_contract::{Baseline, baseline_of, regressions};

fn reconciled(files: &[(&str, &str)]) -> smix_contract::Reconciliation {
    reconcile_tree(&tree(files), &["ios", "android"]).expect("reconciles")
}

fn baseline_of_expected(r: &smix_contract::Reconciliation) -> Baseline {
    baseline_of(r, &["ios", "android"])
}

const BOTH_SIDES: [(&str, &str); 3] = [
    ("app.contracts.yaml", CONTRACTS),
    ("ios/T.swift", "// covers: CTR-0001, CTR-0002\n"),
    ("android/T.kt", "// covers: CTR-0001, CTR-0002\n"),
];

#[test]
fn losing_a_platform_on_a_covered_contract_is_a_regression() {
    let base = baseline_of_expected(&reconciled(&BOTH_SIDES));
    // The android suite stops covering CTR-0002.
    let now = reconciled(&[
        ("app.contracts.yaml", CONTRACTS),
        ("ios/T.swift", "// covers: CTR-0001, CTR-0002\n"),
        ("android/T.kt", "// covers: CTR-0001\n"),
    ]);
    let regs = regressions(&base, &now, &["ios", "android"]);
    assert_eq!(regs.len(), 1);
    let said = regs[0].to_string();
    assert!(said.contains("CTR-0002"), "said: {said}");
    assert!(said.contains("android"), "which platform went: {said}");
}

#[test]
fn losing_every_platform_is_a_regression_too() {
    let base = baseline_of_expected(&reconciled(&BOTH_SIDES));
    let now = reconciled(&[
        ("app.contracts.yaml", CONTRACTS),
        ("ios/T.swift", "// covers: CTR-0001\n"),
        ("android/T.kt", "// covers: CTR-0001\n"),
    ]);
    let regs = regressions(&base, &now, &["ios", "android"]);
    assert_eq!(regs.len(), 1);
    assert!(regs[0].to_string().contains("CTR-0002"));
}

#[test]
fn a_new_contract_nobody_covers_yet_is_not_a_regression() {
    // The ratchet forbids losing what is there. It does not demand
    // that coverage rise, because a rule that did would be a coverage
    // target wearing another name — and a target is met by writing
    // claims, not by covering anything.
    let base = baseline_of_expected(&reconciled(&BOTH_SIDES));
    let now = reconciled(&[
        (
            "app.contracts.yaml",
            "- id: CTR-0001\n  statement: The first thing the app owes\n\
             - id: CTR-0002\n  statement: The second thing the app owes\n\
             - id: CTR-0003\n  statement: Something nobody has covered yet\n",
        ),
        ("ios/T.swift", "// covers: CTR-0001, CTR-0002\n"),
        ("android/T.kt", "// covers: CTR-0001, CTR-0002\n"),
    ]);
    assert!(
        regressions(&base, &now, &["ios", "android"]).is_empty(),
        "a new uncovered requirement is work to do, not a regression"
    );
}

#[test]
fn a_contract_that_no_longer_exists_is_reported_but_is_not_a_regression() {
    // Deleting a requirement is a legitimate act. Doing it silently is
    // not, so it is said out loud without failing anything — the
    // reader decides whether the deletion was meant.
    let base = baseline_of_expected(&reconciled(&BOTH_SIDES));
    let now = reconciled(&[
        (
            "app.contracts.yaml",
            "- id: CTR-0001\n  statement: The first thing the app owes\n",
        ),
        ("ios/T.swift", "// covers: CTR-0001\n"),
        ("android/T.kt", "// covers: CTR-0001\n"),
    ]);
    let regs = regressions(&base, &now, &["ios", "android"]);
    assert!(
        regs.iter().all(|r| !r.blocks()),
        "a deleted requirement must not block: {regs:?}"
    );
    assert!(
        regs.iter().any(|r| r.to_string().contains("CTR-0002")),
        "and it must still be said: {regs:?}"
    );
}

#[test]
fn the_baseline_lists_ids_rather_than_a_number() {
    // A number going down says something went; a name going missing
    // says what. The second shows up in a diff as a line with a name
    // on it, which is what makes "regenerate the baseline until it is
    // green" a visible act rather than a quiet one.
    let text = baseline_of_expected(&reconciled(&BOTH_SIDES)).to_string();
    assert!(text.contains("CTR-0001"), "{text}");
    assert!(text.contains("CTR-0002"), "{text}");
    let round_tripped: Baseline = text.parse().expect("a baseline reads back");
    assert_eq!(round_tripped.to_string(), text);
}

// ---- the verdict, written for whoever reads it next -------------------

use smix_contract::render;

#[test]
fn the_verdict_names_the_requirement_and_where_it_was_read() {
    let now = reconciled(&[
        ("app.contracts.yaml", CONTRACTS),
        ("ios/T.swift", "// covers: CTR-0001, CTR-0002\n"),
        ("android/T.kt", "// covers: CTR-0001\n"),
    ]);
    let out = render(&now, &[]);

    // The id, so it can be looked up.
    assert!(out.contains("CTR-0002"), "{out}");
    // The sentence, so the reader does not have to.
    assert!(out.contains("The second thing the app owes"), "{out}");
    // Which platform is missing — the part anybody acts on.
    assert!(out.contains("android"), "{out}");
    // And where the claim that does exist was read, so the reader can
    // go straight there rather than searching.
    assert!(out.contains("ios/T.swift"), "{out}");
}

#[test]
fn the_verdict_carries_no_percentage() {
    // Written as an assertion so that adding one later is a deliberate
    // act against a test rather than a tidy-looking afternoon.
    //
    // A percentage turns this into a score, and a score is met by
    // writing claims. The three sets say which requirements are in
    // which state; a number says none of that and invites being
    // targeted.
    let now = reconciled(&[
        ("app.contracts.yaml", CONTRACTS),
        ("ios/T.swift", "// covers: CTR-0001\n"),
        ("android/T.kt", "// covers: CTR-0001\n"),
    ]);
    let out = render(&now, &[]);
    assert!(!out.contains('%'), "a percentage crept in: {out}");
    for word in ["coverage:", "covered %", "score", "percent"] {
        assert!(
            !out.to_lowercase().contains(word),
            "a score crept in as `{word}`: {out}"
        );
    }
}

#[test]
fn a_regression_is_rendered_first_because_it_is_the_thing_to_act_on() {
    let base = baseline_of_expected(&reconciled(&BOTH_SIDES));
    let now = reconciled(&[
        ("app.contracts.yaml", CONTRACTS),
        ("ios/T.swift", "// covers: CTR-0001, CTR-0002\n"),
        ("android/T.kt", "// covers: CTR-0001\n"),
    ]);
    let regs = regressions(&base, &now, &["ios", "android"]);
    let out = render(&now, &regs);
    let reg_at = out
        .find("was claimed by")
        .expect("the regression is rendered");
    let sets_at = out.find("claimed by some").unwrap_or(usize::MAX);
    assert!(
        reg_at < sets_at,
        "what was lost comes before what merely is: {out}"
    );
}

#[test]
fn a_clean_tree_says_so_without_pretending_it_proved_anything() {
    let now = reconciled(&BOTH_SIDES);
    let out = render(&now, &[]);
    // It says what it checked...
    assert!(out.contains("2"), "the count of contracts is stated: {out}");
    // ...and never that anything is verified. The whole crate reports
    // who CLAIMED, and the rendering must not be where that slips.
    assert!(!out.to_lowercase().contains("verified"), "{out}");
    assert!(!out.to_lowercase().contains("proven"), "{out}");
}

#[test]
fn the_whole_verdict_reads_as_something_to_act_on() {
    // The rendering asserted line by line above, read once as a whole.
    // Written because "each part is present" and "the thing is usable"
    // are different claims, and only one of them was being tested.
    let now = reconciled(&[
        (
            "app.contracts.yaml",
            "- id: CTR-MENU-0001\n  statement: Every section is separated from the next when none are hidden\n\
             - id: CTR-CALLOUT-0002\n  statement: A callout flips above the thing it points at when it would not fit below\n\
             - id: CTR-OFFLINE-0001\n  statement: A device that has stopped reporting says so on its own card\n",
        ),
        (
            "ios/MenuTests.swift",
            "// covers: CTR-MENU-0001\n// covers: CTR-CALLOUT-0002\n",
        ),
        ("android/MenuTest.kt", "// covers: CTR-MENU-0001\n"),
    ]);
    let out = render(&now, &[]);

    // Every section a reader needs, in the order they need them.
    let nobody = out
        .find("claimed by nobody")
        .expect("the unclaimed section");
    let some = out.find("claimed by some").expect("the partial section");
    assert!(
        nobody < some,
        "nothing at all comes before not enough:\n{out}"
    );

    // The partial entry carries all four things somebody acts on.
    assert!(out.contains("CTR-CALLOUT-0002"), "{out}");
    assert!(out.contains("flips above"), "{out}");
    assert!(out.contains("missing android"), "{out}");
    assert!(out.contains("ios/MenuTests.swift:2"), "{out}");

    // And the closing sentence, which is the crate's own limit stated
    // in its output rather than only in its documentation.
    assert!(
        out.contains("It does not say the test is good"),
        "the limit must travel with the verdict:\n{out}"
    );
}
