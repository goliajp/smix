//! Whether a scroll has brought its target far enough into view to stop.
//!
//! One pure answer that every "scroll until visible" loop asks, so the
//! loops cannot disagree about what "visible" means. They did: one
//! stopped when the target was anywhere in the tree, one when a live
//! query said it touched the viewport, and neither looked at how much of
//! it could be seen — so a row whose middle was below the screen edge
//! counted as reached and the tap that followed landed on something else.
//!
//! Semantics follow maestro's `scrollUntilVisible` (`Orchestra.kt`,
//! `UiElement.kt`), with two deliberate departures named where they
//! happen.

use smix_input::SwipeDirection;
use smix_screen::Rect;

use crate::HostResolveError;

/// A box in shares of the app frame: `(0, 0)` is the frame's top-left,
/// `(1, 1)` its bottom-right. Parts outside `0..=1` are off screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormBox {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

impl NormBox {
    /// Centre x.
    #[must_use]
    pub fn mid_x(&self) -> f64 {
        self.x + self.w / 2.0
    }

    /// Centre y.
    #[must_use]
    pub fn mid_y(&self) -> f64 {
        self.y + self.h / 2.0
    }
}

/// `node` in shares of `frame`.
///
/// # Errors
///
/// [`HostResolveError::UnknownAppFrame`] when the frame has no area, and
/// [`HostResolveError::EmptyMatchedFrame`] when the node has none —
/// dividing by either would produce a box that means nothing.
pub fn norm_box(node: Rect, frame: Rect) -> Result<NormBox, HostResolveError> {
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return Err(HostResolveError::UnknownAppFrame);
    }
    if node.w <= 0.0 || node.h <= 0.0 {
        return Err(HostResolveError::EmptyMatchedFrame);
    }
    Ok(NormBox {
        x: (node.x - frame.x) / frame.w,
        y: (node.y - frame.y) / frame.h,
        w: node.w / frame.w,
        h: node.h / frame.h,
    })
}

/// How much of what COULD be visible is visible, in `0.0..=1.0`.
///
/// The denominator is the largest part of the box the frame could ever
/// show — `min(w, 1) · min(h, 1)` — not the box's whole area. maestro
/// divides by the whole area and special-cases only a box that overhangs
/// all four edges, so a card taller than the screen with a margin at the
/// sides never reaches 100% there and the scroll runs out its timeout.
/// Here that card reads 1.0 once it fills the height.
#[must_use]
pub fn visible_share(b: NormBox) -> f64 {
    let seen_w = (b.x + b.w).min(1.0) - b.x.max(0.0);
    let seen_h = (b.y + b.h).min(1.0) - b.y.max(0.0);
    if seen_w <= 0.0 || seen_h <= 0.0 {
        return 0.0;
    }
    (seen_w * seen_h) / (b.w.min(1.0) * b.h.min(1.0))
}

/// maestro's `isElementNearScreenCenter`: the box's centre has come past
/// the middle of the screen, less a fifth, from the side the content is
/// arriving from.
///
/// `direction` is smix's navigation direction (`Down` = reveal what is
/// below), which is maestro's `ScrollDirection`; maestro evaluates this
/// on the finger's direction, the opposite one, and the inequalities
/// below are written for the navigation direction so no flip is needed.
#[must_use]
pub fn near_center(b: NormBox, direction: SwipeDirection) -> bool {
    match direction {
        SwipeDirection::Down => b.mid_y() < 0.5 + CENTER_MARGIN,
        SwipeDirection::Up => b.mid_y() > 0.5 - CENTER_MARGIN,
        SwipeDirection::Right => b.mid_x() < 0.5 + CENTER_MARGIN,
        SwipeDirection::Left => b.mid_x() > 0.5 - CENTER_MARGIN,
    }
}

/// How close to centre `centerElement` asks for, as a share of the
/// screen: maestro's `screenHeight / 5` (or width).
const CENTER_MARGIN: f64 = 0.2;

/// Below this share, `centerElement` does not try to centre yet — the
/// box has barely appeared. maestro's `visibility > 0.1`.
const CENTER_MIN_SHARE: f64 = 0.1;

/// Slack in comparing a share with what was asked for.
///
/// Shares are pixel boxes divided by a frame, and `(0.6 - 0.5) / 0.1` is
/// `0.9999999999999998`: without slack a box wholly on screen can read
/// just under 100% and the scroll never stops. A millionth of the frame
/// is a thousandth of a pixel on any screen there is, so it cannot turn
/// a box that is really short into one that is in.
const SHARE_TOLERANCE: f64 = 1e-6;

/// How many swipes `centerElement` spends trying to centre before the
/// plain share rule decides. maestro's `maxRetryCenterCount`
/// (`Orchestra.kt`): the last item of a list cannot be centred, and
/// without a limit the scroll would spend its whole timeout trying.
pub const MAX_RECENTER: u32 = 4;

/// What a scroll-until-visible asks of its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reach {
    /// Share of the box that must be visible, in `(0.0, 1.0]`
    /// (maestro's `visibilityPercentage` / 100).
    pub visibility: f64,
    /// Stop only once the box is near the middle of the screen
    /// (maestro's `centerElement`).
    pub center_element: bool,
}

impl Default for Reach {
    /// maestro's defaults: 100% visible, not centred
    /// (`DEFAULT_ELEMENT_VISIBILITY_PERCENTAGE`, `DEFAULT_CENTER_ELEMENT`).
    fn default() -> Self {
        Reach {
            visibility: 1.0,
            center_element: false,
        }
    }
}

/// The answer for one look at the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// Far enough in: stop.
    Reached,
    /// Visible but not centred, and centring is still being tried:
    /// swipe, and count it against [`MAX_RECENTER`].
    Recenter,
    /// Not far enough in: swipe. `share` is what was visible.
    Short {
        /// [`visible_share`] of this look.
        share: f64,
    },
}

/// Whether to stop, given one look at the target.
///
/// `recentered` is how many [`Verdict::Recenter`] swipes this scroll has
/// already spent.
#[must_use]
pub fn verdict(b: NormBox, reach: &Reach, direction: SwipeDirection, recentered: u32) -> Verdict {
    let share = visible_share(b);
    if reach.center_element && share > CENTER_MIN_SHARE && recentered <= MAX_RECENTER {
        return if near_center(b, direction) {
            Verdict::Reached
        } else {
            Verdict::Recenter
        };
    }
    if share >= reach.visibility - SHARE_TOLERANCE {
        Verdict::Reached
    } else {
        Verdict::Short { share }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nb(x: f64, y: f64, w: f64, h: f64) -> NormBox {
        NormBox { x, y, w, h }
    }

    const EPS: f64 = 1e-9;

    #[test]
    fn a_box_inside_the_frame_is_wholly_visible_and_one_half_out_is_half() {
        assert!((visible_share(nb(0.1, 0.2, 0.5, 0.1)) - 1.0).abs() < EPS);
        assert!((visible_share(nb(0.0, 0.9, 1.0, 0.2)) - 0.5).abs() < EPS);
        assert!(visible_share(nb(0.0, 1.2, 1.0, 0.1)).abs() < EPS);
        assert!(visible_share(nb(-0.5, 0.2, 0.2, 0.1)).abs() < EPS);
    }

    #[test]
    fn a_box_taller_than_the_frame_is_wholly_visible_once_it_fills_the_height() {
        // maestro reads this as 0.5 forever: it only special-cases a box
        // that overhangs all four edges.
        assert!((visible_share(nb(0.05, -0.5, 0.9, 2.0)) - 1.0).abs() < EPS);
        // Filling only part of the height is still short.
        assert!((visible_share(nb(0.05, 0.5, 0.9, 2.0)) - 0.5).abs() < EPS);
    }

    #[test]
    fn near_center_is_judged_from_the_side_the_content_arrives_from() {
        assert!(near_center(nb(0.0, 0.64, 1.0, 0.1), SwipeDirection::Down));
        assert!(!near_center(nb(0.0, 0.66, 1.0, 0.1), SwipeDirection::Down));
        assert!(near_center(nb(0.0, 0.26, 1.0, 0.1), SwipeDirection::Up));
        assert!(!near_center(nb(0.0, 0.24, 1.0, 0.1), SwipeDirection::Up));
        assert!(near_center(nb(0.64, 0.0, 0.1, 1.0), SwipeDirection::Right));
        assert!(!near_center(nb(0.66, 0.0, 0.1, 1.0), SwipeDirection::Right));
        assert!(near_center(nb(0.26, 0.0, 0.1, 1.0), SwipeDirection::Left));
        assert!(!near_center(nb(0.24, 0.0, 0.1, 1.0), SwipeDirection::Left));
    }

    #[test]
    fn the_default_stops_only_when_the_whole_box_is_in() {
        let r = Reach::default();
        let d = SwipeDirection::Down;
        assert_eq!(verdict(nb(0.0, 0.5, 1.0, 0.1), &r, d, 0), Verdict::Reached);
        match verdict(nb(0.0, 0.905, 1.0, 0.1), &r, d, 0) {
            Verdict::Short { share } => assert!((share - 0.95).abs() < 1e-6),
            other => panic!("a box 95% in must be short of the default, got {other:?}"),
        }
    }

    #[test]
    fn the_consumers_row_whose_middle_is_below_the_edge_is_short() {
        // insight 09-22 #4: the scroll stopped here and the tap failed
        // with CentroidOutOfFrame { ny: 1.02 }.
        let b = nb(0.0, 0.95, 1.0, 0.14);
        assert!(b.mid_y() > 1.0);
        assert!(matches!(
            verdict(b, &Reach::default(), SwipeDirection::Down, 0),
            Verdict::Short { .. }
        ));
    }

    #[test]
    fn center_element_centres_then_gives_way_to_the_share_rule() {
        let r = Reach {
            visibility: 1.0,
            center_element: true,
        };
        let d = SwipeDirection::Down;
        // In, and its centre is past the middle less a fifth: stop.
        assert_eq!(
            verdict(nb(0.0, 0.5, 1.0, 0.2), &r, d, 0),
            Verdict::Reached
        );
        // Wholly in but low on the screen: keep centring.
        assert_eq!(
            verdict(nb(0.0, 0.8, 1.0, 0.1), &r, d, 0),
            Verdict::Recenter
        );
        assert_eq!(
            verdict(nb(0.0, 0.8, 1.0, 0.1), &r, d, MAX_RECENTER),
            Verdict::Recenter
        );
        // Out of retries: the share rule decides, and it is wholly in.
        assert_eq!(
            verdict(nb(0.0, 0.8, 1.0, 0.1), &r, d, MAX_RECENTER + 1),
            Verdict::Reached
        );
        // Barely in: centring has not started; the share rule says short.
        assert!(matches!(
            verdict(nb(0.0, 0.99, 1.0, 0.2), &r, d, 0),
            Verdict::Short { .. }
        ));
    }

    #[test]
    fn a_lower_visibility_is_taken_at_its_word() {
        // maestro's `visibilityPercentage / 100` is integer division, so
        // every value below 100 means "any match". Here 50 means half.
        let r = Reach {
            visibility: 0.5,
            center_element: false,
        };
        let d = SwipeDirection::Down;
        assert_eq!(verdict(nb(0.0, 0.9, 1.0, 0.2), &r, d, 0), Verdict::Reached);
        assert!(matches!(
            verdict(nb(0.0, 0.95, 1.0, 0.2), &r, d, 0),
            Verdict::Short { .. }
        ));
    }

    #[test]
    fn norm_box_refuses_what_it_cannot_divide_by() {
        let frame = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
        let node = Rect { x: 100.0, y: 400.0, w: 200.0, h: 80.0 };
        assert_eq!(
            norm_box(node, frame),
            Ok(nb(0.25, 0.5, 0.5, 0.1))
        );
        assert_eq!(
            norm_box(node, Rect { x: 0.0, y: 0.0, w: 0.0, h: 800.0 }),
            Err(HostResolveError::UnknownAppFrame)
        );
        assert_eq!(
            norm_box(Rect { x: 1.0, y: 1.0, w: 0.0, h: 5.0 }, frame),
            Err(HostResolveError::EmptyMatchedFrame)
        );
    }
}
