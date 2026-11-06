//! One finger pressed and held, moved to another point, held there, lifted.
//!
//! What a home screen, a launcher or a reorderable list asks for: the
//! press has to last long enough for the item to come loose before the
//! finger moves, and the finger has to rest over the drop target long
//! enough for it to notice before it lets go. A swipe does neither.

use serde::{Deserialize, Serialize};

/// Default time the finger stays still at the start before it moves, in
/// milliseconds: past Android's long-press threshold (400 ms) and short
/// of iOS's context menu. On an iOS home screen a hold of 500 ms then a
/// move carried the icon to the drop point; a hold of 700 ms opened the
/// icon's menu first, and the move chose the menu item it ended over.
pub const DEFAULT_HOLD_MS: u64 = 500;

/// Default length of the move itself, in milliseconds.
pub const DEFAULT_DURATION_MS: u64 = 800;

/// Default time the finger rests at the end before lifting, in
/// milliseconds: a drop target highlights on hover, and lifting the
/// moment the finger arrives can land before it has.
pub const DEFAULT_RELEASE_MS: u64 = 300;

/// The holds both runners accept, in milliseconds.
pub const HOLD_RANGE: std::ops::RangeInclusive<u64> = 0..=10_000;

/// The move lengths both runners accept, in milliseconds.
pub const DURATION_RANGE: std::ops::RangeInclusive<u64> = 50..=10_000;

/// The rests at the end both runners accept, in milliseconds.
pub const RELEASE_RANGE: std::ops::RangeInclusive<u64> = 0..=5_000;

/// A press at `from`, held for `hold_ms`, moved to `to` over
/// `duration_ms`, held there for `release_ms`, then lifted. Points are
/// shares of the screen (0..1).
///
/// Built with [`Drag::new`] and the `with_*` methods: the struct is
/// `#[non_exhaustive]`, so a field added later does not break a caller.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Drag {
    /// Where the finger goes down.
    pub from: (f64, f64),
    /// Where it lifts.
    pub to: (f64, f64),
    /// How long it stays still at `from` before moving.
    pub hold_ms: u64,
    /// How long the move from `from` to `to` takes.
    pub duration_ms: u64,
    /// How long it stays still at `to` before lifting.
    pub release_ms: u64,
}

impl Drag {
    /// A drag from `from` to `to` with the default timings.
    pub fn new(from: (f64, f64), to: (f64, f64)) -> Drag {
        Drag {
            from,
            to,
            hold_ms: DEFAULT_HOLD_MS,
            duration_ms: DEFAULT_DURATION_MS,
            release_ms: DEFAULT_RELEASE_MS,
        }
    }

    /// The same drag, held `ms` at the start.
    pub fn with_hold_ms(self, ms: u64) -> Drag {
        Drag {
            hold_ms: ms,
            ..self
        }
    }

    /// The same drag, moving for `ms`.
    pub fn with_duration_ms(self, ms: u64) -> Drag {
        Drag {
            duration_ms: ms,
            ..self
        }
    }

    /// The same drag, resting `ms` at the end.
    pub fn with_release_ms(self, ms: u64) -> Drag {
        Drag {
            release_ms: ms,
            ..self
        }
    }

    /// The whole gesture, from touch to lift, in milliseconds.
    pub fn total_ms(&self) -> u64 {
        self.hold_ms + self.duration_ms + self.release_ms
    }

    /// Whether both runners can perform this as written; the reason when
    /// not, in the caller's terms.
    pub fn check(&self) -> Result<(), String> {
        for (name, (x, y)) in [("from", self.from), ("to", self.to)] {
            if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                return Err(format!(
                    "{name} ({x}, {y}) is off the screen: both coordinates are shares \
                     of it, 0..1"
                ));
            }
        }
        for (name, ms, range) in [
            ("hold", self.hold_ms, HOLD_RANGE),
            ("duration", self.duration_ms, DURATION_RANGE),
            ("release", self.release_ms, RELEASE_RANGE),
        ] {
            if !range.contains(&ms) {
                return Err(format!(
                    "{name} {ms} ms is outside {}..={} ms",
                    range.start(),
                    range.end()
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_hold_through_a_long_press_and_rest_before_lifting() {
        let d = Drag::new((0.2, 0.3), (0.8, 0.9));
        assert_eq!((d.hold_ms, d.duration_ms, d.release_ms), (500, 800, 300));
        assert_eq!(d.total_ms(), 1600);
        assert_eq!(d.check(), Ok(()));
    }

    #[test]
    fn a_point_off_the_screen_is_refused_by_name() {
        let e = Drag::new((0.2, 1.2), (0.5, 0.5)).check().unwrap_err();
        assert!(e.starts_with("from (0.2, 1.2)"), "{e}");
        let e = Drag::new((0.2, 0.2), (-0.1, 0.5)).check().unwrap_err();
        assert!(e.starts_with("to (-0.1, 0.5)"), "{e}");
    }

    #[test]
    fn each_timing_outside_what_the_runners_take_is_refused_by_name() {
        let d = Drag::new((0.2, 0.2), (0.5, 0.5));
        assert!(
            d.with_hold_ms(10_001)
                .check()
                .unwrap_err()
                .starts_with("hold")
        );
        assert!(
            d.with_duration_ms(49)
                .check()
                .unwrap_err()
                .starts_with("duration")
        );
        assert!(
            d.with_release_ms(5_001)
                .check()
                .unwrap_err()
                .starts_with("release")
        );
        assert_eq!(d.with_hold_ms(0).with_release_ms(0).check(), Ok(()));
    }

    #[test]
    fn a_saved_drag_spells_its_fields_in_camel_case() {
        let v = serde_json::to_value(Drag::new((0.1, 0.2), (0.3, 0.4))).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["durationMs", "from", "holdMs", "releaseMs", "to"]);
    }
}
