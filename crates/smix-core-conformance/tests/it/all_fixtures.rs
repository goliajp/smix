//! Run every JSON fixture in `crates/smix-core-conformance/fixtures/`
//! through `smix_ffi::resolve_selector` and assert the returned id list
//! equals the fixture's `expected` field.
//!
//! This is the Rust backend's T1 conformance ground truth. The Swift +
//! Kotlin SDK backends must produce byte-identical output for the same
//! fixtures (per design.md §11 T1, checked by the cross-binary diff
//! harness).

use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn collect_fixture_ids() -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(fixtures_dir())
        .expect("fixtures dir must exist")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            if path.extension()?.to_str()? == "json" {
                Some(stem)
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

#[test]
fn fixture_directory_has_at_least_20_files() {
    let ids = collect_fixture_ids();
    assert!(
        ids.len() >= 20,
        "expected at least 20 fixtures (1 spike + 19 c5), found {} ({:?})",
        ids.len(),
        ids
    );
}

#[test]
fn every_fixture_resolves_to_its_expected_via_rust_backend() {
    let ids = collect_fixture_ids();
    let mut failed: Vec<String> = vec![];

    for stem in &ids {
        // load_fixture takes the leading id portion before the first `-`
        // suffix (or the full stem). spike-001-empty-tree → spike-001;
        // fixture-002-id-hit → fixture-002.
        let lookup_id: &str = stem
            .splitn(3, '-')
            .take(2)
            .collect::<Vec<_>>()
            .join("-")
            .leak();

        let fixture = match smix_core_conformance::load_fixture(lookup_id) {
            Ok(f) => f,
            Err(e) => {
                failed.push(format!("{stem}: load failed: {e}"));
                continue;
            }
        };

        let tree_json = serde_json::to_string(&fixture.tree).unwrap();
        let selector_json = serde_json::to_string(&fixture.selector).unwrap();

        let actual = match smix_ffi::resolve_selector(tree_json, selector_json) {
            Ok(v) => v,
            Err(e) => {
                failed.push(format!("{stem}: FFI raised: {e}"));
                continue;
            }
        };

        if actual != fixture.expected {
            failed.push(format!(
                "{stem}: expected {:?} but got {:?}",
                fixture.expected, actual
            ));
        }
    }

    assert!(
        failed.is_empty(),
        "{} fixture(s) failed:\n  - {}",
        failed.len(),
        failed.join("\n  - ")
    );
}
