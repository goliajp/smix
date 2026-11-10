//! Waiting for the screen to stop moving before touching a point.
//!
//! A touch aimed by selector waits for its element to hold still (see
//! [`crate::settle`]). A touch aimed at a point had no wait at all, and a
//! point touched while a pushed screen was still coming in was answered
//! `tapped` with nothing happening: UIKit takes no touches during a
//! navigation transition and drops the ones that arrive.
//!
//! The tree cannot see that. A transition animates the views' layers, and
//! the accessibility frames read the final layout from its first frame, so
//! two readings of the tree agree while the screen is still sliding. What
//! does see it is the picture: the touch waits until two screenshots in a
//! row are the same ([`crate::still`]), for at most the same
//! [`Settle`] limit the selector touches use.
//!
//! Away from the point, a part of the screen that keeps changing for over
//! a second — a video, a looping animation — is left out of the
//! comparison ([`playback`]), and the answer names where it was. A screen
//! that still moves near the point is touched when the limit comes, and
//! the answer says it was still moving: refusing would fail every touch on
//! such a screen. A touch that could not look (no screenshot from this
//! runner) says it did not look.

use std::future::Future;
use std::time::Duration;
use tokio::time::Instant;

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_screen::A11yNode;

use crate::settle::Settle;
use crate::still::{QuiescenceParams, decode_gray};

mod playback;
pub use playback::ScreenRegion;
use playback::{Look, Watch};

/// What the screen was doing when a point was touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScreenAtTouch {
    /// Two screenshots in a row agreed, after waiting this long.
    Still { waited: Duration },
    /// It was still changing when the limit came, and was touched anyway.
    Moving { waited: Duration },
    /// Not looked at: no wait was asked for, or no screenshot could be
    /// taken.
    Unseen,
    /// Two in a row agreed everywhere but inside `playing`, a part of the
    /// screen away from the point that kept changing for over a second
    /// and was left out as a video or animation playing.
    StillApartFrom {
        waited: Duration,
        playing: ScreenRegion,
    },
}

impl ScreenAtTouch {
    /// A clause for an answer that touched a point: nothing when the
    /// screen was still, otherwise what the touch could not rule out.
    #[must_use]
    pub fn caveat(&self) -> Option<String> {
        match self {
            ScreenAtTouch::Still { .. } => None,
            ScreenAtTouch::StillApartFrom { waited, playing } => Some(format!(
                "part of the screen ({}–{}% across, {}–{}% down) kept changing for over a \
                 second and was left out as a video or animation playing; the rest was still \
                 after {} ms and the point was touched then",
                playing.left,
                playing.right,
                playing.top,
                playing.bottom,
                waited.as_millis()
            )),
            ScreenAtTouch::Moving { waited } => Some(format!(
                "the screen was still moving after {} ms and was touched anyway; if nothing \
                 happened, the touch landed on a screen still arriving (wait for it first, or \
                 allow longer with `--settle-max-ms`)",
                waited.as_millis()
            )),
            ScreenAtTouch::Unseen => Some(
                "the screen was not watched before the touch, so whether it was still \
                 arriving is not known"
                    .into(),
            ),
        }
    }
}

/// Does the app have a frame? Just after a launch it reads zero, a point
/// scaled by it is not on the screen at all, and a touch sent there ended
/// the iOS runner (XCTest asserts on it).
fn framed(tree: &A11yNode) -> bool {
    tree.bounds.w > 0.0 && tree.bounds.h > 0.0
}

/// Wait, before touching the point `(nx, ny)`, for the app to have a frame
/// and then for the screen to stop moving.
///
/// `read` reads the tree; `shoot` takes a screenshot, `None` when it
/// cannot. Readings are `poll` apart while the app has no frame; frames
/// are taken back to back, the capture being the interval.
///
/// # Errors
///
/// `TIMEOUT`, with nothing touched, when the app has no frame by the
/// limit. Whatever `read` returned, when it failed.
pub(crate) async fn before_point_touch<R, RF, S, SF>(
    nx: f64,
    ny: f64,
    mut read: R,
    mut shoot: S,
    poll: Duration,
    settle: Settle,
) -> Result<ScreenAtTouch, ExpectationFailure>
where
    R: FnMut() -> RF,
    RF: Future<Output = Result<A11yNode, ExpectationFailure>>,
    S: FnMut() -> SF,
    SF: Future<Output = Option<Vec<u8>>>,
{
    let began = Instant::now();
    let budget = crate::poll::Budget::new(settle.limit);
    loop {
        let look = budget.look();
        if framed(&read().await?) {
            break;
        }
        if budget.spent_by(look) {
            return Err(no_frame(nx, ny, settle));
        }
        tokio::time::sleep(poll).await;
    }
    if settle.readings <= 1 {
        return Ok(ScreenAtTouch::Unseen);
    }
    let Some(first) = shoot().await else {
        return Ok(ScreenAtTouch::Unseen);
    };
    // a frame that cannot be read is not a look at the screen
    let Ok(mut previous) = decode_gray(&first) else {
        return Ok(ScreenAtTouch::Unseen);
    };
    let mut watch = Watch::new(QuiescenceParams::default(), nx, ny);
    // frames in a row that agree, counting `previous`
    let mut agreeing = 1;
    let mut playing = None;
    loop {
        let look = budget.look();
        let Some(shot) = shoot().await else {
            return Ok(ScreenAtTouch::Unseen);
        };
        let Ok(next) = decode_gray(&shot) else {
            return Ok(ScreenAtTouch::Unseen);
        };
        match watch.compare(&previous, &next, Instant::now()) {
            Look::Still => agreeing += 1,
            Look::StillApartFrom(region) => {
                agreeing += 1;
                playing = Some(region);
            }
            Look::Moving => {
                agreeing = 1;
                playing = None;
            }
        }
        if agreeing >= settle.readings {
            let waited = began.elapsed();
            return Ok(match playing {
                Some(playing) => ScreenAtTouch::StillApartFrom { waited, playing },
                None => ScreenAtTouch::Still { waited },
            });
        }
        if budget.spent_by(look) {
            return Ok(ScreenAtTouch::Moving {
                waited: began.elapsed(),
            });
        }
        previous = next;
    }
}

fn no_frame(nx: f64, ny: f64, settle: Settle) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::Timeout),
        message: format!(
            "the app had no frame yet after {} ms, so point ({:.0}%, {:.0}%) is not a place \
             on the screen; nothing was touched. An app just launched reads this way until \
             its first window is up.",
            settle.limit.as_millis(),
            nx * 100.0,
            ny * 100.0,
        ),
        hint: Some(
            "wait for the app's first screen before touching it (assert something on it), \
             or allow longer with `--settle-max-ms`"
                .into(),
        ),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn tree(w: f64) -> A11yNode {
        serde_json::from_value(serde_json::json!({
            "rawType": "other", "enabled": true, "selected": false,
            "hasFocus": false, "visible": true,
            "bounds": {"x": 0.0, "y": 0.0, "w": w, "h": w * 2.0}
        }))
        .expect("a minimal node")
    }

    /// A 40×80 grayscale PNG of one level.
    fn frame(level: u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 40, 80);
            e.set_color(png::ColorType::Grayscale);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[level; 40 * 80]).unwrap();
        }
        out
    }

    const POLL: Duration = Duration::from_millis(100);

    #[tokio::test(start_paused = true)]
    async fn a_point_is_touched_once_two_frames_in_a_row_agree() {
        let frames = RefCell::new(vec![10u8, 60, 120, 200, 200].into_iter());
        let shots = Cell::new(0);
        let at = before_point_touch(
            0.5,
            0.5,
            || async { Ok(tree(400.0)) },
            || {
                shots.set(shots.get() + 1);
                let f = frames.borrow_mut().next().map(frame);
                async move { f }
            },
            POLL,
            Settle::default(),
        )
        .await
        .unwrap();
        assert!(matches!(at, ScreenAtTouch::Still { .. }), "{at:?}");
        assert_eq!(shots.get(), 5, "it looked until the screen held still");
    }

    /// A 40×80 frame whose top half shows picture `n` of a video and whose
    /// bottom half is flat.
    fn playing(n: u8) -> Vec<u8> {
        let mut data = vec![200u8; 40 * 80];
        for (i, p) in data[..40 * 40].iter_mut().enumerate() {
            *p = n.wrapping_mul(37).wrapping_add(i as u8);
        }
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 40, 80);
            e.set_color(png::ColorType::Grayscale);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&data).unwrap();
        }
        out
    }

    #[tokio::test(start_paused = true)]
    async fn a_video_away_from_the_point_does_not_hold_the_touch_to_the_limit() {
        let n = Cell::new(0u8);
        let at = before_point_touch(
            0.5,
            0.8,
            || async { Ok(tree(400.0)) },
            || {
                n.set(n.get().wrapping_add(1));
                let f = playing(n.get());
                // a capture takes time; the video plays on meanwhile
                async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Some(f)
                }
            },
            POLL,
            Settle::default(),
        )
        .await
        .unwrap();
        let ScreenAtTouch::StillApartFrom { waited, playing } = at else {
            panic!("{at:?}");
        };
        assert!(waited < Duration::from_secs(2), "{waited:?}");
        assert_eq!((playing.top, playing.bottom), (0, 50), "{playing:?}");
        assert!(
            at.caveat().unwrap().contains("0–50% down"),
            "{:?}",
            at.caveat()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_screen_that_never_stops_is_touched_at_the_limit_and_says_so() {
        let level = Cell::new(0u8);
        let at = before_point_touch(
            0.5,
            0.5,
            || async { Ok(tree(400.0)) },
            || {
                level.set(level.get().wrapping_add(50));
                let f = frame(level.get());
                async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Some(f)
                }
            },
            POLL,
            Settle::new(Duration::from_millis(1000), 2),
        )
        .await
        .unwrap();
        assert!(matches!(at, ScreenAtTouch::Moving { .. }), "{at:?}");
        assert!(at.caveat().unwrap().contains("still moving"));
    }

    #[tokio::test(start_paused = true)]
    async fn no_screenshot_means_the_touch_says_it_did_not_look() {
        let at = before_point_touch(
            0.5,
            0.5,
            || async { Ok(tree(400.0)) },
            || async { None },
            POLL,
            Settle::default(),
        )
        .await
        .unwrap();
        assert_eq!(at, ScreenAtTouch::Unseen);
        assert!(at.caveat().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn an_app_with_no_frame_yet_is_waited_for() {
        let widths = RefCell::new(vec![0.0, 0.0, 400.0].into_iter());
        let reads = Cell::new(0);
        let at = before_point_touch(
            0.5,
            0.5,
            || {
                reads.set(reads.get() + 1);
                let w = widths.borrow_mut().next().unwrap_or(400.0);
                async move { Ok(tree(w)) }
            },
            || async { Some(frame(9)) },
            POLL,
            Settle::default(),
        )
        .await
        .unwrap();
        assert_eq!(reads.get(), 3);
        assert!(matches!(at, ScreenAtTouch::Still { .. }), "{at:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn an_app_that_never_gets_a_frame_is_not_touched() {
        let shots = Cell::new(0);
        let e = before_point_touch(
            0.5,
            0.5,
            || async { Ok(tree(0.0)) },
            || {
                shots.set(shots.get() + 1);
                async { Some(frame(9)) }
            },
            POLL,
            Settle::new(Duration::from_millis(500), 2),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code, FailureCode::Timeout);
        assert!(e.message.contains("no frame yet"), "{}", e.message);
        assert_eq!(shots.get(), 0, "nothing looked at, nothing touched");
    }

    #[tokio::test(start_paused = true)]
    async fn one_frame_asked_for_is_no_wait() {
        let shots = Cell::new(0);
        let at = before_point_touch(
            0.5,
            0.5,
            || async { Ok(tree(400.0)) },
            || {
                shots.set(shots.get() + 1);
                async { Some(frame(9)) }
            },
            POLL,
            Settle::new(Duration::from_millis(1000), 1),
        )
        .await
        .unwrap();
        assert_eq!(at, ScreenAtTouch::Unseen);
        assert_eq!(shots.get(), 0);
    }
}
