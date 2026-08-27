//! Being in the tree and being touchable are two different facts.
//!
//! On iOS a modal does not remove what is behind it from the accessibility
//! tree — SwiftUI leaves it there — and a tap aimed at it is blocked by the
//! presentation. Measured on the fixture: with an alert open, tapping
//! `fixture-submit` behind it exits 0 and the app does not move; close the
//! alert and the same tap works. smix reported success for something a user
//! could not reach.
//!
//! The distinction this pins is the one that decides what to do about it:
//!
//!   Some(true)  — asked, and it can be touched
//!   Some(false) — asked, and it cannot
//!   None        — nobody said, which is NOT the same as "cannot"
//!
//! An older runner says nothing, and refusing on silence would take the tap
//! away from everyone who has not upgraded. That is a worse failure than the
//! one being fixed, so it gets its own test rather than a comment.

use smix_runner_client::{TouchVerdict, touch_verdict};

#[test]
fn a_node_that_says_it_cannot_be_touched_is_refused() {
    match touch_verdict(Some(false)) {
        TouchVerdict::Refuse(why) => {
            assert!(
                why.contains("in the tree"),
                "the refusal does not say the element was found, so a reader \
                 will look for a typo in their selector: {why}"
            );
            assert!(
                why.contains("on top") || why.contains("covering"),
                "the refusal does not say what to do about it: {why}"
            );
        }
        other => panic!("a node reported as untouchable was allowed: {other:?}"),
    }
}

#[test]
fn a_node_that_says_it_can_be_touched_goes_ahead() {
    assert!(matches!(touch_verdict(Some(true)), TouchVerdict::Proceed));
}

#[test]
fn silence_is_not_a_refusal() {
    // The whole reason this is a three-way answer. A runner that predates
    // the field says nothing, and reading that as "cannot be touched" would
    // break every tap for anyone who has not upgraded — worse than the
    // defect this is fixing, and silently so.
    assert!(
        matches!(touch_verdict(None), TouchVerdict::Proceed),
        "an unanswered question was treated as a no"
    );
}
