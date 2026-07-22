//! A frame captured "during" a press has to be provably during it.
//!
//! The consumer this comes from backgrounded a press and screenshotted
//! concurrently, got a resting screen back, and read it as "the press
//! was too short". It was not: the screenshot had been queued behind
//! the gesture. Handing back a frame and calling it a pressed state
//! would repeat that, one layer down — so the placement is judged, and
//! a frame that cannot be placed says so.

use smix_driver::{CaptureSpan, FramePlacement, PressTiming, press_frame_placement};

/// Handler entered ~immediately, pressed 100→1100ms in.
fn press() -> PressTiming {
    PressTiming {
        sent_ms: 1_000,
        received_ms: 2_250,
        latest_down_offset_ms: 100,
        earliest_up_offset_ms: 1_100,
        handler_wall_ms: 1_200,
    }
}

#[test]
fn a_frame_wholly_inside_the_certain_window_is_during_the_press() {
    // transit total = 1250 - 1200 = 50ms, so the touch is certainly
    // down over [1000+50+100, 1000+1100] = [1150, 2100].
    let p = press();
    assert_eq!(
        press_frame_placement(
            &p,
            &CaptureSpan {
                start_ms: 1_200,
                end_ms: 1_450
            }
        ),
        FramePlacement::DuringPress
    );
}

/// The reported failure: the capture ran after the press ended.
#[test]
fn a_frame_after_lift_up_is_outside_and_says_so() {
    let p = press();
    let v = press_frame_placement(
        &p,
        &CaptureSpan {
            start_ms: 2_150,
            end_ms: 2_380,
        },
    );
    match v {
        FramePlacement::Outside(why) => {
            assert!(why.contains("after"), "{why}");
        }
        other => panic!("expected Outside, got {other:?}"),
    }
}

#[test]
fn a_frame_finished_before_touch_down_is_outside() {
    let p = press();
    assert!(matches!(
        press_frame_placement(
            &p,
            &CaptureSpan {
                start_ms: 1_020,
                end_ms: 1_100
            }
        ),
        FramePlacement::Outside(_)
    ));
}

/// Started before the touch was certainly down and still running after
/// — the pixels could be from the resting screen.
#[test]
fn a_frame_straddling_touch_down_is_uncertain() {
    let p = press();
    assert!(matches!(
        press_frame_placement(
            &p,
            &CaptureSpan {
                start_ms: 1_020,
                end_ms: 1_260
            }
        ),
        FramePlacement::Uncertain(_)
    ));
}

/// Straddling the boundary is not "mostly inside" — the pixels could
/// have been sampled at either end of the capture.
#[test]
fn a_frame_straddling_lift_up_is_uncertain_not_during() {
    let p = press();
    assert!(matches!(
        press_frame_placement(
            &p,
            &CaptureSpan {
                start_ms: 1_950,
                end_ms: 2_180
            }
        ),
        FramePlacement::Uncertain(_)
    ));
}

/// A press shorter than the round-trip ambiguity has no interval that
/// is certainly held, so nothing can be placed inside it.
#[test]
fn a_press_shorter_than_the_transit_ambiguity_can_place_nothing() {
    let p = PressTiming {
        sent_ms: 1_000,
        received_ms: 1_400,
        latest_down_offset_ms: 10,
        earliest_up_offset_ms: 60,
        handler_wall_ms: 80,
    };
    let v = press_frame_placement(
        &p,
        &CaptureSpan {
            start_ms: 1_020,
            end_ms: 1_040,
        },
    );
    match v {
        FramePlacement::Uncertain(why) => assert!(
            why.contains("50ms") && why.contains("320ms"),
            "the press length and the ambiguity both belong in it: {why}"
        ),
        other => panic!("expected Uncertain, got {other:?}"),
    }
}

/// The runner answering without timings is not evidence either way.
#[test]
fn missing_timings_are_uncertain_rather_than_assumed_good() {
    let p = PressTiming {
        sent_ms: 1_000,
        received_ms: 2_250,
        latest_down_offset_ms: 0,
        earliest_up_offset_ms: 0,
        handler_wall_ms: 0,
    };
    assert!(matches!(
        press_frame_placement(
            &p,
            &CaptureSpan {
                start_ms: 1_100,
                end_ms: 1_330
            }
        ),
        FramePlacement::Uncertain(_)
    ));
}
