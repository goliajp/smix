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
use smix_driver::settle::{Aim, frames_agree, until_aim_settles};
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

type Reading = std::future::Ready<Result<Option<Aim>, smix_error::ExpectationFailure>>;

/// Readings the target gives, in order; the last repeats for ever.
fn readings(ys: &[f64]) -> impl FnMut() -> Reading {
    let ys = ys.to_vec();
    let at = RefCell::new(0usize);
    move || {
        let i = (*at.borrow()).min(ys.len() - 1);
        *at.borrow_mut() += 1;
        std::future::ready(Ok(Some(aim(ys[i]))))
    }
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
