//! Capsule reconciliation (pure function).
//!
//! Caller: [`crate::App::stop_capsule_recording_and_reconcile`]. After the
//! swift `EventRecorder` returns a `Vec<RecordedEvent>` via `/record/stop`,
//! this function reconciles those events against the SDK's
//! [`crate::IssuedLedger`] issued-action log and produces a
//! [`CapsuleReconciliation`]:
//!
//! - **focus_change** = events whose `raw_code == FOCUS_CHANGE_RAW_CODE`
//!   (1018), i.e. `kAXFirstResponderChangedNotification` ground-truth
//!   focus changes.
//! - **attributed** = focus_change events for which the issued ledger
//!   contains an entry with `(event.ts - issued.ts).abs() <= window_ms`.
//! - **unattributed** = the remainder — i.e. external user input the
//!   SDK did not initiate but that moved focus.
//!
//! `target_hint` is not used for matching (the swift-side element id is
//! hashed and does not round-trip to the SDK selector); only the time
//! window decides same-origin attribution.
//!
//! Default [`DEFAULT_RECONCILE_WINDOW_MS`] is 500 ms.

use smix_runner_client::RecordedEvent;

use crate::issued_ledger::IssuedAction;

/// Raw notification code for `kAXFirstResponderChangedNotification`.
pub const FOCUS_CHANGE_RAW_CODE: i32 = 1018;

/// Default reconciliation window in milliseconds.
///
/// Sized to cover typical SDK fill / tap / scroll round-trip latency to
/// the sim, leaving headroom for keyboard-typing settle time and UI
/// state changes.
pub const DEFAULT_RECONCILE_WINDOW_MS: u64 = 3000;

/// Reconciliation result. `unattributed_events` is the full detail of
/// external user interventions (cloned from the events input), so callers
/// can inspect element ids / timestamps at the assertion layer.
#[derive(Clone, Debug, PartialEq)]
pub struct CapsuleReconciliation {
    pub issued_count: usize,
    pub focus_change_count: usize,
    pub attributed_count: usize,
    pub unattributed_count: usize,
    pub unattributed_events: Vec<RecordedEvent>,
}

/// Pure reconciliation function — no IO, no Mutex, safe on any thread.
pub fn reconcile(
    issued: &[IssuedAction],
    events: &[RecordedEvent],
    window_ms: u64,
) -> CapsuleReconciliation {
    let window = window_ms as f64;
    let focus_changes: Vec<&RecordedEvent> = events
        .iter()
        .filter(|e| e.raw_code == FOCUS_CHANGE_RAW_CODE)
        .collect();
    let mut attributed_count = 0usize;
    let mut unattributed_events: Vec<RecordedEvent> = Vec::new();
    for event in &focus_changes {
        let hit = issued
            .iter()
            .any(|act| (event.timestamp_ms - act.timestamp_ms).abs() <= window);
        if hit {
            attributed_count += 1;
        } else {
            unattributed_events.push((*event).clone());
        }
    }
    CapsuleReconciliation {
        issued_count: issued.len(),
        focus_change_count: focus_changes.len(),
        attributed_count,
        unattributed_count: unattributed_events.len(),
        unattributed_events,
    }
}
