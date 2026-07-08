//! v6.0 c3a — Android `Driver` impl skeleton.
//!
//! Wraps [`HttpRunnerClient`] talking to the Android-side Kotlin runner
//! (v6.0 c3b — APK + KTOR HTTP server backed by UiAutomator2). The
//! runner is reached via `adb forward tcp:HOST tcp:DEVICE` so host-side
//! HTTP transport is identical to iOS.
//!
//! **State** — c3a ships trait skeleton: all 26 sense+act methods
//! compile + return either (a) a transparent delegation to the runner
//! if the wire shape is reusable, or (b) an explicit "v6.0 c3b runner
//! not yet shipped" error so failures are visible (not silent).
//!
//! Acceptance gated by v6.0 c3b (Kotlin runner APK install + booted
//! emulator). c3a is unit-tested via `Box<dyn Driver>` dyn_compat +
//! platform=Android probes; end-to-end smoke lands at c3b.

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
}

impl AndroidDriver {
    #[must_use]
    pub fn new(runner: HttpRunnerClient) -> Self {
        AndroidDriver { runner }
    }

    pub fn runner(&self) -> &HttpRunnerClient {
        &self.runner
    }
}

/// v6.0 c3c-v — host-resolve loop with 5s implicit-wait + 250ms poll.
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

fn defer_err(method: &str) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message: format!(
            "AndroidDriver::{method}: Kotlin runner endpoint not yet shipped (v6.0 c3b earned defer)"
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

    // === Sense ===

    async fn tree(&self, include: Option<IncludeScope>) -> Result<A11yNode, ExpectationFailure> {
        // v6.0 c3c-ii — delegates to Kotlin runner GET /tree (UiAutomator2
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
        // v6.0 c3c-iii — host-resolve over tree (Kotlin runner /find route
        // not needed; tree dump 已含全树).
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
        // v6.3 c2 — Google ML Kit Text Recognition (Latin script package).
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
        // v6.3 c3 — Kotlin /system-popups walks UiAutomation.windows and
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
        // v6.3 c3 — Kotlin /system-popup-action re-walks windows + finds
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
        // v6.0 c3c-iii — poll tree at 250ms cadence up to `timeout`, return
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
        // v6.0 c3c-iii — host-resolve + tap_at_norm_coord (mirror IosDriver
        // Path B). v6.0 c3c-v — refactored to shared helper.
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
        Err(defer_err("tap_with_mode"))
    }

    async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        // v6.0 c3c-iii — direct passthru to Kotlin runner /tap-at-norm-coord.
        self.runner.tap_at_norm_coord(nx, ny).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::tap_at_norm_coord: {e}"),
                ..Default::default()
            })
        })
    }

    async fn tap_by_id(&self, id: &str) -> Result<(), ExpectationFailure> {
        // v6.0 c3c-v — POST /tap-by-id with {id}. Kotlin side finds
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
        // v6.0 c3c-v — host-resolve + /double-tap-at-norm-coord (Kotlin
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
        // v6.0 c3c-v — host-resolve + /long-press-at-norm-coord with
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

    async fn fill(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        // v6.0 c3c-v — host-resolve → tap to focus → /input-text. Mirror
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
        // v6.0 c3c-v — host-resolve → tap to focus → press DELETE N times.
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
        // v6.0 c3c-iv — Kotlin /press-key maps smix KeyName → KeyEvent.KEYCODE_*.
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
        // v6.0 c3c-iv — host-side scroll-until-visible loop. /tree +
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
        // v6.0 c3c-iv — Kotlin /back → UiDevice.pressBack (KEYCODE_BACK).
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
        // v6.3 c1 — Kotlin /foreground runs `am start --activity-single-top
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
        // v6.5 c1 — runner /webview-eval proxies HTTP to fixture's shim
        // server (WebViewEvalServer on :28081, started by MainActivity
        // onCreate). evaluateJavascript callback result is a JSON-encoded
        // string (e.g. "null", "\"hello\"", "42") passed verbatim.
        self.runner.webview_eval(js).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::webview_eval: {e}"),
                ..Default::default()
            })
        })
    }
}
