//! Smoke test asserting `cargo test --workspace` links the build chain
//! (workspace.lints + edition 2024 + zero-warning gate).

#[test]
fn crate_version_is_compiled_in() {
    assert!(!smix_core::__CRATE_VERSION.is_empty());
}
