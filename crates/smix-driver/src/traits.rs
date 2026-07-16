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

    /// Set the target bundle id sent to the runner. For iOS this
    /// becomes the `App-Bundle-Id` header on every request so the
    /// runner's `resolveApp()` rebinds
    /// `XCUIApplication(bundleIdentifier:)` per request. Default no-op
    /// — only iOS currently uses this; Android path stays unaffected.
    fn set_target_bundle_id(&mut self, _bundle: &str) {}

    /// Enable `App-Activate: true` header on every request so the iOS
    /// runner calls `.activate()` on the resolved target before
    /// operating. Default no-op.
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

    // === Act (17) ============================================================

    /// Tap an element (default mode = element-anchored coord tap).
    async fn tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure>;

    /// Tap with explicit mode (Path A vs Path B).
    async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: TapMode,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure>;

    /// Tap at viewport-normalized coordinate (escape hatch).
    async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure>;

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
    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure>;

    /// Fill text into a focused / matched input.
    async fn fill(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
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
