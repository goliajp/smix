//! The Android `runner up` branch must actually call the flag refusal.
//!
//! `reject_ios_only_up_flags` has unit tests, and they pass whether or
//! not anything calls it. That is the shape of every defect this week:
//! a thing that exists, is correct, and is wired to nothing. adb-guard
//! shipped unwired; the app module's tests ran in no gate; the
//! Input-Dispatch-Mode header was sent and read by neither runner.
//!
//! The workspace's `-D dead-code` already refuses "nobody calls it" —
//! deleting the call is a compile error, not a silent regression. What
//! it cannot see is a call in the wrong place: moved to the iOS branch,
//! or left in this one but placed after the runner is already up. Both
//! compile. The second is worse than no check at all, because it reads
//! as protection while the device has already been written to. That
//! ordering is what this file asserts, and injecting it is what proved
//! the assertion works — the first injection tried here (deleting the
//! call) was caught by the compiler before this test ran, so it
//! verified nothing.
//!
//! Testing the wiring by running the CLI is not an option here. If the
//! call were missing, `smix runner up --platform android` would fall
//! through to `runner_android::up`, which begins with `adb install` —
//! and a physical phone is routinely attached to this machine. A test
//! that reaches for the device to prove the device is protected is not
//! a test worth having.
//!
//! So this reads the source instead, the way no_json_state and
//! no_placeholder_identifiers do.

const MAIN: &str = include_str!("../src/main.rs");

/// The Android early-return inside `RunnerAction::Up`, from the platform
/// check to the `return`.
fn android_up_branch() -> &'static str {
    let start = MAIN
        .find("if platform == RunPlatform::Android {")
        .expect("the Android branch of runner up moved or was renamed");
    let rest = &MAIN[start..];
    let end = rest
        .find("return Ok(std::process::ExitCode::SUCCESS);")
        .expect("the Android branch no longer returns early — re-read it before trusting this");
    &rest[..end]
}

#[test]
fn the_branch_refuses_ios_only_flags_before_touching_adb() {
    let branch = android_up_branch();

    let call = branch.find("reject_ios_only_up_flags").expect(
        "the Android branch does not refuse iOS-only flags — --bundle, --runner-project \
                 and --supervise would be accepted and silently dropped again",
    );

    let up = branch
        .find("runner_android::up")
        .expect("the Android branch no longer brings the runner up — re-read it");

    assert!(
        call < up,
        "the refusal must come before runner_android::up, which starts with adb install: \
         refusing after the install has already happened protects nothing"
    );
}
