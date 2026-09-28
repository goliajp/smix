//! A fill that names no field, refused, must say what to do about it.
//!
//! `inputText: "..."` targets `_focused_`. When nothing can receive the
//! text, both platforms now refuse — but the refusal a caller reads was
//! `element not found: { focused }`, which names the runner's internal
//! spelling of the selector and tells nobody anything they can act on.
//! Android has answered exactly that for releases; iOS reached the same
//! answer in 7.1 by no longer claiming success it had no evidence for.
//!
//! Both ways out exist and neither is discoverable from that sentence:
//! name the field, or pass `--force-key-events` for a field the a11y
//! tree cannot address. Saying so once, above both drivers, is also the
//! reason this is not written into each of them — two copies of one
//! sentence is how the two Android gates' refusals drifted apart.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_sdk::focused_fill_refusal;

fn not_found() -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: "element not found: { focused }".into(),
        ..Default::default()
    })
}

#[test]
fn it_names_both_ways_out() {
    let f = focused_fill_refusal(not_found());
    let text = format!("{} {}", f.message, f.hint.clone().unwrap_or_default());
    assert!(
        text.contains("--force-key-events"),
        "the escape hatch for a field the tree cannot address: {text}"
    );
    assert!(
        text.contains("id:") || text.to_lowercase().contains("name the field"),
        "the ordinary way out is to name the field: {text}"
    );
}

#[test]
fn it_keeps_the_code_a_caller_branches_on() {
    let f = focused_fill_refusal(not_found());
    assert_eq!(
        f.code,
        FailureCode::ElementNotFound,
        "enriching the wording must not move the code out from under a caller"
    );
}

#[test]
fn it_leaves_an_unrelated_failure_alone() {
    // A driver error from the same call is not this refusal, and
    // attaching this advice to it would send someone to look at their
    // selector while the runner is unreachable.
    let other = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message: "runner unreachable".into(),
        ..Default::default()
    });
    let f = focused_fill_refusal(other);
    assert!(
        !format!("{} {}", f.message, f.hint.clone().unwrap_or_default())
            .contains("--force-key-events"),
        "advice about focus attached to a transport failure"
    );
}
