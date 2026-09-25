//! The runner saying "the app is not running" is a fact about the app.
//!
//! The iOS runner reads `XCUIApplication.state` and answers `/tree` with
//! a category. `not-running` and `crashed-during-init` say the app is
//! gone; they reached callers as DRIVER_ERROR, the code for "smix is
//! broken", so a gate that wanted to collect the app's crash report had
//! nothing to branch on but the wording (K2, 2026-09-25: CI's fixture
//! left mid-flow twice and the run said DRIVER_ERROR both times).
//!
//! The other categories are about the reading, not the app, and keep
//! DRIVER_ERROR.

use smix_driver::transport_to_failure;
use smix_error::FailureCode;
use smix_runner_client::RunnerTransportError;

fn unavailable(category: Option<&str>) -> RunnerTransportError {
    RunnerTransportError::AppUnavailable {
        endpoint: "/tree".to_string(),
        target: Some("jp.golia.smix.fixture".to_string()),
        reason: category.map(str::to_string),
        category: category.map(str::to_string),
        hint: Some("launch it again".to_string()),
    }
}

#[test]
fn an_app_the_runner_saw_gone_is_app_not_running() {
    for cat in ["not-running", "crashed-during-init"] {
        let f = transport_to_failure(unavailable(Some(cat)));
        assert_eq!(f.code, FailureCode::AppNotRunning, "{cat}");
        assert!(
            f.hint
                .as_deref()
                .unwrap_or_default()
                .contains("launch it again"),
            "the runner's own advice still reaches the reader: {:?}",
            f.hint
        );
    }
}

#[test]
fn a_reading_that_failed_stays_a_driver_error() {
    for cat in [
        Some("alive-but-tree-empty"),
        Some("alive-but-tree-stale"),
        Some("driver-disconnected"),
        Some("unknown"),
        None,
    ] {
        assert_eq!(
            transport_to_failure(unavailable(cat)).code,
            FailureCode::DriverError,
            "{cat:?}"
        );
    }
}
