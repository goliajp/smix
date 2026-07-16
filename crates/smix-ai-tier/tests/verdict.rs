//! Contract tests for the AI-tier verdict path.
//!
//! The contract these pin down is that the tier fails loudly. A missing CLI, a
//! timeout, or output that isn't a verdict must all surface as errors, because
//! the alternative — reporting `pass: false` — says "your app is broken" when
//! the truth is "the judge never ran".

use std::os::unix::fs::PermissionsExt;

use smix_ai_tier::{AiTierConfig, StructuredVerdict, judge};
use smix_error::FailureCode;

/// A PNG signature is enough: the stubs below never decode the image, and the
/// real CLI reads it off disk.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

const CONDITION: &str = "a red error toast is visible";

/// Write an executable stub that stands in for the `claude` CLI.
fn stub_cli(dir: &std::path::Path, body: &str) -> AiTierConfig {
    let bin = dir.join("claude-stub");
    std::fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    AiTierConfig {
        claude_bin: bin.to_string_lossy().into_owned(),
        timeout_secs: 10,
    }
}

#[test]
fn verdict_deserializes_from_cli_json() {
    let v: StructuredVerdict =
        serde_json::from_str(r#"{"pass": true, "reason": "a red toast is on screen"}"#).unwrap();
    assert!(v.pass);
    assert_eq!(v.reason, "a red toast is on screen");
}

#[tokio::test]
async fn missing_cli_reports_driver_error_with_an_install_hint() {
    let cfg = AiTierConfig {
        claude_bin: "/nonexistent/definitely-not-claude".into(),
        timeout_secs: 10,
    };
    let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    let hint = err.hint.unwrap_or_default();
    assert!(
        hint.contains("claude"),
        "hint should tell the user which binary is missing; got: {hint}"
    );
}

#[tokio::test]
async fn a_verdict_round_trips_from_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = stub_cli(dir.path(), r#"echo '{"pass": false, "reason": "no toast on screen"}'"#);
    let v = judge(PNG, CONDITION, &cfg).await.unwrap();
    assert!(!v.pass);
    assert_eq!(v.reason, "no toast on screen");
}

#[tokio::test]
async fn a_verdict_wrapped_in_prose_still_parses() {
    // Models like to introduce themselves. The object is what matters.
    let dir = tempfile::tempdir().unwrap();
    let cfg = stub_cli(
        dir.path(),
        r#"echo 'Looking at the screenshot: {"pass": true, "reason": "red toast, top right"} — hope that helps!'"#,
    );
    let v = judge(PNG, CONDITION, &cfg).await.unwrap();
    assert!(v.pass);
    assert_eq!(v.reason, "red toast, top right");
}

#[tokio::test]
async fn unparseable_output_is_an_error_not_a_false_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = stub_cli(dir.path(), "echo 'I think the toast is probably fine'");
    let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    assert!(
        err.message.contains("verdict"),
        "the error must say the verdict was unreadable, not that the assertion failed; got: {}",
        err.message
    );
}

#[tokio::test]
async fn a_failing_cli_is_an_error_not_a_false_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = stub_cli(dir.path(), "echo 'not logged in' >&2\nexit 1");
    let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    assert!(
        err.message.contains("not logged in"),
        "the CLI's own stderr is the useful part; got: {}",
        err.message
    );
}

#[tokio::test]
async fn a_hanging_cli_times_out_rather_than_blocking_the_flow() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = stub_cli(dir.path(), "sleep 30");
    cfg.timeout_secs = 1;
    let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    assert!(
        err.message.contains("timed out"),
        "got: {}",
        err.message
    );
}
