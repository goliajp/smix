//! SDK-internal ledger of issued actions.
//!
//! Reconciliation logic for detecting outside user interference:
//!   focus-change (1018) events caught by the UITest runner's
//!     EventRecorder ── ground truth
//!     ∖
//!   the SDK-side issued-action ledger (every `app.tap` / `app.fill` /
//!     `app.tap_at_coord` records a timestamp + kind + selector hint)
//!     ── actions the SDK originated
//!   = focus changes caused by outside user interference
//!
//! Capacity is capped at [`LEDGER_CAP`] = 1024; pushing past the cap
//! pops the front (LRU).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// LRU capacity cap. One record per SDK act entry point; 1024 covers a
/// typical capsule session end to end (start_record → many tap/fill →
/// stop_record, spanning a few minutes).
pub const LEDGER_CAP: usize = 1024;

/// Kind of act the SDK originated. `tap_at_coord` carries normalized
/// coordinates (0..1); `Tap` / `Fill` go through the selector path and
/// carry their selector description in [`IssuedAction::target_hint`].
#[derive(Clone, Debug, PartialEq)]
pub enum IssuedKind {
    Tap,
    Fill,
    TapAtCoord {
        nx: f64,
        ny: f64,
    },
    SwipeAtCoord {
        from: (f64, f64),
        to: (f64, f64),
    },
    /// Anchor for the start of capsule recording.
    /// `start_capsule_recording` calls `driver.start_record()`, which
    /// triggers the Swift `EventRecorder.installSwizzle + start`. Any
    /// 1018 focus change caught during the fixture lifecycle settle
    /// window (firstResponder reset after swizzle install, and
    /// similar) is physically caused by the SDK's own capsule start,
    /// so reconcile must attribute it to this act rather than count it
    /// as user interference.
    CapsuleStart,
    /// Anchor for a fixture-side action. A fixture-owned UIKit modal
    /// present (UIActivityViewController /
    /// UIDocumentPickerViewController / SpringBoard permission alert)
    /// fires `kAXFirstResponderChangedNotification` 1018 events, but
    /// the SDK driver never dispatched them — the fixture-side
    /// delegate went through the UIKit native API. A test segment
    /// calls `App::mark_fixture_action(action_id)` before hitting the
    /// fixture trigger button to pin the expected phantom focus change
    /// to the ledger; reconcile attributes any change inside the
    /// window (3000 ms) to this mark instead of counting it as user
    /// interference.
    FixtureAction(String),
}

/// One recorded act originated by the SDK.
#[derive(Clone, Debug, PartialEq)]
pub struct IssuedAction {
    pub kind: IssuedKind,
    /// Unix epoch in milliseconds
    /// (`chrono::Utc::now().timestamp_millis() as f64`).
    pub timestamp_ms: f64,
    /// Selector description — typically `selector.id` / `selector.text`
    /// / `selector.label`. `tap_at_coord` uses `None` (no selector).
    pub target_hint: Option<String>,
}

/// Thread-safe LRU ledger. Clones of `IssuedLedger` share one
/// underlying `VecDeque` via `Arc<Mutex<_>>` — `App` holds one
/// internally and the SDK-side `start_capsule_recording` /
/// `stop_capsule_recording_and_reconcile` reuse it.
#[derive(Clone, Debug, Default)]
pub struct IssuedLedger {
    inner: Arc<Mutex<VecDeque<IssuedAction>>>,
}

impl IssuedLedger {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(LEDGER_CAP))),
        }
    }

    /// Record one act. Pops the oldest entry when over LRU capacity.
    pub fn record(&self, action: IssuedAction) {
        let mut q = self.inner.lock().expect("issued ledger mutex poisoned");
        if q.len() >= LEDGER_CAP {
            q.pop_front();
        }
        q.push_back(action);
    }

    pub fn record_tap(&self, ts_ms: f64, target_hint: Option<String>) {
        self.record(IssuedAction {
            kind: IssuedKind::Tap,
            timestamp_ms: ts_ms,
            target_hint,
        });
    }

    pub fn record_fill(&self, ts_ms: f64, target_hint: Option<String>) {
        self.record(IssuedAction {
            kind: IssuedKind::Fill,
            timestamp_ms: ts_ms,
            target_hint,
        });
    }

    pub fn record_tap_at_coord(&self, ts_ms: f64, nx: f64, ny: f64) {
        self.record(IssuedAction {
            kind: IssuedKind::TapAtCoord { nx, ny },
            timestamp_ms: ts_ms,
            target_hint: None,
        });
    }

    pub fn record_swipe_at_coord(&self, ts_ms: f64, from: (f64, f64), to: (f64, f64)) {
        self.record(IssuedAction {
            kind: IssuedKind::SwipeAtCoord { from, to },
            timestamp_ms: ts_ms,
            target_hint: None,
        });
    }

    pub fn record_capsule_start(&self, ts_ms: f64) {
        self.record(IssuedAction {
            kind: IssuedKind::CapsuleStart,
            timestamp_ms: ts_ms,
            target_hint: None,
        });
    }

    /// Record a fixture-side action anchor. `action_id` is the
    /// human-readable hint surfaced under [`IssuedAction::target_hint`]
    /// for diagnostics; reconcile only matches by timestamp window.
    pub fn record_fixture_action(&self, ts_ms: f64, action_id: String) {
        self.record(IssuedAction {
            kind: IssuedKind::FixtureAction(action_id.clone()),
            timestamp_ms: ts_ms,
            target_hint: Some(action_id),
        });
    }

    /// Snapshot-copy the current ledger (used by `reconcile` calls and
    /// unit-test assertions).
    pub fn get_all(&self) -> Vec<IssuedAction> {
        let q = self.inner.lock().expect("issued ledger mutex poisoned");
        q.iter().cloned().collect()
    }

    pub fn clear(&self) {
        let mut q = self.inner.lock().expect("issued ledger mutex poisoned");
        q.clear();
    }
}
