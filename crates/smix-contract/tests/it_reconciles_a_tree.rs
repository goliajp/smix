//! Reconciling a whole tree: contracts, hand-written claims, and the
//! claims written in the source beside the tests.
//!
//! One answer from three notations, because a team has all three at
//! once during the years it takes to move from one to another.

use smix_contract::{ContractError, reconcile_tree};
use std::fs;
use std::path::PathBuf;

fn tree(files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "smix-contract-tree-{}-{}",
        std::process::id(),
        files.len() * 7 + files.iter().map(|(p, _)| p.len()).sum::<usize>()
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
