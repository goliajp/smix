//! Do the two lists of breaking changes agree?
//!
//! There are two, and on 2026-07-22 they held six entries and eight.
//! `docs/v2.md` has a table headed "六项破坏性变更"; `CHANGELOG.md`
//! has a `### Breaking` section under `## [2.0.0]`. Two of the
//! CHANGELOG's entries had never been written back into the table,
//! which the project charter (§10) requires — a change to what the
//! version *is* belongs in the version's boundary file.
//!
//! Worse, the four segments after it each introduced breaking changes
//! that neither list knew about. The release notes are the only thing
//! a user reads.
//!
//! # WHAT THIS CANNOT SEE
//!
//! * **Whether an entry belongs on the list at all.** Whether adding a
//!   `pub` field breaks a caller depends on `non_exhaustive` and on
//!   whether anyone constructs the struct with a literal. That is a
//!   judgement; a gate that made it would be answering with something
//!   it invented. What goes on the list is decided by a person. Once
//!   it is on one list, this requires it on both.
//! * **Whether `### Added` and `### Fixed` cover what shipped.** Those
//!   sections have no stable shape to check against, and no second
//!   list to check them with.
//! * **Whether a migration note is workable.** It reads that one is
//!   present, not that following it succeeds.

/// The version boundary file's table.
const BOUNDARY: &str = include_str!("../../../docs/v2.md");
/// The release notes.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// The bold phrase opening each `### Breaking` entry, in order.
///
/// Taken verbatim between the `**` pairs — one entry's phrase carries
/// backticks and an underscore, and regularising it would make the
/// join key something neither file actually contains.
fn changelog_breaking_phrases() -> Vec<String> {
    let section = CHANGELOG
        .split("### Breaking")
        .nth(1)
        .expect("CHANGELOG still has a Breaking section under 2.0.0")
        .split("### Added")
        .next()
        .expect("the Breaking section still ends at Added");
    section
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- "))
        .filter_map(|l| l.strip_prefix("**"))
        .filter_map(|l| l.split_once("**").map(|(phrase, _)| phrase.to_string()))
        .collect()
}

/// One row of the boundary table: its number and the CHANGELOG phrase
/// it claims.
fn boundary_rows() -> Vec<(String, String)> {
    let table = BOUNDARY
        .split("## 破坏性变更")
        .nth(1)
        .expect("the boundary file still has a breaking-change table")
        .split("\n## ")
        .next()
        .expect("the table still ends at the next heading");
    let mut out = Vec::new();
    for line in table.lines() {
        let line = line.trim();
        if !line.starts_with("| ") {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split(" | ").map(str::trim).collect();
        let first = cells.first().copied().unwrap_or("");
        if first == "#" || first.starts_with("---") {
            continue;
        }
        assert_eq!(
            cells.len(),
            4,
            "breaking-change row `{first}` has {} cells, not 4 — escape \
             any `|` inside a cell as `\\|`. A row this reader cannot \
             split is a row nothing checks",
            cells.len()
        );
        // A phrase containing a backtick is written in double
        // backticks with padding spaces — the markdown spelling for
        // "this code span contains a backtick". The padding is
        // presentation, the phrase inside is the join key.
        out.push((
            first.to_string(),
            cells[3].trim_matches('`').trim().to_string(),
        ));
    }
    out
}

/// Every breaking change appears on both lists.
#[test]
fn every_breaking_change_is_in_both_lists() {
    let rows = boundary_rows();
    let phrases = changelog_breaking_phrases();
    assert!(
        rows.len() >= 6,
        "only {} rows read out of the boundary table — the shape \
         changed and this would pass by knowing nothing",
        rows.len()
    );
    assert!(
        phrases.len() >= 6,
        "only {} bold phrases read out of the CHANGELOG's Breaking \
         section — same",
        phrases.len()
    );

    let mut dangling = Vec::new();
    for (n, phrase) in &rows {
        if !phrases.contains(phrase) {
            dangling.push(format!(
                "row {n} claims `{phrase}`, which the CHANGELOG does not open an entry with"
            ));
        }
    }
    let mut orphaned = Vec::new();
    for p in &phrases {
        if !rows.iter().any(|(_, phrase)| phrase == p) {
            orphaned.push(format!(
                "the CHANGELOG lists `{p}` and no row of the boundary table claims it"
            ));
        }
    }
    let mut problems = dangling;
    problems.extend(orphaned);
    assert!(
        problems.is_empty(),
        "the two breaking-change lists disagree in {} places:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

/// Print what the two lists came to.
#[test]
fn summary() {
    let rows = boundary_rows();
    println!(
        "release-record: {} breaking changes, both lists agree",
        rows.len()
    );
}

/// This gate is only worth having where it actually runs.
///
/// Both files it reads are `include_str!`ed into this crate, so
/// preflight's doc-to-crate derivation pulls smix-cli in when either
/// changes — the mechanism `guide_gate` needed for the same reason.
/// That derivation is what makes an edit to `CHANGELOG.md` alone reach
/// this check, so its absence is worth failing on rather than assuming.
#[test]
fn this_gate_runs_where_it_must() {
    let preflight = include_str!("../../../scripts/dev/preflight.sh");
    assert!(
        preflight.contains("include_str!(\\\"[^\\\"]*$d\\\")"),
        "preflight no longer maps a changed doc back to the crates \
         whose tests read it — edit only the CHANGELOG and nothing \
         checks it against the boundary file"
    );
    for (name, text) in [
        ("ci.yml", include_str!("../../../.github/workflows/ci.yml")),
        ("ship.sh", include_str!("../../../scripts/release/ship.sh")),
    ] {
        assert!(
            text.contains("cargo test --workspace"),
            "{name} no longer runs the whole workspace, so nothing there \
             runs this gate"
        );
    }
}
