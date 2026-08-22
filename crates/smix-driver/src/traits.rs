//! `Driver` trait: cross-platform sense + act abstraction.
//!
//! Two-trait architecture:
//!
//! - `Driver` (this trait) — sense + act, backed by an HTTP runner
//!   client (XCUITest server for iOS, ktor/UIAutomator2 for Android)
//! - `DeviceControl` (in smix-sdk) — sim/host control, backed by
//!   simctl (iOS) / adb (Android)
//!
//! Methods NOT on this trait (App-layer aggregation or platform-specific
//! getter):
//! - `App::describe()` — calls `Driver::tree()` + `collect_visible_summaries()`
//! - `IosDriver::runner()` — typed HttpRunnerClient getter, iOS-internal
//!   (Android uses a different runner client type)
//! - `IosDriver::dispose()` — runner cleanup, internal
//! - `App::screenshot()` — calls `DeviceControl::screenshot()` (simctl-backed)
//! - `App::mark_fixture_action()` — pure ledger record, no platform dispatch

use async_trait::async_trait;
use smix_error::{FailureCode, FailureInit};
use std::time::Duration;

use smix_error::ExpectationFailure;
use smix_input::{KeyName, SwipeDirection};
use smix_runner_client::{IncludeScope, OcrFrame, SystemPopup, TapMode};
use smix_screen::A11yNode;
use smix_selector::Selector;

use crate::Orientation;

/// Platform identifier for cross-platform dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Ios,
    Android,
}

/// Sense + act capabilities for a single device. Two-trait architecture
/// pair with `smix_sdk::DeviceControl` (sim/host control in smix-sdk).
///
/// Signatures intentionally match iOS impl (`IosDriver` inherent methods)
/// 1:1 so the trait impl is a pure delegation layer.
#[async_trait]
pub trait Driver: Send + Sync {
    /// Platform identifier — `Platform::Ios` or `Platform::Android`.
    fn platform(&self) -> Platform;

    /// iOS-only escape hatch: access `IosDriver`'s inherent
    /// `runner()` for capsule recording (start_record / stop_record) +
    /// `describe()` aggregation. Android impl returns `None`. Default
    /// `None` so non-iOS impls don't need to implement.
    fn as_ios_driver(&self) -> Option<&crate::IosDriver> {
        None
    }

    /// Set the target bundle id sent to the runner as the
    /// `App-Bundle-Id` header on every request. iOS rebinds
    /// `XCUIApplication(bundleIdentifier:)` per request with it;
    /// Android spells `<pkg>:id/<tag>` resource ids with it. Both
    /// override; the default no-op is for impls that talk to neither.
    fn set_target_bundle_id(&mut self, _bundle: &str) {}

    /// Enable `App-Activate: true` header on every request so the iOS
    /// runner calls `.activate()` on the resolved target before
    /// operating.
    ///
    /// iOS-only by design, not by omission: Android foregrounds with an
    /// `am start` shell command, which is a once-per-session action
    /// rather than something to repeat on every request.
    fn set_auto_activate(&mut self, _activate: bool) {}

    /// Force key-event dispatch for text input, bypassing a11y-focus
    /// resolution. Wires as the `Input-Dispatch-Mode: key-events`
    /// header. Default no-op.
    fn set_force_key_events(&mut self, _force: bool) {}

    /// Attach a `Session-Id` header to every subsequent
    /// request. Set to `Some(id)` after `POST /session/open`; set to
    /// `None` to revert to the legacy per-request rebind path (which
    /// is rate-limited to at most one `.activate()` per 5 s per
    /// bundle-id). Default no-op — only impls backed by
    /// an HTTP runner override.
    fn set_session_id(&mut self, _id: Option<String>) {}

    // === Sense (9) ============================================================

    /// Fetch full a11y tree (`GET /tree`).
    async fn tree(&self, include: Option<IncludeScope>) -> Result<A11yNode, ExpectationFailure>;

    /// Boolean existence quick-probe.
    async fn find(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<bool, ExpectationFailure>;

    /// Resolve selector → single matching node.
    async fn find_one(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Option<A11yNode>, ExpectationFailure>;

    /// Resolve selector → all matching nodes.
    async fn find_all(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Vec<A11yNode>, ExpectationFailure>;

    /// Resolve selector → centroid as viewport-normalized `(nx, ny)`.
    async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure>;

    /// Apple Vision / ML Kit OCR find text in current screenshot.
    /// `recognition_level` is `"accurate"` or `"fast"`.
    async fn find_text_by_ocr(
        &self,
        text: &str,
        locales: &[String],
        recognition_level: &str,
    ) -> Result<Option<OcrFrame>, ExpectationFailure>;

    /// List visible system-level popups (alerts, permission dialogs).
    async fn system_popups(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<Vec<SystemPopup>, ExpectationFailure>;

    /// Tap a button on a system popup by id.
    async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, ExpectationFailure>;

    /// Wait until selector resolves (returns matched node when ready).
    async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
        include: Option<IncludeScope>,
    ) -> Result<A11yNode, ExpectationFailure>;

    /// Wait until the selector resolves to nothing.
    ///
    /// A default method rather than a required one: it is the same poll
    /// over `find` on either platform, and this capability already
    /// existed twice — once in the SDK's `App::wait_for_not_visible` and
    /// nowhere else, so the CLI had presence and not absence. A third
    /// copy in `act.rs` was the alternative.
    ///
    /// `find` and not `find_one`: the on-screen sense of visible is what
    /// `wait_for`'s mirror has to use, or "gone" would mean "scrolled
    /// out of view but still in the tree" on one side and not the other.
    async fn wait_for_not_visible(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        let start = std::time::Instant::now();
        loop {
            if !self.find(selector, None).await? {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(ExpectationFailure::new(smix_error::FailureInit {
                    code: Some(smix_error::FailureCode::AssertionFailed),
                    message: format!(
                        "wait_for_not_visible: element still visible after {}ms — {}",
                        timeout.as_millis(),
                        smix_selector::describe_selector(selector)
                    ),
                    selector: Some(selector.clone()),
                    ..Default::default()
                }));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    // === Act (17) ============================================================

    /// Tap an element (default mode = element-anchored coord tap).
    /// Tap a selector `times` times, spaced on the event timeline.
    ///
    /// Default is one resolve per touch through [`Self::tap`], which is
    /// what every platform can already do. A runner that can pack the
    /// touches into one synthesise overrides this and gets an interval
    /// the caller states rather than one the round trip decides.
    async fn tap_burst(
        &self,
        selector: &Selector,
        times: u32,
        _interval_ms: Option<u32>,
        _hold_ms: Option<u32>,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        for _ in 0..times.max(1) {
            self.tap(selector, include).await?;
        }
        Ok(())
    }

    /// Tap a selector, and report what the touch landed on.
    ///
    /// A platform that cannot answer says so with
    /// `ActOutcome::unjudged()` rather than with a bare success. "I
    /// could not tell" is a different fact from "it landed", and only
    /// one of them is what a caller reads out of a tap that returned
    /// without error.
    async fn tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<crate::ActOutcome, ExpectationFailure>;

    /// Tap with explicit mode (Path A vs Path B).
    async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: TapMode,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure>;

    /// Tap at viewport-normalized coordinate (escape hatch).
    async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure>;

    /// Double-tap at a viewport-normalized coordinate.
    ///
    /// Defaulted so an implementor outside this workspace keeps
    /// compiling, and the default refuses rather than doing nothing:
    /// a gesture that silently does not happen is the failure mode this
    /// whole line of work is about. Both drivers here override it.
    async fn double_tap_at_norm_coord(&self, _nx: f64, _ny: f64) -> Result<(), ExpectationFailure> {
        Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "double_tap_at_norm_coord: this driver has no coordinate \
                      double-tap. The caller resolved a point and there is no way \
                      to act on it here."
                .into(),
            ..Default::default()
        }))
    }

    /// Long-press at a viewport-normalized coordinate. Defaulted for
    /// the same reason as [`Driver::double_tap_at_norm_coord`].
    async fn long_press_at_norm_coord(
        &self,
        _nx: f64,
        _ny: f64,
        _duration_ms: u64,
    ) -> Result<(), ExpectationFailure> {
        Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "long_press_at_norm_coord: this driver has no coordinate \
                      long press. The caller resolved a point and there is no way \
                      to act on it here."
                .into(),
            ..Default::default()
        }))
    }

    /// Tap by accessibility identifier via the swift `/tap-by-id`
    /// route (IOHID synthesize at element-frame center).
    async fn tap_by_id(&self, id: &str) -> Result<(), ExpectationFailure>;

    /// Double-tap a selector.
    async fn double_tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure>;

    /// Long-press a selector for `duration`.
    ///
    /// Returns when the touch was held, on this host's clock, so a
    /// caller capturing frames alongside can tell whether they fall
    /// inside the press. A platform whose runner cannot report the
    /// bounds returns [`PressTiming::unplaceable`] — which reads as "I
    /// cannot tell", not as a press that happened at time zero.
    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
        include: Option<IncludeScope>,
    ) -> Result<crate::PressTiming, ExpectationFailure>;

    /// Fill text into a focused / matched input.
    ///
    /// `clear_first` empties the field before typing, which is what
    /// "fill" means and what the guides have always described. It used
    /// to append unconditionally: returning to a field and filling it
    /// again produced the two values concatenated, invisible in a
    /// secure field and visible only as a login that fails.
    ///
    /// False is still reachable because appending is what maestro's
    /// `inputText` does, and a ported flow that types twice into one
    /// field means the second call to continue the first.
    async fn fill(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
        clear_first: bool,
    ) -> Result<(), ExpectationFailure>;

    /// Clear text from a focused / matched input.
    async fn clear(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure>;

    /// Press a named key (Return / Tab / Backspace / etc).
    async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure>;

    /// Scroll until a selector is visible.
    async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure>;

    /// One-shot swipe in a direction (no probe loop).
    async fn swipe_once(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure>;

    /// Swipe between two viewport-normalized coords.
    async fn swipe_at_norm_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure>;

    /// Dismiss the keyboard.
    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure>;

    /// Trigger "back" gesture (iOS edge-swipe / Android KEYCODE_BACK).
    async fn back(&self) -> Result<(), ExpectationFailure>;

    /// Rotate the device.
    async fn set_orientation(&self, orientation: Orientation) -> Result<(), ExpectationFailure>;

    /// Bring app to foreground.
    async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure>;

    /// Evaluate JavaScript in a WebView (iOS: fixture bridge :28080;
    /// Android: Chrome DevTools Protocol via adb forward).
    async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure>;
}
