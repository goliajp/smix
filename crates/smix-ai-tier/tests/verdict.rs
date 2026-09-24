//! Contract tests for the AI-tier verdict path.
//!
//! The contract these pin down is that the tier fails loudly. A missing CLI, a
//! timeout, or output that isn't a verdict must all surface as errors, because
//! the alternative — reporting `pass: false` — says "your app is broken" when
//! the truth is "the judge never ran".

use std::os::unix::fs::PermissionsExt;

use smix_ai_tier::{AiTierConfig, StructuredVerdict, ask, extract, judge};
use smix_error::FailureCode;

/// One test at a time may hold a written executable and a running child.
///
/// Each test writes a stub program and has the code under test run it.
/// The harness runs tests on several threads, so one thread can fork while
/// another still holds its stub open for writing; the forked child carries
/// that descriptor until it execs, and Linux refuses to execute a file that
/// is open for writing — `ETXTBSY`, "Text file busy". It failed about one
/// run in five on Linux and never on macOS. Taking this lock around writing
/// and running makes the two impossible to overlap, rather than retrying
/// until they happen not to.
static EXEC: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn exec_lock() -> std::sync::MutexGuard<'static, ()> {
    // A test that panicked while holding it poisons it; the next test is
    // not the one that failed, so it proceeds.
    EXEC.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One runtime for the whole binary, rather than one per test.
///
/// tokio reaps child processes through a driver owned by a runtime. Give each
/// test its own runtime and a stub's exit can land while that runtime isn't
/// being polled, so `output()` never resolves and the call sits there until the
/// timeout fires — indistinguishable from a judge that hung. Production runs on
/// a single runtime; the tests should model that rather than invent a
/// concurrency shape the real caller never has.
fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    })
}

/// A PNG signature is enough: the stubs never decode the image, and the real
/// CLI reads it off disk.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

const CONDITION: &str = "a red error toast is visible";

/// Write an executable stub that stands in for the `claude` CLI.
fn stub_cli(dir: &std::path::Path, body: &str) -> AiTierConfig {
    let bin = dir.join("claude-stub");
    std::fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    AiTierConfig {
        claude_bin: bin.to_string_lossy().into_owned(),
        // Generous on purpose. These stubs exit immediately, so the
        // ceiling is scaffolding rather than the subject — and a tight
        // one turns "the CI runner was slow to spawn a shell" into a
        // red build about `claude` failing. The one test where the
        // timeout IS the subject sets its own, short.
        timeout_secs: 120,
    }
}

#[test]
fn ask_pub_runs_stub() {
    let _exec = exec_lock();
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let cfg = stub_cli(dir.path(), "echo hi");
        let reply = ask("p".to_string(), &cfg).await.unwrap();
        assert_eq!(reply, "hi\n");
    });
}

#[test]
fn verdict_deserializes_from_cli_json() {
    let v: StructuredVerdict =
        serde_json::from_str(r#"{"pass": true, "reason": "a red toast is on screen"}"#).unwrap();
    assert!(v.pass);
    assert_eq!(v.reason, "a red toast is on screen");
}

#[test]
fn missing_cli_reports_driver_error_with_an_install_hint() {
    // It spawns too: the fork happens before the missing program is found,
    // and carries whatever another test has open for writing.
    let _exec = exec_lock();
    rt().block_on(async {
        let cfg = AiTierConfig {
            claude_bin: "/nonexistent/definitely-not-claude".into(),
            // Also scaffolding: this asserts what a failed spawn
            // reports, and a spawn that cannot happen never reaches a
            // timeout anyway.
            timeout_secs: 120,
        };
        let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
        assert_eq!(err.code, FailureCode::DriverError);
        let hint = err.hint.unwrap_or_default();
        assert!(
            hint.contains("claude"),
            "hint should tell the user which binary is missing; got: {hint}"
        );
    });
}

#[test]
fn a_verdict_round_trips_from_the_cli() {
    let _exec = exec_lock();
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let cfg = stub_cli(
            dir.path(),
            r#"echo '{"pass": false, "reason": "no toast on screen"}'"#,
        );
        let v = judge(PNG, CONDITION, &cfg).await.unwrap();
        assert!(!v.pass);
        assert_eq!(v.reason, "no toast on screen");
    });
}

#[test]
fn a_verdict_wrapped_in_prose_still_parses() {
    let _exec = exec_lock();
    rt().block_on(async {
        // Models like to introduce themselves. The object is what matters.
        let dir = tempfile::tempdir().unwrap();
        let cfg = stub_cli(
            dir.path(),
            r#"echo 'Looking at the screenshot: {"pass": true, "reason": "red toast, top right"} — hope that helps!'"#,
        );
        let v = judge(PNG, CONDITION, &cfg).await.unwrap();
        assert!(v.pass);
        assert_eq!(v.reason, "red toast, top right");
    });
}

#[test]
fn unparseable_output_is_an_error_not_a_false_verdict() {
    let _exec = exec_lock();
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let cfg = stub_cli(dir.path(), "echo 'I think the toast is probably fine'");
        let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
        assert_eq!(err.code, FailureCode::DriverError);
        assert!(
            err.message.contains("verdict"),
            "the error must say the verdict was unreadable, not that the assertion failed; got: {}",
            err.message
        );
    });
}

#[test]
fn a_failing_cli_is_an_error_not_a_false_verdict() {
    let _exec = exec_lock();
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let cfg = stub_cli(dir.path(), "echo 'not logged in' >&2\nexit 1");
        let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
        assert_eq!(err.code, FailureCode::DriverError);
        assert!(
            err.message.contains("not logged in"),
            "the CLI's own stderr is the useful part; got: {}",
            err.message
        );
    });
}

#[test]
fn a_hanging_cli_times_out_rather_than_blocking_the_flow() {
    let _exec = exec_lock();
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = stub_cli(dir.path(), "sleep 30");
        cfg.timeout_secs = 1;
        let err = judge(PNG, CONDITION, &cfg).await.unwrap_err();
        assert_eq!(err.code, FailureCode::DriverError);
        assert!(err.message.contains("timed out"), "got: {}", err.message);
    });
}

#[test]
fn extract_reads_named_fields_off_the_screen() {
    let _exec = exec_lock();
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let cfg = stub_cli(
            dir.path(),
            r#"echo '{"total": "42.00", "currency": "JPY"}'"#,
        );
        let fields = vec!["total".to_string(), "currency".to_string()];
        let got = extract(PNG, &fields, &cfg).await.unwrap();
        assert_eq!(got.get("total").map(String::as_str), Some("42.00"));
        assert_eq!(got.get("currency").map(String::as_str), Some("JPY"));
    });
}

#[test]
fn extract_reports_an_unreadable_reply_rather_than_empty_fields() {
    let _exec = exec_lock();
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let cfg = stub_cli(dir.path(), "echo 'the total looks like 42 yen'");
        let fields = vec!["total".to_string()];
        let err = extract(PNG, &fields, &cfg).await.unwrap_err();
        assert_eq!(err.code, FailureCode::DriverError);
        // Silently returning an empty map would read as "the screen has no
        // total", which is a different claim from "the judge didn't answer".
        assert!(err.message.contains("field object"), "got: {}", err.message);
    });
}
