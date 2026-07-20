//! Android `Driver` impl.
//!
//! Wraps [`HttpRunnerClient`] talking to the Android-side Kotlin runner
//! (an APK running a KTOR HTTP server backed by UiAutomator2). The
//! runner is reached via `adb forward tcp:HOST tcp:DEVICE` so host-side
//! HTTP transport is identical to iOS.
//!
//! Each of the 26 sense+act methods either delegates transparently to
//! the runner where the wire shape is reusable, or returns an explicit
//! "endpoint not yet shipped" error — failures are visible, never
//! silent.
//!
//! End-to-end acceptance needs a Kotlin runner APK installed on a
//! booted emulator; the host side is unit-tested via `Box<dyn Driver>`
//! dyn-compatibility and platform=Android probes.

use async_trait::async_trait;
use std::time::Duration;

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_host_coord_resolver::{HostResolveError, resolve_to_norm_coord};
use smix_input::{KeyName, SwipeDirection};
use smix_runner_client::{HttpRunnerClient, IncludeScope, OcrFrame, SystemPopup, TapMode};
use smix_screen::A11yNode;
use smix_selector::{Selector, describe_selector};
use smix_selector_resolver::{resolve_selector, resolve_selector_all};

use crate::Orientation;
use crate::traits::{Driver, Platform};

/// Android `Driver` impl. Wraps `HttpRunnerClient` connecting to the
/// Kotlin runner via adb-forwarded port (default 28080, configurable
/// via `AndroidDriver::new(port)`).
pub struct AndroidDriver {
    runner: HttpRunnerClient,
    /// Skip host-side focus resolution and type into whatever holds
    /// focus. Android has no runner-side dispatch switch to send —
    /// `/input-text` already types into the focused field — so the
    /// mode is honoured here, by not resolving.
    force_key_events: bool,
}

impl AndroidDriver {
    #[must_use]
    pub fn new(runner: HttpRunnerClient) -> Self {
        AndroidDriver {
            runner,
            force_key_events: false,
        }
    }

    pub fn runner(&self) -> &HttpRunnerClient {
        &self.runner
    }
}

/// Host-resolve loop with 5s implicit-wait + 250ms poll.
/// Returns viewport-normalized centroid coord. Shared by tap / double_tap
/// / long_press / fill / clear.
async fn resolve_with_implicit_wait(
    driver: &AndroidDriver,
    selector: &Selector,
    include: Option<IncludeScope>,
) -> Result<(f64, f64), ExpectationFailure> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(5000);
    loop {
        let tree = driver.tree(include).await?;
        match resolve_to_norm_coord(&tree, selector) {
            Ok(coord) => return Ok(coord),
            Err(HostResolveError::NotFound) => {
                if start.elapsed() > timeout {
                    return Err(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::ElementNotFound),
                        message: format!(
                            "AndroidDriver: element not found: {}",
                            describe_selector(selector)
                        ),
                        selector: Some(selector.clone()),
                        ..Default::default()
                    }));
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            Err(e) => {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver: resolve error: {e:?}"),
                    selector: Some(selector.clone()),
                    ..Default::default()
                }));
            }
        }
    }
}

/// `dispatch:` overrides are an iOS-runner mechanism.
///
/// The guide says this "errors with an explicit unsupported message";
/// what it actually said was "not implemented by the Kotlin runner",
/// which reads as a missing feature someone should wait for. There is
/// nothing to wait for: Android's default tap already IS native event
/// synthesis, which is what the override buys on iOS. The fix is to
/// drop the key, so the error says that.
fn dispatch_unsupported_err() -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message: "tapOn `dispatch:` is an iOS-runner mechanism and has no \
                  meaning on Android"
            .to_string(),
        hint: Some(
            "remove `dispatch:` from this step — Android taps already use \
             native event synthesis, which is what the override selects on iOS"
                .to_string(),
        ),
        ..Default::default()
    })
}

#[async_trait]
impl Driver for AndroidDriver {
    fn platform(&self) -> Platform {
        Platform::Android
    }

    // No as_ios_driver override — uses default `None` from trait.

    /// Android impl: attach / clear the `Session-Id` header on every
    /// subsequent request. Header-only: the Kotlin runner serves no
    /// `/session/*` routes and keeps no session table — it ignores the
    /// header, which is why Android drives sessionless.
    fn set_session_id(&mut self, id: Option<String>) {
        match id {
            Some(sid) => self.runner.set_session_id(sid),
            None => self.runner.clear_session_id(),
        }
    }

    // === Sense ===

    async fn tree(&self, include: Option<IncludeScope>) -> Result<A11yNode, ExpectationFailure> {
        // Delegates to Kotlin runner GET /tree (UiAutomator2
        // dumpWindowHierarchy → A11yNode JSON shape). HttpRunnerClient
        // is platform-agnostic; same wire as iOS.
        self.runner.get_tree(include).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::tree: {e}"),
                ..Default::default()
            })
        })
    }

    async fn find(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<bool, ExpectationFailure> {
        // Host-resolve over tree (the Kotlin runner /find route is not
        // needed; the tree dump already contains the whole tree).
        let tree = self.tree(include).await?;
        Ok(resolve_selector(&tree, selector).is_some())
    }

    async fn find_one(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Option<A11yNode>, ExpectationFailure> {
        let tree = self.tree(include).await?;
        Ok(resolve_selector(&tree, selector).cloned())
    }

    async fn find_all(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Vec<A11yNode>, ExpectationFailure> {
        let tree = self.tree(include).await?;
        Ok(resolve_selector_all(&tree, selector)
            .into_iter()
            .cloned()
            .collect())
    }

    async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        let tree = self.tree(None).await?;
        match resolve_to_norm_coord(&tree, selector) {
            Ok(coord) => Ok(Some(coord)),
            Err(HostResolveError::NotFound | HostResolveError::EmptyMatchedFrame) => Ok(None),
            Err(HostResolveError::UnknownAppFrame) => Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: "AndroidDriver::find_norm_coord: tree bounds w/h ≤ 0 (unknown app frame)"
                    .into(),
                ..Default::default()
            })),
            Err(HostResolveError::CentroidOutOfFrame { .. }) => Ok(None),
        }
    }

    async fn find_text_by_ocr(
        &self,
        text: &str,
        locales: &[String],
        recognition_level: &str,
    ) -> Result<Option<OcrFrame>, ExpectationFailure> {
        // Google ML Kit Text Recognition (Latin script package).
        // Locales + recognition_level args are iOS Apple Vision specific;
        // Kotlin /find-text-by-ocr endpoint reads + ignores them today
        // (ML Kit Latin handles ASCII/European text universally).
        self.runner
            .find_text_by_ocr(text, locales, recognition_level)
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::find_text_by_ocr: {e}"),
                    ..Default::default()
                })
            })
    }

    async fn system_popups(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<Vec<SystemPopup>, ExpectationFailure> {
        // Kotlin /system-popups walks UiAutomation.windows and
        // classifies dialog-shaped TYPE_APPLICATION windows. Returns
        // envelope {popups: [...]} per HttpRunnerClient deserialization.
        self.runner.system_popups(include).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::system_popups: {e}"),
                ..Default::default()
            })
        })
    }

    async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        // Kotlin /system-popup-action re-walks windows + finds
        // popup by id + button by testTag-derived id + UiDevice.click on
        // its bounding box center.
        self.runner
            .system_popup_action(popup_id, button_id)
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::system_popup_action: {e}"),
                    ..Default::default()
                })
            })
    }

    async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
        include: Option<IncludeScope>,
    ) -> Result<A11yNode, ExpectationFailure> {
        // Poll tree at 250ms cadence up to `timeout`, return
        // matched node on first hit. Mirror of iOS wait_for semantics.
        let start = std::time::Instant::now();
        loop {
            let tree = self.tree(include).await?;
            if let Some(node) = resolve_selector(&tree, selector) {
                return Ok(node.clone());
            }
            if start.elapsed() >= timeout {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::ElementNotFound),
                    message: format!(
                        "AndroidDriver::wait_for timeout after {}ms: {}",
                        timeout.as_millis(),
                        describe_selector(selector)
                    ),
                    selector: Some(selector.clone()),
                    ..Default::default()
                }));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    // === Act ===

    async fn tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        // Host-resolve + tap_at_norm_coord (mirrors IosDriver Path B).
        let (nx, ny) = resolve_with_implicit_wait(self, selector, include).await?;
        self.runner.tap_at_norm_coord(nx, ny).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::tap: runner.tap_at_norm_coord: {e}"),
                ..Default::default()
            })
        })
    }

    async fn tap_with_mode(
        &self,
        _selector: &Selector,
        _mode: TapMode,
        _include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        Err(dispatch_unsupported_err())
    }

    async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        // Direct passthru to Kotlin runner /tap-at-norm-coord.
        self.runner.tap_at_norm_coord(nx, ny).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::tap_at_norm_coord: {e}"),
                ..Default::default()
            })
        })
    }

    async fn tap_by_id(&self, id: &str) -> Result<(), ExpectationFailure> {
        // POST /tap-by-id with {id}. Kotlin side finds
        // UiObject2 via By.res(short or fully-qualified) and clicks.
        let ok = self.runner.tap_by_id(id).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::tap_by_id: {e}"),
                ..Default::default()
            })
        })?;
        if !ok {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("AndroidDriver::tap_by_id: no element with resource-id '{id}'"),
                ..Default::default()
            }));
        }
        Ok(())
    }

    async fn double_tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        // Host-resolve + /double-tap-at-norm-coord (Kotlin
        // side dispatches 2 clicks 150ms apart).
        let (nx, ny) = resolve_with_implicit_wait(self, selector, include).await?;
        self.runner
            .double_tap_at_norm_coord(nx, ny)
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::double_tap: {e}"),
                    ..Default::default()
                })
            })
    }

    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        // Host-resolve + /long-press-at-norm-coord with
        // duration. Kotlin uses UiDevice.swipe(x,y,x,y,steps) where
        // steps = duration / 5ms to approximate a sustained press.
        let (nx, ny) = resolve_with_implicit_wait(self, selector, include).await?;
        let duration_ms = duration.as_millis() as u64;
        self.runner
            .long_press_at_norm_coord(nx, ny, duration_ms)
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::long_press: {e}"),
                    ..Default::default()
                })
            })
    }

    /// Android honours `key-events` by skipping focus resolution.
    ///
    /// The iOS driver sends `Input-Dispatch-Mode` to its runner; there
    /// is nothing to send here, because `/input-text` types into the
    /// focused field either way. What the mode changes is whether this
    /// driver resolves and taps the field first — and resolving is
    /// exactly what fails for the callers who ask for this mode.
    fn set_force_key_events(&mut self, force: bool) {
        self.force_key_events = force;
    }

    async fn fill(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        if self.force_key_events {
            // No resolve, no focus tap: type where focus already is.
            // That is the whole mode — it exists for fields the tree
            // cannot address, so resolving first would fail for exactly
            // the callers who asked for it.
            return self.runner.input_text(text).await.map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::fill (key-events): {e}"),
                    ..Default::default()
                })
            });
        }
        // Host-resolve → tap to focus → /input-text. Mirror
        // of swift FlyingFox /fill semantics (selector resolves; client
        // types text into focused field).
        let (nx, ny) = resolve_with_implicit_wait(self, selector, include).await?;
        self.runner.tap_at_norm_coord(nx, ny).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::fill: focus tap failed: {e}"),
                ..Default::default()
            })
        })?;
        self.runner.input_text(text).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::fill: input_text failed: {e}"),
                ..Default::default()
            })
        })
    }

    async fn clear(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        // Host-resolve → tap to focus → press DELETE N times.
        // No Kotlin /clear endpoint needed (avoids UiObject2 fragility);
        // 50 BACKSPACE presses cover near all real-world input fields.
        let (nx, ny) = resolve_with_implicit_wait(self, selector, include).await?;
        self.runner.tap_at_norm_coord(nx, ny).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::clear: focus tap failed: {e}"),
                ..Default::default()
            })
        })?;
        for _ in 0..50 {
            self.runner.press_key(KeyName::Delete).await.map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::clear: delete press failed: {e}"),
                    ..Default::default()
                })
            })?;
        }
        Ok(())
    }

    async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure> {
        // Kotlin /press-key maps smix KeyName → KeyEvent.KEYCODE_*.
        self.runner.press_key(key).await.map(|_| ()).map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::press_key: {e}"),
                ..Default::default()
            })
        })
    }

    async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure> {
        // Host-side scroll-until-visible loop. /tree +
        // /swipe-once primitives — same pattern as iOS.
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(20);
        const MAX_SWIPES: u32 = 30;
        for _ in 0..=MAX_SWIPES {
            let tree = self.tree(None).await?;
            if resolve_selector(&tree, selector).is_some() {
                return Ok(());
            }
            if start.elapsed() > timeout {
                break;
            }
            self.swipe_once(direction).await?;
        }
        Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::ElementNotFound),
            message: format!(
                "AndroidDriver::scroll: element not visible after {} swipes: {}",
                MAX_SWIPES,
                describe_selector(selector)
            ),
            selector: Some(selector.clone()),
            ..Default::default()
        }))
    }

    async fn swipe_once(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        self.runner.swipe_once(direction).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::swipe_once: {e}"),
                ..Default::default()
            })
        })
    }

    async fn swipe_at_norm_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure> {
        self.runner
            .swipe_at_norm_coord(from, to)
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::swipe_at_norm_coord: {e}"),
                    ..Default::default()
                })
            })
    }

    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        self.runner.hide_keyboard().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::hide_keyboard: {e}"),
                ..Default::default()
            })
        })
    }

    async fn back(&self) -> Result<(), ExpectationFailure> {
        // Kotlin /back → UiDevice.pressBack (KEYCODE_BACK).
        self.runner.back().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::back: {e}"),
                ..Default::default()
            })
        })
    }

    async fn set_orientation(&self, orientation: Orientation) -> Result<(), ExpectationFailure> {
        self.runner
            .set_orientation(orientation.as_wire())
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::set_orientation: {e}"),
                    ..Default::default()
                })
            })
    }

    async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        // Kotlin /foreground runs `am start --activity-single-top
        // -n pkg/.MainActivity` (mirror iOS XCUIDevice activate semantic
        // without launching a new instance).
        self.runner.foreground(bundle_id).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::foreground: {e}"),
                ..Default::default()
            })
        })
    }

    async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure> {
        // The Kotlin runner's /webview-eval proxies to the app's shim on
        // :28081 — the emulator's loopback is not the host's, so the
        // direct-bridge method (which dials 127.0.0.1:28080 on the HOST)
        // could never reach an Android app. This comment used to claim
        // the proxy while the code dialed the host port.
        self.runner.webview_eval_via_runner(js).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::webview_eval: {e}"),
                ..Default::default()
            })
        })
    }
}
