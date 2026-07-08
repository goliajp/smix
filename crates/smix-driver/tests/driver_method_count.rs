//! Regression guard: assert `Driver` trait carries at least 26 async
//! sense + act method declarations. Removing a method without updating
//! this floor is intentional API surface reduction and must be an
//! explicit review decision.

use std::fs;
use std::path::PathBuf;

fn traits_rs_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/traits.rs")
}

#[test]
fn trait_has_at_least_26_async_methods() {
    let src = fs::read_to_string(traits_rs_path()).expect("read src/traits.rs");
    let count = src.matches("async fn ").count();
    assert!(
        count >= 26,
        "trait has {} async fn (expected >= 26 per audit-revised iOS inventory)",
        count
    );
}
