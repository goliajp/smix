//! Screenshot backpressure is a thing a caller can act on. Say so in the code.
//!
//! `simctl_to_failure` flattened `CaptureBackpressure` into
//! `DriverError` plus a sentence carrying the one number that mattered:
//! "retry after 2999ms". A consumer's release pipeline met it as
//! `FAIL [DRIVER_ERROR]` and could not tell it apart from a broken
//! driver — and the verb it stopped was `waitForAnimationToEnd`, whose
//! entire meaning is to wait.
//!
//! The machine-readable half is the code. Prose is for the person
//! reading; a caller deciding whether to keep waiting must not have to
//! parse it, which is the anti-pattern this repository names everywhere
//! else.
//!
//! `FailureCode` has been `#[non_exhaustive]` since 6.0. That cost a
//! major once, deliberately, so that no later code would: adding a
//! variant to an exhaustive public enum is a breaking change, and two
//! releases in a row had paid one for saying a failure more precisely.
//! This is the first code to spend what that bought — it is a minor.

use smix_error::FailureCode;
use smix_simctl::DeviceControlError;
use std::time::Duration;

#[test]
fn backpressure_is_not_a_generic_driver_error() {
    let f = smix_sdk::simctl_to_failure(DeviceControlError::CaptureBackpressure {
        retry_after: Duration::from_millis(2999),
    });
    assert_eq!(
        f.code,
        FailureCode::CaptureBackpressure,
        "a caller that can wait needs to recognise this without reading English"
    );
}

#[test]
fn a_real_driver_error_still_is_one() {
    // The other half of the pair: narrowing one variant must not have
    // narrowed the default everything else falls back to.
    let f = smix_sdk::simctl_to_failure(DeviceControlError::Malformed {
        subcommand: "io screenshot".into(),
        detail: "not a png".into(),
    });
    assert_eq!(f.code, FailureCode::DriverError);
}

#[test]
fn the_number_is_still_readable_by_a_person() {
    let f = smix_sdk::simctl_to_failure(DeviceControlError::CaptureBackpressure {
        retry_after: Duration::from_millis(2999),
    });
    let hint = f.hint.unwrap_or_default();
    assert!(
        hint.contains("2999"),
        "the retry window is what a person needs in order to judge the \
         simulator's health: {hint}"
    );
}
