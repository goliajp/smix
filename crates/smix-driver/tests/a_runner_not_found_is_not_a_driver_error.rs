//! The runner saying "I could not find it" is not the transport failing.
//!
//! `/fill` and friends answer 404 with `{"error":"not_found"}` when the
//! selector matched nothing. That reached the caller as
//!
//!     FAIL [DRIVER_ERROR]: runner /fill returned status 404:
//!     {"ok":false,"error":"not_found","selector":{"text":"_focused_"}}
//!
//! — the runner's wire body, verbatim, under a code that means "something
//! is broken". Android answers the same situation with ELEMENT_NOT_FOUND,
//! so the two platforms disagreed about what kind of thing this is, and
//! the iOS half also spilled an internal spelling (`_focused_`) at the
//! reader. Measured 2026-08-25 on the fixture.
//!
//! The code is what callers branch on: `smix_sdk::focused_fill_refusal`
//! only enriches a not-found, so under DRIVER_ERROR the advice about how
//! to fix it never fired either.

use smix_driver::transport_to_failure;
use smix_error::FailureCode;
use smix_runner_client::RunnerTransportError;

fn status(endpoint: &str, status: u16, body: &str) -> RunnerTransportError {
    RunnerTransportError::NonSuccessStatus {
        endpoint: endpoint.to_string(),
        status,
        body: body.to_string(),
    }
}

#[test]
fn a_404_not_found_is_element_not_found() {
    let f = transport_to_failure(status(
        "/fill",
        404,
        r#"{"ok":false,"error":"not_found","selector":{"text":"_focused_"}}"#,
    ));
    assert_eq!(
        f.code,
        FailureCode::ElementNotFound,
        "the runner found nothing; nothing is broken"
    );
}

#[test]
fn another_404_that_is_not_a_miss_stays_a_driver_error() {
    // A 404 whose body does not say `not_found` is a route this runner
    // does not have — a version mismatch, not a selector that matched
    // nothing, and the two want opposite fixes.
    //
    // Same endpoint as the case above on purpose: the body is the only
    // thing that differs, so this isolates what the mapping actually
    // reads. (It also keeps a route-shaped literal that no runner serves
    // out of the tree, which `route-conformance` is right to ask about.)
    let f = transport_to_failure(status("/fill", 404, "no such route"));
    assert_eq!(f.code, FailureCode::DriverError);
}

#[test]
fn a_500_is_still_a_driver_error() {
    let f = transport_to_failure(status("/fill", 500, r#"{"error":"boom"}"#));
    assert_eq!(f.code, FailureCode::DriverError);
}
