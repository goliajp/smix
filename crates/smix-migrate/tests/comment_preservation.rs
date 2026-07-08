//! Comment preservation regression test.
//!
//! Requirements:
//! - Copyright headers (first 3 lines) must survive
//! - Audit-trail comments must survive
//! - `# Reason:` blocks must survive
//! - Blank lines must be preserved (structural readability)
//!
//! Only step-level lines change; every other line byte-identical.

use smix_migrate::Migrator;

const INPUT: &str = include_str!("fixtures/maestro-input-with-comments.yaml");
const EXPECTED: &str = include_str!("fixtures/expected-smix-with-comments.yaml");

#[test]
fn comments_and_blank_lines_preserved_byte_identical() {
    let (actual, report) = Migrator::default()
        .migrate(INPUT)
        .expect("migrate should not error");

    for (idx, in_line) in INPUT.lines().enumerate() {
        let in_trimmed = in_line.trim_start();
        if !in_trimmed.starts_with('#') && !in_trimmed.is_empty() {
            continue;
        }
        let out_lines: Vec<&str> = actual.lines().collect();
        let out_line = out_lines
            .get(idx)
            .unwrap_or_else(|| panic!("output shorter than input at line {idx}"));
        assert_eq!(
            *out_line, in_line,
            "comment/blank line {idx} altered\n  input:  {in_line:?}\n  output: {out_line:?}"
        );
    }

    let actual_trimmed = actual.trim_end();
    let expected_trimmed = EXPECTED.trim_end();
    assert_eq!(
        actual_trimmed, expected_trimmed,
        "line-based rewriter output diverges from expected"
    );

    assert!(!report.renamed.is_empty(), "expected step renames");
    assert!(
        report.unknown_verbs.is_empty(),
        "unexpected unknown verbs: {:?}",
        report.unknown_verbs
    );
}
