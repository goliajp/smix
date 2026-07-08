//! `RecordSession` + `RecordingApp` — host-side capture wrappers.
//!
//! Ported from now-retired TS source (was `legacy/src/recorder/session.ts`, 117 lines, retired in v3.22).

use smix_input::{KeyName, SwipeDirection};
use smix_recorder_ir::IRAction;
use smix_sdk::{App, ExpectationFailure, Selector};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Mutable buffer of `IRAction` captured during a record session.
///
/// `Arc<Mutex<...>>` shared interior so [`RecordingApp`] can hold a
/// non-mut reference to the App and still push into the session buffer.
#[derive(Clone, Default)]
pub struct RecordSession {
    actions: Arc<Mutex<Vec<IRAction>>>,
}

impl RecordSession {
    /// New empty session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an action into the buffer.
    pub fn record(&self, action: IRAction) {
        self.actions
            .lock()
            .expect("RecordSession lock poisoned")
            .push(action);
    }

    /// Defensive snapshot of the buffer (does not mutate).
    pub fn snapshot(&self) -> Vec<IRAction> {
        self.actions
            .lock()
            .expect("RecordSession lock poisoned")
            .clone()
    }

    /// Drain + return all actions, leaving the buffer empty.
    pub fn stop(&self) -> Vec<IRAction> {
        let mut g = self.actions.lock().expect("RecordSession lock poisoned");
        std::mem::take(&mut *g)
    }

    /// Number of actions captured so far.
    pub fn len(&self) -> usize {
        self.actions
            .lock()
            .expect("RecordSession lock poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Wallclock milliseconds since Unix epoch — `Date.now()` analogue used
/// by the TS recorder.
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// Recording wrapper around [`smix_sdk::App`]. Same recorded-subset
/// surface as `App`; falls through everything else (lifecycle, queries,
/// assertions) to the underlying App. Implemented as an explicit struct
/// rather than a generic proxy so the type surface stays explicit and
/// the recorded subset is auditable.
///
/// Mirrors TS `RecordingApp`:
/// - `launch` / `terminate` pass through but are NOT recorded (these
///   typically run in `beforeAll` fixtures, not inline in tests)
/// - `tap` / `fill` / `clear` / `press_key` / `swipe_once` / `go_back` /
///   `wait_for` / `hide_keyboard` are recorded
pub struct RecordingApp<'a> {
    app: &'a App,
    session: RecordSession,
}

impl<'a> RecordingApp<'a> {
    /// Wrap an existing App with a session buffer.
    pub fn new(app: &'a App, session: RecordSession) -> Self {
        Self { app, session }
    }

    /// Access the underlying session (e.g. for `.snapshot()` mid-record).
    pub fn session(&self) -> &RecordSession {
        &self.session
    }

    // ---- lifecycle pass-throughs (NOT recorded) -----------------------

    pub async fn launch(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.app.launch(bundle_id).await
    }

    pub async fn terminate(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.app.terminate(bundle_id).await
    }

    // ---- recorded surface --------------------------------------------

    pub async fn tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::Tap {
            selector: selector.clone(),
            timestamp_ms: now_ms(),
        });
        self.app.tap(selector).await
    }

    pub async fn fill(&self, selector: &Selector, text: &str) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::Fill {
            selector: selector.clone(),
            text: text.to_string(),
            timestamp_ms: now_ms(),
        });
        self.app.fill(selector, text).await
    }

    pub async fn clear(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::Clear {
            selector: selector.clone(),
            timestamp_ms: now_ms(),
        });
        self.app.clear(selector).await
    }

    pub async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::PressKey {
            key,
            timestamp_ms: now_ms(),
        });
        self.app.press_key(key).await
    }

    pub async fn swipe_once(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::Swipe {
            direction,
            from: None,
            timestamp_ms: now_ms(),
        });
        self.app.swipe_once(direction).await
    }

    pub async fn swipe_from(
        &self,
        direction: SwipeDirection,
        from: &Selector,
    ) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::Swipe {
            direction,
            from: Some(from.clone()),
            timestamp_ms: now_ms(),
        });
        self.app.swipe_once(direction).await
    }

    pub async fn go_back(&self) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::GoBack {
            timestamp_ms: now_ms(),
        });
        self.app.go_back().await
    }

    pub async fn wait_for(
        &self,
        selector: &Selector,
        timeout: std::time::Duration,
    ) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::WaitFor {
            selector: selector.clone(),
            timestamp_ms: now_ms(),
        });
        self.app.wait_for(selector, timeout).await.map(|_| ())
    }

    pub async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        self.session.record(IRAction::HideKeyboard {
            timestamp_ms: now_ms(),
        });
        self.app.hide_keyboard().await
    }
}
