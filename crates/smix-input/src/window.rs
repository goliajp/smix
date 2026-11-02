//! The size of an app's desktop window.

use serde::{Deserialize, Serialize};

/// A desktop window's width and height in dp, its caption bar included.
///
/// An app in a desktop window (a Chromebook, Android's desktop mode) lays
/// itself out by the window's size, not the display's, so its breakpoints
/// are reached by resizing the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSize {
    /// Width in dp.
    pub width_dp: u32,
    /// Height in dp, the caption bar included.
    pub height_dp: u32,
}

/// Where an app's desktop window sits on its display, in px, its caption
/// bar included, with the display's size as it is turned now: the region
/// of a screenshot that is the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    /// Left edge, px from the display's left.
    pub left: i32,
    /// Top edge (the caption bar's), px from the display's top.
    pub top: i32,
    /// Right edge, px.
    pub right: i32,
    /// Bottom edge, px.
    pub bottom: i32,
    /// The display's width in px.
    pub display_width: u32,
    /// The display's height in px.
    pub display_height: u32,
}

/// Named sizes, one inside each width class adaptive layouts switch at
/// (compact below 600 dp, medium below 840 dp, expanded from 840 dp). The
/// sizes are smix's choice, not the system's: a phone's width, a small
/// tablet's, and a laptop window's.
pub const PRESETS: [(&str, WindowSize); 3] = [
    (
        "compact",
        WindowSize {
            width_dp: 412,
            height_dp: 800,
        },
    ),
    (
        "medium",
        WindowSize {
            width_dp: 700,
            height_dp: 800,
        },
    ),
    (
        "expanded",
        WindowSize {
            width_dp: 1024,
            height_dp: 768,
        },
    ),
];

impl WindowSize {
    /// Why this cannot be asked for, in the caller's terms.
    ///
    /// # Errors
    ///
    /// A side of zero.
    pub fn check(&self) -> Result<(), String> {
        if self.width_dp == 0 || self.height_dp == 0 {
            return Err(format!(
                "a window of {self} has no area; give both sides in dp"
            ));
        }
        Ok(())
    }

    /// Whether the window read back at `got` after this size was asked for.
    ///
    /// # Errors
    ///
    /// The size it read back at, when that is another one.
    pub fn read_back(&self, got: WindowSize) -> Result<(), String> {
        if got == *self {
            return Ok(());
        }
        Err(format!(
            "the window was asked to be {self} and reads back at {got}: the app or the \
             system kept it at another size"
        ))
    }
}

impl std::fmt::Display for WindowSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} dp", self.width_dp, self.height_dp)
    }
}

impl std::str::FromStr for WindowSize {
    type Err = String;

    /// `1024x768`: width by height in dp, or a name from [`PRESETS`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((_, size)) = PRESETS.iter().find(|(name, _)| *name == s) {
            return Ok(*size);
        }
        let bad = || {
            format!(
                "`{s}` is not a window size; write width x height in dp, like 1024x768, or compact, medium or expanded"
            )
        };
        let (w, h) = s.split_once(['x', 'X']).ok_or_else(bad)?;
        let size = WindowSize {
            width_dp: w.trim().parse().map_err(|_| bad())?,
            height_dp: h.trim().parse().map_err(|_| bad())?,
        };
        size.check()?;
        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn size(w: u32, h: u32) -> WindowSize {
        WindowSize {
            width_dp: w,
            height_dp: h,
        }
    }

    #[test]
    fn a_size_is_written_width_by_height() {
        assert_eq!("1024x768".parse(), Ok(size(1024, 768)));
        assert_eq!("839 X 768".parse(), Ok(size(839, 768)));
        let e = "1024".parse::<WindowSize>().unwrap_err();
        assert!(e.contains("like 1024x768"), "{e}");
        let e = "0x768".parse::<WindowSize>().unwrap_err();
        assert!(e.contains("no area"), "{e}");
    }

    #[test]
    fn each_preset_lies_inside_the_width_class_it_is_named_for() {
        assert_eq!("expanded".parse(), Ok(size(1024, 768)));
        for (name, s) in PRESETS {
            let class = match s.width_dp {
                0..600 => "compact",
                600..840 => "medium",
                _ => "expanded",
            };
            assert_eq!(class, name, "{s}");
        }
    }

    #[test]
    fn another_size_read_back_names_both() {
        assert_eq!(size(1024, 768).read_back(size(1024, 768)), Ok(()));
        let e = size(100, 100).read_back(size(220, 220)).unwrap_err();
        assert!(
            e.contains("asked to be 100x100 dp and reads back at 220x220 dp"),
            "{e}"
        );
    }
}
