//! A mouse pointer: resting over a point, taken off the app, a secondary
//! click, a wheel turn, or a drag with the primary button.

use serde::{Deserialize, Serialize};

/// What a mouse does. Points are shares of the screen (0..1).
///
/// A desktop window (a Chromebook, Android's desktop mode) styles what the
/// pointer rests on and opens menus on a secondary click; a finger does
/// neither, so these go in as mouse events rather than touches.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[non_exhaustive]
pub enum Mouse {
    /// The pointer comes to rest over `at`: whatever is under it sees the
    /// pointer enter, and whatever it left sees it leave.
    Hover {
        /// Where it rests.
        at: (f64, f64),
    },
    /// The pointer leaves the app, so nothing in it is hovered any more.
    /// Not a move to an empty spot of the app: that spot is still inside
    /// some row or container whose hover would stay on.
    Away,
    /// A secondary-button (right) click at `at`: what opens a context menu.
    SecondaryClick {
        /// Where it clicks.
        at: (f64, f64),
    },
    /// The wheel turned with the pointer over `at`. `notches` is the
    /// vertical scroll axis: positive turns away from the user (up),
    /// negative toward (down). `ctrl` holds Ctrl, which apps read as zoom.
    Wheel {
        /// Where the pointer is.
        at: (f64, f64),
        /// Notches turned, signed.
        notches: f32,
        /// Ctrl held during the turn.
        ctrl: bool,
    },
    /// The primary button pressed at `from`, moved to `to`, released.
    Drag {
        /// Where the button goes down.
        from: (f64, f64),
        /// Where it comes up.
        to: (f64, f64),
    },
    /// The primary button pressed at `from`, moved `by_dp` (x, y) in dp,
    /// released. The device's density turns dp into pixels.
    DragBy {
        /// Where the button goes down.
        from: (f64, f64),
        /// How far it moves, in dp: x right, y down.
        by_dp: (f64, f64),
    },
}

impl Mouse {
    /// The wire name of what this does.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Mouse::Hover { .. } => "hover",
            Mouse::Away => "away",
            Mouse::SecondaryClick { .. } => "secondary",
            Mouse::Wheel { .. } => "wheel",
            Mouse::Drag { .. } | Mouse::DragBy { .. } => "drag",
        }
    }

    /// The point it acts at, when it has one.
    #[must_use]
    pub fn at(&self) -> Option<(f64, f64)> {
        match *self {
            Mouse::Hover { at } | Mouse::SecondaryClick { at } | Mouse::Wheel { at, .. } => {
                Some(at)
            }
            Mouse::Drag { from, .. } | Mouse::DragBy { from, .. } => Some(from),
            Mouse::Away => None,
        }
    }

    /// Why this cannot be sent, in the caller's terms.
    ///
    /// # Errors
    ///
    /// A point off the screen.
    pub fn check(&self) -> Result<(), String> {
        let on_screen = |what: &str, (x, y): (f64, f64)| {
            if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) {
                Ok(())
            } else {
                Err(format!(
                    "{what} at ({x}, {y}) is off the screen; both must be shares within 0..1"
                ))
            }
        };
        match *self {
            Mouse::Hover { at } => on_screen("hover", at),
            Mouse::SecondaryClick { at } => on_screen("right click", at),
            Mouse::Wheel { at, notches, .. } => {
                if notches == 0.0 || !notches.is_finite() {
                    return Err(format!(
                        "a wheel turn of {notches} notches turns nothing; give a non-zero number"
                    ));
                }
                on_screen("wheel", at)
            }
            Mouse::Drag { from, to } => {
                on_screen("drag start", from)?;
                on_screen("drag end", to)
            }
            Mouse::DragBy { from, by_dp } => {
                if by_dp == (0.0, 0.0) {
                    return Err("a drag by 0 dp moves nothing; give a distance".into());
                }
                on_screen("drag start", from)
            }
            Mouse::Away => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_off_the_screen_is_named() {
        assert_eq!(Mouse::Hover { at: (0.5, 0.5) }.check(), Ok(()));
        assert_eq!(Mouse::Hover { at: (0.0, 1.0) }.check(), Ok(()));
        let e = Mouse::Hover { at: (1.2, 0.5) }.check().unwrap_err();
        assert!(e.contains("(1.2, 0.5)"), "{e}");
        assert!(Mouse::Hover { at: (0.5, -0.1) }.check().is_err());
        assert_eq!(Mouse::Away.check(), Ok(()));
    }

    #[test]
    fn a_gesture_that_does_nothing_is_named() {
        let e = Mouse::Wheel {
            at: (0.5, 0.5),
            notches: 0.0,
            ctrl: false,
        }
        .check()
        .unwrap_err();
        assert!(e.contains("turns nothing"), "{e}");
        let e = Mouse::DragBy {
            from: (0.5, 0.5),
            by_dp: (0.0, 0.0),
        }
        .check()
        .unwrap_err();
        assert!(e.contains("moves nothing"), "{e}");
        let e = Mouse::Drag {
            from: (0.5, 0.5),
            to: (0.5, 1.5),
        }
        .check()
        .unwrap_err();
        assert!(e.contains("drag end"), "{e}");
    }
}
