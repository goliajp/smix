//! "Already up" has to mean "ours is up", not "something is up".
//!
//! One instrumentation package, one device-side port: every host port
//! forwards onto the same in-process server, so a runner somebody else
//! installed answers `/health` perfectly and `up` reported success while
//! driving theirs.
//!
//! Measured 2026-08-29: this checkout is 10.0.0, what answered was
//! 9.0.0, and v10's `/probe` came back `not_implemented` -- which a gate
//! read out as "the probe is missing from the fixture's build". The
//! probe was in the fixture. The runner was not ours.
//!
//! The freshness check that was there asks whether the APK in this tree
//! needs rebuilding, by its mtime against the Kotlin beside it. That is
//! a different question from whether the thing RUNNING is that APK, and
//! only the second one is what "already up" claims.

use smix_capsule::runner_android::runner_is_not_ours;

#[test]
fn a_different_version_is_not_ours() {
    assert!(runner_is_not_ours(Some("9.0.0"), "10.0.0"));
}

#[test]
fn the_same_version_is() {
    assert!(!runner_is_not_ours(Some("10.0.0"), "10.0.0"));
}

#[test]
fn a_runner_too_old_to_say_is_left_alone() {
    // Runners predating the `runnerVersion` field answer the legacy
    // body. Reading silence as a mismatch would reinstall on every `up`
    // against one -- and the existing contract, written where
    // `health_runner_version` is defined, is that `None` MUST NOT be
    // treated as a mismatch.
    assert!(!runner_is_not_ours(None, "10.0.0"));
}
