//! What a touch was delivered to: the answer every touch route gives.

use serde::{Deserialize, Serialize};

/// When a held touch was down, from `POST /tap-at-norm-coord` with a
/// hold ([`TapAtCoordResult::press`]).
///
/// Bounds, not instants. The synthesised gesture does not report when
/// the touch went down, so the runner reports what it can measure — the
/// call's own span — reduced to the two bounds that hold whatever went
/// on inside it. A reader who takes `latest_down_offset_ms` for "when
/// it went down" will place frames inside a press they were not inside.
///
/// All fields default to zero, which reads as "no window can be
/// established" rather than as a press at time zero.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PressResult {
    /// Handler entry → the latest instant the touch could have gone down.
    #[serde(default)]
    pub latest_down_offset_ms: u64,
    /// Handler entry → the earliest instant the touch could have lifted.
    #[serde(default)]
    pub earliest_up_offset_ms: u64,
    /// Handler entry → handler return.
    #[serde(default)]
    pub handler_wall_ms: u64,
}

/// One named element containing the tapped point.
///
/// Named only: the same point sits inside dozens of anonymous
/// full-screen layout containers, and the host matches selectors by
/// identifier and label, so an unnamed container is something it can
/// neither act on nor tell apart from the next one.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HitChainEntry {
    /// Accessibility identifier; empty when the element has none.
    #[serde(default)]
    pub identifier: String,
    /// Accessibility label; empty when the element has none.
    #[serde(default)]
    pub label: String,
    /// The element's frame in the app's coordinate space.
    pub frame: smix_screen::Rect,
}

/// `POST /tap-at-norm-coord` response.
///
/// The route used to answer with a bare `{ok}`, which meant "a touch
/// was synthesised at that coordinate" and was read as "the element was
/// tapped". Those are different claims, and a consumer watching taps
/// succeed against a button whose counter never moved found out which
/// one they were getting.
///
/// `chain` is every named element containing the point in the state the
/// touch was delivered to, innermost first — not one element, because
/// the innermost thing at a button's centre is usually the button's own
/// label, and not after the touch, because a tap that opens a screen
/// has the destination under that point by then. See
/// `smix_driver::tap_landed_within` for what the host does with it and,
/// more importantly, for what it still cannot see.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TapAtCoordResult {
    /// Named elements containing the tapped point, innermost first.
    ///
    /// Empty when the runner is older than this field or when the point
    /// landed outside every named element. The host cannot tell those
    /// apart from the wire alone, which is why an empty chain is a
    /// verdict of its own rather than a silent pass.
    #[serde(default)]
    pub chain: Vec<HitChainEntry>,
    /// Whether `chain` lists EVERY element under the point, named or not.
    ///
    /// The iOS runner lists named elements only, so a target absent from
    /// its chain may simply be unnamed; the Android runner lists them all,
    /// so there absence is a miss. Without saying which, the host could
    /// only treat both as the weaker kind — and every Android tap was
    /// recorded as "could not be judged".
    #[serde(default)]
    pub complete: bool,
    /// Handler entry → the latest instant a single held touch could have
    /// gone down. With the two below, present only when the request was
    /// one touch; absent from a runner that does not measure it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_down_offset_ms: Option<u64>,
    /// Handler entry → the earliest instant that touch could have lifted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_up_offset_ms: Option<u64>,
    /// Handler entry → handler return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_wall_ms: Option<u64>,
    /// Which tree `chain` was read from. Absent from a runner that has one
    /// reader (iOS) or predates the field: that reader is the accessibility
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reader: Option<TreeReader>,
    /// Why the tree asked for could not be read, when it could not. The
    /// chain is then empty and not complete: nothing was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reader_error: Option<String>,
    /// The tapped point in the display's pixels, as the runner turned the
    /// normalised coordinate back into one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    /// See `x`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i64>,
}

/// A reader of the screen: the accessibility projection, or an app's own
/// semantics tree read through its probe.
///
/// A tap is aimed from one of them and judged — what the touch was
/// delivered to — from the same one. On a Compose screen coming in, the
/// projection lags the semantics tree by 150-300 ms (measured on the
/// fixture), and a touch aimed from one and judged from the other was
/// called a miss while it focused its field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeReader {
    /// The platform's accessibility tree.
    Accessibility,
    /// The app's semantics tree, through the smix probe it carries.
    Semantics,
}

impl TreeReader {
    /// The reader's name as the wire spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TreeReader::Accessibility => "accessibility",
            TreeReader::Semantics => "semantics",
        }
    }
}

impl TapAtCoordResult {
    /// When the touch was held, if the runner said — all three bounds
    /// or none, since a window with one edge missing places nothing.
    #[must_use]
    pub fn press(&self) -> Option<PressResult> {
        Some(PressResult {
            latest_down_offset_ms: self.latest_down_offset_ms?,
            earliest_up_offset_ms: self.earliest_up_offset_ms?,
            handler_wall_ms: self.handler_wall_ms?,
        })
    }
}
