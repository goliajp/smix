//! iOS `Driver` impl. Pure delegation layer onto the existing
//! `IosDriver` (renamed from `SimctlDriver`) inherent methods in lib.rs.
//!
//! 26 trait methods → 26 1-line delegations. No new logic.

use async_trait::async_trait;
use std::time::Duration;

use smix_error::ExpectationFailure;
use smix_input::{KeyName, SwipeDirection};
use smix_runner_client::{IncludeScope, OcrFrame, SystemPopup, TapMode};
use smix_screen::A11yNode;
use smix_selector::Selector;

use crate::traits::{Driver, Platform};
use crate::{IosDriver, Orientation};

#[async_trait]
impl Driver for IosDriver {
    fn platform(&self) -> Platform {
        Platform::Ios
    }

    fn as_ios_driver(&self) -> Option<&IosDriver> {
        Some(self)
    }

    /// iOS impl: mutate the wrapped `HttpRunnerClient` so every
    /// subsequent request carries the `App-Bundle-Id` header. Runner
    /// side rebinds `XCUIApplication(bundleIdentifier:)` per request.
    fn set_target_bundle_id(&mut self, bundle: &str) {
        self.runner_mut().set_target_bundle_id(bundle);
    }

    /// iOS impl: mutate the wrapped client to send
    /// `App-Activate: true` on every request.
    fn set_auto_activate(&mut self, activate: bool) {
        self.runner_mut().set_auto_activate(activate);
    }

    /// iOS impl: force key-event dispatch mode on the
    /// wrapped client. Sends `Input-Dispatch-Mode: key-events` header
    /// on every request; runner-side dispatches via daemon key events
    /// instead of a11y-anchored XCUIElement.typeText.
    fn set_force_key_events(&mut self, force: bool) {
        use smix_runner_client::InputDispatchMode;
        if force {
            self.runner_mut()
                .set_input_dispatch_mode(InputDispatchMode::KeyEvents);
        }
    }

    /// iOS impl: attach / clear the `Session-Id` header on
    /// every subsequent request. When set, the runner short-circuits
    /// per-request activation and reuses the session-cached
    /// XCUIApplication binding.
    fn set_session_id(&mut self, id: Option<String>) {
        match id {
            Some(sid) => self.runner_mut().set_session_id(sid),
            None => self.runner_mut().clear_session_id(),
        }
    }

    // === Sense ===

    async fn tree(&self, include: Option<IncludeScope>) -> Result<A11yNode, ExpectationFailure> {
        IosDriver::tree(self, include).await
    }

    async fn find(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<bool, ExpectationFailure> {
        IosDriver::find(self, selector, include).await
    }

    async fn find_one(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Option<A11yNode>, ExpectationFailure> {
        IosDriver::find_one(self, selector, include).await
    }

    async fn find_all(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Vec<A11yNode>, ExpectationFailure> {
        IosDriver::find_all(self, selector, include).await
    }

    async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        IosDriver::find_norm_coord(self, selector).await
    }

    async fn find_text_by_ocr(
        &self,
        text: &str,
        locales: &[String],
        recognition_level: &str,
    ) -> Result<Option<OcrFrame>, ExpectationFailure> {
        IosDriver::find_text_by_ocr(self, text, locales, recognition_level).await
    }

    async fn system_popups(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<Vec<SystemPopup>, ExpectationFailure> {
        IosDriver::system_popups(self, include).await
    }

    async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        IosDriver::system_popup_action(self, popup_id, button_id).await
    }

    async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
        include: Option<IncludeScope>,
    ) -> Result<A11yNode, ExpectationFailure> {
        IosDriver::wait_for(self, selector, timeout, include).await
    }

    // === Act ===

    async fn tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<crate::ActOutcome, ExpectationFailure> {
        IosDriver::tap(self, selector, include).await
    }

    async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: TapMode,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        IosDriver::tap_with_mode(self, selector, mode, include).await
    }

    async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        IosDriver::tap_at_norm_coord(self, nx, ny).await
    }

    async fn tap_by_id(&self, id: &str) -> Result<(), ExpectationFailure> {
        IosDriver::tap_by_id(self, id).await
    }

    async fn double_tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        IosDriver::double_tap(self, selector, include).await
    }

    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        IosDriver::long_press(self, selector, duration, include).await
    }

    async fn fill(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        IosDriver::fill(self, selector, text, include).await
    }

    async fn clear(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        IosDriver::clear(self, selector, include).await
    }

    async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure> {
        IosDriver::press_key(self, key).await
    }

    async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure> {
        IosDriver::scroll(self, selector, direction).await
    }

    async fn swipe_once(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        IosDriver::swipe_once(self, direction).await
    }

    async fn swipe_at_norm_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure> {
        IosDriver::swipe_at_norm_coord(self, from, to).await
    }

    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        IosDriver::hide_keyboard(self).await
    }

    async fn back(&self) -> Result<(), ExpectationFailure> {
        IosDriver::back(self).await
    }

    async fn set_orientation(&self, orientation: Orientation) -> Result<(), ExpectationFailure> {
        IosDriver::set_orientation(self, orientation).await
    }

    async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        IosDriver::foreground(self, bundle_id).await
    }

    async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure> {
        IosDriver::webview_eval(self, js).await
    }
}
