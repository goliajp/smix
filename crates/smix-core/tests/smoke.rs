//! v3.1 c1 — placeholder smoke test. `cargo test --workspace` 验证 build
//! 链通 (workspace.lints + edition 2024 + zero-warning gate). Real
//! type-level tests land in c3+ when A11yNode / Selector 类型 port over.

#[test]
fn crate_version_is_compiled_in() {
    assert!(!smix_core::__CRATE_VERSION.is_empty());
}
