//! The stores that left must not come back by accident.
//!
//! Six JSON state files, a postgres and a valkey were replaced by one
//! embedded store. Each of them can reappear in a single line — a
//! `std::fs::write` to a familiar filename, a `sqlx` query added
//! because it was the shape someone reached for. None of that would
//! fail a test; it would just quietly split smix's state across two
//! places again, which is the condition this migration existed to end.
//!
//! So the absence is checked. Bringing either back is allowed — it just
//! has to be a decision someone makes here, in the open.

use std::path::{Path, PathBuf};

/// Filenames that used to hold smix's state. Naming one as a write
/// target means a second store is being written.
const RETIRED_FILES: &[&str] = &[
    "sims.json",
    "state.json",
    "flow-attempts.json",
    "subprocess-ring.json",
    "reset-app-data-counters.json",
];

/// External stores that are gone. Either can be reintroduced; it should
/// be visible when it happens.
const RETIRED_STORES: &[&str] = &["sqlx", "redis::"];

/// Files whose whole job is reading the old formats, plus this test.
fn is_exempt(path: &Path) -> bool {
    let p = path.to_string_lossy();
    // The importer reads legacy files by design; the runner and capsule
    // modules read one on first open for the same reason.
    p.ends_with("smix-store/src/import.rs")
        || p.ends_with("smix-cli/src/runner_state.rs")
        || p.ends_with("smix-cli/src/capsule.rs")
        || p.ends_with("smix-simctl/src/registry.rs")
}

fn production_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let crates = root.join("crates");
    let Ok(entries) = std::fs::read_dir(&crates) else {
        return out;
    };
    for entry in entries.flatten() {
        let src = entry.path().join("src");
        if src.is_dir() {
            collect_rs(&src, &mut out);
        }
    }
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn no_production_source_writes_a_retired_state_file() {
    let root = workspace_root();
    let sources = production_sources(&root);
    assert!(
        sources.len() >= 20,
        "found only {} source files — the walk stopped matching and this \
         check would pass by knowing nothing",
        sources.len()
    );

    let mut offenders: Vec<String> = Vec::new();
    for path in &sources {
        if is_exempt(path) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            // Prose about the migration is fine; writing one is not.
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // A user-facing message naming a retired file sends someone
            // to look at a path that no longer exists. Found by running
            // `runner up` on a busy port and being told to check
            // `.smix/runner/state.json`, which nothing writes.
            //
            // Line-by-line was not enough: these messages wrap, and the
            // `format!` sat three lines above the filename. The first
            // version of this check missed exactly the bug that
            // prompted it, which is what testing a gate against a real
            // injection is for.
            if trimmed.starts_with('"') || trimmed.starts_with("     ") {
                for name in RETIRED_FILES {
                    if line.contains(name) {
                        offenders.push(format!(
                            "{}:{}: a message names a retired file",
                            path.strip_prefix(&root).unwrap_or(path).display(),
                            lineno + 1
                        ));
                    }
                }
            }
            for name in RETIRED_FILES {
                // Reading counts too. The first version of this check
                // only looked for writes, and missed `smix down` gating
                // its whole orphan sweep on `.smix/sims.json` existing —
                // which on a store-only machine it never does.
                let touches = line.contains("write")
                    || line.contains("create")
                    || line.contains("is_file")
                    || line.contains("join(");
                if line.contains(name) && touches {
                    offenders.push(format!(
                        "{}:{}: writes `{name}`",
                        path.strip_prefix(&root).unwrap_or(path).display(),
                        lineno + 1
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "state is being written outside the store again:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn no_production_source_reaches_for_a_retired_store() {
    let root = workspace_root();
    let sources = production_sources(&root);
    assert!(sources.len() >= 20, "the source walk found almost nothing");

    let mut offenders: Vec<String> = Vec::new();
    for path in &sources {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            for store in RETIRED_STORES {
                if line.contains(store) {
                    offenders.push(format!(
                        "{}:{}: uses `{store}`",
                        path.strip_prefix(&root).unwrap_or(path).display(),
                        lineno + 1
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "an external store came back:\n  {}\nIf that is deliberate, say so here.",
        offenders.join("\n  ")
    );
}

/// User-facing docs must not teach a file smix stopped writing.
///
/// Seven places — the README's quickstart among them — still told
/// readers that `smix sim register` creates `.smix/sims.json` after it
/// had stopped doing so. Nothing failed: the code was right and the
/// pages were wrong, which is the shape of every doc bug this release
/// spent a week on.
///
/// Naming the file as history is fine ("a pre-2.1 sims.json is
/// imported"); naming it as what happens now is not.
#[test]
fn no_guide_says_smix_writes_a_retired_state_file() {
    const DOCS: &[(&str, &str)] = &[
        ("README", include_str!("../../../README.md")),
        ("05-cli", include_str!("../../../docs/ai-guide/05-cli.md")),
        (
            "wire-format",
            include_str!("../../../docs/ai-guide/wire-format.md"),
        ),
        (
            "activate-header-lifetime",
            include_str!("../../../docs/ai-guide/activate-header-lifetime.md"),
        ),
    ];
    // Present-tense verbs. A sentence about importing or about what a
    // pre-2.1 install has is describing history, not behaviour.
    const PRESENT_TENSE: &[&str] = &["creates", "writes", "resolves via", "stored in", "saved to"];

    let mut checked = 0usize;
    let mut wrong: Vec<String> = Vec::new();
    for (name, doc) in DOCS {
        for (lineno, line) in doc.lines().enumerate() {
            for file in RETIRED_FILES {
                if !line.contains(file) {
                    continue;
                }
                checked += 1;
                let historical = line.contains("pre-2.1")
                    || line.contains("imported")
                    || line.contains("legacy")
                    || line.contains("SMIX_SIMS_JSON");
                if historical {
                    continue;
                }
                if PRESENT_TENSE.iter().any(|v| line.contains(v)) {
                    wrong.push(format!("{name}:{}: {}", lineno + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        checked >= 1,
        "no mention of a retired file anywhere in the guides — the \
         extraction stopped matching and this check would pass by \
         knowing nothing"
    );
    assert!(
        wrong.is_empty(),
        "the guides say smix still writes files it does not:\n  {}",
        wrong.join("\n  ")
    );
}
