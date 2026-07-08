//! `App::launch_fresh` plan/execute split unit test.
//!
//! `plan_launch_fresh_calls` is a pure function that computes the
//! simctl op sequence + warnings from `(clear_state, clear_keychain,
//! app_path)`. Splitting plan from execute means the per-scenario op
//! order can be `assert_eq!`-ed directly — no mock SimctlClient needed
//! (and one wouldn't help much anyway since `SimctlClient` is a ZST).
//!
//! Scenarios:
//! - `Some(path)` + `clear_state` → real wipe via `uninstall + install`
//!   (iOS has no native per-app data wipe API; the maestro semantic is
//!   `uninstall + install`).
//! - `Some(path)` + `clear_keychain` only → `keychain_reset` (no wipe).
//! - `None` + any clear flag → graceful fallback to `terminate +
//!   launch` + warning. Preserves backward compatibility: examples
//!   that don't ship a `SMIX_APP_PATH_<BUNDLE>` env var keep working
//!   under the existing non-clear path.

use smix_sdk::{LaunchFreshOp, plan_launch_fresh_calls};

#[test]
fn plan_some_path_clear_state_only() {
    let (ops, warnings) = plan_launch_fresh_calls(true, false, Some("/tmp/X.app"));
    assert_eq!(
        ops,
        vec![
            LaunchFreshOp::Terminate,
            LaunchFreshOp::Uninstall,
            LaunchFreshOp::Install("/tmp/X.app".into()),
            LaunchFreshOp::Launch,
        ],
    );
    assert!(
        warnings.is_empty(),
        "no warnings expected, got {warnings:?}"
    );
}

#[test]
fn plan_some_path_clear_state_and_keychain() {
    let (ops, warnings) = plan_launch_fresh_calls(true, true, Some("/tmp/X.app"));
    assert_eq!(
        ops,
        vec![
            LaunchFreshOp::Terminate,
            LaunchFreshOp::Uninstall,
            LaunchFreshOp::Install("/tmp/X.app".into()),
            LaunchFreshOp::KeychainReset,
            LaunchFreshOp::Launch,
        ],
    );
    assert!(
        warnings.is_empty(),
        "no warnings expected, got {warnings:?}"
    );
}

#[test]
fn plan_some_path_clear_keychain_only() {
    let (ops, warnings) = plan_launch_fresh_calls(false, true, Some("/tmp/X.app"));
    assert_eq!(
        ops,
        vec![
            LaunchFreshOp::Terminate,
            LaunchFreshOp::KeychainReset,
            LaunchFreshOp::Launch,
        ],
    );
    assert!(
        warnings.is_empty(),
        "no warnings expected, got {warnings:?}"
    );
}

#[test]
fn plan_none_path_clear_state_graceful_fallback() {
    let (ops, warnings) = plan_launch_fresh_calls(true, false, None);
    assert_eq!(ops, vec![LaunchFreshOp::Terminate, LaunchFreshOp::Launch],);
    assert_eq!(
        warnings.len(),
        1,
        "exactly one warning expected, got {warnings:?}"
    );
    assert!(
        warnings[0].contains("app_path missing"),
        "warning should mention app_path missing, got {:?}",
        warnings[0],
    );
}

#[test]
fn plan_no_clear_is_warm_terminate_launch() {
    let (ops, warnings) = plan_launch_fresh_calls(false, false, None);
    assert_eq!(ops, vec![LaunchFreshOp::Terminate, LaunchFreshOp::Launch],);
    assert!(
        warnings.is_empty(),
        "no warnings expected, got {warnings:?}"
    );
}
