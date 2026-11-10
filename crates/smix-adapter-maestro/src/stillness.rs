//! `waitForAnimationToEnd`: sample the screen until it holds still.

use std::time::Duration;

use smix_sdk::FailureCode;
use smix_sdk::quiescence::QuiescenceParams;

use crate::runtime::{AppLike, RunError};

/// How a stillness wait ended.
///
/// Three, not two. A bool said "settled or not", and a window in which
/// nothing could be looked at is neither — reporting it as "still
/// moving" would claim an observation nobody made, and reporting it as
/// a failure is what stopped a consumer's release pipeline on a verb
/// that exists in order to wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stillness {
    /// Two consecutive frames matched within tolerance.
    Settled,
    /// The ceiling expired with the screen still changing.
    StillMoving,
    /// The ceiling expired without one usable pair of frames: every
    /// capture in the window was refused as backpressure.
    NeverObserved,
}

/// Sample the screen until two consecutive frames are still or
/// `ceiling_ms` runs out: settled, still moving, or never given a look.
/// Screenshot backpressure ("not now, shortly") is waited through, as a
/// wait should; until 7.1 it failed the step and stopped a consumer's
/// release pipeline. An undecodable frame or an unreachable runner
/// still ends the step — a frame that cannot be compared is not motion.
pub(crate) async fn until_still<A: AppLike + ?Sized>(
    app: &A,
    params: &QuiescenceParams,
    ceiling_ms: u64,
) -> Result<Stillness, RunError> {
    // No sleep between successful samples: the capture is the
    // interval. The raw-BGRA capture path grabs a frame directly from
    // the resident IOSurface host (~sub-ms) and compares grayscale in
    // place — no PNG encode/decode. It falls back to a simctl PNG
    // frame when the surface is unavailable, and `frames_still`
    // compares BGRA and PNG frames interchangeably.
    //
    // A refusal is different: re-asking a loaded capture path
    // immediately is what put it under pressure, so a refused sample
    // does sleep. The interval is ours rather than the window the
    // pacer states, because the deadline below is what actually ends
    // this — reading a duration back out of a message would be
    // parsing prose for a decision.
    const REFUSED_RETRY: Duration = Duration::from_millis(100);
    let budget = smix_driver::poll::Budget::new(Duration::from_millis(ceiling_ms));

    let mut previous = loop {
        let look = budget.look();
        match app.capture_frame().await {
            Ok(frame) => break frame,
            Err(e) if e.code == FailureCode::CaptureBackpressure => {
                if budget.spent_by(look) {
                    return Ok(Stillness::NeverObserved);
                }
                tokio::time::sleep(REFUSED_RETRY).await;
            }
            Err(e) => return Err(RunError::Sdk(e)),
        }
    };
    loop {
        let look = budget.look();
        let next = match app.capture_frame().await {
            Ok(frame) => frame,
            Err(e) if e.code == FailureCode::CaptureBackpressure => {
                if budget.spent_by(look) {
                    return Ok(Stillness::NeverObserved);
                }
                tokio::time::sleep(REFUSED_RETRY).await;
                continue;
            }
            Err(e) => return Err(RunError::Sdk(e)),
        };
        if smix_sdk::quiescence::frames_still(&previous, &next, params)? {
            return Ok(Stillness::Settled);
        }
        if budget.spent_by(look) {
            return Ok(Stillness::StillMoving);
        }
        previous = next;
    }
}
