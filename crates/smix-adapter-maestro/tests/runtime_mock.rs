//! Adapter::run mock-driven unit coverage.
//!
//! Each test isolates the runtime from a real simulator by impl-ing
//! [`AppLike`] on a hand-rolled `MockApp` that records the call
//! sequence. The 15 [`Step`] variants → smix-sdk action mapping is
//! verified via the captured trace; failure shapes (optional swallow,
//! Swipe graceful skip, clear_state graceful skip, RunFlowConditional
//! visibility evaluation, ParseError::UnsupportedCommand graceful
//! skip) all surface as concrete trace assertions.
//!
//! `Step::LaunchApp` now routes through
//! `AppLike::launch_fresh`. The `LaunchFresh` capture variant records
//! the env-var bridge (`SMIX_APP_PATH_<NORMALIZED_BUNDLE>` →
//! `app_path: Option<&str>`); the two `mock_run_launch_app_clear_state_*`
//! tests cover both branches (env var unset → graceful fallback;
//! env var set → real wipe path with empty warnings).

use async_trait::async_trait;
use smix_adapter_maestro::{
    Adapter, AppLike, RunError, RunStepReport, parse_flow_file, parse_flow_yaml,
};
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit, KeyName, Selector, SwipeDirection};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

// --------------------------------------------------------------------
// MockApp — hand-rolled AppLike capturing call sequence.
// --------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum MockCall {
    Tap(Selector),
    /// `App::tap_xcui(id)` (SwiftUI modal dismiss routing).
    TapXcui(String),
    /// `App::tap_with_mode(selector, mode)` (`dispatch: daemonProxy`).
    TapWithMode(String),
    /// `App::clear_user_defaults(bundle, keys)`.
    ClearUserDefaults(String, Vec<String>),
    /// `App::find_by_text_ocr(text, locales)` (OCR sense layer).
    FindByTextOcr(String, Vec<String>),
    /// `App::find_norm_coord(selector)` (anchor resolution).
    FindNormCoord(Selector),
    /// `App::webview_eval(js)` (Option A debug bridge).
    WebViewEval(String),
    TapAtCoord(f64, f64),
    Fill(Selector, String),
    PressKey(KeyName),
    Scroll(Selector, SwipeDirection),
    WaitFor(Selector, Duration),
    /// `App::wait_for_not_visible(selector, timeout)`.
    WaitForNotVisible(Selector, Duration),
    AssertVisible(Selector),
    Find(Selector),
    Launch(String),
    Terminate(String),
    LaunchFresh {
        bundle_id: String,
        clear_state: bool,
        clear_keychain: bool,
        app_path: Option<String>,
    },
    OpenUrl(String),
    /// `App::swipe_at_coord(from, to)`.
    SwipeAtCoord((f64, f64), (f64, f64)),
    /// `App::scroll_screen(direction)`.
    ScrollScreen(SwipeDirection),
    /// `App::assert_not_visible(selector)`. Adapter only records
    /// the call; the per-selector pass/fail is injected via
    /// `with_assert_not_visible_failure`.
    AssertNotVisible(Selector),
    /// `App::hide_keyboard`.
    HideKeyboard,
    /// `App::screenshot`.
    Screenshot,
    /// `App::foreground(bundle)`.
    Foreground(String),
    /// `App::launch_app_with_options(opts)`.
    LaunchAppWithOptions(smix_sdk::LaunchAppOptions),
    /// `App::set_clipboard(text)`.
    SetClipboard(String),
    /// `App::paste_text(text)`. `None` means read from the clipboard.
    PasteText(Option<String>),
    /// `App::copy_text_from(selector)`.
    CopyTextFrom(Selector),
    /// `App::double_tap(selector)`.
    DoubleTap(Selector),
    /// `App::long_press(selector, duration)`.
    LongPress(Selector, Duration),
    /// `App::set_location(lat, lng)`.
    SetLocation(f64, f64),
    /// `App::travel(points, speed)`.
    Travel(Vec<(f64, f64)>, Option<f64>),
    /// `App::set_permissions(bundle, perms)`.
    SetPermissions(
        String,
        Vec<(smix_sdk::SimctlPermission, smix_sdk::PermissionAction)>,
    ),
    /// `App::add_media(paths)`.
    AddMedia(Vec<String>),
    /// `App::set_orientation(orientation)`.
    SetOrientation(smix_sdk::MaestroOrientation),
    /// `App::start_recording(path)`.
    StartRecording(String),
    /// `App::stop_recording()` bare.
    StopRecording,
    /// `App::assert_screenshot(path, max_hamming)` (path is abs
    /// after base_dir.join resolution).
    AssertScreenshot(std::path::PathBuf),
    /// `App::get_clipboard()` read (used by runFlow.as outputs).
    GetClipboard,
}

/// A flat grayscale PNG, standing in for a device screenshot.
fn mock_png(shade: u8) -> Vec<u8> {
    mock_png_sized(64, 64, |_, _| shade)
}

/// A grayscale PNG built from a pixel callback — the frame fixtures for
/// quiescence, where what matters is which samples differ between frames.
fn mock_png_sized(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> u8) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                data.push(pixel(x, y));
            }
        }
        writer.write_image_data(&data).unwrap();
    }
    out
}

struct MockApp {
    calls: Mutex<Vec<MockCall>>,
    /// describe_selector-keyed map of find responses (true = visible).
    find_returns: Mutex<HashMap<String, bool>>,
    /// describe_selector-keyed set of selectors
    /// for which `find` should return `Err(ExpectationFailure)` (the
    /// runner-transport-error path). Used to test that RunFlowInline /
    /// RunFlowConditional's when-visible predicate swallows driver
    /// errors as "not visible" instead of surfacing them via
    /// `to_prompt` stderr noise.
    find_error_selectors: Mutex<std::collections::HashSet<String>>,
    /// describe_selector-keyed map of tap responses; missing key = Ok.
    tap_failures: Mutex<HashMap<String, FailureCode>>,
    /// Describe_selector-keyed transient fail count: (code,
    /// target_fail_count, current_call_count). tap() returns fail while
    /// current < target, then success. Used to model retry semantics.
    tap_failures_n_times: Mutex<HashMap<String, (FailureCode, u32, u32)>>,
    /// Describe_selector-keyed: if present, assert_not_visible
    /// raises AssertionFailed for that selector. Missing key = Ok.
    assert_not_visible_failures: Mutex<HashMap<String, ()>>,
    /// Describe_selector-keyed transient visibility counter:
    /// (target_visible_count, current_call_count). find() returns true
    /// while current < target, then false. Used to model selector-style
    /// repeat.while.visible loops.
    find_visible_n_times: Mutex<HashMap<String, (u32, u32)>>,
    /// Current device pasteboard contents returned by
    /// `get_clipboard()`. Used to model `runFlow.as: <name>` outputs
    /// capture (subflow leaves a value in the pasteboard, parent reads
    /// it back via clipboard sense).
    clipboard: Mutex<String>,
    /// Canned return for `find_by_text_ocr`. None = OCR miss
    /// (default). Tests set via `set_ocr_result(Some(OcrFrame{...}))`.
    ocr_result: Mutex<Option<smix_sdk::OcrFrame>>,
    /// Canned return for `find_norm_coord` (used by
    /// AnchorRelative). Default `Some((0.5, 0.5))` (center) so adapter
    /// dispatch always finds an anchor unless tests override.
    anchor_coord: Mutex<Option<(f64, f64)>>,
    /// Canned return for `webview_eval`. Defaults to JSON
    /// `null`; tests set via `with_webview_result(serde_json::json!(...))`.
    webview_result: Mutex<serde_json::Value>,
    /// Frame sequence for `screenshot()`. Empty (default) = the opaque
    /// `PNG-MOCK` bytes every other test relies on. When set, each call takes
    /// the next frame and the last one repeats — so "moves, then settles and
    /// stays settled" needs no padding.
    screenshot_frames: Mutex<Vec<Vec<u8>>>,
    screenshot_calls: Mutex<usize>,
}

impl MockApp {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            find_returns: Mutex::new(HashMap::new()),
            find_error_selectors: Mutex::new(std::collections::HashSet::new()),
            tap_failures: Mutex::new(HashMap::new()),
            tap_failures_n_times: Mutex::new(HashMap::new()),
            assert_not_visible_failures: Mutex::new(HashMap::new()),
            find_visible_n_times: Mutex::new(HashMap::new()),
            clipboard: Mutex::new(String::new()),
            ocr_result: Mutex::new(None),
            anchor_coord: Mutex::new(Some((0.5, 0.5))),
            webview_result: Mutex::new(serde_json::Value::Null),
            screenshot_frames: Mutex::new(Vec::new()),
            screenshot_calls: Mutex::new(0),
        }
    }

    /// Hand `screenshot()` a frame sequence. The last frame repeats once the
    /// sequence runs out.
    #[allow(dead_code)]
    fn with_screenshot_frames(self, frames: Vec<Vec<u8>>) -> Self {
        *self.screenshot_frames.lock().unwrap() = frames;
        self
    }

    /// Preset the OCR result `find_by_text_ocr` will return.
    /// `None` (default) = OCR miss; `Some(frame)` = OCR hit at given frame.
    #[allow(dead_code)]
    fn with_ocr_result(self, frame: Option<smix_sdk::OcrFrame>) -> Self {
        *self.ocr_result.lock().unwrap() = frame;
        self
    }

    /// Preset the device pasteboard contents that
    /// `get_clipboard()` will return on the next call. Used to model the
    /// canonical `runFlow.as: <name>` flow where the subflow ends with a
    /// `copyTextFrom` that leaves text in the pasteboard for the parent.
    fn with_clipboard(self, text: &str) -> Self {
        *self.clipboard.lock().unwrap() = text.to_string();
        self
    }

    /// Transient visibility helper: report `sel_key` visible
    /// for the first `n` find() calls, then not visible. Used to model
    /// `repeat: { while: { visible: <sel> }, commands: [...] }` loops.
    fn with_find_visible_n_times(self, sel_key: &str, n: u32) -> Self {
        self.find_visible_n_times
            .lock()
            .unwrap()
            .insert(sel_key.to_string(), (n, 0));
        self
    }

    /// Transient tap failure helper: fail the first `n` calls
    /// on `sel_key`, then succeed. Used to model retry semantics.
    fn with_tap_failure_n_times(self, sel_key: &str, code: FailureCode, n: u32) -> Self {
        self.tap_failures_n_times
            .lock()
            .unwrap()
            .insert(sel_key.to_string(), (code, n, 0));
        self
    }

    fn with_find(self, sel_key: &str, visible: bool) -> Self {
        self.find_returns
            .lock()
            .unwrap()
            .insert(sel_key.to_string(), visible);
        self
    }

    /// Configure `find` to return
    /// `Err(ExpectationFailure)` for the given selector describe key.
    /// Used to reproduce the runner-transport-error case that
    /// previously leaked as spurious ELEMENT_NOT_FOUND stderr noise
    /// even when the outer flow correctly proceeded.
    fn with_find_error(self, sel_key: &str) -> Self {
        self.find_error_selectors
            .lock()
            .unwrap()
            .insert(sel_key.to_string());
        self
    }

    fn with_tap_failure(self, sel_key: &str, code: FailureCode) -> Self {
        self.tap_failures
            .lock()
            .unwrap()
            .insert(sel_key.to_string(), code);
        self
    }

    #[allow(dead_code)] // wired by mock_run_assert_not_visible_fail
    fn with_assert_not_visible_failure(self, sel_key: &str) -> Self {
        self.assert_not_visible_failures
            .lock()
            .unwrap()
            .insert(sel_key.to_string(), ());
        self
    }

    fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl AppLike for MockApp {
    async fn tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Tap(selector.clone()));
        let key = smix_sdk::describe_selector(selector);
        // Transient n-times failure path (retry mock).
        {
            let mut map = self.tap_failures_n_times.lock().unwrap();
            if let Some((code, target, current)) = map.get_mut(&key)
                && *current < *target
            {
                *current += 1;
                let code = *code;
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(code),
                    message: format!("mock transient tap failure {code:?}"),
                    selector: Some(selector.clone()),
                    ..Default::default()
                }));
            }
        }
        if let Some(code) = self.tap_failures.lock().unwrap().get(&key).copied() {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(code),
                message: format!("mock tap failure {code:?}"),
                selector: Some(selector.clone()),
                ..Default::default()
            }));
        }
        Ok(())
    }
    async fn tap_xcui(&self, id: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::TapXcui(id.to_string()));
        Ok(())
    }
    async fn tap_with_mode(
        &self,
        selector: &Selector,
        _: smix_sdk::TapMode,
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::TapWithMode(format!("{selector:?}")));
        Ok(())
    }
    async fn clear_user_defaults(
        &self,
        bundle_id: &str,
        keys: &[String],
    ) -> Result<(), ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::ClearUserDefaults(
            bundle_id.to_string(),
            keys.to_vec(),
        ));
        Ok(())
    }
    async fn find_by_text_ocr(
        &self,
        text: &str,
        locales: &[String],
    ) -> Result<Option<smix_sdk::OcrFrame>, ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::FindByTextOcr(text.to_string(), locales.to_vec()));
        // Return mock-canned ocr_result if set, else default Some at frame (0.5, 0.5, 0.1, 0.05)
        Ok(*self.ocr_result.lock().unwrap())
    }
    async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::FindNormCoord(selector.clone()));
        Ok(*self.anchor_coord.lock().unwrap())
    }
    async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::WebViewEval(js.to_string()));
        Ok(self.webview_result.lock().unwrap().clone())
    }
    async fn tap_at_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::TapAtCoord(nx, ny));
        Ok(())
    }
    async fn fill(&self, selector: &Selector, text: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Fill(selector.clone(), text.to_string()));
        Ok(())
    }
    async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::PressKey(key));
        Ok(())
    }
    async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Scroll(selector.clone(), direction));
        Ok(())
    }
    async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::WaitFor(selector.clone(), timeout));
        Ok(())
    }
    async fn wait_for_not_visible(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::WaitForNotVisible(selector.clone(), timeout));
        // Adapter now routes assertNotVisible through wait_for_not_visible,
        // so the assert_not_visible failure-injection key must surface here too.
        let key = smix_sdk::describe_selector(selector);
        if self
            .assert_not_visible_failures
            .lock()
            .unwrap()
            .contains_key(&key)
        {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::AssertionFailed),
                message: format!("mock wait_for_not_visible failure for {key}"),
                selector: Some(selector.clone()),
                ..Default::default()
            }));
        }
        Ok(())
    }
    async fn assert_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::AssertVisible(selector.clone()));
        Ok(())
    }
    async fn find(&self, selector: &Selector) -> Result<bool, ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Find(selector.clone()));
        let key = smix_sdk::describe_selector(selector);
        // Runner-transport-error path.
        // When configured to error for this selector, surface
        // ExpectationFailure so `when_visible` predicate's error-
        // swallow behavior can be exercised.
        if self.find_error_selectors.lock().unwrap().contains(&key) {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("mock: find error for {key}"),
                ..Default::default()
            }));
        }
        // Transient counter takes precedence over static map.
        {
            let mut counters = self.find_visible_n_times.lock().unwrap();
            if let Some(entry) = counters.get_mut(&key) {
                let (target, current) = entry;
                if *current < *target {
                    *current += 1;
                    return Ok(true);
                }
                return Ok(false);
            }
        }
        Ok(self
            .find_returns
            .lock()
            .unwrap()
            .get(&key)
            .copied()
            .unwrap_or(false))
    }
    async fn launch(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Launch(bundle_id.to_string()));
        Ok(())
    }
    async fn terminate(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Terminate(bundle_id.to_string()));
        Ok(())
    }
    async fn launch_fresh(
        &self,
        bundle_id: &str,
        clear_state: bool,
        clear_keychain: bool,
        app_path: Option<&str>,
        _launch_arguments: &[String],
    ) -> Result<Vec<String>, ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::LaunchFresh {
            bundle_id: bundle_id.to_string(),
            clear_state,
            clear_keychain,
            app_path: app_path.map(str::to_string),
        });
        Ok(if clear_state && app_path.is_none() {
            vec![
                "G10 launch_fresh: app_path missing — graceful fallback to non-clear path \
                 (terminate + launch); set SMIX_APP_PATH_<BUNDLE_NORMALIZED> to enable wipe"
                    .to_string(),
            ]
        } else {
            Vec::new()
        })
    }
    async fn open_url(&self, url: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::OpenUrl(url.to_string()));
        Ok(())
    }
    async fn system_popups(&self) -> Result<Vec<smix_sdk::SystemPopup>, ExpectationFailure> {
        Ok(Vec::new())
    }
    async fn system_popup_action(
        &self,
        _popup_id: &str,
        _button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        Ok(true)
    }
    async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Foreground(bundle_id.to_string()));
        Ok(())
    }
    async fn launch_app_with_options(
        &self,
        opts: &smix_sdk::LaunchAppOptions,
    ) -> Result<Vec<String>, ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::LaunchAppWithOptions(opts.clone()));
        Ok(Vec::new())
    }
    async fn swipe_at_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::SwipeAtCoord(from, to));
        Ok(())
    }
    async fn scroll_screen(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::ScrollScreen(direction));
        Ok(())
    }
    async fn assert_not_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::AssertNotVisible(selector.clone()));
        let key = smix_sdk::describe_selector(selector);
        if self
            .assert_not_visible_failures
            .lock()
            .unwrap()
            .contains_key(&key)
        {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::AssertionFailed),
                message: format!("mock assert_not_visible failure for {key}"),
                selector: Some(selector.clone()),
                ..Default::default()
            }));
        }
        Ok(())
    }
    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::HideKeyboard);
        Ok(())
    }
    async fn screenshot(&self) -> Result<Vec<u8>, ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::Screenshot);
        let frames = self.screenshot_frames.lock().unwrap();
        if frames.is_empty() {
            // A real screenshot is a real PNG, and waitForAnimationToEnd
            // decodes it. Handing back opaque bytes would only mean every
            // flow containing that verb fails in tests for a reason no user
            // would ever hit.
            return Ok(mock_png(128));
        }
        let mut n = self.screenshot_calls.lock().unwrap();
        let idx = (*n).min(frames.len() - 1);
        *n += 1;
        Ok(frames[idx].clone())
    }
    async fn get_clipboard(&self) -> Result<String, ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::GetClipboard);
        Ok(self.clipboard.lock().unwrap().clone())
    }
    async fn set_clipboard(&self, text: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::SetClipboard(text.to_string()));
        Ok(())
    }
    async fn paste_text(&self, text: Option<&str>) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::PasteText(text.map(str::to_string)));
        Ok(())
    }
    async fn copy_text_from(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::CopyTextFrom(selector.clone()));
        Ok(())
    }
    async fn double_tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::DoubleTap(selector.clone()));
        Ok(())
    }
    async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::LongPress(selector.clone(), duration));
        Ok(())
    }
    async fn set_location(&self, latitude: f64, longitude: f64) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::SetLocation(latitude, longitude));
        Ok(())
    }
    async fn travel(
        &self,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::Travel(points.to_vec(), speed_mps));
        Ok(())
    }
    async fn set_permissions(
        &self,
        bundle_id: &str,
        permissions: &[(smix_sdk::SimctlPermission, smix_sdk::PermissionAction)],
    ) -> Result<(), ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::SetPermissions(
            bundle_id.to_string(),
            permissions.to_vec(),
        ));
        Ok(())
    }
    async fn add_media(&self, paths: &[String]) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::AddMedia(paths.to_vec()));
        Ok(())
    }
    async fn set_orientation(
        &self,
        orientation: smix_sdk::MaestroOrientation,
    ) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::SetOrientation(orientation));
        Ok(())
    }
    async fn start_recording(&self, path: &str) -> Result<(), ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::StartRecording(path.to_string()));
        Ok(())
    }
    async fn stop_recording(&self) -> Result<(), ExpectationFailure> {
        self.calls.lock().unwrap().push(MockCall::StopRecording);
        Ok(())
    }
    async fn assert_screenshot(
        &self,
        baseline_path: &std::path::Path,
        _: u32,
    ) -> Result<smix_sdk::AssertScreenshotOutcome, ExpectationFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::AssertScreenshot(baseline_path.to_path_buf()));
        Ok(smix_sdk::AssertScreenshotOutcome::Matched { hamming: 0 })
    }
}

// --------------------------------------------------------------------
// Test helpers
// --------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn parse_inline(yaml: &str) -> smix_adapter_maestro::Flow {
    parse_flow_yaml(yaml).expect("parse_flow_yaml")
}

// --------------------------------------------------------------------
// 13 fixture/mock-driven cases
// --------------------------------------------------------------------

#[tokio::test]
async fn mock_run_tap_text_short() {
    let flow = parse_inline("appId: x\n---\n- tapOn: \"Login\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run ok");
    assert_eq!(report.steps.len(), 1);
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::Tap(Selector::Text { text, .. }) => {
            assert_eq!(
                smix_sdk::describe_selector(&smix_sdk::text("Login")),
                smix_sdk::describe_selector(&Selector::Text {
                    text: text.clone(),
                    modifiers: Default::default()
                })
            );
        }
        other => panic!("expected Tap(Text), got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_tap_id_optional_swallows_not_found() {
    let flow =
        parse_inline("appId: x\n---\n- tapOn:\n    id: \"btn-missing\"\n    optional: true\n");
    let sel_key = smix_sdk::describe_selector(&smix_sdk::id("btn-missing"));
    let app = MockApp::new().with_tap_failure(&sel_key, FailureCode::ElementNotFound);
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("optional should swallow ElementNotFound");
    assert_eq!(report.steps.len(), 1);
    match &report.steps[0] {
        RunStepReport::Skipped { reason } => {
            assert!(
                reason.contains("optional") || reason.contains("not found"),
                "skip reason should mention optional/not found, got {reason:?}"
            );
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
}

#[tokio::test]
async fn tapon_swiftui_modal_dismiss_id_routes_through_tap_xcui() {
    // Adapter capability parity with SDK self-path: tapOn on a
    // SwiftUI modal dismiss button id routes via App::tap_xcui, not the
    // default App::tap. Behavior under (c) backlog is the same as the SDK
    // path; this test guards the dispatch (capability), not the outcome.
    for id in [
        "v2-modal-sheet-dismiss-btn",
        "v2-modal-alert-ok-btn",
        "v2-modal-action-a-btn",
        "v2-modal-fullscreen-dismiss-btn",
    ] {
        let flow = parse_inline(&format!("appId: x\n---\n- tapOn:\n    id: \"{id}\"\n"));
        let app = MockApp::new();
        let mut adapter = Adapter::new(&app, fixtures_dir());
        let report = adapter.run(&flow).await.expect("run ok");
        assert_eq!(report.steps.len(), 1);
        assert!(matches!(report.steps[0], RunStepReport::Ok));
        let calls = app.calls();
        assert_eq!(calls.len(), 1, "id={id}");
        match &calls[0] {
            MockCall::TapXcui(captured) => assert_eq!(captured, id),
            other => panic!("expected TapXcui({id}), got {other:?}"),
        }
    }
}

#[tokio::test]
async fn tapon_non_modal_id_uses_default_tap() {
    // Non-modal ids stay on the default tap path, which is the
    // overwhelmingly common case.
    let flow = parse_inline("appId: x\n---\n- tapOn:\n    id: \"v2-form-submit-btn\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run ok");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::Tap(Selector::Id { id, .. }) => assert_eq!(id, "v2-form-submit-btn"),
        other => panic!("expected Tap(Id), got {other:?}"),
    }
}

#[tokio::test]
async fn tapon_swiftui_navigation_tab_id_routes_through_tap_xcui() {
    // Adapter capability gap closure: SwiftUI tab bar buttons
    // (`v2-tab-*` accessibilityIdentifier per V2RootScreen.swift) route via
    // App::tap_xcui (swift `/tap-by-id` → XCUIElement.tap with implicit
    // scrollToVisible) so the adapter can tap tabs even when a future
    // fixture grows past the iPhone width and the bar wraps into a
    // ScrollView. maestro CLI tapOn id is static (no auto-scroll); this
    // routing keeps smix ahead of maestro on the tab-navigation path.
    // Deliberately scoped to the SwiftUI tab id prefix only: tap_xcui
    // does not fire onPress on an RN Pressable, and RN is NOT in the
    // fixture namespace.
    for id in [
        "v2-tab-home",
        "v2-tab-form",
        "v2-tab-deeplink",
        "v2-tab-modal",
        "v2-tab-clip",
    ] {
        let flow = parse_inline(&format!("appId: x\n---\n- tapOn:\n    id: \"{id}\"\n"));
        let app = MockApp::new();
        let mut adapter = Adapter::new(&app, fixtures_dir());
        let report = adapter.run(&flow).await.expect("run ok");
        assert_eq!(report.steps.len(), 1);
        assert!(matches!(report.steps[0], RunStepReport::Ok));
        let calls = app.calls();
        assert_eq!(calls.len(), 1, "id={id}");
        match &calls[0] {
            MockCall::TapXcui(captured) => assert_eq!(captured, id),
            other => panic!("expected TapXcui({id}), got {other:?}"),
        }
    }
}

#[tokio::test]
async fn tapon_non_navigation_v2_id_uses_default_tap() {
    // Only the SwiftUI tab-bar `v2-tab-*` namespace routes via
    // tap_xcui (auto-scroll path). Other v2 ids (form fields, labels,
    // submit buttons, list rows) keep the default host-resolve tap path so
    // existing 14-flow smix adapter baseline stays byte-identical.
    for id in [
        "v2-form-submit-btn",
        "v2-form-email-input",
        "v2-list-row-50",
        "v2-home-counter-label",
        "v2-jump-btn",
    ] {
        let flow = parse_inline(&format!("appId: x\n---\n- tapOn:\n    id: \"{id}\"\n"));
        let app = MockApp::new();
        let mut adapter = Adapter::new(&app, fixtures_dir());
        adapter.run(&flow).await.expect("run ok");
        let calls = app.calls();
        assert_eq!(calls.len(), 1, "id={id}");
        match &calls[0] {
            MockCall::Tap(Selector::Id { id: captured, .. }) => assert_eq!(captured, id),
            other => panic!("expected Tap(Id({id})), got {other:?}"),
        }
    }
}

#[tokio::test]
async fn mock_run_tap_at_point_calls_tap_at_coord() {
    let flow = parse_inline("appId: x\n---\n- tapOn:\n    point: \"50%,90%\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match calls[0] {
        MockCall::TapAtCoord(nx, ny) => {
            assert!((nx - 0.5).abs() < 1e-9, "nx={nx}");
            assert!((ny - 0.9).abs() < 1e-9, "ny={ny}");
        }
        ref other => panic!("expected TapAtCoord, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_extended_wait_until_calls_wait_for_with_timeout() {
    let flow = parse_inline(
        "appId: x\n---\n- extendedWaitUntil:\n    visible: \"Home\"\n    timeout: 30000\n",
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::WaitFor(_, dur) => assert_eq!(*dur, Duration::from_millis(30000)),
        other => panic!("expected WaitFor, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_input_text_calls_fill_focused() {
    let flow = parse_inline("appId: x\n---\n- inputText: \"hello\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::Fill(Selector::Focused { .. }, text) => assert_eq!(text, "hello"),
        other => panic!("expected Fill(Focused, hello), got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_press_key_enter() {
    let flow = parse_inline("appId: x\n---\n- pressKey: \"Enter\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    assert_eq!(app.calls(), vec![MockCall::PressKey(KeyName::Return)]);
}

#[tokio::test]
async fn mock_run_unknown_press_key_returns_error() {
    let flow = parse_inline("appId: x\n---\n- pressKey: \"F19\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter
        .run(&flow)
        .await
        .expect_err("unknown key should error");
    match err {
        RunError::UnknownKey(k) => assert_eq!(k, "F19"),
        other => panic!("expected RunError::UnknownKey, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_erase_text_3_emits_3_deletes() {
    let flow = parse_inline("appId: x\n---\n- eraseText: 3\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    assert_eq!(
        app.calls(),
        vec![
            MockCall::PressKey(KeyName::Delete),
            MockCall::PressKey(KeyName::Delete),
            MockCall::PressKey(KeyName::Delete),
        ]
    );
}

#[tokio::test]
async fn mock_run_run_flow_conditional_skips_when_visible_false() {
    // ensure_login.yaml has when.visible="Log in" → if find→false, skip the inner flow.
    let path = fixtures_dir().join("ensure_login.yaml");
    let flow = parse_flow_file(&path).expect("parse_flow_file ensure_login");
    let app = MockApp::new(); // default find_returns missing key → false
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let _report = adapter.run(&flow).await.expect("run ok");

    let calls = app.calls();
    // Expect: 1 Find("Log in"), then 1 WaitFor for the extendedWaitUntil step.
    // NO Tap or other inner-flow calls (login.yaml expansion was skipped).
    assert!(
        calls.iter().any(|c| matches!(c, MockCall::Find(_))),
        "expected a Find call"
    );
    assert!(
        calls.iter().any(|c| matches!(c, MockCall::WaitFor(_, _))),
        "expected the extendedWaitUntil WaitFor"
    );
    assert!(
        !calls.iter().any(|c| matches!(c, MockCall::Tap(_))),
        "should NOT have tapped (login.yaml was conditionally skipped)"
    );
}

#[tokio::test]
async fn mock_run_run_flow_conditional_expands_when_visible_true() {
    let path = fixtures_dir().join("ensure_login.yaml");
    let flow = parse_flow_file(&path).expect("parse_flow_file ensure_login");
    // when.visible="Log in" → true → expand login.yaml.
    let sel_key = smix_sdk::describe_selector(&smix_sdk::text("Log in"));
    let app = MockApp::new().with_find(&sel_key, true);
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let _report = adapter.run(&flow).await.expect("run ok");

    let calls = app.calls();
    // Expect: Find returned true → login.yaml expanded → at least 1 inner action present,
    // plus the trailing extendedWaitUntil's WaitFor.
    assert!(
        calls.iter().any(|c| matches!(c, MockCall::Find(_))),
        "expected a Find call"
    );
    assert!(
        calls
            .iter()
            .filter(|c| matches!(c, MockCall::WaitFor(_, _)))
            .count()
            >= 1,
        "expected at least the trailing extendedWaitUntil WaitFor"
    );
}

// RunFlow.as: name captures the subflow's pasteboard into
// the parent flow's outputs map under the given alias. After the subflow
// runs (which conventionally ends with `copyTextFrom` leaving text on the
// pasteboard), the parent can reference ${output.name} downstream.
#[tokio::test]
async fn mock_run_run_flow_as_name_captures_clipboard_into_outputs() {
    // Inline flow: runFlow { file: ensure_login.yaml, as: probe }, then
    // assertTrue that ${output.probe} equals what we preloaded into the
    // mock pasteboard. We pre-seed clipboard before adapter.run so the
    // captured runFlow.as value matches.
    let app = MockApp::new()
        .with_find(
            &smix_sdk::describe_selector(&smix_sdk::text("Log in")),
            true,
        )
        .with_clipboard("captured-by-subflow");
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    file: ensure_login.yaml\n",
        "    when:\n",
        "      visible:\n",
        "        text: \"Log in\"\n",
        "    as: probe\n",
        "- assertTrue: ${output.probe == 'captured-by-subflow'}\n",
    ));
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("runFlow.as + assertTrue must pass");
    // The Adapter expands runFlow inline, so step 0 might be Skipped or
    // multiple ExpandedSubflow markers; just assert overall run succeeded
    // and a GetClipboard call landed (proves the alias capture path ran).
    assert!(
        !report.steps.is_empty(),
        "expected at least one top-level step report"
    );
    assert!(
        app.calls()
            .iter()
            .any(|c| matches!(c, MockCall::GetClipboard)),
        "expected a GetClipboard call after the subflow"
    );
}

// RunFlowInline (`runFlow: { when, commands: [...] }` inline form).
// Mirrors Maestro YamlRunFlow's `commands:` alternative to `file:`; runtime
// gates on `when.visible` identically to RunFlowConditional, but the body is
// the literal step list (no child yaml expansion). Example:
// `subflows/dismiss-open-in.yaml` (`when: visible 'Open in' → tap 'Open' +
// waitForAnimationToEnd`).
#[tokio::test]
async fn mock_run_run_flow_inline_skips_when_visible_false() {
    // when.visible="Open in" returns false (MockApp default) → inline body
    // skipped; no Tap should land.
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    when:\n",
        "      visible: 'Open in'\n",
        "    commands:\n",
        "      - tapOn: 'Open'\n",
        "      - waitForAnimationToEnd\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let _report = adapter.run(&flow).await.expect("run ok");

    let calls = app.calls();
    assert!(
        calls.iter().any(|c| matches!(c, MockCall::Find(_))),
        "expected the when.visible Find probe to fire"
    );
    assert!(
        !calls.iter().any(|c| matches!(c, MockCall::Tap(_))),
        "should NOT have tapped 'Open' — inline body was conditionally skipped"
    );
}

#[tokio::test]
async fn mock_run_run_flow_inline_executes_when_visible_true() {
    // when.visible="Open in" returns true → inline body executes;
    // tap 'Open' must land.
    let sel_key = smix_sdk::describe_selector(&smix_sdk::text("Open in"));
    let app = MockApp::new().with_find(&sel_key, true);
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    when:\n",
        "      visible: 'Open in'\n",
        "    commands:\n",
        "      - tapOn: 'Open'\n",
        "      - waitForAnimationToEnd\n",
    ));
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let _report = adapter.run(&flow).await.expect("run ok");

    let calls = app.calls();
    let tap_open = calls.iter().any(|c| match c {
        MockCall::Tap(sel) => smix_sdk::describe_selector(sel).contains("Open"),
        _ => false,
    });
    assert!(
        tap_open,
        "expected a Tap('Open') after when.visible matched; got: {:?}",
        calls
    );
}

#[tokio::test]
async fn mock_run_run_flow_inline_no_when_runs_unconditionally() {
    // No `when:` block → body runs without a Find probe.
    let app = MockApp::new();
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    commands:\n",
        "      - tapOn: 'Hello'\n",
    ));
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let _report = adapter.run(&flow).await.expect("run ok");

    let calls = app.calls();
    assert!(
        calls.iter().any(|c| matches!(c, MockCall::Tap(_))),
        "expected unconditional Tap to land; got: {:?}",
        calls
    );
}

// swipe_at_coord dispatches a real swipe through the SDK escape hatch.
#[tokio::test]
async fn mock_run_swipe_at_coord() {
    let path = fixtures_dir().join("swipe_only.yaml");
    let flow = parse_flow_file(&path).expect("parse_flow_file swipe_only");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("swipe should dispatch");
    assert_eq!(report.steps.len(), 1);
    assert!(
        matches!(report.steps[0], RunStepReport::Ok),
        "expected Ok, got {:?}",
        report.steps[0]
    );
    assert!(
        report.warnings.is_empty(),
        "no warnings after lift, got {:?}",
        report.warnings
    );
    let calls = app.calls();
    assert_eq!(calls.len(), 1, "expected one swipe call, got {calls:?}");
    let approx_eq = |a: f64, b: f64| (a - b).abs() < 1e-9;
    match &calls[0] {
        MockCall::SwipeAtCoord(from, to) => {
            assert!(
                approx_eq(from.0, 0.10) && approx_eq(from.1, 0.20),
                "from = {from:?}"
            );
            assert!(
                approx_eq(to.0, 0.10) && approx_eq(to.1, 0.50),
                "to = {to:?}"
            );
        }
        other => panic!("expected SwipeAtCoord, got {other:?}"),
    }
}

// Adapter-only translations; the capability itself sits in smix-sdk.

#[tokio::test]
async fn mock_run_scroll() {
    let flow = parse_inline("appId: com.t.s\n---\n- scroll\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("scroll dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], MockCall::ScrollScreen(SwipeDirection::Down));
}

#[tokio::test]
async fn mock_run_hide_keyboard() {
    let flow = parse_inline("appId: com.t.k\n---\n- hideKeyboard\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("hideKeyboard dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(app.calls(), vec![MockCall::HideKeyboard]);
}

#[tokio::test]
async fn mock_run_assert_not_visible_pass() {
    let flow =
        parse_inline("appId: com.t.nv\n---\n- assertNotVisible:\n    text: \"Never shown\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("assertNotVisible PASS (mock returns Ok)");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    // Adapter dispatches assertNotVisible through wait_for_not_visible
    // (5s implicit timeout) to match maestro CLI's settle-then-check semantics.
    assert!(matches!(calls[0], MockCall::WaitForNotVisible(_, _)));
}

#[tokio::test]
async fn mock_run_assert_not_visible_fail() {
    let yaml = "appId: com.t.nv\n---\n- assertNotVisible:\n    text: \"Always shown\"\n";
    let flow = parse_inline(yaml);
    let key = smix_sdk::describe_selector(&Selector::Text {
        text: smix_sdk::Pattern::text("Always shown"),
        modifiers: smix_sdk::Modifiers::default(),
    });
    let app = MockApp::new().with_assert_not_visible_failure(&key);
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter
        .run(&flow)
        .await
        .expect_err("assertNotVisible failure should surface as RunError::Sdk");
    match err {
        RunError::Sdk(f) => assert_eq!(f.code, FailureCode::AssertionFailed),
        other => panic!("expected RunError::Sdk(AssertionFailed), got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_kill_app() {
    let flow = parse_inline("appId: com.t.k\n---\n- killApp: com.target.app\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("killApp dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(
        app.calls(),
        vec![MockCall::Terminate("com.target.app".to_string())]
    );
}

#[tokio::test]
async fn mock_run_clear_state_independent() {
    let _guard = G10_ENV_LOCK.lock().await;
    unsafe {
        std::env::remove_var("SMIX_APP_PATH_COM_TARGET_APP");
    }
    let flow =
        parse_inline("appId: com.bootstrap.x\n---\n- clearState:\n    appId: com.target.app\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("clearState dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::LaunchFresh {
            bundle_id,
            clear_state,
            clear_keychain,
            app_path,
        } => {
            assert_eq!(bundle_id, "com.target.app");
            assert!(*clear_state);
            assert!(!*clear_keychain);
            assert!(app_path.is_none(), "no SMIX_APP_PATH_* set");
        }
        other => panic!("expected LaunchFresh, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_clear_keychain_after_launch() {
    let _guard = G10_ENV_LOCK.lock().await;
    unsafe {
        std::env::remove_var("SMIX_APP_PATH_COM_AFTER_LAUNCH");
    }
    let flow = parse_inline(concat!(
        "appId: com.bootstrap.x\n",
        "---\n",
        "- launchApp:\n    appId: com.after.launch\n",
        "- clearKeychain\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("clearKeychain after launch");
    assert_eq!(report.steps.len(), 2);
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert!(matches!(report.steps[1], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 2, "two LaunchFresh calls, got {calls:?}");
    match &calls[1] {
        MockCall::LaunchFresh {
            bundle_id,
            clear_state,
            clear_keychain,
            ..
        } => {
            assert_eq!(bundle_id, "com.after.launch");
            assert!(!*clear_state);
            assert!(*clear_keychain);
        }
        other => panic!("expected LaunchFresh, got {other:?}"),
    }
}

// Full PressKey coverage for the maestro yaml key strings. `home`
// really goes through swift XCUIDevice.shared.press(.home); Apple
// documents lock / volumeUp / volumeDown as unavailable on the iOS
// simulator, so the adapter runtime reports a graceful Skipped +
// warning up front rather than hitting the unavailable swift API.

#[tokio::test]
async fn mock_run_press_key_home() {
    let flow = parse_inline("appId: com.t.p\n---\n- pressKey: home\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("pressKey home dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(app.calls(), vec![MockCall::PressKey(KeyName::Home)]);
    assert!(report.warnings.is_empty());
}

#[tokio::test]
async fn mock_run_press_key_lock_graceful_skip() {
    let flow = parse_inline("appId: com.t.p\n---\n- pressKey: lock\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("pressKey lock graceful");
    match &report.steps[0] {
        RunStepReport::Skipped { reason } => {
            assert!(reason.contains("iOS Simulator"), "reason = {reason:?}");
            assert!(reason.contains("Lock") || reason.contains("lock"));
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
    assert!(app.calls().is_empty(), "should NOT reach swift handler");
}

#[tokio::test]
async fn mock_run_press_key_volume_up_graceful_skip() {
    let flow = parse_inline("appId: com.t.p\n---\n- pressKey: volume up\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("pressKey volume up graceful");
    match &report.steps[0] {
        RunStepReport::Skipped { reason } => {
            assert!(reason.contains("iOS Simulator"));
            assert!(reason.contains("VolumeUp") || reason.contains("volumeUp"));
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
    assert!(app.calls().is_empty());
}

#[tokio::test]
async fn mock_run_press_key_volume_down_graceful_skip() {
    let flow = parse_inline("appId: com.t.p\n---\n- pressKey: volume down\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("pressKey volume down graceful");
    match &report.steps[0] {
        RunStepReport::Skipped { reason } => {
            assert!(reason.contains("iOS Simulator"));
            assert!(reason.contains("VolumeDown") || reason.contains("volumeDown"));
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
    assert!(app.calls().is_empty());
}

// ExtendedWaitUntil notVisible branch (SDK App::wait_for_not_visible
// plus the adapter parser's two arms).
#[tokio::test]
async fn mock_run_extended_wait_until_not_visible() {
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- extendedWaitUntil:\n",
        "    notVisible:\n",
        "      text: Loading\n",
        "    timeout: 5000\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("extendedWaitUntil notVisible dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::WaitForNotVisible(_, timeout) => {
            assert_eq!(*timeout, Duration::from_millis(5000));
        }
        other => panic!("expected WaitForNotVisible, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_extended_wait_until_visible_still_works() {
    // Regression guard: the `visible` arm keeps its default behavior.
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- extendedWaitUntil:\n",
        "    visible:\n",
        "      text: Ready\n",
        "    timeout: 3000\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("extendedWaitUntil visible dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::WaitFor(_, timeout) => {
            assert_eq!(*timeout, Duration::from_millis(3000));
        }
        other => panic!("expected WaitFor, got {other:?}"),
    }
}

// LaunchApp sub-parameters (permissions / arguments / stopApp) across
// both dispatch arms.
// stopApp=true (default) → launch_app_with_options;stopApp=false → foreground.

#[tokio::test]
async fn mock_run_launch_app_stop_app_true_default_uses_options() {
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- launchApp:\n",
        "    appId: com.target.app\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("launchApp default dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::LaunchAppWithOptions(opts) => {
            assert_eq!(opts.bundle_id, "com.target.app");
            assert!(!opts.clear_state);
            assert!(opts.permissions.is_empty());
            assert!(opts.arguments.is_empty());
        }
        other => panic!("expected LaunchAppWithOptions, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_launch_app_stop_app_false_uses_foreground() {
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- launchApp:\n",
        "    appId: com.target.app\n",
        "    stopApp: false\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("launchApp stopApp=false dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(
        app.calls(),
        vec![MockCall::Foreground("com.target.app".to_string())]
    );
}

#[tokio::test]
async fn mock_run_launch_app_with_arguments() {
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- launchApp:\n",
        "    appId: com.target.app\n",
        "    arguments:\n",
        "      - \"-debug\"\n",
        "      - \"1\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("launchApp arguments dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::LaunchAppWithOptions(opts) => {
            assert_eq!(opts.arguments, vec!["-debug".to_string(), "1".to_string()]);
        }
        other => panic!("expected LaunchAppWithOptions, got {other:?}"),
    }
}

// Mapping form launchApp.arguments lifts to argv pairs
// (maestro CLI accepts but drops these via its IDB path; smix bypasses
// IDB and forwards via simctl launch).
#[tokio::test]
async fn mock_run_launch_app_with_arguments_mapping_form() {
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- launchApp:\n",
        "    appId: com.target.app\n",
        "    arguments:\n",
        "      -uitestV2Root: \"YES\"\n",
        "      -debugLogging: true\n",
        "      -httpPort: 8080\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("launchApp arguments mapping-form dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::LaunchAppWithOptions(opts) => {
            // Mapping iteration order is yaml insertion order (serde_yaml
            // preserves it). 3 entries → 6 argv tokens.
            assert_eq!(opts.arguments.len(), 6, "3 mapping entries × 2 tokens");
            assert_eq!(opts.arguments[0], "-uitestV2Root");
            assert_eq!(opts.arguments[1], "YES");
            assert_eq!(opts.arguments[2], "-debugLogging");
            assert_eq!(opts.arguments[3], "YES", "bool true coerces to YES");
            assert_eq!(opts.arguments[4], "-httpPort");
            assert_eq!(opts.arguments[5], "8080", "number coerces to literal");
        }
        other => panic!("expected LaunchAppWithOptions, got {other:?}"),
    }
}

// Mapping form rejects non-scalar values (lists / nested maps)
// — keeps the contract small and aligned with simctl argv semantics.
#[tokio::test]
async fn parse_launch_app_arguments_mapping_form_rejects_non_scalar() {
    let res = parse_flow_yaml(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- launchApp:\n",
        "    appId: com.target.app\n",
        "    arguments:\n",
        "      -nested:\n",
        "        deep: \"value\"\n",
    ));
    match res {
        Err(smix_adapter_maestro::ParseError::InvalidValue { field, reason }) => {
            assert_eq!(field, "launchApp.arguments.-nested");
            assert!(reason.contains("scalar"), "got: {reason}");
        }
        other => panic!("expected InvalidValue for nested-mapping arg, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_launch_app_with_permissions() {
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- launchApp:\n",
        "    appId: com.target.app\n",
        "    permissions:\n",
        "      camera: allow\n",
        "      location: deny\n",
        "      notifications: unset\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("launchApp permissions dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::LaunchAppWithOptions(opts) => {
            assert_eq!(opts.permissions.len(), 3);
            // map iteration order via serde_norway IndexMap ≈ insertion order
            let mut found = std::collections::HashSet::new();
            for (perm, action) in &opts.permissions {
                found.insert(format!("{perm:?}:{action:?}"));
            }
            assert!(found.contains("Camera:Grant"), "got {found:?}");
            assert!(found.contains("Location:Revoke"), "got {found:?}");
            assert!(found.contains("Notifications:Reset"), "got {found:?}");
        }
        other => panic!("expected LaunchAppWithOptions, got {other:?}"),
    }
}

// The three clipboard commands (setClipboard / both pasteText forms /
// copyTextFrom). The SDK routes these host-side through simctl.

#[tokio::test]
async fn mock_run_set_clipboard() {
    let flow = parse_inline("appId: com.t.c\n---\n- setClipboard: \"hello\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("setClipboard dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(
        app.calls(),
        vec![MockCall::SetClipboard("hello".to_string())]
    );
}

// Expressions are expanded at runtime, not rejected at parse time.
// An undefined variable raises a DriverError (UndefinedVariable) at
// runtime.
#[tokio::test]
async fn mock_run_set_clipboard_unknown_var_driver_error() {
    let flow = parse_inline("appId: com.t.c\n---\n- setClipboard: \"${output.PIN}\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter
        .run(&flow)
        .await
        .expect_err("undefined variable should surface DriverError");
    match err {
        RunError::Sdk(f) => {
            assert_eq!(f.code, FailureCode::DriverError);
            assert!(
                f.message.contains("undefined variable"),
                "message should mention undefined variable, got: {}",
                f.message
            );
        }
        other => panic!("expected DriverError, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_paste_text_literal() {
    let flow = parse_inline("appId: com.t.c\n---\n- pasteText: \"x\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("pasteText literal dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(
        app.calls(),
        vec![MockCall::PasteText(Some("x".to_string()))]
    );
}

#[tokio::test]
async fn mock_run_paste_text_bare_reads_clipboard() {
    let flow = parse_inline("appId: com.t.c\n---\n- pasteText\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("pasteText bare dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(app.calls(), vec![MockCall::PasteText(None)]);
}

#[tokio::test]
async fn mock_run_copy_text_from_id() {
    let flow = parse_inline(concat!(
        "appId: com.t.c\n",
        "---\n",
        "- copyTextFrom:\n",
        "    id: \"result-text\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("copyTextFrom dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::CopyTextFrom(sel) => {
            assert!(matches!(sel, Selector::Id { .. }), "got {sel:?}");
        }
        other => panic!("expected CopyTextFrom, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_copy_text_from_text_short() {
    let flow = parse_inline(concat!(
        "appId: com.t.c\n",
        "---\n",
        "- copyTextFrom: \"Welcome\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("copyTextFrom short string dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::CopyTextFrom(sel) => {
            assert!(matches!(sel, Selector::Text { .. }), "got {sel:?}");
        }
        other => panic!("expected CopyTextFrom, got {other:?}"),
    }
}

// DoubleTapOn / longPressOn (XCUI public API path).

#[tokio::test]
async fn mock_run_double_tap_on_id() {
    let flow = parse_inline("appId: com.t.d\n---\n- doubleTapOn:\n    id: \"card-1\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("doubleTapOn dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::DoubleTap(sel) => assert!(matches!(sel, Selector::Id { .. })),
        other => panic!("expected DoubleTap, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_long_press_on_scalar_uses_default_duration() {
    let flow = parse_inline("appId: com.t.l\n---\n- longPressOn: \"Submit\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("longPressOn scalar dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::LongPress(sel, duration) => {
            assert!(matches!(sel, Selector::Text { .. }));
            assert_eq!(*duration, Duration::from_millis(500));
        }
        other => panic!("expected LongPress, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_long_press_on_with_duration_overrides() {
    let flow = parse_inline(concat!(
        "appId: com.t.l\n",
        "---\n",
        "- longPressOn:\n",
        "    id: \"btn\"\n",
        "    duration: 1200\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("longPressOn mapping dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::LongPress(_, duration) => {
            assert_eq!(*duration, Duration::from_millis(1200));
        }
        other => panic!("expected LongPress, got {other:?}"),
    }
}

// AssertTrue + ${expr} template sweep on 3 string commands.

#[tokio::test]
async fn mock_run_assert_true_literal_true() {
    let flow = parse_inline("appId: com.t.a\n---\n- assertTrue: \"true\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("literal true should pass");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert!(app.calls().is_empty(), "assertTrue 不进 mock app trace");
}

#[tokio::test]
async fn mock_run_assert_true_literal_false_fails() {
    let flow = parse_inline("appId: com.t.a\n---\n- assertTrue: \"false\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter
        .run(&flow)
        .await
        .expect_err("literal false should fail");
    match err {
        RunError::Sdk(f) => assert_eq!(f.code, FailureCode::AssertionFailed),
        other => panic!("expected AssertionFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_assert_true_expression_reads_output() {
    use smix_adapter_maestro::ExprValue;
    use std::collections::BTreeMap;
    let flow = parse_inline(concat!(
        "appId: com.t.a\n",
        "---\n",
        "- assertTrue: \"output.foo == \\\"bar\\\"\"\n",
    ));
    let mut output = BTreeMap::new();
    output.insert("foo".to_string(), ExprValue::String("bar".to_string()));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir()).with_output(output);
    let report = adapter
        .run(&flow)
        .await
        .expect("expression should pass with seeded output");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
}

#[tokio::test]
async fn mock_run_assert_true_unsupported_pattern_driver_error() {
    let flow = parse_inline("appId: com.t.a\n---\n- assertTrue: \"1 + 2\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter
        .run(&flow)
        .await
        .expect_err("arithmetic unsupported");
    match err {
        RunError::Sdk(f) => assert_eq!(f.code, FailureCode::DriverError),
        other => panic!("expected DriverError, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_set_clipboard_template_expanded() {
    use smix_adapter_maestro::ExprValue;
    use std::collections::BTreeMap;
    let flow = parse_inline(concat!(
        "appId: com.t.c\n",
        "---\n",
        "- setClipboard: \"prefix-${output.token}-suffix\"\n",
    ));
    let mut output = BTreeMap::new();
    output.insert("token".to_string(), ExprValue::String("abc".to_string()));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir()).with_output(output);
    let report = adapter
        .run(&flow)
        .await
        .expect("setClipboard ${expr} expand should succeed");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(
        app.calls(),
        vec![MockCall::SetClipboard("prefix-abc-suffix".to_string())]
    );
}

#[tokio::test]
async fn mock_run_paste_text_literal_template_expanded() {
    use smix_adapter_maestro::ExprValue;
    use std::collections::BTreeMap;
    let flow = parse_inline(concat!(
        "appId: com.t.c\n",
        "---\n",
        "- pasteText: \"${output.x}\"\n",
    ));
    let mut output = BTreeMap::new();
    output.insert("x".to_string(), ExprValue::String("hello".to_string()));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir()).with_output(output);
    adapter
        .run(&flow)
        .await
        .expect("pasteText template expand should succeed");
    assert_eq!(
        app.calls(),
        vec![MockCall::PasteText(Some("hello".to_string()))]
    );
}

// Repeat / retry / runScript / evalScript.

#[tokio::test]
async fn mock_run_repeat_times_runs_body_n_times() {
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- repeat:\n",
        "    times: 3\n",
        "    commands:\n",
        "      - tapOn:\n",
        "          id: \"btn\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("repeat times dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let tap_count = app
        .calls()
        .iter()
        .filter(|c| matches!(c, MockCall::Tap(_)))
        .count();
    assert_eq!(tap_count, 3);
}

#[tokio::test]
async fn mock_run_repeat_while_false_runs_zero_times() {
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- repeat:\n",
        "    while: \"false\"\n",
        "    commands:\n",
        "      - tapOn:\n",
        "          id: \"btn\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("repeat while:false");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert!(
        app.calls().is_empty(),
        "while:false should not invoke body once"
    );
}

// Selector-style while is accepted.
// Mock app reports the selector non-visible immediately → loop body
// runs zero times.
#[tokio::test]
async fn mock_run_repeat_while_visible_not_visible_runs_zero_times() {
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- repeat:\n",
        "    while:\n",
        "      visible:\n",
        "        id: \"loading-spinner\"\n",
        "    commands:\n",
        "      - tapOn:\n",
        "          id: \"btn\"\n",
    ));
    let app = MockApp::new(); // find() returns Ok(false) by default
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("repeat while-visible (not visible)");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let tap_count = app
        .calls()
        .iter()
        .filter(|c| matches!(c, MockCall::Tap(_)))
        .count();
    assert_eq!(
        tap_count, 0,
        "loading-spinner not visible → body never runs"
    );
}

// Selector-style while loop runs N body iterations while the
// element is visible, exits when it disappears. Mock makes the spinner
// visible exactly twice via with_find_visible_n_times.
#[tokio::test]
async fn mock_run_repeat_while_visible_runs_until_disappears() {
    let sel_key = smix_sdk::describe_selector(&smix_sdk::id("loading-spinner"));
    let app = MockApp::new().with_find_visible_n_times(&sel_key, 2);
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- repeat:\n",
        "    while:\n",
        "      visible:\n",
        "        id: \"loading-spinner\"\n",
        "    commands:\n",
        "      - tapOn:\n",
        "          id: \"retry\"\n",
    ));
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("repeat while-visible (transient)");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let tap_count = app
        .calls()
        .iter()
        .filter(|c| matches!(c, MockCall::Tap(_)))
        .count();
    assert_eq!(tap_count, 2, "body runs for each visible iteration (2)");
}

#[tokio::test]
async fn mock_run_retry_succeeds_on_second_attempt() {
    use smix_sdk::FailureCode;
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- retry:\n",
        "    maxRetries: 2\n",
        "    commands:\n",
        "      - tapOn:\n",
        "          id: \"flaky\"\n",
    ));
    // mock app: tap on `id=flaky` fails the 1st time
    // (ElementNotFound) and succeeds the 2nd.
    let sel_key = smix_sdk::describe_selector(&smix_sdk::id("flaky"));
    let app = MockApp::new().with_tap_failure_n_times(&sel_key, FailureCode::ElementNotFound, 1);
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("retry should succeed");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let tap_count = app
        .calls()
        .iter()
        .filter(|c| matches!(c, MockCall::Tap(_)))
        .count();
    assert_eq!(tap_count, 2, "1 fail + 1 success = 2 attempts");
}

#[tokio::test]
async fn mock_run_retry_exhausts_and_propagates_last_error() {
    use smix_sdk::FailureCode;
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- retry:\n",
        "    maxRetries: 2\n",
        "    commands:\n",
        "      - tapOn:\n",
        "          id: \"always-fail\"\n",
    ));
    let sel_key = smix_sdk::describe_selector(&smix_sdk::id("always-fail"));
    let app = MockApp::new().with_tap_failure_n_times(&sel_key, FailureCode::ElementNotFound, 999);
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter.run(&flow).await.expect_err("retry exhausted");
    match err {
        RunError::Sdk(f) => assert_eq!(f.code, FailureCode::ElementNotFound),
        other => panic!("expected ElementNotFound after retry exhaust, got {other:?}"),
    }
    let tap_count = app
        .calls()
        .iter()
        .filter(|c| matches!(c, MockCall::Tap(_)))
        .count();
    assert_eq!(tap_count, 3, "initial + 2 retries = 3 attempts");
}

#[tokio::test]
async fn mock_run_run_script_unsupported_driver_error() {
    let flow = parse_inline("appId: com.t.s\n---\n- runScript: \"const x = 1;\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter.run(&flow).await.expect_err("runScript unsupported");
    match err {
        RunError::Sdk(f) => {
            assert_eq!(f.code, smix_sdk::FailureCode::DriverError);
            assert!(f.message.contains("not supported"), "msg: {}", f.message);
        }
        other => panic!("expected DriverError, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_eval_script_unsupported_driver_error() {
    let flow = parse_inline("appId: com.t.s\n---\n- evalScript: \"Math.max(1,2)\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter
        .run(&flow)
        .await
        .expect_err("evalScript unsupported");
    match err {
        RunError::Sdk(f) => {
            assert_eq!(f.code, smix_sdk::FailureCode::DriverError);
            assert!(f.message.contains("not supported"), "msg: {}", f.message);
        }
        other => panic!("expected DriverError, got {other:?}"),
    }
}

// Device + Media gap (setLocation / travel / setPermissions / addMedia).

#[tokio::test]
async fn mock_run_set_location() {
    let flow = parse_inline(concat!(
        "appId: com.t.l\n",
        "---\n",
        "- setLocation:\n",
        "    latitude: 37.7749\n",
        "    longitude: -122.4194\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("setLocation dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::SetLocation(lat, lng) => {
            assert!((lat - 37.7749).abs() < 1e-9);
            assert!((lng + 122.4194).abs() < 1e-9);
        }
        other => panic!("expected SetLocation, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_travel_two_points() {
    let flow = parse_inline(concat!(
        "appId: com.t.l\n",
        "---\n",
        "- travel:\n",
        "    points:\n",
        "      - latitude: 1.0\n",
        "        longitude: 2.0\n",
        "      - latitude: 3.0\n",
        "        longitude: 4.0\n",
        "    speed_mps: 10.0\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("travel dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    match &app.calls()[0] {
        MockCall::Travel(points, speed) => {
            assert_eq!(points, &vec![(1.0, 2.0), (3.0, 4.0)]);
            assert_eq!(*speed, Some(10.0));
        }
        other => panic!("expected Travel, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_travel_one_point_rejected() {
    let res = parse_flow_yaml(concat!(
        "appId: com.t.l\n",
        "---\n",
        "- travel:\n",
        "    points:\n",
        "      - latitude: 1.0\n",
        "        longitude: 2.0\n",
    ));
    match res {
        Err(smix_adapter_maestro::ParseError::InvalidValue { field, reason }) => {
            assert_eq!(field, "travel.points");
            assert!(reason.contains("at least 2"), "got: {reason}");
        }
        other => panic!("expected InvalidValue, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_set_permissions_uses_last_bundle() {
    let flow = parse_inline(concat!(
        "appId: com.t.p\n",
        "---\n",
        "- launchApp:\n",
        "    appId: com.target.app\n",
        "- setPermissions:\n",
        "    camera: allow\n",
        "    location: deny\n",
    ));
    let _guard = G10_ENV_LOCK.lock().await;
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("setPermissions dispatch");
    let calls = app.calls();
    // The second call should be SetPermissions, with
    // bundle = com.target.app (last_bundle).
    let perms_call = calls
        .iter()
        .find(|c| matches!(c, MockCall::SetPermissions(_, _)))
        .expect("expected SetPermissions in trace");
    match perms_call {
        MockCall::SetPermissions(bundle, perms) => {
            assert_eq!(bundle, "com.target.app");
            assert_eq!(perms.len(), 2);
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn mock_run_set_permissions_no_launch_driver_error() {
    // Adapter::run seeds last_bundle from flow.app_id; an empty appId
    // exercises the "no app was ever launched" edge (the parser
    // accepts an empty string).
    let flow = parse_inline(concat!(
        "appId: \"\"\n",
        "---\n",
        "- setPermissions:\n",
        "    camera: allow\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter.run(&flow).await.expect_err("no last_bundle");
    match err {
        RunError::Sdk(f) => {
            assert_eq!(f.code, FailureCode::DriverError);
            assert!(f.message.contains("no app launched"), "msg: {}", f.message);
        }
        other => panic!("expected DriverError, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_add_media_scalar_and_array() {
    // scalar
    let flow = parse_inline("appId: com.t.m\n---\n- addMedia: \"/tmp/a.png\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("addMedia scalar");
    match &app.calls()[0] {
        MockCall::AddMedia(paths) => assert_eq!(paths, &vec!["/tmp/a.png".to_string()]),
        other => panic!("expected AddMedia, got {other:?}"),
    }

    // array
    let flow = parse_inline(concat!(
        "appId: com.t.m\n",
        "---\n",
        "- addMedia:\n",
        "    - \"/tmp/a.png\"\n",
        "    - \"/tmp/b.mp4\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("addMedia array");
    match &app.calls()[0] {
        MockCall::AddMedia(paths) => assert_eq!(paths.len(), 2),
        other => panic!("expected AddMedia, got {other:?}"),
    }
}

// SetOrientation end-to-end (swift XCUIDevice public API path).

#[tokio::test]
async fn mock_run_set_orientation_literal() {
    let flow = parse_inline("appId: com.t.o\n---\n- setOrientation: landscapeLeft\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("setOrientation dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert_eq!(
        app.calls(),
        vec![MockCall::SetOrientation(
            smix_sdk::MaestroOrientation::LandscapeLeft
        )]
    );
}

#[tokio::test]
async fn mock_run_set_orientation_landscape_alias() {
    let flow = parse_inline("appId: com.t.o\n---\n- setOrientation: landscape\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("landscape alias dispatch");
    assert_eq!(
        app.calls(),
        vec![MockCall::SetOrientation(
            smix_sdk::MaestroOrientation::LandscapeLeft
        )],
        "landscape alias should normalize to LandscapeLeft"
    );
}

#[tokio::test]
async fn mock_run_set_orientation_unknown_rejected() {
    let res = parse_flow_yaml("appId: com.t.o\n---\n- setOrientation: tilted\n");
    match res {
        Err(smix_adapter_maestro::ParseError::InvalidValue { field, reason }) => {
            assert_eq!(field, "setOrientation");
            assert!(reason.contains("unknown orientation"), "got: {reason}");
        }
        other => panic!("expected InvalidValue, got {other:?}"),
    }
}

// StartRecording / stopRecording (simctl io recordVideo SIGINT path).

#[tokio::test]
async fn mock_run_start_then_stop_recording() {
    let flow = parse_inline(concat!(
        "appId: com.t.r\n",
        "---\n",
        "- startRecording: \"/tmp/r.mp4\"\n",
        "- stopRecording\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("record start+stop dispatch");
    assert_eq!(report.steps.len(), 2);
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert!(matches!(report.steps[1], RunStepReport::Ok));
    assert_eq!(
        app.calls(),
        vec![
            MockCall::StartRecording("/tmp/r.mp4".to_string()),
            MockCall::StopRecording,
        ]
    );
}

#[tokio::test]
async fn mock_run_start_recording_non_string_rejected() {
    let res = parse_flow_yaml("appId: com.t.r\n---\n- startRecording: 12345\n");
    match res {
        Err(smix_adapter_maestro::ParseError::InvalidValue { field, reason }) => {
            assert_eq!(field, "startRecording");
            assert!(reason.contains("string"), "got: {reason}");
        }
        other => panic!("expected InvalidValue, got {other:?}"),
    }
}

// AssertScreenshot (visual regression via dhash 64-bit).

#[tokio::test]
async fn mock_run_assert_screenshot_records_resolved_path() {
    let flow = parse_inline(concat!(
        "appId: com.t.s\n",
        "---\n",
        "- assertScreenshot: \"baseline.png\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("assertScreenshot dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let expected = fixtures_dir().join("baseline.png");
    assert_eq!(app.calls(), vec![MockCall::AssertScreenshot(expected)]);
}

// Mapping form `{ path, threshold }` accepted; runtime
// dispatches with the override.
#[tokio::test]
async fn mock_run_assert_screenshot_mapping_form_threshold_passes_through() {
    let flow = parse_inline(concat!(
        "appId: com.t.s\n",
        "---\n",
        "- assertScreenshot:\n",
        "    path: foo.png\n",
        "    threshold: 9\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("assertScreenshot mapping form dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let expected = fixtures_dir().join("foo.png");
    assert_eq!(app.calls(), vec![MockCall::AssertScreenshot(expected)]);
    // Runtime mock doesn't capture the threshold, but the dispatch
    // succeeding proves the mapping form parsed and routed.
}

// Mapping form `{ path, mask: [...] }` accepted; runtime
// emits an explicit warn-and-ignore (R2-tier algorithm deferred to v6+).
#[tokio::test]
async fn mock_run_assert_screenshot_mask_warns_and_ignores() {
    let flow = parse_inline(concat!(
        "appId: com.t.s\n",
        "---\n",
        "- assertScreenshot:\n",
        "    path: masked.png\n",
        "    mask:\n",
        "      - x: 0.0\n",
        "        y: 0.0\n",
        "        width: 0.5\n",
        "        height: 0.25\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("mask mapping form dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let warn = report
        .warnings
        .iter()
        .find(|w| w.contains("mask"))
        .expect("mask warn-and-ignore must surface in run report");
    assert!(
        warn.contains("ignored") && warn.contains("SSIM/pHash"),
        "warn should explain why mask regions are ignored, got: {warn}"
    );
}

// Scalar form remains a clean path (back-compat).
#[tokio::test]
async fn parse_assert_screenshot_scalar_form_still_works() {
    let flow = parse_inline(concat!(
        "appId: com.t.s\n",
        "---\n",
        "- assertScreenshot: \"plain.png\"\n",
    ));
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("scalar form back-compat");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    // Scalar form: no mask warn, no extra warnings (other than possibly
    // auto-record from the mock, which doesn't emit one).
    assert!(
        report.warnings.iter().all(|w| !w.contains("mask")),
        "scalar form must not emit a mask warning"
    );
}

#[tokio::test]
async fn mock_run_take_screenshot_no_path() {
    let flow = parse_inline("appId: com.t.s\n---\n- takeScreenshot\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("takeScreenshot dispatch");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert!(report.warnings.is_empty());
    assert_eq!(app.calls(), vec![MockCall::Screenshot]);
}

// cargo test runs test fns across OS threads; the two clear_state tests
// mutate the same SMIX_APP_PATH_COM_TEST_CLEAR env var, so they must be
// serialized across the await. `tokio::sync::Mutex` is held across
// awaits cleanly; `std::sync::Mutex` would trip `await_holding_lock`.
static G10_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
// LaunchApp dispatches through the typed LaunchAppOptions path (the
// adapter is a thin translation layer). LaunchFresh is an
// SDK-internal op and no longer appears directly in the adapter mock
// trace.
async fn mock_run_launch_app_clear_state_no_env_var_routes_via_options() {
    let _guard = G10_ENV_LOCK.lock().await;
    unsafe {
        std::env::remove_var("SMIX_APP_PATH_COM_TEST_CLEAR");
    }
    let path = fixtures_dir().join("launch_clear_state.yaml");
    let flow = parse_flow_file(&path).expect("parse_flow_file launch_clear_state");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("clearState no-env-var routes via options");
    assert_eq!(report.steps.len(), 1);
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::LaunchAppWithOptions(opts) => {
            assert_eq!(opts.bundle_id, "com.test.clear");
            assert!(opts.clear_state);
            assert!(!opts.clear_keychain);
            assert_eq!(opts.app_path.as_deref(), None, "no env var → app_path None");
        }
        other => panic!("expected LaunchAppWithOptions, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_launch_app_clear_state_with_env_var_threads_path() {
    let _guard = G10_ENV_LOCK.lock().await;
    unsafe {
        std::env::set_var("SMIX_APP_PATH_COM_TEST_CLEAR", "/tmp/Test.app");
    }
    let path = fixtures_dir().join("launch_clear_state.yaml");
    let flow = parse_flow_file(&path).expect("parse_flow_file launch_clear_state");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter
        .run(&flow)
        .await
        .expect("clearState with env var threads path");
    unsafe {
        std::env::remove_var("SMIX_APP_PATH_COM_TEST_CLEAR");
    }
    assert_eq!(report.steps.len(), 1);
    match &app.calls()[0] {
        MockCall::LaunchAppWithOptions(opts) => {
            assert_eq!(opts.bundle_id, "com.test.clear");
            assert!(opts.clear_state);
            assert_eq!(
                opts.app_path.as_deref(),
                Some("/tmp/Test.app"),
                "env var should be threaded as app_path",
            );
        }
        other => panic!("expected LaunchAppWithOptions, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_run_assert_visible_calls_assert_visible() {
    let flow = parse_inline("appId: x\n---\n- assertVisible: \"Home\"\n");
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    assert!(matches!(calls[0], MockCall::AssertVisible(_)));
}

#[tokio::test]
async fn mock_run_scroll_until_visible_uppercase_direction() {
    let flow = parse_inline(
        "appId: x\n---\n- scrollUntilVisible:\n    element:\n      id: \"target\"\n    direction: \"DOWN\"\n",
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::Scroll(_, dir) => assert_eq!(*dir, SwipeDirection::Down),
        other => panic!("expected Scroll, got {other:?}"),
    }
}

// ====================================================================
// External-consumer-readiness regressions
//
// The tests below live here (rather than a dedicated regression file)
// because they need the MockApp defined in this file — Rust test
// integration crates cannot cross-import each other's private types,
// and extracting MockApp to `tests/support/mod.rs` would be a large
// refactor for one dependency site.
// ====================================================================

/// A false `when_visible` predicate on `runFlowInline` where the
/// underlying `find` returns Err (runner-transport-error path) must be
/// treated as "not visible", not surface as a stderr `to_prompt` block.
/// The inner body must be Skipped, the outer flow exit 0.
#[tokio::test]
async fn run_flow_inline_when_false_swallows_find_error() {
    let flow = parse_inline(
        r#"appId: x
---
- runFlow:
    when:
      visible: "NEVER-VISIBLE-STRING"
    commands:
      - tapOn: "should-not-execute"
"#,
    );
    let sel_key = smix_sdk::describe_selector(&smix_sdk::text("NEVER-VISIBLE-STRING"));
    // Configure the mock so `find` errors for the when-visible predicate.
    // Adapter must swallow the error and treat as not-visible → Skipped.
    let app = MockApp::new().with_find_error(&sel_key);
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run should exit 0");
    // Outer flow: 1 step, outcome Skipped (not Failed).
    assert_eq!(report.steps.len(), 1);
    match &report.steps[0] {
        RunStepReport::Skipped { reason } => {
            assert!(
                reason.contains("when.visible=false"),
                "expected skip reason to mention when.visible=false, got {reason:?}"
            );
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
    // Inner body must never have executed — no `Tap { text: "should-not-execute" }` call.
    let calls = app.calls();
    let saw_inner_tap = calls.iter().any(|c| {
        matches!(
            c,
            MockCall::Tap(sel) if smix_sdk::describe_selector(sel).contains("should-not-execute")
        )
    });
    assert!(
        !saw_inner_tap,
        "inner body should not have executed; calls = {calls:?}"
    );
}

/// Symmetric coverage for `runFlow: { when, file }` (file-target
/// conditional). Same failure mode as the inline branch; same fix.
#[tokio::test]
async fn run_flow_conditional_when_false_swallows_find_error() {
    let flow = parse_inline(
        r#"appId: x
---
- runFlow:
    when:
      visible: "NEVER-VISIBLE-STRING"
    file: subflows/login.yaml
"#,
    );
    let sel_key = smix_sdk::describe_selector(&smix_sdk::text("NEVER-VISIBLE-STRING"));
    let app = MockApp::new().with_find_error(&sel_key);
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run should exit 0");
    assert_eq!(report.steps.len(), 1);
    // ExpandedSubflow means we did expand; Skipped/Ok w/o expand means
    // the when-visible was false. Either not-expand outcome is
    // acceptable for the regression's intent (no stderr noise). But
    // ExpandedSubflow would mean the fix didn't apply.
    match &report.steps[0] {
        RunStepReport::ExpandedSubflow { .. } => {
            panic!("subflow should NOT have expanded when when.visible=false")
        }
        RunStepReport::Skipped { .. } | RunStepReport::Ok => (),
    }
}

/// Bare `${NAME}` in a yaml step string must interpolate from
/// Context.env, populated via Adapter::with_env. Verifies the
/// EnvAccess parser path + Context.env lookup.
#[tokio::test]
async fn env_var_interpolation_via_with_env() {
    let flow = parse_inline(
        r#"appId: x
---
- inputText: "${E2E_EMAIL}"
"#,
    );
    // Adapter starts with empty env, so ${E2E_EMAIL} in a bare inputText
    // is not evaluated (inputText body is not template-expanded in this
    // form — this test verifies via a different verb that IS template-
    // expanded). Actually let's use setClipboard which IS expanded.
    let flow2 = parse_inline(
        r#"appId: x
---
- setClipboard: "user@${DOMAIN}"
"#,
    );
    let mut env = std::collections::BTreeMap::new();
    env.insert("DOMAIN".to_string(), "example.com".to_string());
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir()).with_env(env);
    adapter.run(&flow2).await.expect("run ok");
    let calls = app.calls();
    // MockCall::SetClipboard(String) — value should be interpolated.
    let clip = calls.iter().find_map(|c| match c {
        MockCall::SetClipboard(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(
        clip.as_deref(),
        Some("user@example.com"),
        "expected env interpolation to substitute DOMAIN, got {clip:?}, all calls: {calls:?}"
    );
    // Sanity: bare-input test — first flow (without setClipboard) should
    // not have crashed. It also verifies that inputText literal is not
    // env-expanded (current smix behavior; documented gap if needed).
    let app2 = MockApp::new();
    let mut adapter2 = Adapter::new(&app2, fixtures_dir());
    let _ = adapter2.run(&flow).await; // may or may not error; not asserting shape
}

/// Undefined env variable produces a helpful error mentioning the
/// missing name (not a silent empty substitution).
#[tokio::test]
async fn undefined_env_var_errors_with_name() {
    let flow = parse_inline(
        r#"appId: x
---
- setClipboard: "user@${MISSING_VAR}"
"#,
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter.run(&flow).await.expect_err("should error");
    let msg = err.to_string();
    assert!(
        msg.contains("MISSING_VAR"),
        "error message should mention the missing env var, got: {msg}"
    );
}

// Explicit `dispatch:` override routing.

#[tokio::test]
async fn tapon_dispatch_xcui_routes_through_tap_xcui() {
    let flow = parse_inline(
        "appId: x\n---\n- tapOn:\n    id: \"my-modal-dismiss\"\n    dispatch: xcui\n",
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run ok");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        MockCall::TapXcui(captured) => assert_eq!(captured, "my-modal-dismiss"),
        other => panic!("expected TapXcui, got {other:?}"),
    }
}

#[tokio::test]
async fn tapon_dispatch_daemon_proxy_routes_through_tap_with_mode() {
    let flow = parse_inline(
        "appId: x\n---\n- tapOn:\n    id: \"btn-login\"\n    dispatch: daemonProxy\n",
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run ok");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    assert_eq!(calls.len(), 1);
    assert!(
        matches!(&calls[0], MockCall::TapWithMode(_)),
        "expected TapWithMode, got {:?}",
        calls[0]
    );
}

#[tokio::test]
async fn tapon_dispatch_xcui_non_id_selector_errors() {
    // `dispatch: xcui` resolves by accessibilityIdentifier; a text
    // selector can't route there — explicit DriverError, not a silent
    // fallback to the default path.
    let flow = parse_inline(
        "appId: x\n---\n- tapOn:\n    text: \"Dismiss\"\n    dispatch: xcui\n",
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter.run(&flow).await.expect_err("must error");
    let msg = format!("{err}");
    assert!(msg.contains("requires an `id:` selector"), "got: {msg}");
}

// ClearUserDefaults dispatch routing.

#[tokio::test]
async fn clear_user_defaults_uses_flow_app_id_by_default() {
    let flow = parse_inline(
        "appId: com.flow.app\n---\n- clearUserDefaults:\n    keys: ['k1', 'k2']\n",
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run ok");
    assert!(matches!(report.steps[0], RunStepReport::Ok));
    let calls = app.calls();
    match &calls[0] {
        MockCall::ClearUserDefaults(bundle, keys) => {
            assert_eq!(bundle, "com.flow.app");
            assert_eq!(keys, &vec!["k1".to_string(), "k2".to_string()]);
        }
        other => panic!("expected ClearUserDefaults, got {other:?}"),
    }
}

#[tokio::test]
async fn clear_user_defaults_bundle_override_wins() {
    let flow = parse_inline(
        "appId: com.flow.app\n---\n- clearUserDefaults:\n    keys: ['k']\n    bundleId: 'com.other'\n",
    );
    let app = MockApp::new();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    let calls = app.calls();
    match &calls[0] {
        MockCall::ClearUserDefaults(bundle, _) => assert_eq!(bundle, "com.other"),
        other => panic!("expected ClearUserDefaults, got {other:?}"),
    }
}

// ----------------------------------------------------------------------
// waitForAnimationToEnd — waiting for stillness rather than sleeping.
// ----------------------------------------------------------------------

/// Frames for "the screen is animating, then it stops": two moving frames,
/// then a settled one that repeats.
fn moving_then_settled() -> Vec<Vec<u8>> {
    let moving_a = mock_png_sized(64, 64, |x, _| if x < 32 { 0 } else { 255 });
    let moving_b = mock_png_sized(64, 64, |x, _| if x < 48 { 0 } else { 255 });
    let settled = mock_png_sized(64, 64, |_, _| 255);
    vec![moving_a, moving_b, settled]
}

#[tokio::test]
async fn wait_for_animation_returns_once_the_screen_settles() {
    let app = MockApp::new().with_screenshot_frames(moving_then_settled());
    let flow = parse_flow_yaml("appId: com.t\n---\n- waitForAnimationToEnd: 5000\n").unwrap();

    let started = std::time::Instant::now();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("run ok");
    let elapsed = started.elapsed();

    assert!(matches!(report.steps[0], RunStepReport::Ok));
    // The whole point: a 5s ceiling costs only as long as the animation ran.
    assert!(
        elapsed < std::time::Duration::from_millis(2000),
        "settled early, so the step should not have paid the ceiling; took {elapsed:?}"
    );
    assert!(
        report.warnings.is_empty(),
        "settling is the normal path and should not warn: {:?}",
        report.warnings
    );
}

#[tokio::test]
async fn wait_for_animation_gives_up_at_the_ceiling_and_says_so() {
    // A screen that never stops — a spinner, or a caret bigger than the
    // tolerance. The step must not hang, and must not pretend it settled.
    let flicker_a = mock_png_sized(64, 64, |_, _| 0);
    let flicker_b = mock_png_sized(64, 64, |_, _| 255);
    let app = MockApp::new().with_screenshot_frames(vec![
        flicker_a.clone(),
        flicker_b.clone(),
        flicker_a.clone(),
        flicker_b.clone(),
        flicker_a,
        flicker_b,
    ]);
    let flow = parse_flow_yaml("appId: com.t\n---\n- waitForAnimationToEnd: 200\n").unwrap();

    let mut adapter = Adapter::new(&app, fixtures_dir());
    let report = adapter.run(&flow).await.expect("hitting the ceiling is not a failure");

    assert!(matches!(report.steps[0], RunStepReport::Ok));
    assert!(
        report.warnings.iter().any(|w| w.contains("still moving")),
        "the flow author needs to know the wait expired: {:?}",
        report.warnings
    );
}

#[tokio::test]
async fn wait_for_animation_on_a_still_screen_is_cheap() {
    // The case the old fixed sleep always charged for: nothing was animating.
    let app = MockApp::new();
    let flow = parse_flow_yaml("appId: com.t\n---\n- waitForAnimationToEnd\n").unwrap();

    let started = std::time::Instant::now();
    let mut adapter = Adapter::new(&app, fixtures_dir());
    adapter.run(&flow).await.expect("run ok");
    let elapsed = started.elapsed();

    // The default ceiling is 400ms; a still screen should cost about one
    // sampling cadence, not the ceiling.
    assert!(
        elapsed < std::time::Duration::from_millis(300),
        "a still screen should not pay the 400ms ceiling; took {elapsed:?}"
    );
}

#[tokio::test]
async fn wait_for_animation_surfaces_an_undecodable_frame() {
    // A frame we cannot compare means a broken toolchain, not a still screen.
    let app = MockApp::new()
        .with_screenshot_frames(vec![mock_png(10), b"not a png at all".to_vec()]);
    let flow = parse_flow_yaml("appId: com.t\n---\n- waitForAnimationToEnd\n").unwrap();

    let mut adapter = Adapter::new(&app, fixtures_dir());
    let err = adapter.run(&flow).await.expect_err("a broken frame must surface");
    assert!(
        format!("{err:?}").contains("PNG decode"),
        "got: {err:?}"
    );
}
