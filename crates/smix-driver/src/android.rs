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
use smix_screen::{A11yNode, DEFAULT_VISIBLE_LIMIT, collect_visible_summaries};
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
    /// What the reader that just failed cannot see, or nothing when it has
    /// nothing to admit.
    ///
    /// Asked only here, on a path that has already failed, so it costs a
    /// request nobody pays for twice. A Compose dialog's controls arrive in
    /// the accessibility tree as anonymous views: a flow looking for one by
    /// id gets "not found", which is true and useless — the fix is a line
    /// in a build file, and a reader who does not already suspect that will
    /// never guess it from a timeout.
    async fn reader_caveat(&self) -> String {
        let Some(app) = self.runner.target_bundle_id().map(str::to_string) else {
            return String::new();
        };
        match self.runner.probe_status(&app).await {
            Ok(s) if s.present => String::new(),
            Ok(s) => format!(
                "\n  the tree came from the accessibility reader, which sees a \
                 Compose dialog's contents as unnamed views. {} — adding \
                 `debugImplementation(\"jp.golia.smix:smix-probe\")` to its debug \
                 build lets smix read the semantics tree instead.",
                s.why.unwrap_or_else(|| format!("{app} has no smix probe")),
            ),
            // The probe route itself not answering says nothing about the
            // app, and guessing here would send a reader to edit a build
            // file over a runner that is simply older.
            Err(_) => String::new(),
        }
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

    /// Empty the field that already holds focus, in one request.
    ///
    /// This sent fifty `/press-key DELETE` posts — fifty sequential
    /// round trips over the adb forward, and once `fill` began
    /// clearing first, on every fill. It was also wrong: fifty deletes
    /// do not empty a field holding more than fifty characters, so the
    /// new text landed after the remainder while the caller was told
    /// its value had been replaced.
    ///
    /// The runner does it now, exactly, through the focused node's
    /// `ACTION_SET_TEXT`.
    ///
    /// `at` names the field by where it was tapped. Without it the
    /// runner empties whatever holds focus, and focus does not move
    /// synchronously with the tap that moves it — a fill naming one
    /// Compose field was measured emptying another.
    async fn clear_focused_field(
        &self,
        stage: &str,
        at: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), ExpectationFailure> {
        let done = match at {
            Some(rect) => self.runner.clear_text_in(rect).await,
            None => self.runner.clear_text().await,
        };
        done.map(|_| ()).map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("{stage}: clear-first failed: {e}"),
                ..Default::default()
            })
        })
    }
}

/// Host-resolve loop with 5s implicit-wait + 250ms poll.
/// Returns viewport-normalized centroid coord. Shared by tap / double_tap
/// / long_press / fill / clear.
/// The element's own box, viewport-normalized, alongside its centre.
///
/// A fill needs the box, not just the point. A field named by the
/// layout around it — a consumer's views carry the contentDescription
/// on the wrapper and nothing on the input — resolves to the wrapper,
/// and the wrapper's centre can sit on a label rather than on the
/// field. What identifies the field is that it lies *inside* what was
/// named.
async fn resolve_rect_with_implicit_wait(
    driver: &AndroidDriver,
    selector: &Selector,
    include: Option<IncludeScope>,
) -> Result<((f64, f64), (f64, f64, f64, f64)), ExpectationFailure> {
    let coord = resolve_with_implicit_wait(driver, selector, include).await?;
    let tree = driver.tree(include).await?;
    let frame = tree.bounds;
    let Some(named) = smix_selector_resolver::resolve_selector(&tree, selector)
        .filter(|_| frame.w > 0.0 && frame.h > 0.0)
    else {
        return Ok((coord, (coord.0, coord.1, 0.0, 0.0)));
    };
    let rect = (
        (named.bounds.x - frame.x) / frame.w,
        (named.bounds.y - frame.y) / frame.h,
        named.bounds.w / frame.w,
        named.bounds.h / frame.h,
    );
    // Aim at the field, not at the middle of what names it. A layout
    // whose contentDescription names the field it wraps has its centre
    // wherever its tallest child is — often a label — and tapping a
    // label focuses nothing, so the fill that followed had no field to
    // type into. If what was named is not itself typeable and holds
    // exactly one thing that is, that is what the caller meant.
    let aim = if named.role == Some(smix_screen::Role::TextField) {
        coord
    } else {
        match sole_text_field(named) {
            Some(field) => (
                (field.bounds.x + field.bounds.w / 2.0 - frame.x) / frame.w,
                (field.bounds.y + field.bounds.h / 2.0 - frame.y) / frame.h,
            ),
            None => coord,
        }
    };
    Ok((aim, rect))
}

/// The one typeable descendant, when there is exactly one.
///
/// Exactly one on purpose: with two, which the caller meant is a guess,
/// and a guess that types into the wrong field is the defect this whole
/// line of work is about. They can name the field itself.
fn sole_text_field(node: &smix_screen::A11yNode) -> Option<&smix_screen::A11yNode> {
    let mut found: Option<&smix_screen::A11yNode> = None;
    let mut stack: Vec<&smix_screen::A11yNode> = node.children.iter().collect();
    while let Some(n) = stack.pop() {
        if n.role == Some(smix_screen::Role::TextField) {
            if found.is_some() {
                return None;
            }
            found = Some(n);
        }
        stack.extend(n.children.iter());
    }
    found
}

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
        self.runner.get_tree(include).await.map(|t| t.root).map_err(|e| {
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
        let start = std::time::Instant::now();
        loop {
            let tree = self.tree(include).await?;
            if let Some(node) = resolve_selector(&tree, selector) {
                return Ok(node.clone());
            }
            if start.elapsed() >= timeout {
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
                let visible = collect_visible_summaries(&tree, 10);
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::ElementNotFound),
                    message: format!(
                        "AndroidDriver::wait_for timeout after {}ms: {}{}",
                        timeout.as_millis(),
                        describe_selector(selector),
                        self.reader_caveat().await,
                    ),
                    selector: Some(selector.clone()),
                    visible_elements: visible,
                    suggestions,
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
    ) -> Result<crate::ActOutcome, ExpectationFailure> {
        // Host-resolve + tap_at_norm_coord (mirrors IosDriver Path B).
        let (nx, ny) = resolve_with_implicit_wait(self, selector, include).await?;
        // Android reports no chain yet — only the iOS runner fills it
        // in — so the outcome says it could not be judged rather than
        // claiming the tap landed. Wiring the Kotlin side is the other
        // half of this checkpoint, not a line to sneak in here.
        self.runner
            .tap_at_norm_coord(nx, ny)
            .await
            .map(|_| crate::ActOutcome::unjudged())
            .map_err(|e| {
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
    ) -> Result<crate::PressTiming, ExpectationFailure> {
        // Host-resolve + /long-press-at-norm-coord with
        // duration. Kotlin uses UiDevice.swipe(x,y,x,y,steps) where
        // steps = duration / 5ms to approximate a sustained press.
        let (nx, ny) = resolve_with_implicit_wait(self, selector, include).await?;
        let duration_ms = duration.as_millis() as u64;
        // `UiDevice.swipe` reports nothing about when the touch was
        // down, so the bounds are unavailable rather than guessed —
        // `captureDuring` refuses on Android instead of handing back a
        // frame it cannot place.
        self.runner
            .long_press_at_norm_coord(nx, ny, duration_ms)
            .await
            .map(|()| crate::PressTiming::unplaceable())
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
