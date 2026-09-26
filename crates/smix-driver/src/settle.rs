//! Waiting for a tap's target to stop moving.
//!
//! The point to touch is computed from one reading of the tree, and on a
//! screen still coming in that reading is out of date by the time the
//! touch lands: a Compose field found and tapped during its screen's
//! entrance took the touch in its container (`TAP_MISSED`, 2026-09-26).
//!
//! maestro re-reads the element every 100 ms until two consecutive
//! readings give the same bounds, for up to 3 s, and then taps the last
//! position it saw (`Maestro.refreshElementUntilStable`,
//! `ELEMENT_STABILITY_POLL_INTERVAL_MS` / `ELEMENT_STABILITY_TIMEOUT_MS`).
//! The wait here is the same; the ending is not: a target still moving at
//! the limit is a failure that says so, because a touch aimed where the
//! target was is the miss the wait exists to prevent.
//!
//! Host side, so both platforms' taps — and double-tap and long-press,
//! which aim through the same resolve — go through one copy.

use crate::HitElement;
use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use std::future::Future;
use std::time::{Duration, Instant};

/// An element's `(x, y, w, h)` as the tree reports it.
pub type Frame = (f64, f64, f64, f64);

/// Where a touch goes: the normalised point, and the element it is aimed at.
pub type Aim = (f64, f64, Option<HitElement>);

/// Time between readings — maestro's `ELEMENT_STABILITY_POLL_INTERVAL_MS`.
pub const POLL: Duration = Duration::from_millis(100);

/// How long a target may keep moving before the tap gives up — maestro's
/// `ELEMENT_STABILITY_TIMEOUT_MS`.
pub const LIMIT: Duration = Duration::from_millis(3000);

/// Do two readings of a frame put the element in the same place?
///
/// Every edge within one device-independent point. maestro compares for
/// exact equality; a point of slack costs nothing a tap can notice — a
/// touch target is at least 44 pt (Apple) or 48 dp (Material) across, so
/// a sub-point shift cannot move its centre off it — and it keeps a frame
/// reported in fractional points from reading as motion. Android reports
/// pixels, so the difference is divided by the device's pixels per point.
pub fn frames_agree(a: &Frame, b: &Frame, pixels_per_point: f64) -> bool {
    let within = |p: f64, q: f64| (p - q).abs() / pixels_per_point < 1.0;
    within(a.0, b.0) && within(a.1, b.1) && within(a.2, b.2) && within(a.3, b.3)
}

/// Two readings of an aim that agree: the same frame, or — for an aim with
/// no element to compare — the same point.
fn aims_agree(a: &Aim, b: &Aim, pixels_per_point: f64) -> bool {
    match (&a.2, &b.2) {
        (Some(x), Some(y)) => frames_agree(&x.frame, &y.frame, pixels_per_point),
        _ => a.0 == b.0 && a.1 == b.1,
    }
}

fn describe(aim: &Aim) -> String {
    match &aim.2 {
        Some(e) => format!(
            "({:.0},{:.0} {:.0}×{:.0})",
            e.frame.0, e.frame.1, e.frame.2, e.frame.3
        ),
        None => format!("({:.3},{:.3})", aim.0, aim.1),
    }
}

/// Re-read the target until two consecutive readings agree, and return the
/// later one.
///
/// `first` is the reading the tap was about to use; it counts as the
/// first of the two, so a target already still costs one extra reading.
/// `reread` reads the tree once more: `Ok(None)` is a reading in which the
/// target is absent (waited through, as maestro does), and an error ends
/// the wait with that error.
///
/// # Errors
///
/// `TIMEOUT` when the target is still moving after `limit`, naming its
/// last two readings; whatever `reread` returned, when it failed.
pub async fn until_aim_settles<F, Fut>(
    first: Aim,
    mut reread: F,
    pixels_per_point: f64,
    poll: Duration,
    limit: Duration,
) -> Result<Aim, ExpectationFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<Aim>, ExpectationFailure>>,
{
    let start = Instant::now();
    let mut last = first;
    let mut before: Option<Aim> = None;
    loop {
        tokio::time::sleep(poll).await;
        if let Some(now) = reread().await? {
            if aims_agree(&last, &now, pixels_per_point) {
                return Ok(now);
            }
            before = Some(std::mem::replace(&mut last, now));
        }
        if start.elapsed() >= limit {
            let target = last.2.as_ref().map_or_else(
                || "the target".to_string(),
                |e| {
                    if e.identifier.is_empty() {
                        format!("the element labelled {:?}", e.label)
                    } else {
                        format!("id={}", e.identifier)
                    }
                },
            );
            let readings = match &before {
                Some(b) => format!("{} then {}", describe(b), describe(&last)),
                None => format!("last seen at {}", describe(&last)),
            };
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::Timeout),
                message: format!(
                    "{target} kept moving: its last two readings were {readings}, and it \
                     did not hold still between two readings in {} ms. A touch aimed at \
                     where it was would land where it no longer is.",
                    limit.as_millis()
                ),
                hint: Some(
                    "wait for what moves it to finish (an entrance animation, a scroll's \
                     momentum, content still loading) — e.g. `waitForAnimationToEnd` before \
                     the tap"
                        .into(),
                ),
                ..Default::default()
            }));
        }
    }
}
