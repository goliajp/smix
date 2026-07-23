//! Runtime adapter mapping [`Step`] → smix-sdk [`App`] calls.
//!
//! The parser produces a [`Flow`] of typed [`Step`]s. This module's
//! [`Adapter::run`] walks the step list and dispatches each to an
//! [`AppLike`] surface — which has a blanket impl for [`smix_sdk::App`]
//! so production code uses the real driver, while unit tests substitute
//! a mock that captures the call sequence.
//!
//! # Graceful runtime behaviors
//!
//! - `Step::Swipe` → dispatched via `App::swipe_at_coord`.
//! - `Step::LaunchApp { clear_state | clear_keychain: true }` →
//!   `App::launch_fresh` orchestration. The
//!   `SMIX_APP_PATH_<NORMALIZED_BUNDLE>` env var (e.g.
//!   `com.example.app` → `SMIX_APP_PATH_COM_EXAMPLE_APP`) supplies
//!   the app bundle path for the wipe (Maestro `clearState` semantic
//!   is `uninstall + install`). Missing env → graceful fallback to
//!   the non-clear path (`terminate + launch`) with a warning.
//! - `Step::TapOn { optional: true }` swallowing `ElementNotFound` →
//!   reported as `Skipped { reason }`.
//! - `RunFlow` / `RunFlowConditional` inner
//!   `ParseError::UnsupportedCommand` → warn + skip the entire subflow
//!   with graceful op semantics. Other `ParseError` variants propagate
//!   as [`RunError::Parse`].
//!
//! # AppLike object-safety
//!
//! The trait uses `#[async_trait::async_trait]` for object-safety
//! ergonomics. A blanket impl forwards every method to the
//! corresponding [`smix_sdk::App`] method 1:1.

use crate::{Flow, ParseError, RepeatMode, Step, parse_flow_file};

/// Safety valve for `repeat.while: <expr>` loops. The output store is
/// read-only during a flow, so an expression-driven repeat whose
/// condition never changes would run forever. Hitting this cap
/// surfaces an explicit DriverError naming the condition expression.
const MAX_REPEAT_ITERATIONS: u32 = 1000;
use async_trait::async_trait;
use smix_sdk::{
    App, ExpectationFailure, FailureCode, FailureInit, KeyName, LaunchAppOptions, Pattern,
    PermissionAction, Selector, SimctlPermission, SwipeDirection,
};

/// SwiftUI modal dismiss id classifier. Returns true when the
/// accessibility id encodes a fixture modal dismiss button (sheet /
/// alert / confirmationDialog / fullScreenCover). `Step::TapOn`
/// routes matching selectors through [`App::tap_xcui`] for capability
/// parity with the SDK self-path. The match is a prefix + suffix
/// conjunction — the prefix filters to the modal area and the suffix
/// excludes triggers (`-open-...-btn`) and unrelated elements.
///
/// Scope note: the prefixes below are the smix selftest fixture id
/// namespace (`v2-*`), so this auto-routing can never fire on a
/// third-party consumer's ids — it is back-compat for smix's own e2e
/// yamls only. The generic surface for the same capability is
/// `tapOn: { id: …, dispatch: xcui }`: consumers whose SwiftUI modal
/// dismiss buttons need XCUIElement-anchored dispatch declare it
/// explicitly. New yaml (including smix's own) should use
/// `dispatch: xcui`; this heuristic is not extended.
fn is_swiftui_modal_dismiss_id(id: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "v2-modal-sheet-",
        "v2-modal-alert-",
        "v2-modal-action-",
        "v2-modal-fullscreen-",
    ];
    const SUFFIXES: &[&str] = &[
        "-dismiss-btn",
        "-ok-btn",
        "-cancel-btn",
        "-a-btn", // confirmationDialog Choice A
    ];
    PREFIXES.iter().any(|p| id.starts_with(p)) && SUFFIXES.iter().any(|s| id.ends_with(s))
}

/// SwiftUI tab-bar id classifier. Returns true when the accessibility
/// id matches the fixture tab-bar button namespace (`v2-tab-<area>`,
/// set in `V2RootScreen.swift::tabBar`). `Step::TapOn` routes
/// matching selectors through [`App::tap_xcui`] (swift `/tap-by-id` →
/// XCUIElement.tap with implicit scrollToVisible) so the tab can be
/// tapped even when the tab bar wraps into a ScrollView. maestro
/// CLI's `tapOn id:` is static (no auto-scroll), so this routing keeps
/// the adapter strictly ahead of maestro on the tab-navigation path.
///
/// Deliberately scoped to the SwiftUI tab namespace only:
/// XCUIElement.tap does NOT fire `onPress` on an RN Pressable, and
/// widening this predicate would silently break RN taps.
fn is_swiftui_navigation_or_tab_id(id: &str) -> bool {
    id.starts_with("v2-tab-")
}

#[cfg(test)]
mod modal_dismiss_id_tests {
    use super::{is_swiftui_modal_dismiss_id, is_swiftui_navigation_or_tab_id};

    #[test]
    fn matches_v2_modal_dismiss_ids() {
        assert!(is_swiftui_modal_dismiss_id("v2-modal-sheet-dismiss-btn"));
        assert!(is_swiftui_modal_dismiss_id("v2-modal-alert-ok-btn"));
        assert!(is_swiftui_modal_dismiss_id("v2-modal-action-a-btn"));
        assert!(is_swiftui_modal_dismiss_id(
            "v2-modal-fullscreen-dismiss-btn"
        ));
    }

    #[test]
    fn rejects_non_modal_dismiss_ids() {
        assert!(!is_swiftui_modal_dismiss_id("v2-form-submit-btn"));
        assert!(!is_swiftui_modal_dismiss_id("v2-modal-open-sheet-btn"));
        assert!(!is_swiftui_modal_dismiss_id("v2-modal-sheet-other-label"));
        assert!(!is_swiftui_modal_dismiss_id("some-random-id"));
    }

    #[test]
    fn matches_v2_tab_ids() {
        assert!(is_swiftui_navigation_or_tab_id("v2-tab-home"));
        assert!(is_swiftui_navigation_or_tab_id("v2-tab-deeplink"));
        assert!(is_swiftui_navigation_or_tab_id("v2-tab-modal"));
        assert!(is_swiftui_navigation_or_tab_id("v2-tab-share"));
    }

    #[test]
    fn rejects_non_tab_ids() {
        assert!(!is_swiftui_navigation_or_tab_id("v2-form-submit-btn"));
        assert!(!is_swiftui_navigation_or_tab_id(
            "v2-modal-sheet-dismiss-btn"
        ));
        assert!(!is_swiftui_navigation_or_tab_id("v2-jump-btn"));
        assert!(!is_swiftui_navigation_or_tab_id("v2-list-row-0"));
        assert!(!is_swiftui_navigation_or_tab_id("tab-home"));
    }
}
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Look up `SMIX_APP_PATH_<NORMALIZED_BUNDLE>` for the clear-state
/// wipe wire. Normalization: `.` → `_`, ASCII uppercase. Example:
/// `com.example.app` → `SMIX_APP_PATH_COM_EXAMPLE_APP`. Returns `None`
/// when the env var is unset or empty, signalling graceful fallback to
/// the non-clear path in
/// [`App::launch_fresh`](smix_sdk::App::launch_fresh).
fn launch_fresh_app_path_from_env(bundle_id: &str) -> Option<String> {
    let normalized = bundle_id.replace('.', "_").to_ascii_uppercase();
    let key = format!("SMIX_APP_PATH_{normalized}");
    std::env::var(&key).ok().filter(|s| !s.is_empty())
}

// ----------------------------------------------------------------------
// AppLike trait + blanket impl for smix_sdk::App
// ----------------------------------------------------------------------

/// Subset of [`smix_sdk::App`] needed by [`Adapter::run`]. The blanket
/// impl below forwards 1:1 to the real `App`; mock impls in tests
/// substitute capturing recorders for unit isolation.
#[async_trait]
pub trait AppLike: Send + Sync {
    /// Tap an element matched by selector. Mirrors [`App::tap`].
    async fn tap(&self, selector: &Selector) -> Result<(), ExpectationFailure>;
    /// Route tap via SDK [`App::tap_xcui`] (swift `/tap-by-id` →
    /// XCUIElement-anchored coord.tap). `Step::TapOn` calls this when
    /// the selector is `Selector::Id` and the id matches the SwiftUI
    /// modal dismiss whitelist (see `is_swiftui_modal_dismiss_id`),
    /// keeping capability parity with the SDK self-path.
    async fn tap_xcui(&self, id: &str) -> Result<(), ExpectationFailure>;
    /// Tap with an explicit [`smix_sdk::TapMode`]. Mirrors
    /// [`App::tap_with_mode`]. Used by `tapOn: { dispatch: daemonProxy }`
    /// (XCTRunnerDaemonSession synthesize for stubborn RN Pressables).
    async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: smix_sdk::TapMode,
    ) -> Result<(), ExpectationFailure>;
    /// Delete keys from the target app's persisted
    /// user-defaults store. Mirrors [`App::clear_user_defaults`].
    async fn clear_user_defaults(
        &self,
        bundle_id: &str,
        keys: &[String],
    ) -> Result<(), ExpectationFailure>;
    /// Tap at normalized coordinates. Mirrors [`App::tap_at_coord`].
    async fn tap_at_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure>;
    /// Apple Vision OCR find. Mirrors [`App::find_by_text_ocr`].
    /// Returns the matching text observation's bounding box (UIKit
    /// normalized) or `None`.
    async fn find_by_text_ocr(
        &self,
        text: &str,
        locales: &[String],
    ) -> Result<Option<smix_sdk::OcrFrame>, ExpectationFailure>;

    /// Find a selector's centroid as viewport-normalized
    /// `(nx, ny)`. Mirrors [`App::find_norm_coord`].
    async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure>;

    /// Eval JS via fixture-side WKWebView bridge.
    async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure>;
    /// Type text into the matched field. Mirrors [`App::fill`].
    async fn fill(&self, selector: &Selector, text: &str) -> Result<(), ExpectationFailure>;
    /// Press a hardware / IME key. Mirrors [`App::press_key`].
    async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure>;
    /// Scroll until selector is visible. Mirrors [`App::scroll`].
    async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure>;
    /// Wait for selector to become visible within `timeout`. Mirrors
    /// [`App::wait_for`] but discards the matched node.
    async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure>;
    /// Wait until selector is NOT visible within `timeout`. Mirrors
    /// [`App::wait_for_not_visible`].
    async fn wait_for_not_visible(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure>;
    /// Assert selector is currently visible. Mirrors [`App::assert_visible`].
    async fn assert_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure>;
    /// Boolean visibility probe. Mirrors [`App::find`].
    async fn find(&self, selector: &Selector) -> Result<bool, ExpectationFailure>;
    /// Launch the app by bundle id. Mirrors [`App::launch`].
    async fn launch(&self, bundle_id: &str) -> Result<(), ExpectationFailure>;
    /// Terminate the app by bundle id. Mirrors [`App::terminate`].
    async fn terminate(&self, bundle_id: &str) -> Result<(), ExpectationFailure>;
    /// Atomic fresh launch with state/keychain wipe. Mirrors
    /// [`App::launch_fresh`](smix_sdk::App::launch_fresh). Returns the
    /// warnings the planner emitted (e.g. graceful fallback when
    /// `app_path` is `None` but `clear_state=true`).
    /// `launch_arguments` is process-level argv; an empty `&[]` means
    /// no arguments are passed.
    async fn launch_fresh(
        &self,
        bundle_id: &str,
        clear_state: bool,
        clear_keychain: bool,
        app_path: Option<&str>,
        launch_arguments: &[String],
    ) -> Result<Vec<String>, ExpectationFailure>;
    /// Open a URL / deep link. Mirrors [`App::open_url`].
    async fn open_url(&self, url: &str) -> Result<(), ExpectationFailure>;
    /// Enumerate any system-level (SpringBoard / share-sheet)
    /// alert currently on screen. Mirrors [`App::system_popups`]. Used by
    /// the adapter to auto-dismiss the iOS 17+ "Open in `<App>`?" alert that
    /// follows `openLink` against a custom URL scheme.
    async fn system_popups(&self) -> Result<Vec<smix_sdk::SystemPopup>, ExpectationFailure>;
    /// Invoke a button on a popup surfaced by [`Self::system_popups`].
    /// Mirrors [`App::system_popup_action`].
    async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, ExpectationFailure>;
    /// Bring an already-installed bundle to foreground without
    /// restart. Mirrors [`App::foreground`]. Used by `Step::LaunchApp`
    /// `stopApp=false` arm.
    async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure>;
    /// Typed `launchApp` dispatch entry for the sub-parameter set. Mirrors
    /// [`App::launch_app_with_options`].
    async fn launch_app_with_options(
        &self,
        opts: &LaunchAppOptions,
    ) -> Result<Vec<String>, ExpectationFailure>;
    /// Swipe between two normalized coordinates. Mirrors
    /// [`App::swipe_at_coord`] escape hatch.
    async fn swipe_at_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure>;
    /// Viewport scroll one swipe in the given direction. Mirrors
    /// [`App::scroll_screen`].
    async fn scroll_screen(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure>;
    /// Assert selector is NOT visible. Mirrors [`App::assert_not_visible`].
    async fn assert_not_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure>;
    /// Dismiss the on-screen keyboard. Mirrors [`App::hide_keyboard`].
    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure>;
    /// Capture a screenshot. Mirrors [`App::screenshot`].
    async fn screenshot(&self) -> Result<Vec<u8>, ExpectationFailure>;
    /// Capture a frame preferring the fast raw-BGRA path. Mirrors
    /// [`App::capture_bgra`]. The default impl wraps [`screenshot`](Self::screenshot)
    /// as a PNG frame so mock backends keep working.
    async fn capture_frame(&self) -> Result<smix_sdk::CapturedFrame, ExpectationFailure> {
        self.screenshot().await.map(smix_sdk::CapturedFrame::Png)
    }
    /// Session-scoped in-place data clear. Default
    /// impl errors — the real path is only meaningful when a session
    /// is open and the driver is iOS + host has SimctlClient access.
    /// The `App` impl overrides with the full three-step
    /// orchestration.
    async fn clear_app_data(&self) -> Result<(), ExpectationFailure> {
        Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "AppLike::clear_app_data not implemented on this backend".to_string(),
            ..Default::default()
        }))
    }
    /// Same as [`clear_app_data`], but applies caller-
    /// supplied launchArguments + launchEnvironment on the runner-side
    /// launch step. Default impl delegates to `clear_app_data`
    /// (ignores args) so mock backends stay working.
    async fn clear_app_data_with_launch(
        &self,
        _launch_args: &[String],
        _launch_env: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), ExpectationFailure> {
        self.clear_app_data().await
    }
    /// Snapshot the runner's a11y tree. Mirrors
    /// [`App::tree`]. Used by `write_step_debug` to persist
    /// `step-<N>-fail.tree.json` alongside the fail PNG. Default impl
    /// returns an `ExpectationFailure` so mocks that don't need this
    /// call keep compiling without changes.
    async fn tree(&self) -> Result<smix_sdk::A11yNode, ExpectationFailure> {
        Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "AppLike::tree not implemented on this backend".to_string(),
            ..Default::default()
        }))
    }
    /// Write a literal to device pasteboard.
    /// Mirrors [`App::set_clipboard`].
    async fn set_clipboard(&self, text: &str) -> Result<(), ExpectationFailure>;
    /// Read the device pasteboard. Mirrors [`App::get_clipboard`].
    /// Used by `runFlow.as: <name>` to capture the subflow's pasteboard
    /// state (canonical "what the subflow's `copyTextFrom` left behind")
    /// into the parent flow's outputs map.
    async fn get_clipboard(&self) -> Result<String, ExpectationFailure>;
    /// Paste into focused field. `None` reads clipboard first.
    /// Mirrors [`App::paste_text`].
    async fn paste_text(&self, text: Option<&str>) -> Result<(), ExpectationFailure>;
    /// Copy matched element's text content to device pasteboard.
    /// Mirrors [`App::copy_text_from`].
    async fn copy_text_from(&self, selector: &Selector) -> Result<(), ExpectationFailure>;
    /// Double-tap an element. Mirrors [`App::double_tap`].
    async fn double_tap(&self, selector: &Selector) -> Result<(), ExpectationFailure>;
    /// Tap one element several times, spaced on the event timeline.
    async fn tap_burst(
        &self,
        selector: &Selector,
        times: u32,
        interval_ms: Option<u32>,
        hold_ms: Option<u32>,
    ) -> Result<(), ExpectationFailure>;

    /// Navigation back — iOS navbar back / edge swipe, Android
    /// KEYCODE_BACK. Distinct from any keyboard key.
    async fn go_back(&self) -> Result<(), ExpectationFailure>;
    /// Long-press for `duration`. Mirrors [`App::long_press`].
    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
    ) -> Result<(), ExpectationFailure>;
    /// Long-press while capturing the held state. Mirrors
    /// [`App::long_press_capturing`].
    async fn long_press_capturing(
        &self,
        selector: &Selector,
        duration: Duration,
    ) -> Result<smix_sdk::PressCapture, ExpectationFailure>;
    /// Set sim location. Mirrors [`App::set_location`].
    async fn set_location(&self, latitude: f64, longitude: f64) -> Result<(), ExpectationFailure>;
    /// Interpolate sim location. Mirrors [`App::travel`].
    async fn travel(
        &self,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), ExpectationFailure>;
    /// Batch permission setter. Mirrors [`App::set_permissions`].
    async fn set_permissions(
        &self,
        bundle_id: &str,
        permissions: &[(SimctlPermission, PermissionAction)],
    ) -> Result<(), ExpectationFailure>;
    /// Add media to sim library. Mirrors [`App::add_media`].
    async fn add_media(&self, paths: &[String]) -> Result<(), ExpectationFailure>;
    /// Rotate sim via swift XCUIDevice. Mirrors [`App::set_orientation`].
    async fn set_orientation(
        &self,
        orientation: smix_sdk::MaestroOrientation,
    ) -> Result<(), ExpectationFailure>;
    /// Start sim display recording. Mirrors [`App::start_recording`].
    async fn start_recording(&self, path: &str) -> Result<(), ExpectationFailure>;
    /// Stop active recording. Mirrors [`App::stop_recording`].
    async fn stop_recording(&self) -> Result<(), ExpectationFailure>;
    /// Visual regression assertion. Mirrors
    /// [`App::assert_screenshot`]. Outcome distinguishes auto-record
    /// (first run) from diff-matched (steady state).
    async fn assert_screenshot(
        &self,
        baseline_path: &std::path::Path,
        max_hamming: u32,
    ) -> Result<smix_sdk::AssertScreenshotOutcome, ExpectationFailure>;
}

#[async_trait]
impl AppLike for App {
    async fn tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        // The outcome stops at the AppLike boundary for now. Carrying
        // it into RunStepReport is what puts "what the tap landed on"
        // into run-summary.json, and that is its own step rather than a
        // field threaded through on the way past.
        App::tap(self, selector).await.map(|_| ())
    }
    async fn tap_xcui(&self, id: &str) -> Result<(), ExpectationFailure> {
        App::tap_xcui(self, id).await
    }
    async fn find_by_text_ocr(
        &self,
        text: &str,
        locales: &[String],
    ) -> Result<Option<smix_sdk::OcrFrame>, ExpectationFailure> {
        App::find_by_text_ocr(self, text, locales).await
    }
    async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        App::find_norm_coord(self, selector).await
    }
    async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure> {
        App::webview_eval(self, js).await
    }

    async fn tap_at_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        App::tap_at_coord(self, nx, ny).await
    }
    async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: smix_sdk::TapMode,
    ) -> Result<(), ExpectationFailure> {
        App::tap_with_mode(self, selector, mode).await
    }
    async fn clear_user_defaults(
        &self,
        bundle_id: &str,
        keys: &[String],
    ) -> Result<(), ExpectationFailure> {
        App::clear_user_defaults(self, bundle_id, keys).await
    }
    async fn fill(&self, selector: &Selector, text: &str) -> Result<(), ExpectationFailure> {
        App::fill(self, selector, text).await
    }
    async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure> {
        App::press_key(self, key).await
    }
    async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure> {
        App::scroll(self, selector, direction).await
    }
    async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        App::wait_for(self, selector, timeout).await.map(|_| ())
    }
    async fn wait_for_not_visible(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        App::wait_for_not_visible(self, selector, timeout).await
    }
    async fn assert_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        App::assert_visible(self, selector).await
    }
    async fn find(&self, selector: &Selector) -> Result<bool, ExpectationFailure> {
        App::find(self, selector).await
    }
    async fn launch(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        App::launch(self, bundle_id).await
    }
    async fn terminate(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        App::terminate(self, bundle_id).await
    }
    async fn launch_fresh(
        &self,
        bundle_id: &str,
        clear_state: bool,
        clear_keychain: bool,
        app_path: Option<&str>,
        launch_arguments: &[String],
    ) -> Result<Vec<String>, ExpectationFailure> {
        App::launch_fresh(
            self,
            bundle_id,
            clear_state,
            clear_keychain,
            app_path,
            launch_arguments,
        )
        .await
    }
    async fn open_url(&self, url: &str) -> Result<(), ExpectationFailure> {
        App::open_url(self, url).await
    }
    async fn system_popups(&self) -> Result<Vec<smix_sdk::SystemPopup>, ExpectationFailure> {
        App::system_popups(self).await
    }
    async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        App::system_popup_action(self, popup_id, button_id).await
    }
    async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        App::foreground(self, bundle_id).await
    }
    async fn launch_app_with_options(
        &self,
        opts: &LaunchAppOptions,
    ) -> Result<Vec<String>, ExpectationFailure> {
        App::launch_app_with_options(self, opts).await
    }
    async fn swipe_at_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure> {
        App::swipe_at_coord(self, from, to).await
    }
    async fn scroll_screen(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        App::scroll_screen(self, direction).await
    }
    async fn assert_not_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        App::assert_not_visible(self, selector).await
    }
    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        App::hide_keyboard(self).await
    }
    async fn screenshot(&self) -> Result<Vec<u8>, ExpectationFailure> {
        App::screenshot(self).await
    }
    async fn capture_frame(&self) -> Result<smix_sdk::CapturedFrame, ExpectationFailure> {
        App::capture_bgra(self).await
    }
    async fn tree(&self) -> Result<smix_sdk::A11yNode, ExpectationFailure> {
        App::tree(self).await
    }
    async fn clear_app_data(&self) -> Result<(), ExpectationFailure> {
        App::clear_app_data(self).await.map(|_wall_ms| ())
    }
    async fn clear_app_data_with_launch(
        &self,
        launch_args: &[String],
        launch_env: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), ExpectationFailure> {
        App::clear_app_data_with_launch(self, launch_args, launch_env)
            .await
            .map(|_wall_ms| ())
    }
    async fn set_clipboard(&self, text: &str) -> Result<(), ExpectationFailure> {
        App::set_clipboard(self, text).await
    }
    async fn get_clipboard(&self) -> Result<String, ExpectationFailure> {
        App::get_clipboard(self).await
    }
    async fn paste_text(&self, text: Option<&str>) -> Result<(), ExpectationFailure> {
        App::paste_text(self, text).await
    }
    async fn copy_text_from(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        App::copy_text_from(self, selector).await
    }
    async fn double_tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        App::double_tap(self, selector).await
    }
    async fn tap_burst(
        &self,
        selector: &Selector,
        times: u32,
        interval_ms: Option<u32>,
        hold_ms: Option<u32>,
    ) -> Result<(), ExpectationFailure> {
        App::tap_burst(self, selector, times, interval_ms, hold_ms).await
    }
    async fn go_back(&self) -> Result<(), ExpectationFailure> {
        App::go_back(self).await
    }
    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
    ) -> Result<(), ExpectationFailure> {
        App::long_press(self, selector, duration).await
    }
    async fn long_press_capturing(
        &self,
        selector: &Selector,
        duration: Duration,
    ) -> Result<smix_sdk::PressCapture, ExpectationFailure> {
        App::long_press_capturing(self, selector, duration).await
    }
    async fn set_location(&self, latitude: f64, longitude: f64) -> Result<(), ExpectationFailure> {
        App::set_location(self, latitude, longitude).await
    }
    async fn travel(
        &self,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), ExpectationFailure> {
        App::travel(self, points, speed_mps).await
    }
    async fn set_permissions(
        &self,
        bundle_id: &str,
        permissions: &[(SimctlPermission, PermissionAction)],
    ) -> Result<(), ExpectationFailure> {
        App::set_permissions(self, bundle_id, permissions).await
    }
    async fn add_media(&self, paths: &[String]) -> Result<(), ExpectationFailure> {
        App::add_media(self, paths).await
    }
    async fn set_orientation(
        &self,
        orientation: smix_sdk::MaestroOrientation,
    ) -> Result<(), ExpectationFailure> {
        App::set_orientation(self, orientation).await
    }
    async fn start_recording(&self, path: &str) -> Result<(), ExpectationFailure> {
        App::start_recording(self, path).await
    }
    async fn stop_recording(&self) -> Result<(), ExpectationFailure> {
        App::stop_recording(self).await
    }
    async fn assert_screenshot(
        &self,
        baseline_path: &std::path::Path,
        max_hamming: u32,
    ) -> Result<smix_sdk::AssertScreenshotOutcome, ExpectationFailure> {
        App::assert_screenshot(self, baseline_path, max_hamming).await
    }
}

// ----------------------------------------------------------------------
// Report types
// ----------------------------------------------------------------------

/// Outcome of a single [`Step`] dispatch.
#[derive(Debug)]
pub enum RunStepReport {
    /// Step executed normally.
    Ok,
    /// Step was deliberately skipped (graceful warn path).
    Skipped {
        /// Human-readable explanation surfaced to logs / warnings.
        reason: String,
    },
    /// Step was a `runFlow` / `runFlowConditional` whose inner flow was
    /// expanded and run. The nested report holds the inner step outcomes.
    ExpandedSubflow {
        /// Absolute path of the subflow that was expanded.
        path: PathBuf,
        /// Inner report (boxed to keep enum variant size bounded).
        inner: Box<RunReport>,
    },
}

/// Per-step trace record populated when `--debug-output` is
/// set. Feeds an enriched `run-summary.json` and separately-written
/// `step-<N>-<verb>.tree.json` on failure.
#[derive(Debug, Clone, serde::Serialize)]
#[non_exhaustive]
pub struct StepDebugRecord {
    /// 1-based step index inside the flow.
    pub n: usize,
    /// Verb slug (e.g. `"tapon"`, `"assertvisible"`).
    pub verb: String,
    /// Human-readable summary of the step (e.g. `"tapOn Sign In"`).
    pub summary: String,
    /// Terminal verdict — `"ok"` | `"skipped"` | `"expanded-subflow"` | `"failed"`.
    pub verdict: String,
    /// Wall-clock milliseconds the step took (dispatch + I/O + wait).
    pub wall_ms: u64,
    /// Relative path (under the debug-output dir) of the per-step
    /// outcome JSON. Always present.
    pub json_path: PathBuf,
    /// Relative path of the fail-annotated PNG. Present on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png_path: Option<PathBuf>,
    /// Relative path of the a11y tree JSON snapshot. Present on
    /// failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_path: Option<PathBuf>,
    /// On failure: kind slug from `failure_kind(err)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
    /// On failure: `err.to_string()` for grep-friendly display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
}

/// Aggregate report for one [`Adapter::run`] invocation.
#[derive(Debug, Default)]
pub struct RunReport {
    /// Per-step outcome in execution order.
    pub steps: Vec<RunStepReport>,
    /// Free-form warning messages (graceful skips, G10 clear_state warns,
    /// unsupported inner-subflow command warns).
    pub warnings: Vec<String>,
}

/// Runtime dispatch errors.
// ExpectationFailure is a deliberately rich AI-readable error (≈280 B
// for visibleElements + suggestions + deviceLog). Boxing the Sdk variant
// would buy lint compliance at the cost of an extra allocation per
// graceful path; matches the workspace-level rationale for
// `result_large_err = "allow"` in `Cargo.toml`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// An underlying smix-sdk call failed (and was not graceful-swallowed).
    #[error("sdk: {0}")]
    Sdk(#[from] ExpectationFailure),
    /// A subflow yaml failed to parse (non-graceful variants).
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
    /// `pressKey` got an unknown key name.
    #[error("unknown key: {0}")]
    UnknownKey(String),
    /// `scrollUntilVisible` got an unknown direction.
    #[error("unknown direction: {0}")]
    UnknownDirection(String),
    /// `runFlow` recursion re-entered an in-progress file.
    #[error("runFlow cycle at {0}")]
    RunFlowCycle(PathBuf),
    /// I/O failure resolving a subflow path.
    #[error("io: {0}")]
    Io(String),
}

/// Internal enum for `write_step_debug` describing the terminal
/// outcome of one step. Success outcome carries the `RunStepReport` so
/// debug JSON can distinguish `Ok` vs `Skipped` vs `ExpandedSubflow`;
/// failure carries the `RunError`.
enum StepOutcome<'a> {
    Ok(&'a RunStepReport),
    Failed(&'a RunError),
}

/// Verb name for filename slugification. Kept separate from
/// `entry::summarize_step` (which is human-readable) so the file name
/// only carries the verb noun. Falls back to `step` on any variant we
/// have not listed — safe against future Step additions.
fn summarize_step_verb(step: &Step) -> String {
    match step {
        Step::LaunchApp { .. } => "launchApp",
        Step::StopApp => "stopApp",
        Step::ClearAppData { .. } => "clearAppData",
        Step::ResetAppData { .. } => "resetAppData",
        Step::ClearUserDefaults { .. } => "clearUserDefaults",
        Step::AssertCondition { .. } => "assertCondition",
        Step::ExtractWithAI { .. } => "extractWithAI",
        Step::KillApp { .. } => "killApp",
        Step::TapOn { .. } => "tapOn",
        Step::TapAtPoint { .. } => "tapAtPoint",
        Step::DoubleTapOn { .. } => "doubleTapOn",
        Step::RepeatTap { .. } => "repeatTap",
        Step::LongPressOn { .. } => "longPressOn",
        Step::InputText(_) => "inputText",
        Step::InputTextInto { .. } => "inputText",
        Step::EraseText(_) => "eraseText",
        Step::ClearState { .. } => "clearState",
        Step::ClearKeychain => "clearKeychain",
        Step::HideKeyboard => "hideKeyboard",
        Step::PressKey(_) => "pressKey",
        Step::Back => "back",
        Step::Scroll => "scroll",
        Step::ScrollUntilVisible { .. } => "scrollUntilVisible",
        Step::Swipe { .. } => "swipe",
        Step::AssertVisible { .. } => "assertVisible",
        Step::AssertNotVisible { .. } => "assertNotVisible",
        Step::AssertTrue { .. } => "assertTrue",
        Step::AssertScreenshot { .. } => "assertScreenshot",
        Step::ExpectSignal { .. } => "expectSignal",
        Step::ExpectSignals { .. } => "expectSignals",
        Step::ExpectLogClean => "expectLogClean",
        Step::Fixture { .. } => "fixture",
        Step::WaitForAnimationToEnd { .. } => "waitForAnimationToEnd",
        Step::ExtendedWaitUntil { .. } => "extendedWaitUntil",
        Step::TakeScreenshot { .. } => "takeScreenshot",
        Step::OpenLink(_) => "openLink",
        Step::CopyTextFrom { .. } => "copyTextFrom",
        Step::PasteText { .. } => "pasteText",
        Step::SetClipboard(_) => "setClipboard",
        Step::Travel { .. } => "travel",
        Step::SetLocation { .. } => "setLocation",
        Step::SetPermissions { .. } => "setPermissions",
        Step::SetOrientation { .. } => "setOrientation",
        Step::AddMedia { .. } => "addMedia",
        Step::StartRecording { .. } => "startRecording",
        Step::StopRecording => "stopRecording",
        Step::WebViewEval { .. } => "webViewEval",
        Step::Repeat { .. } => "repeat",
        Step::Retry { .. } => "retry",
        Step::RunFlow(_) => "runFlow",
        Step::RunFlowInline { .. } => "runFlowInline",
        Step::RunFlowConditional { .. } => "runFlowConditional",
        Step::RunScript { .. } => "runScript",
        Step::EvalScript { .. } => "evalScript",
    }
    .to_string()
}

/// `RunError` → short kind tag for debug JSON.
fn failure_kind(err: &RunError) -> String {
    match err {
        RunError::Sdk(f) => format!("{:?}", f.code),
        RunError::Parse(_) => "Parse".into(),
        RunError::UnknownKey(_) => "UnknownKey".into(),
        RunError::UnknownDirection(_) => "UnknownDirection".into(),
        RunError::RunFlowCycle(_) => "RunFlowCycle".into(),
        RunError::Io(_) => "Io".into(),
    }
}

/// Is this a wait/find timeout worth capturing a screenshot for?
/// Currently only `FailureCode::Timeout` triggers the
/// unconditional dump; other failure codes (e.g. `NotVisible`,
/// `ElementNotFound`) already emit rich context and don't benefit from
/// the visual capture.
fn is_timeout_error(err: &RunError) -> bool {
    matches!(err, RunError::Sdk(f) if matches!(f.code, FailureCode::Timeout))
}

/// Merge the `capture_timeout_dump` sink paths into the
/// failure's existing hint. If the capture failed entirely (returned
/// `None`) the failure is returned unchanged.
fn annotate_timeout_error(err: RunError, capture_hint: Option<String>) -> RunError {
    let Some(hint_suffix) = capture_hint else {
        return err;
    };
    match err {
        RunError::Sdk(mut f) => {
            let capture_line = format!("timeout capture: {hint_suffix}");
            f.hint = Some(match f.hint.take() {
                Some(existing) => format!("{existing}\n{capture_line}"),
                None => capture_line,
            });
            RunError::Sdk(f)
        }
        other => other,
    }
}

// ----------------------------------------------------------------------
// Adapter — dispatch state
// ----------------------------------------------------------------------

/// Stateful step dispatcher. Holds a borrow of the [`AppLike`] surface
/// plus the current base directory for resolving relative `runFlow`
/// paths. `last_bundle` is updated by [`Step::LaunchApp`] and consumed
/// by [`Step::StopApp`].
pub struct Adapter<'a, A: AppLike + ?Sized> {
    app: &'a A,
    base_dir: PathBuf,
    last_bundle: Option<String>,
    run_stack: Vec<PathBuf>,
    /// Yaml flow-level `output` store, used by the expression
    /// engine (`${output.x}` template + `assertTrue` evaluation).
    /// Seeded from the flow and then written only by
    /// [`Step::ExtractWithAI`], which is the one verb that reads values
    /// off the screen into it; defaults to an empty map.
    output: std::collections::BTreeMap<String, crate::ExprValue>,
    /// Env store used by the expression engine for bare `${NAME}`
    /// lookup. Populated from CLI `--env KEY=VAL` (repeatable) + the
    /// inherited process env as fallback. Enables yaml with
    /// `${E2E_EMAIL}` / `${IMAP_PASSWORD}` interpolation.
    env: std::collections::BTreeMap<String, String>,
    /// Per-step debug artifact directory. When Some, `run_steps`
    /// writes `<dir>/step-<N>-<verb>.json` after every step (both
    /// success and skipped) and, when a step fails,
    /// `<dir>/step-<N>-fail.png` (screenshot) + a failure record in
    /// the same json.
    debug_output: Option<PathBuf>,
    /// Accumulator for [`StepDebugRecord`] entries as
    /// steps execute. Populated only when `debug_output` is set.
    /// Kept on the adapter (not on `RunReport`) so a failed run still
    /// exposes the partial trace via [`Adapter::debug_records`].
    debug_records: Vec<StepDebugRecord>,
    /// Step index counter, monotonic per top-level `run()` call. Kept
    /// in Adapter (not thread-local, not a param) so recursive
    /// `run_steps_inner` from `repeat` / `retry` / `runFlow`-inline
    /// shares the same counter (inner-block steps still get unique
    /// file names).
    step_index: usize,
    /// Last `-AppleLanguages` value seen in a `launchApp.arguments`
    /// step (e.g. "en" / "ja" / "es"). Used by [`Selector::LocalizedText`]
    /// desugar to pick the right per-locale text. Defaults to "en" when no
    /// launchApp arg has been parsed yet (or arg lacked `-AppleLanguages`).
    last_locale: String,
    /// Attached metro/expo log tail. When Some, the
    /// runtime dispatches [`Step::ExpectSignal`] / [`Step::ExpectSignals`]
    /// / [`Step::ExpectLogClean`] via [`smix_metro_log::MetroLogTail`].
    /// None → those verbs fail immediately with an actionable hint to
    /// configure `.smix/config.yaml` `metroLog` section.
    metro_tail: Option<smix_metro_log::MetroLogTail>,
    /// Step-end `ms_since_start` cursors. Populated at
    /// the end of each step execution; the [`SignalWindow::SinceStep`]
    /// yaml field maps to `Window::SinceMs(step_ms_cursors[n])`. Vec
    /// index N holds ms at end of step N (1-indexed to match run report
    /// step numbering).
    step_ms_cursors: Vec<u64>,
    /// Attached fixture registry. When Some, the
    /// runtime dispatches [`Step::Fixture`] by looking up the yaml
    /// `id:` in this registry. None → the verb fails with an
    /// actionable hint to configure `.smix/config.yaml`.
    fixture_registry: Option<smix_fixture::FixtureRegistry>,
    /// The Android launch activity `resolve_app_into_flow` put on the
    /// flow, seeded in [`Self::run`] alongside `last_bundle`.
    launch_activity: Option<String>,
    /// When true, `write_step_debug`'s fail-PNG output
    /// stays raw (no annotation overlay). Opt-out via `smix run
    /// --no-fail-annotate`. Default false = annotate.
    no_fail_annotate: bool,
    /// How the AI-assertion verbs reach their judge. A seam as much as a
    /// setting: tests point `claude_bin` at a stub so a mock run never
    /// spends a real model call.
    ai_cfg: smix_ai_tier::AiTierConfig,
    /// What counts as a still screen for `waitForAnimationToEnd`. The
    /// defaults are reasoned rather than measured and want tuning against a
    /// real simulator.
    quiescence: smix_sdk::quiescence::QuiescenceParams,
}

impl<'a, A: AppLike + ?Sized> Adapter<'a, A> {
    /// Construct a dispatcher bound to `app` and `base_dir`.
    pub fn new(app: &'a A, base_dir: PathBuf) -> Self {
        Self {
            app,
            base_dir,
            last_bundle: None,
            run_stack: Vec::new(),
            output: std::collections::BTreeMap::new(),
            env: std::collections::BTreeMap::new(),
            debug_output: None,
            debug_records: Vec::new(),
            step_index: 0,
            last_locale: "en".to_string(),
            metro_tail: None,
            step_ms_cursors: vec![0],
            fixture_registry: None,
            launch_activity: None,
            no_fail_annotate: false,
            ai_cfg: smix_ai_tier::AiTierConfig::default(),
            quiescence: smix_sdk::quiescence::QuiescenceParams::default(),
        }
    }

    /// Accumulated per-step trace records populated when
    /// `--debug-output` is set. Available after `run()` returns
    /// regardless of Ok/Err, so a failed run's partial trace is still
    /// consumable by the CLI summary writer.
    pub fn debug_records(&self) -> &[StepDebugRecord] {
        &self.debug_records
    }

    /// Point the AI-assertion verbs at a specific judge.
    #[must_use]
    pub fn with_ai_config(mut self, cfg: smix_ai_tier::AiTierConfig) -> Self {
        self.ai_cfg = cfg;
        self
    }

    /// Override what counts as a still screen for `waitForAnimationToEnd`.
    #[must_use]
    pub fn with_quiescence(mut self, params: smix_sdk::quiescence::QuiescenceParams) -> Self {
        self.quiescence = params;
        self
    }

    /// Opt-out for auto-annotate on --debug-output
    /// fail-PNG. Consumer sets via `smix run --no-fail-annotate`.
    #[must_use]
    pub fn with_no_fail_annotate(mut self, no_annotate: bool) -> Self {
        self.no_fail_annotate = no_annotate;
        self
    }

    /// Attach a [`smix_metro_log::MetroLogTail`] for
    /// signal-await verbs. Idempotent: subsequent calls replace the
    /// tail (subscriber lifecycle is caller's responsibility).
    #[must_use]
    pub fn with_metro_tail(mut self, tail: smix_metro_log::MetroLogTail) -> Self {
        self.metro_tail = Some(tail);
        self
    }

    /// Attach a fixture registry for the
    /// [`Step::Fixture`] verb.
    #[must_use]
    pub fn with_fixture_registry(mut self, reg: smix_fixture::FixtureRegistry) -> Self {
        self.fixture_registry = Some(reg);
        self
    }

    /// Seed the env store used by `${NAME}` env lookup. Builder-style.
    /// The preferred entry point is `run_flow` in `entry.rs`, which
    /// wires `FlowArgs.env_vars` through.
    pub fn with_env(mut self, env: std::collections::BTreeMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Enable per-step debug artifacts under `dir`. `run_steps` will
    /// write `<dir>/step-<N>-<verb>.json` + (on fail)
    /// `<dir>/step-<N>-fail.png`. Builder-style. `dir` is created if it
    /// doesn't exist on first write.
    pub fn with_debug_output(mut self, dir: PathBuf) -> Self {
        self.debug_output = Some(dir);
        self
    }

    /// Extract `(xx)` from `-AppleLanguages "(xx)"` pattern in a
    /// `launchApp.arguments` array. Returns Some("xx") on match; None when
    /// arg pair missing or shape mismatches. Used by [`Adapter::run_step`]
    /// to update `last_locale` for [`Selector::LocalizedText`] desugar.
    fn extract_apple_languages(args: &[String]) -> Option<String> {
        let mut iter = args.iter();
        while let Some(a) = iter.next() {
            if a == "-AppleLanguages"
                && let Some(v) = iter.next()
            {
                // v has shape "(en)" / "(ja)" / etc; strip parens
                let trimmed = v
                    .trim()
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .trim();
                if !trimmed.is_empty() {
                    // BCP-47 codes can be "en" / "en-US"; we use just
                    // the language subtag for table lookup.
                    let lang = trimmed.split(&['-', '_'][..]).next().unwrap_or(trimmed);
                    return Some(lang.to_string());
                }
            }
        }
        None
    }

    /// Seed the yaml flow-level `output` store. Builder-style;
    /// useful for tests that exercise `${output.x}` expansion / assertTrue.
    pub fn with_output(
        mut self,
        output: std::collections::BTreeMap<String, crate::ExprValue>,
    ) -> Self {
        self.output = output;
        self
    }

    /// Expand `${expr}` placeholders inside `src`. Non-`${...}`
    /// segments preserved verbatim. Engine errors surface as
    /// `RunError::Sdk(DriverError)` (AI-readable hint).
    fn expand_template(&self, src: &str) -> Result<String, RunError> {
        let mut out = String::new();
        let bytes = src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
                let mut j = i + 2;
                let mut depth = 1usize;
                while j < bytes.len() && depth > 0 {
                    if bytes[j] == b'{' {
                        depth += 1;
                    } else if bytes[j] == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    j += 1;
                }
                if j >= bytes.len() || depth != 0 {
                    return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: format!("unterminated ${{...}} in template: {src}"),
                        ..Default::default()
                    })));
                }
                let inner = std::str::from_utf8(&bytes[i + 2..j]).unwrap_or("");
                let ctx = crate::ExprContext {
                    output: self.output.clone(),
                    env: self.env.clone(),
                };
                let value = crate::expr_eval(inner, &ctx).map_err(|e| {
                    RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: format!("expression error in template `${{{inner}}}`: {e}"),
                        suggestions: vec![format!("{e}")],
                        ..Default::default()
                    }))
                })?;
                out.push_str(&value.to_template_string());
                i = j + 1;
            } else {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        Ok(out)
    }

    /// Eval a raw expression (no `${}` wrapping). Used by
    /// `assertTrue`. Errors → DriverError.
    fn eval_expr_or_driver(&self, src: &str) -> Result<crate::ExprValue, RunError> {
        let trimmed = src.trim();
        // Tolerate `${...}` wrapping: assertTrue: "${expr}" → strip it.
        let inner = if trimmed.starts_with("${") && trimmed.ends_with('}') {
            &trimmed[2..trimmed.len() - 1]
        } else {
            trimmed
        };
        let ctx = crate::ExprContext {
            output: self.output.clone(),
            env: self.env.clone(),
        };
        crate::expr_eval(inner, &ctx).map_err(|e| {
            RunError::Sdk(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("expression error in `{src}`: {e}"),
                suggestions: vec![format!("{e}")],
                ..Default::default()
            }))
        })
    }

    /// Walk the flow and dispatch each step. Returns the aggregate
    /// report. Per-variant graceful behaviors are documented at the
    /// module level.
    pub async fn run(&mut self, flow: &Flow) -> Result<RunReport, RunError> {
        // Seed last_bundle so a bare `stopApp` at the top of a flow can
        // target the declared appId. Subsequent launchApp overrides it.
        if self.last_bundle.is_none() && !flow.app_id.is_empty() {
            self.last_bundle = Some(flow.app_id.clone());
        }
        if self.launch_activity.is_none() {
            self.launch_activity = flow.launch_activity.clone();
        }
        self.run_steps(&flow.steps).await
    }

    async fn run_steps(&mut self, steps: &[Step]) -> Result<RunReport, RunError> {
        let mut report = RunReport::default();
        for step in steps {
            self.step_index += 1;
            let idx = self.step_index;
            let step_summary = summarize_step_verb(step);
            let step_start = std::time::Instant::now();
            let outcome_result = self.run_step(step, &mut report.warnings).await;
            let wall_ms = step_start.elapsed().as_millis() as u64;
            let debug_record = match &outcome_result {
                Ok(outcome) => {
                    self.write_step_debug(idx, &step_summary, StepOutcome::Ok(outcome), wall_ms)
                        .await
                }
                Err(e) => {
                    self.write_step_debug(idx, &step_summary, StepOutcome::Failed(e), wall_ms)
                        .await
                }
            };
            if let Some(rec) = debug_record {
                self.debug_records.push(rec);
            }
            let outcome = outcome_result?;
            // Surface a Skipped `reason` on stderr as each step
            // completes. The reason otherwise only lands in
            // `--debug-output/step-N.json`, so consumers running under
            // `stdio: inherit` (or without `--debug-output`) never see
            // WHY a conditional runFlow or optional tapOn was skipped.
            // One stderr line keeps it greppable. Non-Skipped outcomes
            // stay quiet.
            if let RunStepReport::Skipped { reason } = &outcome {
                eprintln!("STEP {idx}: {step_summary} → SKIPPED: {reason}");
            }
            report.steps.push(outcome);
        }
        Ok(report)
    }

    /// Write `<debug_output>/step-<N>-<verb>.json` with the step
    /// outcome. On failure additionally emits `step-<N>-fail.png` via
    /// `App::screenshot` AND `step-<N>-fail.tree.json` via `App::tree`
    /// Best-effort: any I/O, screenshot, or tree error
    /// is logged to stderr but does not affect the run result.
    ///
    /// Returns a [`StepDebugRecord`] with the paths + timing +
    /// verdict for aggregation into `run-summary.json`. Returns `None`
    /// when `--debug-output` is not set.
    async fn write_step_debug(
        &self,
        idx: usize,
        step_summary: &str,
        outcome: StepOutcome<'_>,
        wall_ms: u64,
    ) -> Option<StepDebugRecord> {
        let dir = self.debug_output.as_ref()?;
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("WARN: debug-output mkdir {dir:?} failed: {e}");
            return None;
        }
        // Verb slug from "tapOn (optional)" → "tapon", or empty → "step".
        let verb_slug: String = step_summary
            .split_whitespace()
            .next()
            .unwrap_or("step")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let base = dir.join(format!("step-{idx}-{verb_slug}"));
        let mut record = serde_json::json!({
            "index": idx,
            "summary": step_summary,
            "wallMs": wall_ms,
        });
        let mut png_path_rec: Option<PathBuf> = None;
        let mut tree_path_rec: Option<PathBuf> = None;
        let mut failure_kind_rec: Option<String> = None;
        let mut failure_message_rec: Option<String> = None;
        let verdict = match &outcome {
            StepOutcome::Ok(rep) => {
                let (verdict_str, outcome_val) = match rep {
                    RunStepReport::Ok => ("ok", serde_json::json!("ok")),
                    RunStepReport::Skipped { reason } => {
                        ("skipped", serde_json::json!({ "skipped": reason }))
                    }
                    RunStepReport::ExpandedSubflow { path, .. } => (
                        "expanded-subflow",
                        serde_json::json!({ "expandedSubflow": path.display().to_string() }),
                    ),
                };
                record["outcome"] = outcome_val;
                verdict_str.to_string()
            }
            StepOutcome::Failed(err) => {
                let kind = failure_kind(err);
                let message = err.to_string();
                failure_kind_rec = Some(kind.clone());
                failure_message_rec = Some(message.clone());
                let mut fail = serde_json::json!({
                    "kind": kind,
                    "message": message,
                });
                let png_path = base.with_extension("fail.png");
                match self.app.screenshot().await {
                    Ok(mut bytes) => {
                        // Auto-annotate the fail screenshot with a
                        // corner banner + a red circle approximately
                        // centered (viewport 50%, 50%). The circle is
                        // placed blind rather than on the failing
                        // element because selector→pixel resolution is
                        // not available here. `--no-fail-annotate`
                        // disables. Best-effort: on annotation failure
                        // ship the raw bytes.
                        if !self.no_fail_annotate {
                            let annotations = vec![
                                crate::AnnotationSpec::Circle {
                                    at: crate::AnnotationPos::Normalized { nx: 0.5, ny: 0.5 },
                                    color: "red".into(),
                                    radius: 60,
                                    stroke: 6,
                                },
                                crate::AnnotationSpec::Text {
                                    at: crate::AnnotationPos::Pixel { x: 20, y: 40 },
                                    content: format!("step {}: FAIL", idx),
                                    color: "red".into(),
                                    size: 28.0,
                                },
                                crate::AnnotationSpec::Text {
                                    at: crate::AnnotationPos::Pixel { x: 20, y: 76 },
                                    content: step_summary.to_string(),
                                    color: "yellow".into(),
                                    size: 18.0,
                                },
                            ];
                            match crate::annotate_bridge::render_yaml_annotations(
                                &bytes,
                                &annotations,
                            ) {
                                Ok((annotated, _warns)) => {
                                    bytes = annotated;
                                    fail["annotated"] = serde_json::json!(true);
                                }
                                Err(e) => {
                                    fail["annotateError"] = serde_json::json!(format!("{e}"));
                                }
                            }
                        }
                        let mut warns: Vec<String> = Vec::new();
                        match crate::output::write_yaml_output_lenient(
                            &png_path, &bytes, &mut warns,
                        ) {
                            Some(written) => {
                                fail["screenshotPath"] =
                                    serde_json::json!(written.display().to_string());
                                png_path_rec = Some(written);
                            }
                            None => {
                                for w in warns {
                                    eprintln!("WARN: {w}");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        fail["screenshotError"] = serde_json::json!(format!("{e}"));
                    }
                }
                // Snapshot the a11y tree alongside the fail PNG so
                // post-mortem analysis can see what the runner
                // saw, not just what the sim rendered. Best-effort;
                // any tree() error is recorded but does not affect
                // the run result.
                let tree_path = base.with_extension("fail.tree.json");
                match self.app.tree().await {
                    Ok(tree) => {
                        let tree_bytes = serde_json::to_vec_pretty(&tree).unwrap_or_default();
                        let mut warns: Vec<String> = Vec::new();
                        match crate::output::write_yaml_output_lenient(
                            &tree_path,
                            &tree_bytes,
                            &mut warns,
                        ) {
                            Some(written) => {
                                fail["treePath"] = serde_json::json!(written.display().to_string());
                                tree_path_rec = Some(written);
                            }
                            None => {
                                for w in warns {
                                    eprintln!("WARN: {w}");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        fail["treeError"] = serde_json::json!(format!("{e}"));
                    }
                }
                record["outcome"] = serde_json::json!({ "failed": fail });
                "failed".to_string()
            }
        };
        let json_path = base.with_extension("json");
        let mut warns: Vec<String> = Vec::new();
        let json_written = crate::output::write_yaml_output_lenient(
            &json_path,
            &serde_json::to_vec_pretty(&record).unwrap_or_default(),
            &mut warns,
        );
        if json_written.is_none() {
            for w in warns {
                eprintln!("WARN: {w}");
            }
        }
        let final_json_path = json_written.unwrap_or(json_path);
        Some(StepDebugRecord {
            n: idx,
            verb: verb_slug,
            summary: step_summary.to_string(),
            verdict,
            wall_ms,
            json_path: final_json_path,
            png_path: png_path_rec,
            tree_path: tree_path_rec,
            failure_kind: failure_kind_rec,
            failure_message: failure_message_rec,
        })
    }

    /// Poll the smix runner for any SpringBoard / share-sheet
    /// alert currently on screen and tap the first non-cancel button. iOS
    /// 17+ raises an "Open in <App>?" alert after `simctl openurl` against
    /// a registered custom scheme; without dismissing it every subsequent
    /// step races a popup overlay the host-resolver does not see. Best-
    /// effort: silently no-ops when no popup surfaces inside the 3s budget.
    async fn dismiss_open_link_popup(&mut self) -> Result<(), ExpectationFailure> {
        for _ in 0..6 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let popups = self.app.system_popups().await?;
            let Some(popup) = popups.into_iter().next() else {
                continue;
            };
            let Some(button) = popup
                .buttons
                .iter()
                .find(|b| b.role != "cancel" && !b.dangerous)
                .or_else(|| popup.buttons.first())
            else {
                return Ok(());
            };
            let _ = self.app.system_popup_action(&popup.id, &button.id).await?;
            return Ok(());
        }
        Ok(())
    }

    /// Recursive body runner for `repeat` / `retry`. Re-uses
    /// the caller's `warnings` collector + does not push outer-level
    /// `RunStepReport`. Box-pinned at call sites (async-trait Send
    /// recursion bound).
    async fn run_steps_inner(
        &mut self,
        steps: &[Step],
        warnings: &mut Vec<String>,
    ) -> Result<(), RunError> {
        for step in steps {
            let _ = self.run_step(step, warnings).await?;
        }
        Ok(())
    }

    /// Sample the screen until two consecutive frames are still, or until
    /// `ceiling_ms` runs out. Returns whether it settled.
    ///
    /// A screenshot that won't decode propagates rather than reading as
    /// motion: a frame we cannot compare is a broken toolchain, and silently
    /// treating it as "still moving" would burn the whole ceiling on every
    /// step and look like a slow device.
    async fn wait_until_still(&self, ceiling_ms: u64) -> Result<bool, RunError> {
        // No sleep between samples: the capture is the interval. The raw-BGRA
        // capture path grabs a frame directly from the resident IOSurface host
        // (~sub-ms) and compares grayscale in place — no PNG encode/decode. It
        // falls back to a simctl PNG frame when the surface is unavailable, and
        // `frames_still` compares BGRA and PNG frames interchangeably.
        let deadline = std::time::Instant::now() + Duration::from_millis(ceiling_ms);
        let mut previous = self.app.capture_frame().await?;
        loop {
            let next = self.app.capture_frame().await?;
            if smix_sdk::quiescence::frames_still(&previous, &next, &self.quiescence)? {
                return Ok(true);
            }
            if std::time::Instant::now() >= deadline {
                return Ok(false);
            }
            previous = next;
        }
    }

    async fn run_step(
        &mut self,
        step: &Step,
        warnings: &mut Vec<String>,
    ) -> Result<RunStepReport, RunError> {
        match step {
            Step::TapOn {
                selector,
                optional,
                dispatch,
            } => {
                // Explicit dispatch override. Which taps need which
                // mechanism is runtime knowledge (SwiftUI modal
                // binding vs RN Pressable) that belongs to the test
                // author — see `TapDispatch` docs.
                match dispatch {
                    Some(crate::TapDispatch::Xcui) => {
                        let desugared = self.desugar_localized_text(selector);
                        let Selector::Id { id, .. } = desugared.as_ref() else {
                            return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                                code: Some(FailureCode::DriverError),
                                message: "tapOn dispatch: xcui requires an `id:` selector".into(),
                                selector: Some(selector.clone()),
                                hint: Some(
                                    "XCUIElement-anchored dispatch resolves by \
                                     accessibilityIdentifier; give the target an id or \
                                     drop `dispatch: xcui` to use default routing"
                                        .into(),
                                ),
                                ..Default::default()
                            })));
                        };
                        match self.app.tap_xcui(id).await {
                            Ok(()) => Ok(RunStepReport::Ok),
                            Err(e) if *optional && e.code == FailureCode::ElementNotFound => {
                                Ok(RunStepReport::Skipped {
                                    reason: format!(
                                        "optional tapOn (dispatch: xcui): element not found (id={id}); skipped"
                                    ),
                                })
                            }
                            Err(e) => Err(RunError::Sdk(e)),
                        }
                    }
                    Some(crate::TapDispatch::DaemonProxy) => {
                        let desugared = self.desugar_localized_text(selector);
                        match self
                            .app
                            .tap_with_mode(&desugared, smix_sdk::TapMode::DaemonProxySynthesize)
                            .await
                        {
                            Ok(()) => Ok(RunStepReport::Ok),
                            Err(e) if *optional && e.code == FailureCode::ElementNotFound => {
                                Ok(RunStepReport::Skipped {
                                    reason: format!(
                                        "optional tapOn (dispatch: daemonProxy): element not found ({}); skipped",
                                        smix_sdk::describe_selector(selector)
                                    ),
                                })
                            }
                            Err(e) => Err(RunError::Sdk(e)),
                        }
                    }
                    None => self.run_tap(selector, *optional).await,
                }
            }
            Step::TapAtPoint { nx, ny } => {
                self.app.tap_at_coord(*nx, *ny).await?;
                Ok(RunStepReport::Ok)
            }
            Step::WaitForAnimationToEnd { ceiling_ms } => {
                if !self.wait_until_still(*ceiling_ms).await? {
                    // Not a failure. A screen that never settles is usually
                    // a spinner or a caret the flow doesn't care about, and
                    // failing here would make the verb unusable on any screen
                    // with one.
                    warnings.push(format!(
                        "waitForAnimationToEnd: the screen was still moving after {ceiling_ms}ms; carrying on. Raise the ceiling if the animation really is longer."
                    ));
                }
                Ok(RunStepReport::Ok)
            }
            Step::WebViewEval { js, assert_eq } => {
                let result = self.app.webview_eval(js).await?;
                if let Some(expected) = assert_eq
                    && &result != expected
                {
                    return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::AssertionFailed),
                        message: format!(
                            "webview_eval assert_eq mismatch: got {result}, expected {expected}"
                        ),
                        hint: Some(format!(
                            "Eval'd `{}` returned {result}, not the expected value",
                            if js.len() > 80 { &js[..80] } else { js }
                        )),
                        ..Default::default()
                    })));
                }
                Ok(RunStepReport::Ok)
            }
            Step::ExtendedWaitUntil {
                selector,
                timeout_ms,
                expect_visible,
            } => {
                // Desugar LocalizedText by current locale.
                let desugared = self.desugar_localized_text(selector);
                let timeout = Duration::from_millis(*timeout_ms);
                let result = if *expect_visible {
                    // OCR + Fallback support inside extendedWaitUntil.
                    // Dispatching every selector shape to
                    // `app.wait_for` would use the tree resolver,
                    // which silently skips OcrText members inside a
                    // Fallback chain — OCR would never fire during the
                    // poll window. So:
                    //   - Fallback chain containing OcrText → poll
                    //     both tree-resolvable subs AND OCR subs per
                    //     iteration; first hit wins.
                    //   - Standalone OcrText → poll `find_by_text_ocr`
                    //     within the timeout budget.
                    //   - Everything else → plain tree-based path.
                    self.wait_for_visible_with_ocr(&desugared, timeout).await
                } else {
                    self.app
                        .wait_for_not_visible(&desugared, timeout)
                        .await
                        .map(|_| ())
                        .map_err(RunError::Sdk)
                };
                // Unconditional screenshot + tree JSON capture on wait
                // timeout, even without `--debug-output`. Without a
                // visual + tree snapshot at the failure moment,
                // triaging tree-degradation on iOS 26.5 + RN Fabric
                // means eyeballing metro logs and inferring. Every
                // `extendedWaitUntil` timeout attaches a WRITTEN path
                // to the failure hint. Sink dir: `.smix/timeouts/` in
                // CWD when writeable, else
                // `~/.local/share/smix/timeouts/`. Best-effort — any
                // I/O / screenshot / tree error is logged but does
                // not affect the failure verdict.
                match result {
                    Ok(_) => Ok(RunStepReport::Ok),
                    Err(err) => {
                        if is_timeout_error(&err) {
                            let hint = self.capture_timeout_dump("extendedWaitUntil").await;
                            Err(annotate_timeout_error(err, hint))
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            Step::AssertVisible { selector } => {
                // Desugar LocalizedText by current locale.
                let desugared = self.desugar_localized_text(selector);
                self.app.assert_visible(&desugared).await?;
                Ok(RunStepReport::Ok)
            }
            Step::InputText(s) => {
                let expanded = self.expand_template(s)?;
                self.app.fill(&smix_sdk::focused(), &expanded).await?;
                Ok(RunStepReport::Ok)
            }
            Step::InputTextInto { selector, text } => {
                let expanded = self.expand_template(text)?;
                self.app.fill(selector, &expanded).await?;
                Ok(RunStepReport::Ok)
            }
            Step::Back => {
                self.app.go_back().await?;
                Ok(RunStepReport::Ok)
            }
            Step::PressKey(s) => {
                let key = parse_key_name(s)?;
                // iOS Simulator hardware-button restrictions:
                //   Apple documents XCUIDevice.Button.volumeUp /
                //   .volumeDown as unavailable on the simulator; lock
                //   is not exposed in the public enum at all and
                //   simctl has no lock verb. maestro has the same
                //   limitation on the iOS simulator. Rather than
                //   no-op silently, report an explicit Skipped +
                //   warning so the caller knows.
                if matches!(key, KeyName::Lock | KeyName::VolumeUp | KeyName::VolumeDown) {
                    let reason = format!(
                        "pressKey {key}: unavailable in iOS Simulator (Apple XCUIDevice.Button restriction); maestro has the same limitation"
                    );
                    warnings.push(reason.clone());
                    return Ok(RunStepReport::Skipped { reason });
                }
                self.app.press_key(key).await?;
                Ok(RunStepReport::Ok)
            }
            Step::EraseText(n) => {
                for _ in 0..*n {
                    self.app.press_key(KeyName::Delete).await?;
                }
                Ok(RunStepReport::Ok)
            }
            Step::ScrollUntilVisible {
                selector,
                direction,
            } => {
                let dir = parse_swipe_direction(direction)?;
                // When the selector contains any `OcrText` sub, the
                // driver's `scroll` (tree-only resolver) can't see
                // off-screen targets in degraded a11y trees (RN 0.86
                // Fabric LazyColumn/LazyRow on iOS 26.5 drops
                // off-screen items). Use an adapter-side loop instead:
                // probe the tree via `App::find` AND OCR via
                // `App::find_by_text_ocr` between each swipe. First
                // hit stops. Perf caveat: OCR costs ~500ms per
                // iteration on top of the ~250ms each swipe takes.
                if self.selector_contains_ocr(selector) {
                    self.scroll_until_visible_with_ocr(selector, dir).await?;
                } else {
                    self.app.scroll(selector, dir).await?;
                }
                Ok(RunStepReport::Ok)
            }
            Step::Swipe { from, to } => {
                self.app.swipe_at_coord(*from, *to).await?;
                Ok(RunStepReport::Ok)
            }
            Step::Scroll => {
                self.app.scroll_screen(SwipeDirection::Down).await?;
                Ok(RunStepReport::Ok)
            }
            Step::HideKeyboard => {
                self.app.hide_keyboard().await?;
                Ok(RunStepReport::Ok)
            }
            Step::AssertNotVisible { selector } => {
                // Match maestro CLI implicit retry semantics: wait up
                // to 5s for the element to disappear. SwiftUI sheet/alert/
                // confirmation-dialog dismiss animations take 200-700ms, and
                // a bare one-shot assert_not_visible races them.
                self.app
                    .wait_for_not_visible(selector, Duration::from_secs(5))
                    .await?;
                Ok(RunStepReport::Ok)
            }
            Step::KillApp { app_id } => {
                let Some(bundle) = app_id.clone().or_else(|| self.last_bundle.clone()) else {
                    return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: "killApp: no app launched yet and no appId given".into(),
                        hint: Some(
                            "launchApp first, or name the bundle: `killApp: com.acme.app`".into(),
                        ),
                        ..Default::default()
                    })));
                };
                self.app.terminate(&bundle).await?;
                Ok(RunStepReport::Ok)
            }
            Step::ClearAppData {
                launch_args,
                launch_env,
            } => {
                // Session-scoped in-place data clear.
                // Orchestrates: runner cooperative
                // terminate + host sandbox wipe + runner cooperative
                // launch (with caller-supplied launchArgs /
                // launchEnvironment when provided). Preserves
                // XCUITest binding and does not signal ReportCrash.
                //
                // Requires a session to be open (Rust adapter runs
                // through Session-Id from `smix run` auto-session
                // path). AppLike's `clear_app_data_with_launch` on
                // the trait routes through to the SDK
                // `App::clear_app_data_with_launch`.
                self.app
                    .clear_app_data_with_launch(launch_args, launch_env)
                    .await?;
                Ok(RunStepReport::Ok)
            }
            Step::ClearUserDefaults { keys, bundle_id } => {
                // Host-side per-key NSUserDefaults deletion — the
                // motivating case is neutralizing expo-dev-launcher's
                // deep-link replay. Bundle resolution: explicit
                // `bundleId:` wins, else the flow's resolved app id
                // (seeded into last_bundle at run start).
                let Some(bundle) = bundle_id.clone().or_else(|| self.last_bundle.clone()) else {
                    return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: "clearUserDefaults: no target bundle id".into(),
                        hint: Some(
                            "set `bundleId:` on the step, or give the flow an `appId:` \
                             header / pass --bundle-id"
                                .into(),
                        ),
                        ..Default::default()
                    })));
                };
                self.app.clear_user_defaults(&bundle, keys).await?;
                Ok(RunStepReport::Ok)
            }
            Step::ResetAppData {
                url,
                wait_for,
                timeout_ms,
            } => {
                // URL-scheme JS-wipe. Fires
                // `simctl openurl <UDID> <url>` on host side (no
                // runner HTTP round-trip), then optionally waits for
                // a completion signal per `wait_for`. Distinct from
                // ClearAppData: no container wipe → dev-fixture state
                // (Expo dev-launcher metro URL cache, dev-tools
                // state, MMKV keys the app chooses to preserve)
                // survives, so consumers don't pay the dev-client
                // ceremony cost on every reset.
                //
                // Runtime dispatch (as opposed to a bare
                // `App::reset_app_data`) — needs the runtime's
                // MetroLogTail reference to satisfy the
                // `waitFor: { logLinePattern }` shape, and the
                // MetroLogTail is only threaded through the runtime
                // (not down into `App`). CLI-side reset counter
                // increments happen here on both success and
                // timeout so `smix diagnostic dump` can surface the
                // outcome even when the whole run failed.
                self.app.open_url(url).await?;
                smix_simctl::increment_reset_app_data_total();
                match wait_for {
                    None => {}
                    Some(smix_sdk::ResetAppDataWaitFor::Sleep(ms)) => {
                        tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
                    }
                    Some(smix_sdk::ResetAppDataWaitFor::LogLinePattern(pattern)) => {
                        let Some(tail) = self.metro_tail.as_ref() else {
                            // Best-effort: no metro log wired.
                            // Sleep 500 ms so the URL propagates —
                            // but say so in the RunReport: "the
                            // caller knew" left no trace that the
                            // completion signal was never awaited.
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            warnings.push(format!(
                                "resetAppData: waitFor.logLinePattern {pattern:?} \
                                 requires --metro-log, which is not configured — \
                                 waited a blind 500ms instead of the signal"
                            ));
                            return Ok(RunStepReport::Ok);
                        };
                        let matcher = smix_metro_log::SignalMatcher::new(pattern, None)
                            .map_err(|e| {
                                RunError::Sdk(smix_sdk::ExpectationFailure::new(
                                    smix_sdk::FailureInit {
                                        code: Some(smix_sdk::FailureCode::DriverError),
                                        message: format!(
                                            "resetAppData.waitFor.logLinePattern invalid regex ({pattern:?}): {e}"
                                        ),
                                        ..Default::default()
                                    },
                                ))
                            })?;
                        let outcome = tail
                            .await_signal(
                                &matcher,
                                std::time::Duration::from_millis(*timeout_ms),
                                smix_metro_log::Window::LastMs(u64::MAX),
                            )
                            .await;
                        if outcome.is_err() {
                            smix_simctl::increment_reset_app_data_timed_out();
                            return Err(RunError::Sdk(smix_sdk::ExpectationFailure::new(
                                smix_sdk::FailureInit {
                                    code: Some(smix_sdk::FailureCode::Timeout),
                                    message: format!(
                                        "resetAppData: URL fired ({url}) but log-line pattern ({pattern:?}) did not appear on the metro log within {timeout_ms}ms"
                                    ),
                                    ..Default::default()
                                },
                            )));
                        }
                    }
                }
                Ok(RunStepReport::Ok)
            }
            Step::ClearState { app_id } => {
                let Some(bundle) = app_id.clone().or_else(|| self.last_bundle.clone()) else {
                    return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: "clearState: no app launched yet and no appId given".into(),
                        hint: Some(
                            "launchApp first, or name the bundle:                              `clearState: { appId: com.acme.app }`"
                                .into(),
                        ),
                        ..Default::default()
                    })));
                };
                let app_path = launch_fresh_app_path_from_env(&bundle);
                let extra = self
                    .app
                    .launch_fresh(&bundle, true, false, app_path.as_deref(), &[])
                    .await?;
                warnings.extend(extra);
                self.last_bundle = Some(bundle);
                Ok(RunStepReport::Ok)
            }
            Step::ClearKeychain => match self.last_bundle.clone() {
                Some(b) => {
                    let app_path = launch_fresh_app_path_from_env(&b);
                    let extra = self
                        .app
                        .launch_fresh(&b, false, true, app_path.as_deref(), &[])
                        .await?;
                    warnings.extend(extra);
                    Ok(RunStepReport::Ok)
                }
                None => {
                    let reason = "clearKeychain with no last_bundle and empty flow appId; skipped"
                        .to_string();
                    warnings.push(reason.clone());
                    Ok(RunStepReport::Skipped { reason })
                }
            },
            Step::TakeScreenshot { path, annotations } => {
                let mut bytes = self.app.screenshot().await?;
                // Compose annotations onto the PNG BEFORE writing.
                // Empty annotations = plain screenshot.
                // Selector-relative positions are unsupported in this
                // yaml verb: callers use `{x, y}` or `{nx, ny}`
                // shapes. A Selector shape logs a warning and skips.
                if !annotations.is_empty() {
                    match crate::annotate_bridge::render_yaml_annotations(&bytes, annotations) {
                        Ok((annotated, extra_warns)) => {
                            bytes = annotated;
                            warnings.extend(extra_warns);
                        }
                        Err(e) => {
                            warnings.push(format!(
                                "takeScreenshot annotate: {e}; falling back to unannotated screenshot"
                            ));
                        }
                    }
                }
                if let Some(p) = path {
                    let yaml_path = std::path::Path::new(p);
                    let written = crate::output::write_yaml_output(
                        yaml_path,
                        &bytes,
                        crate::output::OutputIntent::LoadBearing,
                    )
                    .map_err(|e| {
                        RunError::Io(format!(
                            "takeScreenshot write {p:?}: {e}; {} bytes captured but not persisted",
                            bytes.len()
                        ))
                    })?;
                    if written != yaml_path {
                        warnings.push(format!(
                            "takeScreenshot: yaml path {p:?} had no image extension; auto-appended → {written:?}"
                        ));
                    }
                }
                Ok(RunStepReport::Ok)
            }
            Step::SetClipboard(text) => {
                let expanded = self.expand_template(text)?;
                self.app.set_clipboard(&expanded).await?;
                Ok(RunStepReport::Ok)
            }
            Step::PasteText { text } => {
                let expanded = match text {
                    Some(t) => Some(self.expand_template(t)?),
                    None => None,
                };
                self.app.paste_text(expanded.as_deref()).await?;
                Ok(RunStepReport::Ok)
            }
            Step::CopyTextFrom { selector } => {
                self.app.copy_text_from(selector).await?;
                Ok(RunStepReport::Ok)
            }
            Step::DoubleTapOn { selector } => {
                self.app.double_tap(selector).await?;
                Ok(RunStepReport::Ok)
            }
            Step::RepeatTap {
                selector,
                times,
                interval_ms,
                hold_ms,
            } => {
                self.app
                    .tap_burst(selector, *times, *interval_ms, *hold_ms)
                    .await?;
                Ok(RunStepReport::Ok)
            }
            Step::LongPressOn {
                selector,
                duration_ms,
                capture_during,
            } => {
                let duration = Duration::from_millis(*duration_ms);
                if !*capture_during {
                    self.app.long_press(selector, duration).await?;
                    return Ok(RunStepReport::Ok);
                }
                let capture = self.app.long_press_capturing(selector, duration).await?;
                self.write_press_frames(selector, &capture)?;
                Ok(RunStepReport::Ok)
            }
            Step::AssertTrue { expr } => {
                let value = self.eval_expr_or_driver(expr)?;
                if !value.is_truthy() {
                    return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::AssertionFailed),
                        message: format!(
                            "assertTrue: expression evaluated to {value:?}, expected truthy — source: {expr}"
                        ),
                        suggestions: vec![
                            "Verify `output.*` referenced in the expression".to_string(),
                            "yaml expression engine supports == != && || ! () output.x .contains()"
                                .to_string(),
                        ],
                        ..Default::default()
                    })));
                }
                Ok(RunStepReport::Ok)
            }
            Step::Repeat { mode, commands } => {
                match mode {
                    RepeatMode::Times(n) => {
                        for _ in 0..*n {
                            Box::pin(self.run_steps_inner(commands, warnings)).await?;
                        }
                    }
                    RepeatMode::While { condition_expr } => {
                        let mut iter = 0u32;
                        loop {
                            let cond = self.eval_expr_or_driver(condition_expr)?;
                            if !cond.is_truthy() {
                                break;
                            }
                            Box::pin(self.run_steps_inner(commands, warnings)).await?;
                            iter += 1;
                            if iter >= MAX_REPEAT_ITERATIONS {
                                return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                                    code: Some(FailureCode::DriverError),
                                    message: format!(
                                        "repeat.while: max iterations ({MAX_REPEAT_ITERATIONS}) exceeded — condition `{condition_expr}` never became falsy; it has to depend on something that changes between iterations (of the stores it can read, only `output` changes, and only `extractWithAI` writes it)"
                                    ),
                                    suggestions: vec![
                                        "Convert to `repeat: { times: N }` with a known bound"
                                            .to_string(),
                                    ],
                                    ..Default::default()
                                })));
                            }
                        }
                    }
                    RepeatMode::WhileVisible { selector } => {
                        let mut iter = 0u32;
                        loop {
                            // Visible iff find returns Ok(true). `find`
                            // maps to the driver's `find` route and
                            // returns a bool rather than raising
                            // ElementNotFound — exactly the truthy
                            // probe this loop needs.
                            let visible = self.app.find(selector).await.unwrap_or(false);
                            if !visible {
                                break;
                            }
                            Box::pin(self.run_steps_inner(commands, warnings)).await?;
                            iter += 1;
                            if iter >= MAX_REPEAT_ITERATIONS {
                                return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                                    code: Some(FailureCode::DriverError),
                                    message: format!(
                                        "repeat.while.visible: max iterations ({MAX_REPEAT_ITERATIONS}) exceeded — selector `{}` stayed visible; expected the loop body to hide / dismiss it eventually",
                                        smix_sdk::describe_selector(selector)
                                    ),
                                    suggestions: vec![
                                        "Convert to `repeat: { times: N }` with a known bound, or ensure the body dismisses the watched element".to_string(),
                                    ],
                                    ..Default::default()
                                })));
                            }
                        }
                    }
                    RepeatMode::WhileNotVisible { selector } => {
                        let mut iter = 0u32;
                        loop {
                            // Loop continues while element is NOT
                            // visible; exits once it appears. Mirrors the
                            // `WhileVisible` arm but inverts the truthy test.
                            let visible = self.app.find(selector).await.unwrap_or(false);
                            if visible {
                                break;
                            }
                            Box::pin(self.run_steps_inner(commands, warnings)).await?;
                            iter += 1;
                            if iter >= MAX_REPEAT_ITERATIONS {
                                return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                                    code: Some(FailureCode::DriverError),
                                    message: format!(
                                        "repeat.while.notVisible: max iterations ({MAX_REPEAT_ITERATIONS}) exceeded — selector `{}` never became visible; expected the loop body to make it appear eventually",
                                        smix_sdk::describe_selector(selector)
                                    ),
                                    suggestions: vec![
                                        "Convert to `repeat: { times: N }` with a known bound, or ensure the body triggers the watched element to appear".to_string(),
                                    ],
                                    ..Default::default()
                                })));
                            }
                        }
                    }
                }
                Ok(RunStepReport::Ok)
            }
            Step::Retry {
                max_retries,
                commands,
            } => {
                let mut last_err: Option<RunError> = None;
                for _ in 0..=*max_retries {
                    match Box::pin(self.run_steps_inner(commands, warnings)).await {
                        Ok(()) => return Ok(RunStepReport::Ok),
                        Err(e) => last_err = Some(e),
                    }
                }
                Err(last_err.expect("at least one attempt"))
            }
            Step::RunScript { source } => {
                Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!(
                        "runScript: a complete JS runtime is not supported (maestro runs these on GraalJS). Move scripting logic out of yaml, or use assertTrue with the minimal expression engine (== != && || ! () output.x .contains()). source snippet: {:?}",
                        source.chars().take(80).collect::<String>()
                    ),
                    suggestions: vec![
                        "Replace inline JS with assertTrue + minimal expression engine".to_string(),
                        "Or wait for v6+ when full JS runtime ships".to_string(),
                    ],
                    ..Default::default()
                })))
            }
            Step::EvalScript { source } => {
                Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!(
                        "evalScript: a complete JS runtime is not supported. For boolean expressions, use `assertTrue: \"<expr>\"` with the minimal engine subset. source snippet: {:?}",
                        source.chars().take(80).collect::<String>()
                    ),
                    suggestions: vec![
                        "Use assertTrue + minimal expression engine for boolean checks".to_string(),
                    ],
                    ..Default::default()
                })))
            }
            Step::SetLocation {
                latitude,
                longitude,
            } => {
                self.app.set_location(*latitude, *longitude).await?;
                Ok(RunStepReport::Ok)
            }
            Step::Travel { points, speed_mps } => {
                self.app.travel(points, *speed_mps).await?;
                Ok(RunStepReport::Ok)
            }
            Step::SetPermissions {
                app_id,
                permissions,
            } => {
                let bundle = match app_id.as_deref() {
                    Some(b) => b.to_string(),
                    None => self.last_bundle.clone().ok_or_else(|| {
                        RunError::Sdk(ExpectationFailure::new(FailureInit {
                            code: Some(FailureCode::DriverError),
                            message:
                                "setPermissions: no app launched in this flow — call `launchApp` first, or use `launchApp.permissions` instead"
                                    .into(),
                            suggestions: vec![
                                "Add a `- launchApp: <bundleId>` step before this `setPermissions`".to_string(),
                                "Or move permission setup into `launchApp.permissions`".to_string(),
                            ],
                            ..Default::default()
                        }))
                    })?,
                };
                let sdk_perms = permissions
                    .iter()
                    .map(|(name, action)| {
                        let perm = parse_simctl_permission(name).map_err(RunError::Parse)?;
                        Ok::<_, RunError>((perm, action.to_sdk()))
                    })
                    .collect::<Result<Vec<_>, RunError>>()?;
                self.app.set_permissions(&bundle, &sdk_perms).await?;
                Ok(RunStepReport::Ok)
            }
            Step::AddMedia { paths } => {
                self.app.add_media(paths).await?;
                Ok(RunStepReport::Ok)
            }
            Step::SetOrientation { orientation } => {
                self.app.set_orientation(*orientation).await?;
                Ok(RunStepReport::Ok)
            }
            Step::StartRecording { path } => {
                self.app.start_recording(path).await?;
                Ok(RunStepReport::Ok)
            }
            Step::StopRecording => {
                self.app.stop_recording().await?;
                Ok(RunStepReport::Ok)
            }
            Step::AssertScreenshot {
                path,
                max_hamming,
                mask,
            } => {
                let abs = self.base_dir.join(path);
                let cap = max_hamming.unwrap_or(5);
                if !mask.is_empty() {
                    // Surface-only: the parser carried the regions but
                    // the runtime skips them. Warn into the RunReport
                    // rather than dropping them silently.
                    warnings.push(format!(
                        "assertScreenshot.mask: {} region(s) accepted but ignored (region exclusion needs SSIM/pHash; the current dhash compares the full frame); full-frame dhash dispatched with max_hamming={}",
                        mask.len(),
                        cap
                    ));
                }
                let outcome = self.app.assert_screenshot(&abs, cap).await?;
                if let smix_sdk::AssertScreenshotOutcome::Recorded { path } = outcome {
                    warnings.push(format!(
                        "assertScreenshot: auto-recorded baseline at {} (first run; subsequent runs will diff against it)",
                        path.display()
                    ));
                }
                Ok(RunStepReport::Ok)
            }
            Step::AssertCondition { condition } => {
                let png = self.app.screenshot().await?;
                let verdict = smix_ai_tier::judge(&png, condition, &self.ai_cfg).await?;
                if !verdict.pass {
                    return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::AssertionFailed),
                        message: format!(
                            "[AI · non-deterministic] assertCondition did not hold: {condition} — the judge saw: {}",
                            verdict.reason
                        ),
                        hint: Some(
                            "this verdict is a judgement rather than a measurement; another run may answer differently"
                                .into(),
                        ),
                        ..Default::default()
                    })));
                }
                Ok(RunStepReport::Ok)
            }
            Step::ExtractWithAI { into, fields } => {
                let png = self.app.screenshot().await?;
                let extracted = smix_ai_tier::extract(&png, fields, &self.ai_cfg).await?;
                for (field, value) in extracted {
                    // The output store is flat — no nested objects — so `into`
                    // is a key prefix, not an object. Read a field back with
                    // `${output["order.total"]}`.
                    self.output
                        .insert(format!("{into}.{field}"), crate::ExprValue::String(value));
                }
                Ok(RunStepReport::Ok)
            }
            Step::ExpectSignal {
                regex,
                level,
                timeout_ms,
                window,
            } => {
                self.run_expect_signal(regex, level.as_deref(), *timeout_ms, *window)
                    .await
            }
            Step::ExpectSignals {
                signals,
                order,
                timeout_ms,
                window,
            } => {
                self.run_expect_signals(signals, *order, *timeout_ms, *window)
                    .await
            }
            Step::ExpectLogClean => self.run_expect_log_clean(warnings).await,
            Step::Fixture { id, timeout_ms } => self.run_fixture(id, *timeout_ms).await,
            Step::LaunchApp {
                app_id,
                clear_state,
                clear_keychain,
                permissions,
                arguments,
                stop_app,
                wait_for_interactive_ms,
            } => {
                // Update last_locale from the `-AppleLanguages "(xx)"`
                // pair in arguments, for Selector::LocalizedText desugar.
                // Applies on both stopApp=true (kill+launch) and stopApp=false
                // foreground; in foreground path arguments are ignored by
                // launch (warned below), but we still update locale state
                // because the yaml author's intent was "pin locale" — keep
                // state consistent with what they declared.
                if let Some(lang) = Self::extract_apple_languages(arguments) {
                    self.last_locale = lang;
                }
                if !*stop_app {
                    // maestro `stopApp: false` = foreground resume.
                    // permissions / arguments / clear flags are
                    // meaningless here because foreground does not
                    // restart the process — warn explicitly rather
                    // than dropping them silently.
                    if !permissions.is_empty()
                        || !arguments.is_empty()
                        || *clear_state
                        || *clear_keychain
                    {
                        warnings.push(format!(
                            "launchApp.stopApp=false ignores permissions/arguments/clearState/clearKeychain (foreground path); bundle={app_id}"
                        ));
                    }
                    self.app.foreground(app_id).await?;
                    self.last_bundle = Some(app_id.clone());
                    Ok(RunStepReport::Ok)
                } else {
                    // maestro `stopApp: true` (default) = kill+launch
                    // with optional wipe + args + permissions.
                    let app_path = launch_fresh_app_path_from_env(app_id);
                    let sdk_perms = permissions
                        .iter()
                        .map(|(name, action)| {
                            let perm = parse_simctl_permission(name).map_err(RunError::Parse)?;
                            Ok::<_, RunError>((perm, action.to_sdk()))
                        })
                        .collect::<Result<Vec<(SimctlPermission, PermissionAction)>, RunError>>()?;
                    let opts = LaunchAppOptions {
                        bundle_id: app_id.clone(),
                        clear_state: *clear_state,
                        clear_keychain: *clear_keychain,
                        arguments: arguments.clone(),
                        permissions: sdk_perms,
                        app_path,
                        launch_activity: self.launch_activity.clone(),
                    };
                    // `waitForInteractiveMs` on `launchApp` yaml is a
                    // warning-only marker. The
                    // launch-with-options SDK path uses `simctl
                    // launch --args` (host-side, not
                    // session-scoped), so it can't populate the
                    // `/session/launch-app` request field the runner
                    // reads for interactive polling. Consumers who
                    // want interactive gating use the `clearAppData`
                    // yaml verb instead — its SDK path
                    // (`App::clear_app_data_with_launch`) defaults
                    // `wait_for_interactive_ms: Some(30_000)` and
                    // populates `launchAppReachedInteractive` in the
                    // diagnostic dump. Warn so the yaml author knows
                    // this rather than silently doing nothing.
                    if wait_for_interactive_ms.is_some() {
                        warnings.push(format!(
                            "launchApp.waitForInteractiveMs (bundle={app_id}) is a v1.0.16 marker \
                             — the launch pathway is host-side simctl and doesn't route to the \
                             /session/launch-app interactive polling. Use `clearAppData` yaml \
                             (which SDK-defaults `wait_for_interactive_ms: 30000`) to opt into \
                             interactive counter observability."
                        ));
                    }
                    let extra = self.app.launch_app_with_options(&opts).await?;
                    warnings.extend(extra);
                    self.last_bundle = Some(app_id.clone());
                    Ok(RunStepReport::Ok)
                }
            }
            Step::OpenLink(url) => {
                self.app.open_url(url).await?;
                // iOS 17+ raises a SpringBoard "Open in <App>?"
                // confirmation alert when an external URL (here: simctl
                // openurl) is routed to a registered custom scheme. Host
                // maestro CLI's openLink swallows the popup via the XCUITest
                // SpringBoard handle; smix adapter mirrors that by polling
                // system_popups (a11y tree-via-runner sees the alert even
                // though the host-resolver tree does not) and tapping the
                // first non-cancel button. Without this, every subsequent
                // step blocked behind a popup overlay would race-fail.
                let _ = self.dismiss_open_link_popup().await;
                Ok(RunStepReport::Ok)
            }
            Step::StopApp => {
                match self.last_bundle.clone() {
                    Some(b) => {
                        // graceful: terminate failure is non-fatal (app may already be dead).
                        let _ = self.app.terminate(&b).await;
                        Ok(RunStepReport::Ok)
                    }
                    None => {
                        let reason =
                            "stopApp with no last_bundle and empty flow appId; skipped".to_string();
                        warnings.push(reason.clone());
                        Ok(RunStepReport::Skipped { reason })
                    }
                }
            }
            Step::RunFlow(rel) => self.expand_subflow(rel, warnings).await,
            Step::RunFlowInline {
                when_visible,
                when_not_visible,
                steps,
            } => {
                // Inline-commands form. Same visibility gate as
                // RunFlowConditional, but the body is the literal step
                // list (no child file lookup). Mirrors maestro yaml
                // `runFlow: { when: { visible }, commands: [...] }`.
                //
                // The visibility predicate goes through
                // `check_selector_visible`, which fires OCR when the
                // selector contains OcrText anywhere. Using
                // `self.app.find(sel)` instead would route through the
                // tree-only resolver, where `Selector::OcrText` is
                // silently dropped (compile returns false): a
                // `when.visible` with `fallback: [text, ocrText]`
                // under a degraded a11y tree (RN Fabric on iOS 26.5)
                // would return false and skip the whole conditional
                // body with no signal — which surfaces downstream as a
                // verb appearing to "silently no-op" when in fact the
                // conditional was never entered.
                //
                // `when.notVisible` is the inverse gate, for the
                // idempotency pattern: only enter the ceremony if the
                // target state hasn't been reached yet.
                let (should_run, gate_reason) = self
                    .evaluate_run_flow_gate(when_visible.as_ref(), when_not_visible.as_ref())
                    .await;
                if should_run {
                    Box::pin(self.run_steps_inner(steps, warnings)).await?;
                    Ok(RunStepReport::Ok)
                } else {
                    let reason = format!(
                        "runFlow {}; skipped inline body ({} step{})",
                        gate_reason,
                        steps.len(),
                        if steps.len() == 1 { "" } else { "s" }
                    );
                    Ok(RunStepReport::Skipped { reason })
                }
            }
            Step::RunFlowConditional {
                file,
                when_visible,
                when_not_visible,
                as_name,
            } => {
                // Same OCR-aware gate as RunFlowInline.
                let (should_run, gate_reason) = self
                    .evaluate_run_flow_gate(when_visible.as_ref(), when_not_visible.as_ref())
                    .await;
                if should_run {
                    let result = self.expand_subflow(file, warnings).await?;
                    // `as: <name>` outputs alias capture. After the
                    // subflow runs (which conventionally ends with a
                    // copyTextFrom that wrote to the device pasteboard), read
                    // the clipboard and write it into the parent's outputs
                    // map under the alias. Mirrors maestro `runFlow.as: name`
                    // capture: caller can then reference ${output.name}.
                    if let Some(name) = as_name {
                        match self.app.get_clipboard().await {
                            Ok(captured) => {
                                self.output
                                    .insert(name.clone(), crate::ExprValue::String(captured));
                            }
                            Err(e) => {
                                warnings.push(format!(
                                    "runFlow.as=`{name}`: clipboard read failed ({}); output[{name}] left unset",
                                    e.message
                                ));
                            }
                        }
                    }
                    Ok(result)
                } else {
                    let reason = format!("runFlow {}; skipped subflow {file}", gate_reason);
                    Ok(RunStepReport::Skipped { reason })
                }
            }
        }
    }

    /// Evaluate a runFlow gate.
    ///
    /// Returns `(should_run, reason)` where `reason` is a human-readable
    /// description of the outcome:
    /// - No gate: `should_run=true`, reason `unconditional`.
    /// - `when.visible` present + selector visible: `should_run=true`.
    ///   The subflow proceeds and the reason is never surfaced.
    /// - `when.visible` present + selector NOT visible: `should_run=false`,
    ///   reason names the selector's describe form so consumers see
    ///   exactly WHAT was checked and know to grep OCR logs or bump
    ///   settle-wait.
    /// - `when.notVisible` present: symmetric to above with inverted
    ///   sense.
    ///
    /// Visibility check goes through `check_selector_visible`, which
    /// fires OCR (`App::find_by_text_ocr`) when the selector contains
    /// `OcrText` anywhere. A tree-only `App::find` would silently drop
    /// OCR sub-selectors, which misfires the gate under an RN 0.86
    /// Fabric a11y drop.
    ///
    /// Any driver error is treated as "not visible": on predicate
    /// ambiguity, skipping is the caller-safe default.
    async fn evaluate_run_flow_gate(
        &mut self,
        when_visible: Option<&Selector>,
        when_not_visible: Option<&Selector>,
    ) -> (bool, String) {
        if let Some(sel) = when_visible {
            let visible = self.check_selector_visible(sel).await.unwrap_or(false);
            let reason = format!(
                "when.visible={} ({})",
                visible,
                smix_sdk::describe_selector(sel)
            );
            return (visible, reason);
        }
        if let Some(sel) = when_not_visible {
            let visible = self.check_selector_visible(sel).await.unwrap_or(false);
            let reason = format!(
                "when.notVisible visible={} ({})",
                visible,
                smix_sdk::describe_selector(sel)
            );
            // notVisible fires when the selector is NOT visible.
            return (!visible, reason);
        }
        (true, "unconditional".to_string())
    }

    /// Dispatch [`Step::Fixture`].
    ///
    /// Sequence:
    ///   1. Look up `id` in the attached fixture registry
    ///   2. Tap the qa-bubble toggle (a11y id `qa-bubble-toggle`)
    ///   3. Tap the chip by `testID`
    ///   4. Await the chip's declared signal via metro tail
    ///   5. Tap the qa-bubble toggle again to close the overlay
    ///
    /// Missing registry → DriverError with actionable hint.
    async fn run_fixture(
        &mut self,
        id: &str,
        timeout_override: Option<u64>,
    ) -> Result<RunStepReport, RunError> {
        let registry = self.fixture_registry.as_ref().ok_or_else(|| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::DriverError),
                message: format!("fixture `{id}` used but no fixture registry attached"),
                hint: Some(
                    "set `.smix/config.yaml` `fixturesRegistry` to the JSON registry path".into(),
                ),
                ..Default::default()
            }))
        })?;
        let decl = registry.require(id).map_err(|e| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::DriverError),
                message: format!("{e}"),
                ..Default::default()
            }))
        })?;
        let tail = self.metro_tail.as_ref().ok_or_else(|| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::DriverError),
                message: format!(
                    "fixture `{id}` requires metro tail to await its signal — configure `.smix/config.yaml` `metroLog` or pass `--metro-log-url`"
                ),
                ..Default::default()
            }))
        })?;

        let toggle_selector = smix_selector::Selector::Id {
            id: "qa-bubble-toggle".into(),
            modifiers: smix_selector::Modifiers::default(),
        };
        let chip_selector = smix_selector::Selector::Id {
            id: decl.test_id.clone(),
            modifiers: smix_selector::Modifiers::default(),
        };

        // 2. open overlay
        self.app.tap(&toggle_selector).await?;
        // 3. tap chip
        self.app.tap(&chip_selector).await?;
        // 4. await signal
        let matcher = smix_metro_log::SignalMatcher::new(
            &decl.signal.regex,
            decl.signal
                .level
                .as_deref()
                .map(smix_metro_log::LogLevel::parse),
        )
        .map_err(|e| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::Timeout),
                message: format!("fixture `{id}` signal regex compile: {e}"),
                ..Default::default()
            }))
        })?;
        let timeout_ms = timeout_override.unwrap_or(decl.timeout_ms);
        let outcome = tail
            .await_signal(
                &matcher,
                std::time::Duration::from_millis(timeout_ms),
                smix_metro_log::Window::SinceRun,
            )
            .await;
        // 5. always close overlay, regardless of outcome
        let _ = self.app.tap(&toggle_selector).await;
        outcome.map_err(|e| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::Timeout),
                message: format!("fixture `{id}` timed out: {e}"),
                hint: Some(format!(
                    "chip {} tapped but expected signal /{}/ not observed within {timeout_ms}ms",
                    decl.test_id, decl.signal.regex
                )),
                ..Default::default()
            }))
        })?;
        Ok(RunStepReport::Ok)
    }

    /// Dispatch [`Step::ExpectSignal`] against the
    /// attached metro tail.
    async fn run_expect_signal(
        &mut self,
        regex: &str,
        level: Option<&str>,
        timeout_ms: u64,
        window: crate::SignalWindow,
    ) -> Result<RunStepReport, RunError> {
        let tail = self.metro_tail.as_ref().ok_or_else(|| RunError::Sdk(
            smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::DriverError),
                message: "expect.signal used but no metro log tail attached — add `metroLog` block to .smix/config.yaml or pass `--metro-log-url`".into(),
                ..Default::default()
            })
        ))?;
        let matcher =
            smix_metro_log::SignalMatcher::new(regex, level.map(smix_metro_log::LogLevel::parse))
                .map_err(|e| {
                RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                    code: Some(smix_sdk::FailureCode::Timeout),
                    message: format!("expect.signal: regex compile: {e}"),
                    ..Default::default()
                }))
            })?;
        let ml_window = self.signal_window_to_ml(window);
        match tail
            .await_signal(
                &matcher,
                std::time::Duration::from_millis(timeout_ms),
                ml_window,
            )
            .await
        {
            Ok(_) => Ok(RunStepReport::Ok),
            Err(e) => Err(RunError::Sdk(smix_sdk::ExpectationFailure::new(
                smix_sdk::FailureInit {
                    code: Some(smix_sdk::FailureCode::Timeout),
                    message: format!("expect.signal timeout: {e}"),
                    hint: Some(format!(
                        "no metro log line matched /{regex}/ within {timeout_ms}ms (window={window:?}); check the app is emitting the expected signal"
                    )),
                    ..Default::default()
                },
            ))),
        }
    }

    async fn run_expect_signals(
        &mut self,
        signals: &[crate::SignalMatch],
        order: crate::SignalOrderKind,
        timeout_ms: u64,
        window: crate::SignalWindow,
    ) -> Result<RunStepReport, RunError> {
        let tail = self.metro_tail.as_ref().ok_or_else(|| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::DriverError),
                message: "expect.signals used but no metro log tail attached".into(),
                ..Default::default()
            }))
        })?;
        let matchers: Result<Vec<_>, _> = signals
            .iter()
            .map(|s| {
                smix_metro_log::SignalMatcher::new(
                    &s.regex,
                    s.level.as_deref().map(smix_metro_log::LogLevel::parse),
                )
            })
            .collect();
        let matchers = matchers.map_err(|e| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::Timeout),
                message: format!("expect.signals: regex compile: {e}"),
                ..Default::default()
            }))
        })?;
        let ml_order = match order {
            crate::SignalOrderKind::Any => smix_metro_log::SignalOrder::Any,
            crate::SignalOrderKind::Strict => smix_metro_log::SignalOrder::Strict,
        };
        let ml_window = self.signal_window_to_ml(window);
        match tail
            .await_signals(
                &matchers,
                ml_order,
                std::time::Duration::from_millis(timeout_ms),
                ml_window,
            )
            .await
        {
            Ok(_) => Ok(RunStepReport::Ok),
            Err(e) => Err(RunError::Sdk(smix_sdk::ExpectationFailure::new(
                smix_sdk::FailureInit {
                    code: Some(smix_sdk::FailureCode::Timeout),
                    message: format!("expect.signals failed: {e}"),
                    ..Default::default()
                },
            ))),
        }
    }

    async fn run_expect_log_clean(
        &mut self,
        warnings: &mut Vec<String>,
    ) -> Result<RunStepReport, RunError> {
        let tail = self.metro_tail.as_ref().ok_or_else(|| {
            RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
                code: Some(smix_sdk::FailureCode::DriverError),
                message: "expect.logClean used but no metro log tail attached".into(),
                ..Default::default()
            }))
        })?;
        let violations = tail.assert_clean();
        if violations.is_empty() {
            Ok(RunStepReport::Ok)
        } else {
            for v in &violations {
                warnings.push(format!("log-hygiene: [{:?}] {}", v.level, v.message));
            }
            Err(RunError::Sdk(smix_sdk::ExpectationFailure::new(
                smix_sdk::FailureInit {
                    code: Some(smix_sdk::FailureCode::Timeout),
                    message: format!(
                        "expect.logClean failed: {} non-allowlisted log entr{}",
                        violations.len(),
                        if violations.len() == 1 { "y" } else { "ies" }
                    ),
                    ..Default::default()
                },
            )))
        }
    }

    fn signal_window_to_ml(&self, window: crate::SignalWindow) -> smix_metro_log::Window {
        match window {
            crate::SignalWindow::SinceRun => smix_metro_log::Window::SinceRun,
            crate::SignalWindow::SinceStep { since_step } => {
                let ms = self.step_ms_cursors.get(since_step).copied().unwrap_or(0);
                smix_metro_log::Window::SinceMs(ms)
            }
            crate::SignalWindow::LastMs { last_ms } => smix_metro_log::Window::LastMs(last_ms),
        }
    }

    /// Desugar `Selector::LocalizedText` → `Selector::Text` using
    /// the adapter's `last_locale` state (updated by the last `launchApp`
    /// step's `-AppleLanguages` argument). Returns the picked text via
    /// `Cow::Owned(Selector::Text { ... })` when input is LocalizedText;
    /// borrows the original selector otherwise. Per debug/decomposition-
    /// before-attack: this is a deterministic transform with no fallback
    /// silent dropping — empty table is rejected at parse time, missing
    /// locale falls back to "en", missing "en" falls back to first table
    /// entry by BTreeMap iteration order (alphabetic, deterministic).
    fn desugar_localized_text<'sel>(
        &self,
        selector: &'sel Selector,
    ) -> std::borrow::Cow<'sel, Selector> {
        use std::borrow::Cow;
        if let Selector::LocalizedText {
            localized_text,
            modifiers,
        } = selector
        {
            let text = localized_text
                .get(&self.last_locale)
                .or_else(|| localized_text.get("en"))
                .or_else(|| localized_text.values().next())
                .cloned()
                .unwrap_or_default();
            return Cow::Owned(Selector::Text {
                text: Pattern::Text(text),
                modifiers: modifiers.clone(),
            });
        }
        Cow::Borrowed(selector)
    }

    /// extendedWaitUntil visible with OCR + Fallback dispatch.
    /// Routing every selector through `App::wait_for` would use its
    /// tree resolver, which silently skips `OcrText` inside a
    /// `Fallback` chain (compile returns false, resolver treats it as
    /// never-matching): a `fallback: [id, text, ocrText]` would spend
    /// the whole 45 s timeout on pure `/tree` polls without ever
    /// making a single OCR call.
    ///
    /// Now:
    /// - `Fallback` chain — per poll iteration walk every sub-selector:
    ///   tree-resolvable ones (Id/Text/Label/Role/Anchor/AnchorRelative/
    ///   Focused/LocalizedText/Point) dispatch via `App::find`; OcrText
    ///   dispatches via `App::find_by_text_ocr`. First hit wins.
    ///   OCR members go LAST in each iteration so the cheap tree-based
    ///   layers get a chance first.
    /// - Standalone `OcrText` — poll `find_by_text_ocr` on the same
    ///   250 ms cadence as the driver's `wait_for`.
    /// - Anything else — delegate to `App::wait_for` unchanged.
    ///
    /// Poll cadence matches the driver's `POLL_INTERVAL_MS` (250 ms).
    /// On timeout emits `ExpectationFailure { code: Timeout }` with the
    /// per-layer trace so AI-readable output can show WHICH layer
    /// missed how (`L1 id=btn-…: NOT_FOUND; L2 text=Log in: NOT_FOUND;
    /// L3 ocrText=Log in: NO_OCR_MATCH`).
    async fn wait_for_visible_with_ocr(
        &mut self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), RunError> {
        use std::time::Instant;
        use tokio::time::sleep;
        const POLL: Duration = Duration::from_millis(250);

        // Fast path: no OCR anywhere in the selector → delegate to
        // driver-side `wait_for` unchanged. Detecting "OCR anywhere"
        // covers both top-level `Selector::OcrText` and any `OcrText`
        // hidden inside a `Fallback` chain.
        fn contains_ocr(sel: &Selector) -> bool {
            match sel {
                Selector::OcrText { .. } => true,
                Selector::Fallback { fallback } => fallback.iter().any(contains_ocr),
                _ => false,
            }
        }
        if !contains_ocr(selector) {
            self.app.wait_for(selector, timeout).await?;
            return Ok(());
        }

        let start = Instant::now();
        loop {
            // Fallback chain: try each sub-selector; return on first hit.
            if let Selector::Fallback { fallback } = selector {
                // Try tree-based sub-selectors first (cheaper). OcrText
                // last so tree hits pre-empt OCR cost.
                for sub in fallback.iter() {
                    if matches!(sub, Selector::OcrText { .. }) {
                        continue;
                    }
                    if self.app.find(sub).await.unwrap_or(false) {
                        return Ok(());
                    }
                }
                for sub in fallback.iter() {
                    if let Selector::OcrText {
                        ocr_text, locales, ..
                    } = sub
                    {
                        let eff_locales: Vec<String> = if locales.is_empty() {
                            vec![self.last_locale.clone()]
                        } else {
                            locales.clone()
                        };
                        if self
                            .app
                            .find_by_text_ocr(ocr_text, &eff_locales)
                            .await
                            .ok()
                            .flatten()
                            .is_some()
                        {
                            return Ok(());
                        }
                    }
                }
            } else if let Selector::OcrText {
                ocr_text, locales, ..
            } = selector
            {
                // Standalone OCR waitFor.
                let eff_locales: Vec<String> = if locales.is_empty() {
                    vec![self.last_locale.clone()]
                } else {
                    locales.clone()
                };
                if self
                    .app
                    .find_by_text_ocr(ocr_text, &eff_locales)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
                {
                    return Ok(());
                }
            }

            if start.elapsed() >= timeout {
                // Compose per-layer trace for the Fallback case;
                // singleton trace for standalone OcrText.
                let trace = if let Selector::Fallback { fallback } = selector {
                    fallback
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            format!("L{} {}: MISS", i + 1, smix_sdk::describe_selector(s))
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                } else {
                    format!("{}: MISS", smix_sdk::describe_selector(selector))
                };
                return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::Timeout),
                    message: format!(
                        "extendedWaitUntil.visible({}) timed out after {:?}",
                        smix_sdk::describe_selector(selector),
                        timeout
                    ),
                    selector: Some(selector.clone()),
                    hint: Some(format!(
                        "OCR-aware waitFor exhausted budget. Trace: [{trace}]. \
                         If the a11y tree is degraded (RN Fabric on iOS 26.5), \
                         consider adding `ocrText` last in a `fallback:` chain \
                         so smix falls through to Vision OCR; smix v1.0.22+ \
                         actually fires OCR in this path (pre-v1.0.22 silently \
                         skipped it)."
                    )),
                    ..Default::default()
                })));
            }
            sleep(POLL).await;
        }
    }

    /// Write the frames taken during a press, and refuse to call the
    /// step done when none of them can be placed inside it.
    ///
    /// `captureDuring` promises a frame of the held state. Writing
    /// frames that might be of the resting screen and reporting Ok
    /// would hand back exactly the artefact that misled the consumer
    /// who asked for this — a resting screen read as a pressed one.
    fn write_press_frames(
        &self,
        selector: &Selector,
        capture: &smix_sdk::PressCapture,
    ) -> Result<(), ExpectationFailure> {
        let during = capture.during_press().count();
        if during == 0 {
            let why = capture
                .frames
                .iter()
                .find_map(|f| match &f.placement {
                    smix_driver::FramePlacement::DuringPress => None,
                    smix_driver::FramePlacement::Outside(w)
                    | smix_driver::FramePlacement::Uncertain(w) => Some(w.clone()),
                })
                .unwrap_or_else(|| "no frame was captured at all".to_string());
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!(
                    "longPressOn captureDuring: {} frame(s) captured, none of them \
                     provably while the touch was down — {why}",
                    capture.frames.len()
                ),
                selector: Some(selector.clone()),
                suggestions: vec![
                    "Raise `duration:` — the hold has to outlast one capture (~230ms) \
                     plus resolving the element"
                        .to_string(),
                ],
                ..Default::default()
            }));
        }
        let dir = match self.capture_sink_dir() {
            Some(d) => d,
            None => {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: "longPressOn captureDuring: nowhere to write frames — \
                              neither --debug-output, a working directory, nor a home \
                              directory could be resolved"
                        .to_string(),
                    ..Default::default()
                }));
            }
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("longPressOn captureDuring: mkdir {dir:?} failed: {e}"),
                ..Default::default()
            }));
        }
        let stem = format!("press-{}", capture.timing.sent_ms);
        let mut written: Vec<String> = Vec::new();
        for (i, frame) in capture.frames.iter().enumerate() {
            let label = match frame.placement {
                smix_driver::FramePlacement::DuringPress => "during",
                smix_driver::FramePlacement::Outside(_) => "outside",
                smix_driver::FramePlacement::Uncertain(_) => "unplaced",
            };
            let path = dir.join(format!("{stem}-{i}-{label}.png"));
            match std::fs::write(&path, &frame.png) {
                Ok(()) => written.push(path.display().to_string()),
                Err(e) => eprintln!("WARN: longPressOn captureDuring: write {path:?}: {e}"),
            }
        }
        eprintln!(
            "longPressOn captureDuring: {} frame(s), {during} during the press → {}",
            capture.frames.len(),
            written
                .first()
                .map_or_else(|| dir.display().to_string(), Clone::clone)
        );
        Ok(())
    }

    /// Where artefacts go when the consumer named no directory. Same
    /// three-tier resolution as the timeout dump.
    fn capture_sink_dir(&self) -> Option<PathBuf> {
        if let Some(d) = self.debug_output.as_ref() {
            return Some(d.clone());
        }
        std::env::current_dir()
            .ok()
            .map(|c| c.join(".smix").join("press"))
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share/smix/press")))
    }

    /// Capture screenshot + a11y tree at the moment of a
    /// wait timeout and write to a well-known path so post-mortem
    /// triage always has a visual + tree snapshot. Runs even when the
    /// consumer did NOT pass `--debug-output`. Returns a
    /// human-readable hint containing the written paths (both, either,
    /// or an error explanation on partial failure) suitable for
    /// concatenation into an ExpectationFailure hint.
    ///
    /// Sink path resolution:
    /// 1. If `self.debug_output` is set, write into it — same dir as
    ///    per-step debug artifacts, so consumers with `--debug-output`
    ///    already wired see the timeout capture in the expected place.
    /// 2. Else try `<CWD>/.smix/timeouts/` — same convention as the
    ///    other CLI-scoped smix data (session persistence, flow
    ///    attempts). Preferred for repo-scoped triage; the dir is
    ///    already in most gitignores.
    /// 3. Else fall back to `~/.local/share/smix/timeouts/` — matches
    ///    the runner-sources / flow-attempts sink location.
    async fn capture_timeout_dump(&self, verb: &str) -> Option<String> {
        let ts_ms: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let base_name = format!("timeout-{verb}-{ts_ms}");
        let dir: PathBuf = if let Some(d) = self.debug_output.as_ref() {
            d.clone()
        } else {
            let local = std::env::current_dir()
                .ok()
                .map(|c| c.join(".smix").join("timeouts"));
            let home = dirs::home_dir().map(|h| h.join(".local/share/smix/timeouts"));
            local.or(home)?
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("WARN: timeout capture: mkdir {dir:?} failed: {e}");
            return None;
        }
        let png_path = dir.join(format!("{base_name}.png"));
        let tree_path = dir.join(format!("{base_name}.tree.json"));

        let mut segments: Vec<String> = Vec::new();
        match self.app.screenshot().await {
            Ok(bytes) => match std::fs::write(&png_path, &bytes) {
                Ok(_) => segments.push(format!("screenshot={}", png_path.display())),
                Err(e) => {
                    eprintln!("WARN: v1.0.22 timeout capture: write {png_path:?} failed: {e}");
                }
            },
            Err(e) => {
                eprintln!("WARN: v1.0.22 timeout capture: screenshot failed: {e}");
            }
        }
        match self.app.tree().await {
            Ok(tree) => match serde_json::to_vec_pretty(&tree) {
                Ok(bytes) => match std::fs::write(&tree_path, &bytes) {
                    Ok(_) => segments.push(format!("tree={}", tree_path.display())),
                    Err(e) => {
                        eprintln!("WARN: v1.0.22 timeout capture: write {tree_path:?} failed: {e}")
                    }
                },
                Err(e) => eprintln!("WARN: v1.0.22 timeout capture: tree serialize failed: {e}"),
            },
            Err(e) => {
                eprintln!("WARN: v1.0.22 timeout capture: tree failed: {e}");
            }
        }
        if segments.is_empty() {
            None
        } else {
            Some(segments.join(" "))
        }
    }

    /// Is `OcrText` present anywhere in this selector tree?
    /// Standalone or inside a `Fallback` chain both count. Used by
    /// tapOn / scrollUntilVisible dispatch to decide whether to activate
    /// the OCR-aware polling variant.
    fn selector_contains_ocr(&self, sel: &Selector) -> bool {
        match sel {
            Selector::OcrText { .. } => true,
            Selector::Fallback { fallback } => {
                fallback.iter().any(|s| self.selector_contains_ocr(s))
            }
            _ => false,
        }
    }

    /// scrollUntilVisible with an OCR probe between swipes.
    /// RN 0.86 Fabric LazyColumn/LazyRow on iOS
    /// 26.5 drops off-screen items from the a11y tree, so
    /// `driver.scroll`'s tree-only resolver can never see them. OCR
    /// reads pixels — sees whatever's on screen after each swipe.
    ///
    /// Cadence: probe (tree + OCR if present) → swipe → probe → swipe
    /// until a hit or 30 swipes (matches driver's SCROLL_MAX_SWIPES) or
    /// 20 s wall (matches driver's timeout).
    async fn scroll_until_visible_with_ocr(
        &mut self,
        selector: &Selector,
        direction: smix_sdk::SwipeDirection,
    ) -> Result<(), RunError> {
        use std::time::Instant;
        const MAX_SWIPES: usize = 30;
        let deadline = Instant::now() + Duration::from_secs(20);
        for _ in 0..=MAX_SWIPES {
            // Probe the selector. If it's a Fallback we walk each sub;
            // otherwise probe the standalone selector.
            if self.check_selector_visible(selector).await? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::ElementNotFound),
                    message: format!(
                        "scrollUntilVisible({}, '{:?}'): OCR-aware poll exhausted \
                         after {} swipes / 20 s deadline",
                        smix_sdk::describe_selector(selector),
                        direction,
                        MAX_SWIPES,
                    ),
                    selector: Some(selector.clone()),
                    hint: Some(
                        "Tree probe + OCR probe both missed on every swipe. \
                         Either the target text isn't on any screen the swipe \
                         direction can reveal, OR the OCR text spelling doesn't \
                         match what Apple Vision recognizes. Try a shorter \
                         substring in `ocrText` and re-run."
                            .into(),
                    ),
                    ..Default::default()
                })));
            }
            self.app.scroll_screen(direction).await?;
        }
        Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::ElementNotFound),
            message: format!(
                "scrollUntilVisible({}, '{:?}'): OCR-aware poll exhausted \
                 {} swipe budget",
                smix_sdk::describe_selector(selector),
                direction,
                MAX_SWIPES
            ),
            selector: Some(selector.clone()),
            ..Default::default()
        })))
    }

    /// One-shot visibility probe. Returns Ok(true) if the
    /// selector matches on the current screen (via tree or OCR). Used
    /// by tapOn poll + scrollUntilVisible poll. Uses `App::find` for
    /// tree-based subs and `App::find_by_text_ocr` for OcrText subs.
    async fn check_selector_visible(&mut self, sel: &Selector) -> Result<bool, RunError> {
        match sel {
            Selector::OcrText {
                ocr_text, locales, ..
            } => {
                let eff_locales: Vec<String> = if locales.is_empty() {
                    vec![self.last_locale.clone()]
                } else {
                    locales.clone()
                };
                Ok(self
                    .app
                    .find_by_text_ocr(ocr_text, &eff_locales)
                    .await
                    .ok()
                    .flatten()
                    .is_some())
            }
            Selector::Fallback { fallback } => {
                // Tree-based subs first (cheap), then OCR subs.
                for sub in fallback.iter() {
                    if matches!(sub, Selector::OcrText { .. }) {
                        continue;
                    }
                    if self.app.find(sub).await.unwrap_or(false) {
                        return Ok(true);
                    }
                }
                for sub in fallback.iter() {
                    if let Selector::OcrText {
                        ocr_text, locales, ..
                    } = sub
                    {
                        let eff_locales: Vec<String> = if locales.is_empty() {
                            vec![self.last_locale.clone()]
                        } else {
                            locales.clone()
                        };
                        if self
                            .app
                            .find_by_text_ocr(ocr_text, &eff_locales)
                            .await
                            .ok()
                            .flatten()
                            .is_some()
                        {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            _ => Ok(self.app.find(sel).await.unwrap_or(false)),
        }
    }

    async fn run_tap(
        &mut self,
        selector: &Selector,
        optional: bool,
    ) -> Result<RunStepReport, RunError> {
        // Fallback chain: try each sub-selector in order and tap the
        // first hit. Misses record a per-layer trace for an
        // AI-readable suggestion; an all-miss raises ElementNotFound
        // carrying that chain trace as a hint.
        //
        // When the fallback contains any `OcrText` sub, the whole
        // chain is POLLED within an implicit wait window
        // (`SMIX_TAP_OCR_POLL_MS`, default 3000 ms) instead of trying
        // once and failing fast: on iOS 26.5 + RN 0.86 Fabric the tap
        // moment often races the app's post-transition mount, so OCR
        // misses because Vision snapshots a frame BEFORE the target
        // text is visible, not because it isn't there. Polling closes
        // the race the same way `extendedWaitUntil`'s
        // `wait_for_visible_with_ocr` does. Fast path (no OCR
        // anywhere): single pass, no poll.
        if let Selector::Fallback { fallback } = selector {
            fn contains_ocr(sel: &Selector) -> bool {
                match sel {
                    Selector::OcrText { .. } => true,
                    Selector::Fallback { fallback } => fallback.iter().any(contains_ocr),
                    _ => false,
                }
            }
            let has_ocr = fallback.iter().any(contains_ocr);
            let poll_budget: Duration = if has_ocr {
                Duration::from_millis(
                    std::env::var("SMIX_TAP_OCR_POLL_MS")
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(3000),
                )
            } else {
                // No OCR anywhere: budget = 0 ⇒ single pass.
                Duration::from_millis(0)
            };
            use std::time::Instant;
            use tokio::time::sleep;
            const POLL: Duration = Duration::from_millis(250);
            let start = Instant::now();
            let mut trace: Vec<String> = Vec::with_capacity(fallback.len());
            loop {
                trace.clear();
                for (i, sub) in fallback.iter().enumerate() {
                    let layer = format!("L{}", i + 1);
                    // Recursively dispatch sub-selector through run_tap. Each
                    // sub-selector handles its own miss; we treat
                    // ElementNotFound as "this layer missed, try next" but
                    // propagate other errors immediately.
                    let sub_result = Box::pin(self.run_tap(sub, true /* optional */)).await;
                    match sub_result {
                        Ok(RunStepReport::Ok) => {
                            return Ok(RunStepReport::Ok);
                        }
                        Ok(RunStepReport::Skipped { reason }) => {
                            trace.push(format!(
                                "{layer} {} MISS: {}",
                                smix_sdk::describe_selector(sub),
                                reason
                            ));
                        }
                        Err(e) => return Err(e),
                        _ => {
                            trace.push(format!(
                                "{layer} {} unexpected report",
                                smix_sdk::describe_selector(sub)
                            ));
                        }
                    }
                }
                if start.elapsed() >= poll_budget {
                    break;
                }
                sleep(POLL).await;
            }
            if optional {
                return Ok(RunStepReport::Skipped {
                    reason: format!(
                        "optional tapOn fallback chain: all {} layers missed after {:?} poll; trace=[{}]",
                        fallback.len(),
                        poll_budget,
                        trace.join("; ")
                    ),
                });
            }
            return Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!(
                    "tapOn fallback chain: all {} layers missed after {:?} poll",
                    fallback.len(),
                    poll_budget
                ),
                selector: Some(selector.clone()),
                hint: Some(format!(
                    "Per-layer trace (AI-readable): [{}]. \
                     Suggested fixes: 1) push upstream lib to set \
                     accessibilityIdentifier (L1 hit, fastest+stable); \
                     2) if multi-locale, add localized_text table (L4); \
                     3) if visible text varies, use ocrText (L5). \
                     Avoid L7 absolute coord as default. \
                     v1.0.23 poll budget (SMIX_TAP_OCR_POLL_MS, default 3000 ms) \
                     activates automatically when Fallback contains OCR — bump it \
                     if your post-transition mount is slower than 3 s.",
                    trace.join("; ")
                )),
                ..Default::default()
            })));
        }
        // Point is the last-resort direct coord form: dispatch
        // straight to tap_at_coord (IOHID synthesize, same as
        // Step::TapAtPoint).
        if let Selector::Point { nx, ny } = selector {
            self.app.tap_at_coord(*nx, *ny).await?;
            return Ok(RunStepReport::Ok);
        }
        // AnchorRelative is an escape hatch: find the anchor's norm
        // coord → add the (dx, dy) shift → tap_at_norm_coord (IOHID
        // synthesize). It bypasses the resolver because the target
        // has no a11y form of its own — only the anchor does.
        if let Selector::AnchorRelative { anchor, dx, dy } = selector {
            return match self.app.find_norm_coord(anchor).await? {
                Some((anchor_nx, anchor_ny)) => {
                    let nx = (anchor_nx + dx).clamp(0.0, 1.0);
                    let ny = (anchor_ny + dy).clamp(0.0, 1.0);
                    self.app.tap_at_coord(nx, ny).await?;
                    Ok(RunStepReport::Ok)
                }
                None => {
                    if optional {
                        Ok(RunStepReport::Skipped {
                            reason: format!(
                                "optional tapOn anchored: anchor not found ({})",
                                smix_sdk::describe_selector(anchor)
                            ),
                        })
                    } else {
                        Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                            code: Some(FailureCode::ElementNotFound),
                            message: format!(
                                "tapOn anchored: anchor not found ({})",
                                smix_sdk::describe_selector(anchor)
                            ),
                            selector: Some(selector.clone()),
                            hint: Some(
                                "anchored coord requires the anchor sub-selector to \
                                 resolve to exactly one node; check anchor selector"
                                    .into(),
                            ),
                            ..Default::default()
                        })))
                    }
                }
            };
        }
        // OcrText dispatches straight through App::find_by_text_ocr →
        // tap_at_norm_coord (IOHID synthesize, same as the
        // /tap-at-norm-coord path). It bypasses the resolver and is
        // NOT desugared into Selector::Text, because an element OCR
        // can see may not exist in the a11y tree at all. Empty
        // `locales` falls back to the adapter's last_locale.
        if let Selector::OcrText {
            ocr_text, locales, ..
        } = selector
        {
            let effective_locales: Vec<String> = if locales.is_empty() {
                vec![self.last_locale.clone()]
            } else {
                locales.clone()
            };
            return match self
                .app
                .find_by_text_ocr(ocr_text, &effective_locales)
                .await?
            {
                Some(frame) => {
                    self.app.tap_at_coord(frame.mid_x(), frame.mid_y()).await?;
                    Ok(RunStepReport::Ok)
                }
                None => {
                    if optional {
                        Ok(RunStepReport::Skipped {
                            reason: format!(
                                "optional tapOn ocrText: OCR found no match for \"{ocr_text}\" (locales={effective_locales:?})"
                            ),
                        })
                    } else {
                        Err(RunError::Sdk(ExpectationFailure::new(FailureInit {
                            code: Some(FailureCode::ElementNotFound),
                            message: format!(
                                "tapOn ocrText: OCR found no match for \"{ocr_text}\""
                            ),
                            selector: Some(selector.clone()),
                            hint: Some(
                                "Apple Vision OCR returned 0 matching observations; check \
                                 spelling / recognition language / surface contrast"
                                    .into(),
                            ),
                            ..Default::default()
                        })))
                    }
                }
            };
        }
        // Desugar LocalizedText → Text by last_locale before all
        // routing below.
        let desugared = self.desugar_localized_text(selector);
        let selector: &Selector = &desugared;
        // SwiftUI modal dismiss ids route via App::tap_xcui (swift
        // /tap-by-id → XCUIElement-anchored coord.tap) to match the
        // SDK self-path. This branch exists for capability parity —
        // the adapter must not quietly downgrade an SDK capability —
        // and does NOT address the underlying iOS 17+ behavior where a
        // SwiftUI .sheet / .alert / .confirmationDialog /
        // .fullScreenCover binding doesn't fire from
        // XCUIElement.tap(). Only Id-form selectors with default
        // modifiers route through tap_xcui; Text/Label/Role and
        // Id-with-modifiers selectors keep the default tap path.
        if let Selector::Id { id, modifiers } = selector
            && modifiers == &smix_sdk::Modifiers::default()
            && is_swiftui_modal_dismiss_id(id)
        {
            return match self.app.tap_xcui(id).await {
                Ok(()) => Ok(RunStepReport::Ok),
                Err(e) if optional && e.code == FailureCode::ElementNotFound => {
                    Ok(RunStepReport::Skipped {
                        reason: format!(
                            "optional tapOn (modal-dismiss tap_xcui): element not found ({}); skipped",
                            smix_sdk::describe_selector(selector)
                        ),
                    })
                }
                Err(e) => Err(RunError::Sdk(e)),
            };
        }
        // Auto-scroll on tab tap: SwiftUI tab-bar ids (`v2-tab-*`)
        // route via App::tap_xcui so the tap survives a fixture where
        // the bar wraps into a ScrollView. maestro CLI's `tapOn id:`
        // is static, so this routing keeps the adapter ahead of
        // maestro on the tab-navigation path. Deliberately scoped to
        // the SwiftUI tab namespace: tap_xcui does not fire onPress on
        // an RN Pressable, and RN is not in the fixture id space, so
        // widening this branch would risk that regression.
        if let Selector::Id { id, modifiers } = selector
            && modifiers == &smix_sdk::Modifiers::default()
            && is_swiftui_navigation_or_tab_id(id)
        {
            return match self.app.tap_xcui(id).await {
                Ok(()) => Ok(RunStepReport::Ok),
                Err(e) if optional && e.code == FailureCode::ElementNotFound => {
                    Ok(RunStepReport::Skipped {
                        reason: format!(
                            "optional tapOn (nav-tab tap_xcui): element not found ({}); skipped",
                            smix_sdk::describe_selector(selector)
                        ),
                    })
                }
                Err(e) => Err(RunError::Sdk(e)),
            };
        }
        let result = self.app.tap(selector).await;
        match result {
            Ok(()) => Ok(RunStepReport::Ok),
            Err(e) if optional && e.code == FailureCode::ElementNotFound => {
                Ok(RunStepReport::Skipped {
                    reason: format!(
                        "optional tapOn: element not found ({}); skipped",
                        smix_sdk::describe_selector(selector)
                    ),
                })
            }
            Err(e) => Err(RunError::Sdk(e)),
        }
    }

    async fn expand_subflow(
        &mut self,
        rel: &str,
        warnings: &mut Vec<String>,
    ) -> Result<RunStepReport, RunError> {
        // The `std/<name>.yaml` prefix resolves to the
        // shipped std-subflow catalogue. Priority:
        //   1. <cwd>/std/<name>.yaml (consumer override)
        //   2. <SMIX_STD_SUBFLOWS>/<name>.yaml (env override)
        //   3. ~/.local/share/smix/std/<name>.yaml (install shipped)
        //   4. <crate>/std/<name>.yaml (source tree, dev)
        let raw = if let Some(name) = rel.strip_prefix("std/") {
            resolve_std_subflow(name, &self.base_dir).ok_or_else(|| {
                RunError::Io(format!(
                    "std subflow `{name}` not found in cwd/std/, $SMIX_STD_SUBFLOWS, ~/.local/share/smix/std/, or source tree"
                ))
            })?
        } else {
            self.base_dir.join(rel)
        };
        let abs = match raw.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return Err(RunError::Io(format!("canonicalize {}: {e}", raw.display())));
            }
        };

        if self.run_stack.contains(&abs) {
            return Err(RunError::RunFlowCycle(abs));
        }

        let parsed = match parse_flow_file(&abs) {
            Ok(f) => f,
            Err(ParseError::UnsupportedCommand(cmd)) => {
                let reason = format!(
                    "subflow {} contains unsupported command `{}`; skipped (§9 #5)",
                    abs.display(),
                    cmd
                );
                warnings.push(reason.clone());
                return Ok(RunStepReport::Skipped { reason });
            }
            Err(other) => return Err(RunError::Parse(other)),
        };

        let new_base = abs
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.base_dir.clone());
        let saved_base = std::mem::replace(&mut self.base_dir, new_base);
        self.run_stack.push(abs.clone());

        // Box::pin breaks the async recursion cycle (expand_subflow → run_steps
        // → run_step → expand_subflow) — without it the future type would be
        // infinitely sized.
        let result = Box::pin(self.run_steps(&parsed.steps)).await;

        self.run_stack.pop();
        self.base_dir = saved_base;

        let inner = result?;
        Ok(RunStepReport::ExpandedSubflow {
            path: abs,
            inner: Box::new(inner),
        })
    }
}

// ----------------------------------------------------------------------
// Helper parsers
// ----------------------------------------------------------------------

/// Map the maestro yaml `launchApp.permissions.<name>` string to a
/// typed [`SimctlPermission`]. Never guesses: an unknown permission
/// name raises [`ParseError::InvalidValue`] listing the full supported
/// set rather than silently no-op'ing.
fn parse_simctl_permission(name: &str) -> Result<SimctlPermission, ParseError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "camera" => Ok(SimctlPermission::Camera),
        "photos" => Ok(SimctlPermission::Photos),
        "location" => Ok(SimctlPermission::Location),
        "location-always" | "locationalways" => Ok(SimctlPermission::LocationAlways),
        "notifications" => Ok(SimctlPermission::Notifications),
        "microphone" => Ok(SimctlPermission::Microphone),
        "contacts" => Ok(SimctlPermission::Contacts),
        "calendar" => Ok(SimctlPermission::Calendar),
        "reminders" => Ok(SimctlPermission::Reminders),
        "media" | "media-library" => Ok(SimctlPermission::Media),
        "motion" => Ok(SimctlPermission::Motion),
        "homekit" => Ok(SimctlPermission::HomeKit),
        "health" => Ok(SimctlPermission::Health),
        "bluetooth" => Ok(SimctlPermission::Bluetooth),
        "faceid" => Ok(SimctlPermission::Faceid),
        other => Err(ParseError::InvalidValue {
            field: format!("launchApp.permissions.{other}"),
            reason: format!(
                "unknown permission name '{other}' — supported: camera, photos, \
                 location, location-always, notifications, microphone, contacts, \
                 calendar, reminders, media (media-library), motion, homekit, \
                 health, bluetooth, faceid"
            ),
        }),
    }
}

fn parse_key_name(s: &str) -> Result<KeyName, RunError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "enter" | "return" => Ok(KeyName::Return),
        // "back" is deliberately NOT an alias: maestro's `back` is
        // navigation, and aliasing it to Delete once turned every
        // `- back` step into a silent backspace that reported success.
        "delete" | "backspace" => Ok(KeyName::Delete),
        "tab" => Ok(KeyName::Tab),
        "space" => Ok(KeyName::Space),
        "escape" | "esc" => Ok(KeyName::Escape),
        // The underscored spellings exist for the same reason the
        // volume ones do: the guides write key names in SCREAMING_SNAKE
        // (`VOLUME_UP`), `to_ascii_lowercase` leaves the underscore, and
        // a reader following that convention for the arrows landed on
        // "unknown key".
        "arrowup" | "arrow_up" | "up" => Ok(KeyName::ArrowUp),
        "arrowdown" | "arrow_down" | "down" => Ok(KeyName::ArrowDown),
        "arrowleft" | "arrow_left" | "left" => Ok(KeyName::ArrowLeft),
        "arrowright" | "arrow_right" | "right" => Ok(KeyName::ArrowRight),
        // iOS hardware keys (full maestro yaml key coverage):
        // home / lock go through
        // XCUIDevice.shared.perform(.homeButton/.lockButton); volume
        // up / volume down go through
        // XCUIDevice.Button.volumeUp/volumeDown.
        // to_ascii_lowercase() leaves spaces intact, and maestro
        // documents `pressKey: volume up` as a real format — hence a
        // separate arm for each of the spaced / underscored / joined
        // spellings.
        "home" => Ok(KeyName::Home),
        "lock" => Ok(KeyName::Lock),
        "volumeup" | "volume up" | "volume_up" => Ok(KeyName::VolumeUp),
        "volumedown" | "volume down" | "volume_down" => Ok(KeyName::VolumeDown),
        _ => Err(RunError::UnknownKey(s.to_string())),
    }
}

fn parse_swipe_direction(s: &str) -> Result<SwipeDirection, RunError> {
    match s.trim().to_ascii_lowercase().as_str() {
        "up" => Ok(SwipeDirection::Up),
        "down" => Ok(SwipeDirection::Down),
        "left" => Ok(SwipeDirection::Left),
        "right" => Ok(SwipeDirection::Right),
        _ => Err(RunError::UnknownDirection(s.to_string())),
    }
}

/// Resolve `std/<name>.yaml` against the shipped
/// subflow catalogue. Priority:
///   1. `<base_dir>/std/<name>.yaml` (consumer override, wins)
///   2. `$SMIX_STD_SUBFLOWS/<name>.yaml`
///   3. `~/.local/share/smix/std/<name>.yaml` (install-local.sh target)
///   4. `<crate>/std/<name>.yaml` (source-tree default, for dev)
fn resolve_std_subflow(name: &str, base_dir: &Path) -> Option<PathBuf> {
    let file = name;
    let candidates = [
        base_dir.join("std").join(file),
        std::env::var_os("SMIX_STD_SUBFLOWS")
            .map(|p| PathBuf::from(p).join(file))
            .unwrap_or_default(),
        dirs::home_dir()
            .map(|h| h.join(".local/share/smix/std").join(file))
            .unwrap_or_default(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("smix-cli/std").join(file))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|c| c.exists())
}
