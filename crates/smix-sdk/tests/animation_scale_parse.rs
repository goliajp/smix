//! Did the animation settings actually take?
//!
//! `simctl ui appearance` is documented per-simulator and behaves
//! globally, which is why a setting written by smix is not trusted
//! until it has been read back. The same doubt applies here: an
//! animation switch that reports success while the device kept its
//! animations is worse than no switch, because the run that follows
//! looks deterministic and is not.
//!
//! The comparison is a free function so it can be checked without a
//! device; issuing the settings needs one, judging the result does not.

use smix_sdk::animation_settings_verified;

#[test]
fn every_setting_at_zero_passes() {
    assert!(
        animation_settings_verified(&[
            ("window_animation_scale", "0.0"),
            ("transition_animation_scale", "0"),
            ("animator_duration_scale", "0.0"),
        ])
        .is_ok()
    );
}

/// One setting that did not take is named, not swallowed.
#[test]
fn a_setting_that_did_not_take_is_named() {
    let err = animation_settings_verified(&[
        ("window_animation_scale", "0.0"),
        ("transition_animation_scale", "1.0"),
        ("animator_duration_scale", "0.0"),
    ])
    .expect_err("a scale of 1.0 is not off");
    assert_eq!(err.len(), 1);
    assert!(
        err[0].contains("transition_animation_scale") && err[0].contains("1.0"),
        "the failure must name the setting and what came back: {err:?}"
    );
}

/// An absent setting reads as empty, and empty is not zero.
///
/// `settings get` prints `null` or nothing for a key that was never
/// written. Treating that as success is how a switch comes to report a
/// device state it never established.
#[test]
fn an_absent_setting_is_not_success() {
    for absent in ["", "null"] {
        let err = animation_settings_verified(&[("window_animation_scale", absent)])
            .expect_err("an absent setting is not off");
        assert!(
            err[0].contains("window_animation_scale"),
            "{absent:?} was accepted as off"
        );
    }
}

/// Reduce Motion reads back as the bool `defaults` wrote.
#[test]
fn reduce_motion_reads_back_as_one() {
    assert!(animation_settings_verified(&[("UIAccessibilityReduceMotionEnabled", "1")]).is_ok());
    let err = animation_settings_verified(&[("UIAccessibilityReduceMotionEnabled", "0")])
        .expect_err("0 means Reduce Motion is off");
    assert!(err[0].contains("UIAccessibilityReduceMotionEnabled"));
}

/// The state a freshly booted emulator is actually in.
///
/// Read off `sim-smix-android-01` (API 33) on 2026-07-22 before smix
/// touched it: two scales at `1.0` and the third never written, which
/// `settings get` prints as `null`. Both shapes have to be judged as
/// "not quiet" — and `null` is the interesting one, because a check
/// that only compared against `1` would have called an absent setting
/// off and reported a device state it never established.
#[test]
fn a_fresh_device_is_not_quiet() {
    let err = animation_settings_verified(&[
        ("window_animation_scale", "1.0"),
        ("transition_animation_scale", "1.0"),
        ("animator_duration_scale", "null"),
    ])
    .expect_err("a fresh emulator animates");
    assert_eq!(
        err.len(),
        3,
        "every one of the three has to be named: {err:?}"
    );
}
