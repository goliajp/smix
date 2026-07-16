//! Fixture-diff regression test. Any change to the codemod that would
//! alter the fixture output surfaces here.

use smix_migrate::Migrator;

const INPUT: &str = include_str!("fixtures/maestro-input.yaml");
const EXPECTED: &str = include_str!("fixtures/expected-smix.yaml");

#[test]
fn maestro_input_maps_to_expected_smix() {
    let (actual, report) = Migrator::default().migrate(INPUT).expect("parse ok");
    let actual_trim = actual.trim();
    let expected_trim = EXPECTED.trim();
    if actual_trim != expected_trim {
        eprintln!("=== actual ===\n{actual_trim}\n=== expected ===\n{expected_trim}\n=== end ===");
        panic!("fixture diff mismatch");
    }
    // Every rule in the table gets one fixture
    // exercise. If we add / remove a rule and forget the fixture pair,
    // this count goes stale.
    assert!(
        report.renamed.len() >= 7,
        "expected ≥7 renames, got {}",
        report.renamed.len()
    );
    assert!(
        report.unknown_verbs.is_empty(),
        "unexpected unknown verbs: {:?}",
        report.unknown_verbs
    );
}
