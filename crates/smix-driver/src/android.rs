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
use smix_screen::{A11yNode, DEFAULT_VISIBLE_LIMIT, collect_visible_summaries, screen_facts};
use smix_selector::{Selector, describe_selector};
use smix_selector_resolver::{resolve_selector, resolve_selector_all};

use crate::Orientation;
use crate::android_aim::{resolve_aimed, resolve_rect_with_implicit_wait};
use crate::android_notes::dispatch_unsupported_err;
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
    /// The screen, and which reader it came from — the tree a selector act
    /// aims from, kept with its source so the landing is judged from the
    /// same one.
    pub(crate) async fn perceive(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<smix_runner_client::PerceivedTree, ExpectationFailure> {
        self.runner.get_tree(include).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::tree: {e}"),
                ..Default::default()
            })
        })
    }

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

#[async_trait]
impl Driver for AndroidDriver {
    fn platform(&self) -> Platform {
        Platform::Android
    }

    fn runner_client(&self) -> Option<&smix_runner_client::HttpRunnerClient> {
        Some(self.runner())
    }

    // No as_ios_driver override — uses default `None` from trait.

    /// Android impl: send the package of the app under test as
    /// `App-Bundle-Id` on every request.
    ///
    /// It does not pin the runner to one app the way the iOS header
    /// does — Android's `/tree` walks every attached window and there
    /// is no `XCUIApplication` to rebind. What needs it is id lookup:
    /// Compose emits `<pkg>:id/<tag>` on some layouts, and the runner
    /// cannot construct that spelling without knowing the package.
    fn set_target_bundle_id(&mut self, bundle: &str) {
        self.runner.set_target_bundle_id(bundle);
    }

    /// Android impl: attach / clear the `Session-Id` header on every
    /// subsequent request.
    ///
    /// The Kotlin runner does serve the `/session/*` routes and keeps a
    /// `SessionTable`; sessions are optional there rather than absent,
    /// which is what "Android drives sessionless" is shorthand for. A
    /// flow that never opens one still works, because every action
    /// route resolves without consulting the table.
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
        self.runner
            .get_tree(include)
            .await
            .map(|t| t.root)
            .map_err(|e| {
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
        // Google ML Kit Text Recognition, Latin script package. The
        // Kotlin route reads `locales` and ignores it, which is fine
        // while every caller sends none and wrong the moment one can
        // send some: a Chinese needle would be read by a Latin
        // recogniser and come back "no matching text" — a sentence
        // about the screen when the truth is about the recogniser.
        if let Some(tag) = crate::latin_script_only(locales) {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!(
                    "ocrText locale `{tag}` needs a script this Android runner \
                     cannot read: it ships ML Kit's Latin package, so Chinese, \
                     Japanese, Korean and Cyrillic are not available here. Name \
                     the element from the accessibility tree instead — Android's \
                     is usually richer than iOS's — or drive this check on iOS"
                ),
                ..Default::default()
            }));
        }
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
        let budget = crate::poll::Budget::new(timeout);
        loop {
            let look = budget.look();
            let tree = self.tree(include).await?;
            if let Some(node) = resolve_selector(&tree, selector) {
                return Ok(node.clone());
            }
            if budget.spent_by(look) {
                // Suggestions scan the whole visible tree, not just the ten
                // displayed elements: an Android window dump leads with the
                // navigation / status bar chrome, so the first ten
                // identity-bearing nodes are all system UI and the app's own
                // content (which the near-miss target actually resembles)
                // sits far deeper. Truncating the candidate set to the
                // display limit would blind "Did you mean ...?" to every real
                // app element.
                let candidates = collect_visible_summaries(&tree, DEFAULT_VISIBLE_LIMIT);
                let target = crate::base_text_or_id(selector);
                let suggestions = smix_error::build_suggestions(target.as_deref(), &candidates);
                return Err(ExpectationFailure::new(
                    FailureInit {
                        code: Some(FailureCode::ElementNotFound),
                        message: format!(
                            "AndroidDriver::wait_for timeout after {}ms: {}{}",
                            timeout.as_millis(),
                            describe_selector(selector),
                            self.reader_caveat().await,
                        ),
                        selector: Some(selector.clone()),
                        suggestions,
                        ..Default::default()
                    }
                    .with_screen(screen_facts(&tree, 10)),
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    // === Act ===

    async fn tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<crate::ActOutcome, ExpectationFailure> {
        // Host-resolve + tap_at_norm_coord (mirrors IosDriver Path B),
        // judged by what the runner says the touch was delivered to —
        // the same judgement the iOS tap gets.
        let (nx, ny, aimed, reader) = resolve_aimed(self, selector, include).await?;
        let landed = self
            .runner
            .tap_at_norm_coord_aimed(nx, ny, 1, None, None, Some(reader))
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::tap: runner.tap_at_norm_coord: {e}"),
                    ..Default::default()
                })
            })?;
        crate::landing_outcome(selector, (nx, ny, aimed, reader), &landed)
    }

    /// Found once — read and held still, as a tap is — then touched
    /// `times` times in one request, `interval_ms` apart. One resolve per
    /// touch, the default, read the screen and waited for the target to
    /// hold still before every one, and the gaps ran past what a gesture
    /// gated on "ten taps, each within 1.5 s" allows. What the first touch
    /// is delivered to is judged as a tap is; after it the screen may
    /// change, so the later ones are not.
    async fn tap_burst(
        &self,
        selector: &Selector,
        times: u32,
        interval_ms: Option<u32>,
        hold_ms: Option<u32>,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        let (nx, ny, aimed, reader) = resolve_aimed(self, selector, include).await?;
        let landed = self
            .runner
            .tap_at_norm_coord_aimed(nx, ny, times.max(1), interval_ms, hold_ms, Some(reader))
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::tap_burst: runner.tap_at_norm_coord: {e}"),
                    ..Default::default()
                })
            })?;
        crate::landing_outcome(selector, (nx, ny, aimed, reader), &landed).map(|_| ())
    }

    async fn tap_with_mode(
        &self,
        _selector: &Selector,
        _mode: TapMode,
        _include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        Err(dispatch_unsupported_err())
    }

    async fn double_tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        self.runner
            .double_tap_at_norm_coord(nx, ny)
            .await
            .map(|_| ())
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::double_tap_at_norm_coord: {e}"),
                    ..Default::default()
                })
            })
    }

    async fn long_press_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
    ) -> Result<(), ExpectationFailure> {
        self.runner
            .long_press_at_norm_coord(nx, ny, duration_ms)
            .await
            .map(|_| ())
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::long_press_at_norm_coord: {e}"),
                    ..Default::default()
                })
            })
    }

    async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        // Direct passthru to Kotlin runner /tap-at-norm-coord.
        self.runner
            .tap_at_norm_coord(nx, ny)
            .await
            .map(|_| ())
            .map_err(|e| {
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
        // side dispatches 2 clicks 150ms apart), judged like a tap. The
        // trait has no outcome to return here, so a miss is the error.
        let (nx, ny, aimed, reader) = resolve_aimed(self, selector, include).await?;
        let landed = self
            .runner
            .double_tap_at_norm_coord_aimed(nx, ny, Some(reader))
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::double_tap: {e}"),
                    ..Default::default()
                })
            })?;
        crate::landing_outcome(selector, (nx, ny, aimed, reader), &landed).map(|_| ())
    }

    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
        include: Option<IncludeScope>,
    ) -> Result<crate::PressTiming, ExpectationFailure> {
        // Host-resolve + /long-press-at-norm-coord with
        // duration. Kotlin uses UiDevice.swipe(x,y,x,y,steps) where
        // steps = duration / 5ms to approximate a sustained press.
        let (nx, ny, aimed, reader) = resolve_aimed(self, selector, include).await?;
        let duration_ms = duration.as_millis() as u64;
        // `UiDevice.swipe` reports nothing about when the touch was
        // down, so the bounds are unavailable rather than guessed —
        // `captureDuring` refuses on Android instead of handing back a
        // frame it cannot place. Where it was delivered is known, and
        // judged like a tap.
        let landed = self
            .runner
            .long_press_at_norm_coord_aimed(nx, ny, duration_ms, Some(reader))
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::long_press: {e}"),
                    ..Default::default()
                })
            })?;
        crate::landing_outcome(selector, (nx, ny, aimed, reader), &landed)
            .map(|_| crate::PressTiming::unplaceable())
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
        clear_first: bool,
    ) -> Result<(), ExpectationFailure> {
        if self.force_key_events {
            // No resolve, no focus tap: type where focus already is.
            // That is the whole mode — it exists for fields the tree
            // cannot address, so resolving first would fail for exactly
            // the callers who asked for it.
            if clear_first {
                // force-key-events resolves nothing by design, so there
                // is no field to name: type where focus already is.
                self.clear_focused_field("AndroidDriver::fill (key-events)", None)
                    .await?;
            }
            return self.runner.input_text(text).await.map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver::fill (key-events): {e}"),
                    ..Default::default()
                })
            });
        }
        if matches!(selector, Selector::Focused { .. }) {
            return self.fill_focused(text, clear_first).await;
        }
        // Host-resolve → tap to focus → /input-text. Mirror
        // of swift FlyingFox /fill semantics (selector resolves; client
        // types text into focused field).
        let ((nx, ny), rect) = resolve_rect_with_implicit_wait(self, selector, include).await?;
        self.runner.tap_at_norm_coord(nx, ny).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::fill: focus tap failed: {e}"),
                ..Default::default()
            })
        })?;
        if clear_first {
            self.clear_focused_field("AndroidDriver::fill", Some(rect))
                .await?;
        }
        self.runner.input_text_in(text, rect).await.map_err(|e| {
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
        // Host-resolve → tap to focus → the runner's one-request clear,
        // the same one `fill` reaches once it already holds focus.
        let ((nx, ny), rect) = resolve_rect_with_implicit_wait(self, selector, include).await?;
        self.runner.tap_at_norm_coord(nx, ny).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("AndroidDriver::clear: focus tap failed: {e}"),
                ..Default::default()
            })
        })?;
        self.clear_focused_field("AndroidDriver::clear", Some(rect))
            .await
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

    /// `true`: there is no stale frame here to check.
    ///
    /// UiAutomator's bounds are the node's live, clipped on-screen
    /// rectangle, and the probe's semantics tree is read from the app at
    /// the moment it is asked. The probe reports layout rather than what
    /// is visible — a row below the screen edge carries its full box —
    /// and that is what the scroll's visible-share rule reads, so it
    /// needs no live query to be told.
    async fn confirm_on_screen(&self, _matched: &[&A11yNode]) -> Result<bool, ExpectationFailure> {
        Ok(true)
    }

    async fn pixels_per_point(&self) -> Result<f64, ExpectationFailure> {
        // Both readers here — accessibility and the semantics probe —
        // report physical pixels. The runner says how many make a point.
        let d = self.runner.display().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!(
                    "AndroidDriver::pixels_per_point: {e} — a runner older than this \
                     host has no `/display`; `smix runner up --force` installs this one"
                ),
                ..Default::default()
            })
        })?;
        Ok(d.pixels_per_point)
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
        // Kotlin /back injects KEYCODE_BACK and reads whether the screen
        // moved. Its refusal keeps its branch, as it does on iOS.
        self.runner.back().await.map_err(|e| {
            let mut f = crate::transport_to_failure(e);
            f.message = format!("AndroidDriver::back: {}", f.message);
            f
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
