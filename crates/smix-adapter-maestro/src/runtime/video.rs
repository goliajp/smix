//! `assertVideoPlaying` / `assertVideoFrozen`: whether the picture inside
//! an element keeps changing.
//!
//! Frames are screenshots, compared only inside the element's box. A
//! simulator's frames of a still screen are pixel-identical, so any sample
//! that changes is the picture moving; a couple are allowed for a blinking
//! caret or a rounding somewhere.

use std::time::Duration;

use smix_driver::still::{GrayView, decode_gray};
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit, Rect, Selector};
use std::time::Instant;

use super::{Adapter, RunError, RunStepReport};
use crate::AppLike;

/// Samples across and down the element's box.
const GRID: usize = 24;
/// A sample has changed when its grey level moves by more than this.
const DELTA: u8 = 4;
/// Changed samples allowed before the picture counts as moving.
const TOLERATED: usize = 1;
/// Least time between two frames; a screenshot takes longer than this on
/// most devices, so frames come as fast as the device gives them.
const PACE: Duration = Duration::from_millis(100);

/// Part of a frame, as fractions of its width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Share {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Share {
    /// `element` within `root`, both in the tree's units, cut to the root.
    pub(super) fn of(element: Rect, root: Rect) -> Option<Self> {
        if root.w <= 0.0 || root.h <= 0.0 {
            return None;
        }
        let x1 = ((element.x - root.x) / root.w).clamp(0.0, 1.0);
        let y1 = ((element.y - root.y) / root.h).clamp(0.0, 1.0);
        let x2 = ((element.x + element.w - root.x) / root.w).clamp(0.0, 1.0);
        let y2 = ((element.y + element.h - root.y) / root.h).clamp(0.0, 1.0);
        (x2 > x1 && y2 > y1).then_some(Self {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        })
    }
}

/// Whether the picture inside `share` differs between two frames. Frames
/// of different sizes (a rotation) count as different.
pub(super) fn moved(a: &GrayView, b: &GrayView, share: Share) -> bool {
    if a.w != b.w || a.h != b.h {
        return true;
    }
    let at = |gx: usize, n: usize, start: f64, len: f64| {
        ((start + len * (gx as f64 + 0.5) / GRID as f64) * n as f64) as usize
    };
    let mut changed = 0;
    for gy in 0..GRID {
        let y = at(gy, a.h, share.y, share.h);
        for gx in 0..GRID {
            let x = at(gx, a.w, share.x, share.w);
            if a.gray(x, y).abs_diff(b.gray(x, y)) > DELTA {
                changed += 1;
                if changed > TOLERATED {
                    return true;
                }
            }
        }
    }
    false
}

impl<A: AppLike + ?Sized> Adapter<'_, A> {
    /// Watch `selector`'s box for `within`: `playing` passes at the first
    /// frame that differs from the one before, and marks how long that took
    /// as the step's `firstChange` phase; frozen passes when no frame did.
    pub(super) async fn assert_video(
        &mut self,
        selector: &Selector,
        playing: bool,
        within_ms: u64,
    ) -> Result<RunStepReport, RunError> {
        let selector = &self.desugar_localized_text(selector).into_owned();
        let within = Duration::from_millis(within_ms);
        let verb = if playing {
            "assertVideoPlaying"
        } else {
            "assertVideoFrozen"
        };
        let what = format!("{verb}: no element to watch");
        let (root, element) = self.wait_for_bounds(selector, &what).await?;
        let Some(share) = Share::of(element, root) else {
            return Err(fail(
                verb,
                selector,
                "the element has no part on the screen to watch".into(),
            ));
        };
        let began = Instant::now();
        let mut last = decode_gray(&self.app.screenshot().await?)?;
        let mut frames = 1usize;
        while began.elapsed() < within {
            tokio::time::sleep(PACE).await;
            let next = decode_gray(&self.app.screenshot().await?)?;
            frames += 1;
            if moved(&last, &next, share) {
                let after = began.elapsed().as_millis();
                if playing {
                    smix_driver::phases::mark("firstChange", began);
                    return Ok(RunStepReport::Ok);
                }
                return Err(fail(
                    verb,
                    selector,
                    format!("the picture changed {after} ms in, at frame {frames}; it is playing"),
                ));
            }
            last = next;
        }
        if playing {
            return Err(fail(
                verb,
                selector,
                format!(
                    "the picture did not change in {} ms ({frames} frames); it is frozen, or \
                     not started yet — a longer `within` waits for a first frame",
                    within.as_millis()
                ),
            ));
        }
        Ok(RunStepReport::Ok)
    }
}

fn fail(verb: &str, selector: &Selector, why: String) -> RunError {
    RunError::Sdk(ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::AssertionFailed),
        message: format!(
            "{verb} {}: {why}",
            smix_selector::describe_selector(selector)
        ),
        selector: Some(selector.clone()),
        ..Default::default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, pixel: impl Fn(u32, u32) -> u8) -> GrayView {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, w, h);
            e.set_color(png::ColorType::Grayscale);
            e.set_depth(png::BitDepth::Eight);
            let mut wr = e.write_header().expect("header");
            let data: Vec<u8> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .map(|(x, y)| pixel(x, y))
                .collect();
            wr.write_image_data(&data).expect("data");
        }
        decode_gray(&out).expect("decodes")
    }

    const TILE: Share = Share {
        x: 0.0,
        y: 0.0,
        w: 0.5,
        h: 0.5,
    };

    #[test]
    fn a_change_inside_the_box_is_motion() {
        let a = frame(100, 200, |_, _| 50);
        let b = frame(100, 200, |x, y| if x < 50 && y < 100 { 200 } else { 50 });
        assert!(moved(&a, &b, TILE));
    }

    #[test]
    fn a_change_outside_the_box_is_not() {
        let a = frame(100, 200, |_, _| 50);
        let b = frame(100, 200, |x, y| if x >= 50 || y >= 100 { 200 } else { 50 });
        assert!(!moved(&a, &b, TILE));
    }

    #[test]
    fn identical_frames_are_still_and_a_resize_is_not() {
        let a = frame(100, 200, |x, y| ((x + y) % 255) as u8);
        assert!(!moved(
            &a,
            &frame(100, 200, |x, y| ((x + y) % 255) as u8),
            TILE
        ));
        assert!(moved(&a, &frame(200, 100, |_, _| 0), TILE));
    }

    #[test]
    fn a_box_is_a_share_of_the_root_cut_to_it() {
        let root = Rect {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 800.0,
        };
        let s = Share::of(
            Rect {
                x: 100.0,
                y: 700.0,
                w: 200.0,
                h: 200.0,
            },
            root,
        )
        .expect("on screen");
        assert_eq!(
            s,
            Share {
                x: 0.25,
                y: 0.875,
                w: 0.5,
                h: 0.125
            }
        );
        assert!(
            Share::of(
                Rect {
                    x: 0.0,
                    y: 900.0,
                    w: 10.0,
                    h: 10.0
                },
                root
            )
            .is_none()
        );
    }
}
