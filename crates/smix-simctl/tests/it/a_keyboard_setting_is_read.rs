//! `defaults read` of a boolean answers `1` or `0`; anything else is not
//! an answer, and must not be read as "off".

use smix_simctl::parse_defaults_bool;

#[test]
fn one_is_on_and_zero_is_off() {
    assert_eq!(parse_defaults_bool("1\n"), Some(true));
    assert_eq!(parse_defaults_bool("0\n"), Some(false));
}

#[test]
fn anything_else_is_no_answer() {
    assert_eq!(parse_defaults_bool(""), None);
    assert_eq!(parse_defaults_bool("(\n    1\n)\n"), None);
    assert_eq!(parse_defaults_bool("yes\n"), None);
}

use smix_simctl::{DEFAULTS_PROGRAMS, defaults_key_is_absent, spawn_did_not_start};

/// Xcode 27's runtime root has no `/usr/bin`: the absolute path fails to
/// start (111, "Invalid or missing Program"). Earlier runtimes refused
/// the bare name the other way (255, nothing on stderr). Either is "this
/// spelling did not start", and only that moves on to the next spelling.
#[test]
fn a_program_that_did_not_start_is_told_apart_from_one_that_answered() {
    assert!(spawn_did_not_start(
        111,
        "An error was encountered processing the command (domain=com.apple.CoreSimulator.LaunchdSimError, code=111):\nProcess spawn via launchd failed.\nUnderlying error (domain=SimXPCErrorDomain, code=111):\n\tInvalid or missing Program/ProgramArguments"
    ));
    assert!(spawn_did_not_start(255, ""));
    // `defaults` ran and said no: not a spelling problem.
    assert!(!spawn_did_not_start(
        1,
        "The domain/default pair of (com.apple.keyboard.preferences, X) does not exist"
    ));
    // A device that is not booted is not fixed by another spelling.
    assert!(!spawn_did_not_start(
        149,
        "Process spawn via launchd failed because device is not booted."
    ));
}

#[test]
fn both_spellings_are_tried_bare_first() {
    assert_eq!(DEFAULTS_PROGRAMS, &["defaults", "/usr/bin/defaults"]);
}

#[test]
fn only_defaults_own_words_mean_the_key_is_unset() {
    assert!(defaults_key_is_absent(
        "2026-09-25 09:21:33.087 defaults[29110:71275193] \nThe domain/default pair of (com.apple.keyboard.preferences, NoSuchKeyX) does not exist"
    ));
    assert!(!defaults_key_is_absent("Process spawn via launchd failed."));
    assert!(!defaults_key_is_absent(""));
}
