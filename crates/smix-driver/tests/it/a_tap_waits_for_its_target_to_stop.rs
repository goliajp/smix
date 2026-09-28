//! A tap is aimed at a target that has stopped moving.
//!
//! The point to touch is computed from one reading of the tree. On a screen
//! still coming in, that reading is already out of date when the touch
//! lands: on 2026-09-26 an Android Compose screen's `compose_input` was
//! found, tapped, and the touch landed in its container — `TAP_MISSED`, in
//! one round of five. maestro re-reads the element every 100 ms until two
//! consecutive readings agree, for up to 3 s (`Maestro.refreshElementUntilStable`,
//! `ELEMENT_STABILITY_TIMEOUT_MS`), and then taps the last position it saw.
//! smix waits the same way and, where maestro taps anyway, says the target
//! kept moving: a touch aimed at a place the target has left is the miss
//! this exists to prevent.

use smix_driver::HitElement;
use smix_driver::settle::{Aim, Reading, frames_agree, until_aim_settles};
use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use std::cell::RefCell;
use std::time::Duration;

fn aim(y: f64) -> Aim {
    (
        0.5,
        y / 1000.0,
        Some(HitElement {
            identifier: "compose_input".into(),
            label: String::new(),
            frame: (44.0, y, 992.0, 140.0),
        }),
    )
}

type Next = std::future::Ready<Result<Reading, ExpectationFailure>>;

/// Readings the target gives, in order; the last repeats for ever. `None`
/// is a reading in which the target is not on screen.
fn readings_of(ys: Vec<Option<f64>>) -> impl FnMut() -> Next {
    let at = RefCell::new(0usize);
    move || {
        let i = (*at.borrow()).min(ys.len() - 1);
        *at.borrow_mut() += 1;
        std::future::ready(Ok(match ys[i] {
            Some(y) => Reading::At(aim(y)),
            None => Reading::Gone(Box::new(not_found())),
        }))
    }
}

fn readings(ys: &[f64]) -> impl FnMut() -> Next + use<> {
    readings_of(ys.iter().copied().map(Some).collect())
}

/// What the caller reports for an element the screen does not have.
fn not_found() -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: "element not found: id=compose_input".into(),
        suggestions: vec!["compose_output".into()],
        ..Default::default()
    })
}

const FAST: Duration = Duration::from_millis(0);
const LIMIT: Duration = Duration::from_millis(200);

#[tokio::test]
async fn a_target_already_still_is_tapped_where_it_is() {
    let got = until_aim_settles(aim(180.0), readings(&[180.0]), 1.0, FAST, LIMIT)
        .await
        .expect("a still target settles");
    assert_eq!(got.2.expect("aimed").frame.1, 180.0);
}

#[tokio::test]
async fn a_target_that_moves_then_stops_is_tapped_where_it_stopped() {
    let got = until_aim_settles(
        aim(900.0),
        readings(&[700.0, 400.0, 180.0, 180.0]),
        1.0,
        FAST,
        LIMIT,
    )
    .await
    .expect("a target that stops settles");
    assert_eq!(got.2.expect("aimed").frame.1, 180.0);
}

#[tokio::test]
async fn a_target_that_never_stops_is_named_not_tapped() {
    let ys: Vec<f64> = (0..10_000).map(|i| 180.0 + f64::from(i)).collect();
    let err = until_aim_settles(aim(179.0), readings(&ys), 1.0, FAST, LIMIT)
        .await
        .expect_err("a target that keeps moving is not tapped");
    assert_eq!(err.code, smix_error::FailureCode::Timeout);
    assert!(err.message.contains("kept moving"), "{}", err.message);
    // Two readings, and two different ones: "moving" is a claim about a
    // difference, and it names both sides of it.
    let frames: Vec<&str> = err
        .message
        .match_indices("(44,")
        .map(|(i, _)| &err.message[i..i + 16])
        .collect();
    assert_eq!(frames.len(), 2, "{}", err.message);
    assert_ne!(frames[0], frames[1], "{}", err.message);
}

// 2026-09-27: a control that lives three seconds was found, then gone for
// the rest of the wait, and the tap failed as `TIMEOUT … kept moving` with
// one frame named twice. It had not moved; it had left.
#[tokio::test]
async fn a_target_that_goes_away_is_reported_as_not_found() {
    let err = until_aim_settles(aim(632.0), readings_of(vec![None]), 1.0, FAST, LIMIT)
        .await
        .expect_err("a target that left is not tapped");
    assert_eq!(err.code, FailureCode::ElementNotFound, "{}", err.message);
    assert!(!err.message.contains("kept moving"), "{}", err.message);
    assert!(
        err.message.contains("(44,632 992×140)"),
        "names where it was last: {}",
        err.message
    );
    assert_eq!(err.suggestions, vec!["compose_output".to_string()]);
}

#[tokio::test]
async fn a_target_that_moves_and_then_goes_away_is_reported_as_not_found() {
    let err = until_aim_settles(
        aim(900.0),
        readings_of(vec![Some(700.0), Some(400.0), None]),
        1.0,
        FAST,
        LIMIT,
    )
    .await
    .expect_err("a target that left is not tapped");
    assert_eq!(err.code, FailureCode::ElementNotFound, "{}", err.message);
}

#[tokio::test]
async fn a_target_missing_for_one_reading_and_back_in_place_is_tapped() {
    let got = until_aim_settles(
        aim(180.0),
        readings_of(vec![None, Some(180.0)]),
        1.0,
        FAST,
        LIMIT,
    )
    .await
    .expect("a target back where it was settles");
    assert_eq!(got.2.expect("aimed").frame.1, 180.0);
}

#[test]
fn edges_within_a_point_agree_and_a_point_apart_do_not() {
    let a = (44.0, 180.0, 992.0, 140.0);
    // Android reports pixels; 2.75 pixels make a point on the fixture AVD.
    assert!(frames_agree(&a, &(44.0, 182.0, 992.0, 140.0), 2.75));
    assert!(!frames_agree(&a, &(44.0, 183.0, 992.0, 140.0), 2.75));
    // iOS reports points.
    assert!(frames_agree(&a, &(44.0, 180.5, 992.0, 140.0), 1.0));
    assert!(!frames_agree(&a, &(44.0, 181.0, 992.0, 140.0), 1.0));
}

#[tokio::test]
async fn a_slow_reading_begun_before_the_limit_is_not_the_last_word() {
    // One reading of the screen has taken two seconds on a loaded device.
    // Begun inside the budget it reports where the target was then, and
    // returns after the limit. Timing out on it gave `kept moving` about a
    // target that the next reading would have found still.
    let calls = std::cell::Cell::new(0u32);
    let reread = || {
        calls.set(calls.get() + 1);
        let n = calls.get();
        async move {
            if n == 1 {
                tokio::time::sleep(Duration::from_millis(120)).await;
                Ok(Reading::At(aim(400.0)))
            } else {
                Ok(Reading::At(aim(400.0)))
            }
        }
    };
    let got = until_aim_settles(
        aim(900.0),
        reread,
        1.0,
        Duration::from_millis(1),
        Duration::from_millis(50),
    )
    .await
    .expect("a reading begun after the limit found it still");
    assert_eq!(got.2.expect("aimed").frame.1, 400.0);
}
