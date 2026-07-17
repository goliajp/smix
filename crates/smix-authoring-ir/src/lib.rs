#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-authoring-ir — IRAction (intermediate representation) +
//! RecorderError + sort helper (stone, cold path).
//!
//! Each [`IRAction`] is one user-visible side-effecting step in a
//! recorded session; the IR feeds the generator-smix-ts +
//! generator-maestro-yaml dual output.
//!
//! Scope note: IR is captured from the **smix API channel** (host-side
//! instrumentation in `RecordSession`), not from a sim-side AX
//! notification swizzle. The swizzle path cannot surface user-tap events
//! originating outside the smix API channel. "User taps sim screen
//! manually with smix watching" is a separate architecture — see
//! docs/roadmap.md.

#![doc(html_root_url = "https://docs.smix.dev/smix-authoring-ir")]

use serde::{Deserialize, Serialize};
use smix_input::{KeyName, SwipeDirection};
use smix_selector::Selector;
use std::fmt;

/// One user-visible side-effecting step in a recorded session.
///
/// Wire serializes as an internally-tagged enum keyed by `kind`: serde
/// `tag = "kind", rename_all = "camelCase"` emits
/// `{ kind: 'tap', selector: ..., timestampMs: ... }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IRAction {
    /// Tap an element matched by `selector`.
    Tap {
        /// Selector picking the tap target.
        selector: Selector,
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
    /// Tap an element and type `text` into it.
    Fill {
        /// Selector picking the input field.
        selector: Selector,
        /// Text to type after the tap.
        text: String,
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
    /// Tap an element and clear its value.
    Clear {
        /// Selector picking the input to clear.
        selector: Selector,
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
    /// Press a single keyboard key.
    PressKey {
        /// Key name (`Return` / `Tab` / arrows / etc.).
        key: KeyName,
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
    /// Swipe in a direction, optionally anchored to an element.
    Swipe {
        /// Swipe direction.
        direction: SwipeDirection,
        /// Optional anchor element (swipe starts from its centroid).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Selector>,
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
    /// Navigate back (system back gesture / button).
    GoBack {
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
    /// Block playback until an element becomes visible.
    WaitFor {
        /// Selector to wait for.
        selector: Selector,
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
    /// Dismiss the on-screen keyboard.
    HideKeyboard {
        /// Capture-time timestamp in milliseconds.
        #[serde(rename = "timestampMs")]
        timestamp_ms: f64,
    },
}

impl IRAction {
    /// Timestamp in milliseconds (Unix epoch or session-relative, depending
    /// on capture site). Stable accessor across variants.
    #[must_use]
    pub fn timestamp_ms(&self) -> f64 {
        match self {
            IRAction::Tap { timestamp_ms, .. }
            | IRAction::Fill { timestamp_ms, .. }
            | IRAction::Clear { timestamp_ms, .. }
            | IRAction::PressKey { timestamp_ms, .. }
            | IRAction::Swipe { timestamp_ms, .. }
            | IRAction::GoBack { timestamp_ms }
            | IRAction::WaitFor { timestamp_ms, .. }
            | IRAction::HideKeyboard { timestamp_ms } => *timestamp_ms,
        }
    }

    /// Wire `kind` string for log / generator / debug rendering. Mirrors
    /// the camelCase serde tag.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            IRAction::Tap { .. } => "tap",
            IRAction::Fill { .. } => "fill",
            IRAction::Clear { .. } => "clear",
            IRAction::PressKey { .. } => "pressKey",
            IRAction::Swipe { .. } => "swipe",
            IRAction::GoBack { .. } => "goBack",
            IRAction::WaitFor { .. } => "waitFor",
            IRAction::HideKeyboard { .. } => "hideKeyboard",
        }
    }
}

// -------------------- RecorderError --------------------------------------

/// Reasons a recorder session might fail. The `cleanup-*` variants
/// surface claude-CLI AI-cleanup failures distinctly from failures of
/// the recording itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecorderErrorReason {
    /// Session had zero actions when generation was requested.
    EmptySession,
    /// One action in the session was structurally invalid.
    MalformedAction,
    /// Claude CLI cleanup subprocess exited non-zero.
    CleanupFailed,
    /// Claude CLI cleanup returned an empty body.
    CleanupEmptyOutput,
    /// Claude CLI cleanup returned a body that failed validation.
    CleanupInvalidOutput,
}

impl RecorderErrorReason {
    /// kebab-case wire string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RecorderErrorReason::EmptySession => "empty-session",
            RecorderErrorReason::MalformedAction => "malformed-action",
            RecorderErrorReason::CleanupFailed => "cleanup-failed",
            RecorderErrorReason::CleanupEmptyOutput => "cleanup-empty-output",
            RecorderErrorReason::CleanupInvalidOutput => "cleanup-invalid-output",
        }
    }
}

impl fmt::Display for RecorderErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Recorder-side failure, distinct from SDK-side ExpectationFailure.
/// CLI / SDK consumers may surface this directly or wrap with context.
#[derive(Clone, Debug)]
pub struct RecorderError {
    /// Failure-reason discriminator.
    pub reason: RecorderErrorReason,
    /// Free-form human-readable detail.
    pub message: String,
}

impl RecorderError {
    /// Construct a [`RecorderError`] with reason + message.
    #[must_use]
    pub fn new<S: Into<String>>(reason: RecorderErrorReason, message: S) -> Self {
        RecorderError {
            reason,
            message: message.into(),
        }
    }
}

impl fmt::Display for RecorderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RecorderError[{}]: {}", self.reason, self.message)
    }
}

impl std::error::Error for RecorderError {}

// -------------------- sort helper ----------------------------------------

/// Sort actions by timestamp ascending. Idempotent. Used when multiple
/// recorders feed the same session (e.g. parallel cells, future feature)
/// to merge chronologically. Mirrors TS `sortByTimestamp` 1:1.
#[must_use]
pub fn sort_by_timestamp(actions: &[IRAction]) -> Vec<IRAction> {
    let mut out = actions.to_vec();
    out.sort_by(|a, b| {
        a.timestamp_ms()
            .partial_cmp(&b.timestamp_ms())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}
