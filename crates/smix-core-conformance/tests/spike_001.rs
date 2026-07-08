//! Conformance fixture #1 — empty tree + id selector miss → empty Vec.
//!
//! Mirrors `crates/smix-ffi/src/lib.rs#tests#spike_001_empty_tree_id_miss`
//! but goes through the public conformance harness (load_fixture +
//! run_rust path), proving the harness is wired correctly end-to-end.

use smix_core_conformance::load_fixture;

#[test]
fn spike_001_rust_backend() {
    let fixture = load_fixture("spike-001").expect("fixture spike-001 must load");
    assert_eq!(fixture.id, "spike-001");
    assert!(fixture.description.contains("Empty"));

    let tree_json = serde_json::to_string(&fixture.tree).unwrap();
    let selector_json = serde_json::to_string(&fixture.selector).unwrap();
    let actual = smix_ffi::resolve_selector(tree_json, selector_json)
        .expect("resolve_selector OK on valid fixture inputs");

    assert_eq!(actual, fixture.expected, "rust backend ≠ fixture.expected");
    assert!(actual.is_empty(), "empty tree + id miss must yield []");
}
