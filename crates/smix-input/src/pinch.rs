//! A two-finger pinch: where, how far apart the fingers start and end,
//! how long it takes, and an optional drag before they lift.

use serde::{Deserialize, Serialize};

/// The span the larger end of a pinch given as scales is mapped to, as a
/// share of the screen's shorter side. Large enough that 1→3 starts with
/// the fingers clearly apart (0.2), small enough that the far end leaves
/// room to drag afterwards.
pub const SCALE_SPAN: f64 = 0.6;

/// Default length of the pinch itself, in milliseconds.
pub const DEFAULT_DURATION_MS: u64 = 500;

/// Default length of the drag after the pinch, in milliseconds.
pub const DEFAULT_PAN_MS: u64 = 300;

/// The lengths both runners accept for the pinch, in milliseconds.
pub const DURATION_RANGE: std::ops::RangeInclusive<u64> = 50..=10_000;

/// Two fingers on a horizontal line through `centre`, moved from
/// `from_span` apart to `to_span` apart. Spans are shares of the screen's
/// shorter side, so a ratio means the same zoom in either orientation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pinch {
    /// Midpoint of the two fingers, as shares of the screen (0..1).
    pub centre: (f64, f64),
    /// Distance between the fingers at the start.
    pub from_span: f64,
    /// Distance between the fingers at the end of the pinch.
    pub to_span: f64,
    /// Length of the pinch.
    pub duration_ms: u64,
    /// A drag after the pinch, with the fingers still down.
    pub pan: Option<Pan>,
}

/// Both fingers moving together after the pinch, before they lift.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pan {
    /// Horizontal distance, as a share of the screen's width (-1..1).
    pub dx: f64,
    /// Vertical distance, as a share of the screen's height (-1..1).
    pub dy: f64,
    /// Length of the drag, in milliseconds.
    pub ms: u64,
}

/// How far apart the fingers are, given one of two ways.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PinchSpans {
    /// Zoom factors at the start and end, e.g. 1 → 3. Converted with
    /// [`SCALE_SPAN`].
    Scales {
        /// Scale at the start.
        from: f64,
        /// Scale at the end.
        to: f64,
    },
    /// Finger distances as shares of the screen's shorter side.
    Spans {
        /// Distance at the start.
        from: f64,
        /// Distance at the end.
        to: f64,
    },
}

impl PinchSpans {
    /// The finger distances, or why these numbers are not a pinch.
    pub fn resolve(self) -> Result<(f64, f64), String> {
        match self {
            PinchSpans::Scales { from, to } => {
                if !(from > 0.0 && to > 0.0) {
                    return Err(format!("scales must be above 0, got {from} → {to}"));
                }
                if from == to {
                    return Err(format!("from and to are both {from}; that is no pinch"));
                }
                let big = from.max(to);
                Ok((SCALE_SPAN * from / big, SCALE_SPAN * to / big))
            }
            PinchSpans::Spans { from, to } => {
                for (name, v) in [("fromSpan", from), ("toSpan", to)] {
                    if !(v > 0.0 && v <= 1.0) {
                        return Err(format!(
                            "{name} is a share of the screen's shorter side, above 0 and at most 1; got {v}"
                        ));
                    }
                }
                if from == to {
                    return Err(format!(
                        "fromSpan and toSpan are both {from}; that is no pinch"
                    ));
                }
                Ok((from, to))
            }
        }
    }
}

impl Pinch {
    /// Why this pinch cannot be sent, checked before it is: the same
    /// ranges the runners refuse, said here in the caller's terms.
    pub fn check(&self) -> Result<(), String> {
        let (x, y) = self.centre;
        if !((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)) {
            return Err(format!(
                "centre ({x}, {y}) is off the screen; both must be within 0..1"
            ));
        }
        PinchSpans::Spans {
            from: self.from_span,
            to: self.to_span,
        }
        .resolve()?;
        if !DURATION_RANGE.contains(&self.duration_ms) {
            return Err(format!(
                "duration {} ms is outside {}..={} ms",
                self.duration_ms,
                DURATION_RANGE.start(),
                DURATION_RANGE.end()
            ));
        }
        if let Some(p) = self.pan {
            if !((-1.0..=1.0).contains(&p.dx) && (-1.0..=1.0).contains(&p.dy)) {
                return Err(format!(
                    "pan ({}, {}) must be shares of the screen within -1..1",
                    p.dx, p.dy
                ));
            }
            if !(1..=10_000).contains(&p.ms) {
                return Err(format!("pan duration {} ms is outside 1..=10000 ms", p.ms));
            }
        }
        Ok(())
    }

    /// How long the whole touch lasts, pinch and drag.
    pub fn total_ms(&self) -> u64 {
        self.duration_ms + self.pan.map_or(0, |p| p.ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_map_the_larger_end_to_the_scale_span() {
        let (a, b) = PinchSpans::Scales { from: 1.0, to: 3.0 }.resolve().unwrap();
        assert!((a - 0.2).abs() < 1e-9 && (b - 0.6).abs() < 1e-9, "{a} {b}");
        let (a, b) = PinchSpans::Scales { from: 2.0, to: 1.0 }.resolve().unwrap();
        assert!((a - 0.6).abs() < 1e-9 && (b - 0.3).abs() < 1e-9, "{a} {b}");
    }

    #[test]
    fn equal_ends_are_no_pinch() {
        let e = PinchSpans::Scales { from: 2.0, to: 2.0 }
            .resolve()
            .unwrap_err();
        assert!(e.contains("no pinch"), "{e}");
        let e = PinchSpans::Spans { from: 0.3, to: 0.3 }
            .resolve()
            .unwrap_err();
        assert!(e.contains("no pinch"), "{e}");
    }

    #[test]
    fn spans_outside_the_short_side_are_refused() {
        let e = PinchSpans::Spans { from: 0.2, to: 1.5 }
            .resolve()
            .unwrap_err();
        assert!(e.starts_with("toSpan"), "{e}");
        assert!(PinchSpans::Scales { from: 0.0, to: 3.0 }.resolve().is_err());
    }

    fn zoom() -> Pinch {
        Pinch {
            centre: (0.5, 0.5),
            from_span: 0.2,
            to_span: 0.6,
            duration_ms: 500,
            pan: None,
        }
    }

    #[test]
    fn check_accepts_an_ordinary_pinch_and_counts_the_drag() {
        assert_eq!(zoom().check(), Ok(()));
        let p = Pinch {
            pan: Some(Pan {
                dx: 0.0,
                dy: -0.2,
                ms: 300,
            }),
            ..zoom()
        };
        assert_eq!(p.check(), Ok(()));
        assert_eq!(p.total_ms(), 800);
        assert_eq!(zoom().total_ms(), 500);
    }

    #[test]
    fn check_names_what_is_out_of_range() {
        let e = Pinch {
            centre: (1.2, 0.5),
            ..zoom()
        }
        .check()
        .unwrap_err();
        assert!(e.contains("off the screen"), "{e}");
        let e = Pinch {
            duration_ms: 20,
            ..zoom()
        }
        .check()
        .unwrap_err();
        assert!(e.contains("20 ms"), "{e}");
        let e = Pinch {
            pan: Some(Pan {
                dx: 0.0,
                dy: 0.1,
                ms: 0,
            }),
            ..zoom()
        }
        .check()
        .unwrap_err();
        assert!(e.contains("pan duration"), "{e}");
    }
}
