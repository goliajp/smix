//! Did the tap land on the element it aimed at?
//!
//! `tapOn` reported success ten times in a row against an icon button
//! whose app-side counter never moved (EXT1, 2026-07-22). It was
//! telling the truth about what it did — a touch was synthesised at a
//! coordinate — and that is not what a reader takes "tapped" to mean.
//!
//! The host resolves a selector to an element and sends its centre.
//! The runner reports what is at that point. This decides whether they
//! are the same thing, and it is a free function because the rule is
//! the part that can be wrong: getting a coordinate onto the wire is
//! mechanical, deciding that two element descriptions are the same
//! element is a judgement with edge cases.

use smix_driver::{HitElement, TapHitVerdict, tap_hit_verdict};

fn el(id: &str, label: &str, frame: (f64, f64, f64, f64)) -> HitElement {
    HitElement {
        identifier: id.to_string(),
        label: label.to_string(),
        frame,
    }
}

#[test]
fn the_same_identifier_is_the_same_element() {
    let aimed = el("hdr-back-btn", "", (10.0, 20.0, 44.0, 44.0));
    let hit = el("hdr-back-btn", "", (10.0, 20.0, 44.0, 44.0));
    assert_eq!(
        tap_hit_verdict(&aimed, Some(&hit)),
        TapHitVerdict::Confirmed
    );
}

/// The reported case: something else was at the point.
#[test]
fn a_different_identifier_names_both() {
    let aimed = el("landing-logo", "", (0.0, 0.0, 100.0, 100.0));
    let hit = el("overlay-scrim", "", (0.0, 0.0, 400.0, 800.0));
    let verdict = tap_hit_verdict(&aimed, Some(&hit));
    let TapHitVerdict::Missed(why) = verdict else {
        panic!("expected Missed, got {verdict:?}");
    };
    assert!(
        why.contains("landing-logo") && why.contains("overlay-scrim"),
        "a miss has to name what was aimed at and what was hit: {why}"
    );
}

#[test]
fn labels_decide_when_neither_carries_an_identifier() {
    let aimed = el("", "Submit", (10.0, 20.0, 80.0, 30.0));
    assert_eq!(
        tap_hit_verdict(&aimed, Some(&el("", "Submit", (10.0, 20.0, 80.0, 30.0)))),
        TapHitVerdict::Confirmed
    );
    assert!(matches!(
        tap_hit_verdict(&aimed, Some(&el("", "Cancel", (10.0, 20.0, 80.0, 30.0)))),
        TapHitVerdict::Missed(_)
    ));
}

/// Geometry is the last resort, with a tolerance, because the frame
/// makes a round trip through normalised coordinates and back.
#[test]
fn frames_decide_when_nothing_else_can() {
    let aimed = el("", "", (10.0, 20.0, 80.0, 30.0));
    assert_eq!(
        tap_hit_verdict(&aimed, Some(&el("", "", (10.4, 20.0, 80.0, 30.0)))),
        TapHitVerdict::Confirmed
    );
    assert!(matches!(
        tap_hit_verdict(&aimed, Some(&el("", "", (30.0, 20.0, 80.0, 30.0)))),
        TapHitVerdict::Missed(_)
    ));
}

/// Nothing at the point at all.
#[test]
fn an_empty_point_is_a_miss() {
    let aimed = el("btn-login", "", (10.0, 20.0, 80.0, 30.0));
    let TapHitVerdict::Missed(why) = tap_hit_verdict(&aimed, None) else {
        panic!("a tap that landed on nothing is a miss");
    };
    assert!(
        why.contains("btn-login") && why.to_lowercase().contains("nothing"),
        "{why}"
    );
}

/// Two elements with nothing comparable are NOT confirmed.
///
/// The load-bearing case. Saying "confirmed" when there was no way to
/// compare would put this check in the same category as the thing it
/// replaces: an answer that asserts more than it checked. It has its
/// own verdict so a caller can decide, and it says why it could not
/// tell.
#[test]
fn what_cannot_be_compared_is_not_confirmed() {
    let aimed = el("", "", (0.0, 0.0, 0.0, 0.0));
    let hit = el("", "", (0.0, 0.0, 0.0, 0.0));
    let verdict = tap_hit_verdict(&aimed, Some(&hit));
    let TapHitVerdict::Unconfirmable(why) = verdict else {
        panic!("expected Unconfirmable, got {verdict:?}");
    };
    assert!(
        why.contains("identifier") || why.contains("label"),
        "it has to say what it was missing: {why}"
    );
}
