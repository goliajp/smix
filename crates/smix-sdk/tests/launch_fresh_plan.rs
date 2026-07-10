//! `App::launch_fresh` plan/execute split unit test.
//!
//! `plan_launch_fresh_calls` is a pure function that computes the
//! simctl op sequence + warnings from `(clear_state, clear_keychain,
//! app_path)`. Splitting plan from execute means the per-scenario op
//! order can be `assert_eq!`-ed directly — no mock SimctlClient needed
//! (and one wouldn't help much anyway since `SimctlClient` is a ZST).
//!
//! v1.0.4 §D12 changed the DEFAULT clear-state semantic to in-place
//! (`Terminate + PrivacyResetAll + SandboxClearInPlace + Launch`) to
//! avoid the iOS 26.5 XCUITest binding loss (feedback §F) and the
//! ReportCrash "Insight quit unexpectedly" dialog (feedback §H) that
//! the legacy uninstall+install sequence tripped. Consumers on the
//! legacy path opt in via `SMIX_LAUNCH_FRESH_FORCE_REINSTALL=1` or
//! `plan_launch_fresh_calls_v2(..., force_reinstall: true)`.
//!
//! Scenarios covered here match the runtime shipping behaviour so the
//! test suite regressions surface a real behaviour break.

use smix_sdk::{LaunchFreshOp, plan_launch_fresh_calls, plan_launch_fresh_calls_v2};

#[test]
fn plan_some_path_clear_state_default_is_in_place() {
    let (ops, warnings) = plan_launch_fresh_calls(true, false, Some("/tmp/X.app"));
    assert_eq!(
        ops,
        vec![
            LaunchFreshOp::Terminate,
            LaunchFreshOp::PrivacyResetAll,
            LaunchFreshOp::SandboxClearInPlace(String::new()),
            LaunchFreshOp::Launch,
        ],
    );
    // v1.0.4 emits one advisory warning when app_path is set but
    // in-place clear is used, pointing consumers at the opt-in env.
    assert_eq!(warnings.len(), 1, "expected one advisory: {warnings:?}");
    assert!(
        warnings[0].contains("SMIX_LAUNCH_FRESH_FORCE_REINSTALL"),
        "warning should mention the opt-in env: {:?}",
        warnings[0],
    );
}

#[test]
fn plan_some_path_clear_state_and_keychain_default_is_in_place() {
    let (ops, warnings) = plan_launch_fresh_calls(true, true, Some("/tmp/X.app"));
    assert_eq!(
        ops,
        vec![
            LaunchFreshOp::Terminate,
            LaunchFreshOp::PrivacyResetAll,
            LaunchFreshOp::SandboxClearInPlace(String::new()),
            LaunchFreshOp::KeychainReset,
            LaunchFreshOp::Launch,
        ],
    );
    assert_eq!(warnings.len(), 1);
}

#[test]
fn plan_force_reinstall_v2_still_does_uninstall_and_install() {
    let (ops, warnings) =
        plan_launch_fresh_calls_v2(true, false, Some("/tmp/X.app"), true);
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
        "no warnings expected on force-reinstall path: {warnings:?}"
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
        "no warnings expected on non-clear-state path: {warnings:?}"
    );
}

#[test]
fn plan_none_path_clear_state_default_is_in_place() {
    // v1.0.4 §D12: when app_path is missing on the default path we
    // still do the in-place clear (no reinstall to fall back to).
    let (ops, _warnings) = plan_launch_fresh_calls(true, false, None);
    assert_eq!(
        ops,
        vec![
            LaunchFreshOp::Terminate,
            LaunchFreshOp::PrivacyResetAll,
            LaunchFreshOp::SandboxClearInPlace(String::new()),
            LaunchFreshOp::Launch,
        ],
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
