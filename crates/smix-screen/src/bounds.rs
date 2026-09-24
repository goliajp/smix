//! An element's box as a value a flow can keep and compare.
//!
//! Two questions, one place each: what a reader's rectangle is in
//! device-independent pixels, and whether a box moved between two
//! readings. Both are pure; the device is asked for its density once, by
//! the driver, and handed in.

use crate::Rect;

/// `r` in device-independent pixels, given how many of the reader's units
/// make one.
///
/// iOS reports points already (`pixels_per_point` 1); Android's readers
/// report physical pixels, and its density is that ratio. A comparison in
/// raw pixels would make `within: 1` mean a third of a point on one phone
/// and a whole one on another.
#[must_use]
pub fn rect_in_points(r: Rect, pixels_per_point: f64) -> Rect {
    Rect {
        x: r.x / pixels_per_point,
        y: r.y / pixels_per_point,
        w: r.w / pixels_per_point,
        h: r.h / pixels_per_point,
    }
}

/// What two readings of the same place may differ by without it being a
/// move: the last bits of a division, not a fraction of a pixel.
const ARITHMETIC: f64 = 1e-6;

/// How far each edge of a box moved, in the units the two boxes are in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Movement {
    /// Left edge.
    pub dx: f64,
    /// Top edge.
    pub dy: f64,
    /// Width.
    pub dw: f64,
    /// Height.
    pub dh: f64,
}

impl Movement {
    /// The largest change among the four, as a magnitude.
    #[must_use]
    pub fn largest(&self) -> f64 {
        self.dx
            .abs()
            .max(self.dy.abs())
            .max(self.dw.abs())
            .max(self.dh.abs())
    }
}

/// `Some` when `now` differs from `was` by more than `within` on any of
/// x, y, width or height; `None` when every one is within it.
///
/// Width and height are compared as well as the corner: a control that
/// grew to the right has not moved its corner, and "nothing shifted" is
/// false for it all the same.
#[must_use]
pub fn bounds_moved(was: Rect, now: Rect, within: f64) -> Option<Movement> {
    let m = Movement {
        dx: now.x - was.x,
        dy: now.y - was.y,
        dw: now.w - was.w,
        dh: now.h - was.h,
    };
    (m.largest() > within + ARITHMETIC).then_some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_box_that_did_not_move_is_unchanged_at_zero() {
        let a = r(10.0, 20.0, 48.0, 48.0);
        assert_eq!(bounds_moved(a, a, 0.0), None);
    }

    #[test]
    fn eight_down_is_a_move_at_zero_and_not_at_eight() {
        let was = r(10.0, 20.0, 48.0, 48.0);
        let now = r(10.0, 28.0, 48.0, 48.0);
        let m = bounds_moved(was, now, 0.0).expect("an 8-point move went unseen");
        assert_eq!(m.dy, 8.0);
        assert_eq!(m.largest(), 8.0);
        assert_eq!(
            bounds_moved(was, now, 8.0),
            None,
            "within: 8 refused an 8-point move"
        );
        assert!(
            bounds_moved(was, now, 7.9).is_some(),
            "within: 7.9 let an 8-point move through"
        );
    }

    #[test]
    fn growing_without_moving_the_corner_is_a_change() {
        let was = r(10.0, 20.0, 48.0, 48.0);
        let now = r(10.0, 20.0, 56.0, 48.0);
        let m = bounds_moved(was, now, 0.0).expect("a width change went unseen");
        assert_eq!(m.dw, 8.0);
    }

    #[test]
    fn pixels_become_points_by_the_density() {
        // 2.625 is a common Android density (420 dpi); 21 px is 8 points.
        let p = rect_in_points(r(21.0, 42.0, 126.0, 126.0), 2.625);
        assert_eq!(p, r(8.0, 16.0, 48.0, 48.0));
        // iOS: already points.
        let q = rect_in_points(r(8.0, 16.0, 48.0, 48.0), 1.0);
        assert_eq!(q, r(8.0, 16.0, 48.0, 48.0));
    }

    #[test]
    fn a_reading_that_rounds_differently_is_not_a_move() {
        // A pixel reading converted twice can differ in the last bit;
        // that is arithmetic, not layout.
        let was = rect_in_points(r(21.0, 42.0, 126.0, 126.0), 2.625);
        let now = r(8.0 + 1e-12, 16.0, 48.0, 48.0);
        assert_eq!(bounds_moved(was, now, 0.0), None);
    }
}
