//! A mouse pointer: resting over a point, or taken off the app.

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
}

impl Mouse {
    /// The wire name of what this does.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Mouse::Hover { .. } => "hover",
            Mouse::Away => "away",
        }
    }

    /// The point it acts at, when it has one.
    #[must_use]
    pub fn at(&self) -> Option<(f64, f64)> {
        match *self {
            Mouse::Hover { at } => Some(at),
            Mouse::Away => None,
        }
    }

    /// Why this cannot be sent, in the caller's terms.
    ///
    /// # Errors
    ///
    /// A point off the screen.
    pub fn check(&self) -> Result<(), String> {
        match self {
            Mouse::Hover { at: (x, y) } => {
                if (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y) {
                    Ok(())
                } else {
                    Err(format!(
                        "hover at ({x}, {y}) is off the screen; both must be shares within 0..1"
                    ))
                }
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
}
