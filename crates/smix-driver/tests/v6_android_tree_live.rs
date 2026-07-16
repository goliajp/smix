//! Live `AndroidDriver::tree` integration test.
//!
//! `#[ignore]` by default — requires booted emulator + running
//! smix-android-runner instrumentation + adb forward tcp:28080. Drive
//! via the smoke harness:
//!
//!   bash android-runner/scripts/smoke-tree.sh   # full e2e
//!
//! For programmatic verification (this test) — bring up runner first
//! via smoke-tree.sh stages 1-4 then run:
//!
//!   cargo test -p smix-driver --test v6_android_tree_live -- --ignored
//!
//! This test makes a real HTTP call to 127.0.0.1:28080/tree and asserts
//! the returned A11yNode has a populated root (rawType + bounds + at
//! least one child).

use smix_driver::{AndroidDriver, Driver, HttpRunnerClient};

#[ignore = "requires booted Android emulator + smix-android-runner; see smoke-tree.sh"]
#[tokio::test]
async fn android_tree_returns_populated_root() {
    let driver = AndroidDriver::new(HttpRunnerClient::new(28080));
    let tree = driver
        .tree(None)
        .await
        .expect("tree should succeed when runner is up");

    assert!(!tree.raw_type.is_empty(), "rawType should be set");
    assert!(tree.bounds.w > 0.0, "root bounds width should be > 0");
    assert!(tree.bounds.h > 0.0, "root bounds height should be > 0");
    assert!(
        !tree.children.is_empty(),
        "tree should have at least one child element (root + system UI)"
    );
}
