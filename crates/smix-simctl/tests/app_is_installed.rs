//! The half of `app_is_installed` that needs no simulator.
//!
//! simctl exits non-zero both for an app that is not there and for a udid
//! it does not know. Reading every non-zero exit as "not installed" would
//! answer a confident, plausible, wrong sentence about a device that does
//! not exist -- and `foreground` would print it as the reason a flow
//! failed. This pins the discrimination from the side that needs no
//! device: a udid no machine has.
//!
//! It lives in its own binary rather than in the crate's unit tests
//! because it runs a real subprocess, and `subprocess_ring`'s unit test
//! asserts an exact count of the records this process has written. Put
//! here, that test saw two where it had written one.

#![cfg(target_os = "macos")]

use smix_simctl::SimctlClient;

#[tokio::test]
async fn an_unknown_device_is_not_an_uninstalled_app() {
    let client = SimctlClient::new();
    let err = client
        .app_is_installed(
            "00000000-0000-0000-0000-000000000000",
            "jp.golia.smix.fixture",
        )
        .await
        .expect_err("an unknown udid is not an answer about an app");
    let said = err.to_string();
    assert!(
        said.contains("Invalid device") || said.contains("00000000-0000"),
        "the error should carry simctl's own words about the device, got: {said}"
    );
}
