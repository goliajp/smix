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
