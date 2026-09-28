//! A tap into a space the touch will not be read in must refuse.
//!
//! On a landscape screen, `/tree` and `XCUIApplication.frame` agree
//! with each other — both 874×402, cell for cell — while the
//! synthesised event carries a portrait stamp, so the point computed in
//! one space is read in the other. Every tap reports the button it
//! aimed at. Nothing moves.
//!
//! The runner can now say so (`GET /coordinate-space`), which turns a
//! silent wrong answer into a decidable one. Invariant §9 #1 ③ says a
//! capability that is not available must be a loud error and never a
//! quiet degradation; reporting a successful tap into a space the touch
//! will not land in is precisely the quiet degradation.
//!
//! The decision is a pure function so this file can pin it without a
//! device. Fetching the numbers is the caller's job.

use smix_driver::{CoordinateSpace, Rect, TapSpaceVerdict, decide_tap_outcome};

fn landscape_app_portrait_stamp() -> CoordinateSpace {
    CoordinateSpace {
        app_frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 874.0,
            h: 402.0,
        },
        snapshot_root_frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 874.0,
            h: 402.0,
        },
        device_orientation: "portrait".into(),
        event_record_orientation: "portrait".into(),
        spaces_agree: false,
    }
}

fn portrait_everywhere() -> CoordinateSpace {
    CoordinateSpace {
        app_frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 402.0,
            h: 874.0,
        },
        snapshot_root_frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 402.0,
            h: 874.0,
        },
        device_orientation: "portrait".into(),
        event_record_orientation: "portrait".into(),
        spaces_agree: true,
    }
}

#[test]
fn a_disagreeing_space_refuses_rather_than_reporting_success() {
    match decide_tap_outcome(&landscape_app_portrait_stamp()) {
        TapSpaceVerdict::Refuse { .. } => {}
        TapSpaceVerdict::Proceed => {
            panic!("a tap into a space the touch is not read in was allowed to report success")
        }
    }
}

#[test]
fn an_agreeing_space_proceeds() {
    assert_eq!(
        decide_tap_outcome(&portrait_everywhere()),
        TapSpaceVerdict::Proceed
    );
}

/// "The spaces disagree" is not actionable. Which two, how, and whose
/// fault — a reader who cannot see the numbers has no way to tell a
/// harness defect from their own selector, and the consumer who
/// reported this spent that time on six coordinate mappings that could
/// never have worked.
#[test]
fn the_refusal_carries_both_spaces_and_the_stamp() {
    let TapSpaceVerdict::Refuse { message } = decide_tap_outcome(&landscape_app_portrait_stamp())
    else {
        panic!("expected a refusal");
    };

    for needle in ["874", "402", "portrait"] {
        assert!(
            message.contains(needle),
            "the refusal must name {needle:?} — without the numbers it is not actionable:\n{message}"
        );
    }
}

/// The refusal has to say this outright. Everything a test author can
/// see points at their own selector: the element resolved, the aim was
/// inside it, and the screen did not change.
#[test]
fn the_refusal_says_it_is_not_the_caller_s_selector() {
    let TapSpaceVerdict::Refuse { message } = decide_tap_outcome(&landscape_app_portrait_stamp())
    else {
        panic!("expected a refusal");
    };
    let lowered = message.to_lowercase();
    assert!(
        lowered.contains("selector"),
        "the refusal must tell the reader their selector is not the problem:\n{message}"
    );
}
