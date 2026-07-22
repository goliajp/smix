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
    let behaviour = CLAIMS
        .lines()
        .filter(|l| l.contains("| behaviour |"))
        .count();
    println!(
        "release-record: {} breaking changes, both lists agree · {behaviour} \
         behaviour changes in the release notes · publish list {} crates, \
         topological",
        rows.len(),
        publish_list().len()
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

/// The guide-executability list, whose rows are the last four segments'
/// user-visible changes.
const CLAIMS: &str = include_str!("../../../docs/guide-executability.md");

/// Every bold phrase opening an entry anywhere under `## [2.0.0]`.
///
/// All three subsections, because a reader does not care which one a
/// change was filed under.
fn changelog_phrases() -> Vec<String> {
    let section = CHANGELOG
        .split("## [2.0.0]")
        .nth(1)
        .expect("CHANGELOG still has a 2.0.0 section")
        .split("\n## [")
        .next()
        .expect("the 2.0.0 section still ends at the next release");
    section
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- "))
        .filter_map(|l| l.strip_prefix("**"))
        .filter_map(|l| l.split_once("**").map(|(phrase, _)| phrase.to_string()))
        .collect()
}

/// Every change that altered behaviour is in the release notes.
///
/// The eight rows of `docs/guide-executability.md` are the last four
/// segments' findings, and six of them changed what smix does. None had
/// reached the release notes: the port ladder, the tap routes learning
/// id and label, the expression grammar, the explicit regex form — a
/// user upgrading would have met all of them undocumented.
///
/// The `kind` column is filled by a person. Whether a change is visible
/// to a user is a judgement, the same kind this file already refuses to
/// make about breaking changes; what is checkable is that the column
/// and the citation agree, and that a `behaviour` row names an entry
/// that exists.
#[test]
fn every_behaviour_change_reaches_the_release_notes() {
    let phrases = changelog_phrases();
    assert!(
        phrases.len() >= 20,
        "only {} bold phrases read out of the 2.0.0 section — the shape \
         changed and this would pass by knowing nothing",
        phrases.len()
    );

    let mut behaviour = 0usize;
    let mut problems = Vec::new();
    for line in CLAIMS.lines() {
        let line = line.trim();
        if !line.starts_with("| ") {
            continue;
        }
        let cells: Vec<&str> = line
            .trim_matches('|')
            .split(" | ")
            .map(|c| c.trim().trim_matches('`').trim())
            .collect();
        let id = cells.first().copied().unwrap_or("");
        if id == "id" || id.starts_with("---") {
            continue;
        }
        assert_eq!(
            cells.len(),
            11,
            "claim row `{id}` has {} cells, not 11 — escape any `|` \
             inside a cell as `\\|`",
            cells.len()
        );
        let kind = cells[9];
        let citation = cells[10];
        match kind {
            "docs" => {
                if citation != "—" {
                    problems.push(format!("{id} is marked docs-only and cites `{citation}`"));
                }
            }
            "behaviour" => {
                behaviour += 1;
                if !phrases.iter().any(|p| p == citation) {
                    problems.push(format!(
                        "{id} changed behaviour and cites `{citation}`, which \
                         opens no entry under 2.0.0"
                    ));
                }
            }
            other => problems.push(format!(
                "{id} has kind `{other}` — the vocabulary is docs / behaviour"
            )),
        }
    }
    assert!(
        behaviour >= 5,
        "only {behaviour} rows marked as behaviour changes — the column \
         emptied and this would pass by knowing nothing"
    );
    assert!(
        problems.is_empty(),
        "{} claims are not reflected in the release notes:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

/// The release script's publish DAG.
const SHIP: &str = include_str!("../../../scripts/release/ship.sh");
/// The workspace manifest, for the member list.
const WORKSPACE: &str = include_str!("../../../Cargo.toml");

/// Names in `CRATES=( … )`, in the order they are published.
fn publish_list() -> Vec<String> {
    SHIP.split("CRATES=(")
        .nth(1)
        .expect("ship.sh still declares a publish DAG")
        .split(')')
        .next()
        .expect("the DAG literal still closes")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Workspace members, and whether each opts out of publishing.
///
/// The manifest says `members = ["crates/*"]`, so the list is the
/// directory. Reading the glob rather than a names list here means a
/// crate added tomorrow is covered without touching this.
fn members() -> Vec<(String, bool, Vec<String>)> {
    assert!(
        WORKSPACE.contains("members = [\"crates/*\"]"),
        "the workspace stopped globbing crates/ — this reader assumed \
         that shape and would now report members it invented"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).expect("reading crates/").flatten() {
        let manifest_path = entry.path().join("Cargo.toml");
        let Ok(manifest) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let name = entry
            .file_name()
            .to_str()
            .expect("a crate directory name is utf-8")
            .to_string();
        let opted_out = manifest.lines().any(|l| {
            let l = l.trim();
            l.starts_with("publish") && l.contains("false")
        });
        // Which sibling crates it needs before it can be published.
        // Dev-dependencies do not count: the registry does not require
        // them to exist when publishing.
        let deps = manifest
            .split("[dev-dependencies]")
            .next()
            .unwrap_or(&manifest)
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let dep = l.split_whitespace().next()?;
                (dep.starts_with("smix-") && l.contains("path = \"../")).then(|| dep.to_string())
            })
            .collect();
        out.push((name, opted_out, deps));
    }
    out.sort();
    out
}

/// Everything that ships is in the publish list, in an order that works.
///
/// The list was written by hand and the workspace grew past it:
/// `smix-store` was absent while `smix-cli` and `smix-simctl` both
/// depended on it at `^2.0.0`, so `cargo publish -p smix-simctl` would
/// have been refused by the registry — seventeen crates into a DAG
/// whose earlier steps cannot be taken back.
///
/// Opting out is `publish = false` in the crate's own manifest, which
/// is cargo's way of saying it and the only place `cargo publish` will
/// look. A second list of exceptions here would be a second thing to
/// forget.
#[test]
fn the_publish_list_covers_everything_that_ships() {
    let listed = publish_list();
    let members = members();
    assert!(
        members.len() >= 25,
        "only {} workspace members read — the manifest's shape changed \
         and this would pass by knowing nothing",
        members.len()
    );

    let mut problems = Vec::new();
    for (name, opted_out, _) in &members {
        match (listed.contains(name), opted_out) {
            (false, false) => problems.push(format!(
                "{name} is a workspace member that does not opt out of \
                 publishing and is not in the DAG"
            )),
            (true, true) => problems.push(format!(
                "{name} declares `publish = false` and is in the DAG anyway"
            )),
            _ => {}
        }
    }
    for name in &listed {
        if !members.iter().any(|(m, _, _)| m == name) {
            problems.push(format!("the DAG names {name}, which is not a member"));
        }
    }

    // Topological: a crate's siblings must already have been published.
    let mut published: Vec<&str> = Vec::new();
    for name in &listed {
        if let Some((_, _, deps)) = members.iter().find(|(m, _, _)| m == name) {
            for d in deps {
                if listed.contains(d) && !published.contains(&d.as_str()) {
                    problems.push(format!(
                        "{name} is published before {d}, which it depends on"
                    ));
                }
            }
        }
        published.push(name);
    }

    assert!(
        problems.is_empty(),
        "the publish DAG and the workspace disagree in {} places:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

/// The semver gate does not abort on a crate it cannot check.
///
/// `cargo semver-checks --workspace` stops the whole run when a crate
/// has no published baseline or a baseline with no library target —
/// three crates are new in v2 and `smix-mcp` gained its library here,
/// so the gate immediately before publishing would have failed on all
/// four. The script's own comment had said the tool was "blind to
/// brand-new crates", a sentence nobody had run.
///
/// Checked as text, not behaviour: running it needs the network and a
/// registry baseline. What this can see is that the retry-and-exclude
/// shape is still there and that coverage is still reported from the
/// run's output rather than from the exclusion count.
#[test]
fn the_semver_gate_survives_a_crate_it_cannot_check() {
    for needle in [
        "SEMVER_EXCLUDE",
        "failed to build rustdoc for crate",
        "not found in registry",
        "of $SEMVER_TOTAL crates checked",
    ] {
        assert!(
            SHIP.contains(needle),
            "ship.sh no longer contains {needle:?} — the semver step \
             stopped tolerating crates the tool refuses, or stopped \
             saying how many it really checked"
        );
    }
}
