//! smix-sdk — user-facing public surface for the smix Rust library.
//!
//! Wraps [`SimctlDriver`] + [`SimctlClient`] + [`HttpRunnerClient`] with
//! an ergonomic Rust API.
//!
//! ```no_run
//! use smix_sdk::{App, text};
//! use std::time::Duration;
//!
//! # async fn demo() -> Result<(), smix_sdk::ExpectationFailure> {
//! let app = App::connect_to_runner(22087).await?;
//! app.launch("com.example.app").await?;
//! app.wait_for(&text("Login"), Duration::from_secs(5)).await?;
//! app.tap(&text("Login")).await?;
//! app.fill(&text("Email"), "user@example.com").await?;
//! app.press_key(smix_sdk::KeyName::Return).await?;
//! # Ok(())
//! # }
//! ```

#![doc(html_root_url = "https://docs.smix.dev/smix-sdk")]

/// Visual regression perceptual hash (dhash 64-bit). Crate-internal:
/// `compute_dhash` + `hamming_distance` back the public
/// `App::assert_screenshot`, not part of the SDK surface.
pub(crate) mod screenshot_hash;

pub mod issued_ledger;
pub use issued_ledger::{IssuedAction, IssuedKind, IssuedLedger};

// DeviceControl trait + cross-platform Permission enum + iOS impl.
// Two-trait architecture pair with smix-driver::Driver.
pub mod device_control;
pub mod ios_device;
pub use device_control::{DeviceControl, Permission};
pub use ios_device::IosDeviceControl;

// Android DeviceControl impl backed by smix-adb.
pub mod android_device;
pub use android_device::AndroidDeviceControl;

pub mod capsule;
pub use capsule::{
    CapsuleReconciliation, DEFAULT_RECONCILE_WINDOW_MS, FOCUS_CHANGE_RAW_CODE, reconcile,
};

use std::time::Duration;

/// Unix epoch in milliseconds — used by the issued-action ledger timestamps.
fn now_ms() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64
}

// -- re-exports for downstream user convenience ------------------------

pub use smix_driver::{
    HttpRunnerClient, IncludeScope, OcrFrame, RunnerScrollSelector, RunnerTransportError,
    SimctlDriver, SystemPopup, TapMode,
};
pub use smix_error::{
    ExpectationFailure, FailureCode, FailureInit, build_suggestions, edit_distance, similarity,
};
pub use smix_input::{KeyName, SwipeDirection};
pub use smix_screen::{
    A11yNode, Bounds, ElementSummary, Rect, Role, ScreenDescription, collect_visible_summaries,
    is_visible_enough, summarize_node, visible_area,
};
pub use smix_selector::{
    AnchorBox, IndexModifiers, Modifiers, Pattern, Selector, True, describe_selector, match_text,
    match_text_compiled,
};
pub use smix_simctl::{Appearance, LaunchResult, SimctlClient, SimctlError, SimctlPermission};

/// Nucleus of `App::assert_screenshot`. Wraps fs IO + the dhash algorithm
/// without any `App` dependency, so it can be exercised in host-side
/// unit tests. Helper fn — not a user-facing capability.
pub fn assert_screenshot_inner(
    png_bytes: &[u8],
    baseline_path: &std::path::Path,
    max_hamming: u32,
    strict: bool,
) -> Result<AssertScreenshotOutcome, ExpectationFailure> {
    use std::io::ErrorKind;
    let baseline_bytes = match std::fs::read(baseline_path) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            if strict {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!(
                        "assert_screenshot: baseline missing at {} and SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD=1 set; record a baseline first",
                        baseline_path.display()
                    ),
                    suggestions: vec![
                        "Run once without SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD to auto-record"
                            .into(),
                    ],
                    ..Default::default()
                }));
            }
            // auto-record: ensure parent dir exists + write current PNG.
            if let Some(parent) = baseline_path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: format!(
                            "assert_screenshot: failed to create baseline parent dir {}: {e}",
                            parent.display()
                        ),
                        ..Default::default()
                    })
                })?;
            }
            std::fs::write(baseline_path, png_bytes).map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!(
                        "assert_screenshot: failed to write baseline {}: {e}",
                        baseline_path.display()
                    ),
                    ..Default::default()
                })
            })?;
            return Ok(AssertScreenshotOutcome::Recorded {
                path: baseline_path.to_path_buf(),
            });
        }
        Err(e) => {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!(
                    "assert_screenshot: failed to read baseline {}: {e}",
                    baseline_path.display()
                ),
                ..Default::default()
            }));
        }
    };

    let h_current = screenshot_hash::compute_dhash(png_bytes)?;
    let h_baseline = screenshot_hash::compute_dhash(&baseline_bytes)?;
    let hamming = screenshot_hash::hamming_distance(h_current, h_baseline);
    if hamming <= max_hamming {
        Ok(AssertScreenshotOutcome::Matched { hamming })
    } else {
        Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::AssertionFailed),
            message: format!(
                "assertScreenshot: dhash hamming distance {hamming} exceeds threshold {max_hamming} (baseline {})",
                baseline_path.display()
            ),
            suggestions: vec![
                "Re-record baseline (delete the file) if the UI intentionally changed".into(),
                "Or pin/wait for animations to settle before assertScreenshot".into(),
            ],
            ..Default::default()
        }))
    }
}

/// Outcome of [`App::assert_screenshot`]. Distinguishes the
/// first-run "auto-record baseline" path (which writes the captured PNG to
/// disk and treats as Ok) from the steady-state diff path (which compares
/// dhash hamming distance against the recorded baseline).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssertScreenshotOutcome {
    /// First run — baseline did not exist, captured PNG was written to
    /// `path`. Subsequent runs will diff against this file.
    Recorded {
        /// Absolute path written.
        path: std::path::PathBuf,
    },
    /// Baseline existed and matched within tolerance; `hamming` is the
    /// observed dhash distance (≤ max_hamming).
    Matched {
        /// dhash hamming distance against baseline.
        hamming: u32,
    },
}

/// Maestro `setOrientation: <variant>` literal enum.
/// `landscape` yaml alias normalizes to `LandscapeLeft` at the parser
/// layer (same as maestro default). 1:1 mirrors `smix_driver::Orientation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaestroOrientation {
    /// Standard upright portrait.
    Portrait,
    /// Upside-down portrait.
    PortraitUpsideDown,
    /// Landscape with home indicator to the right (the default for
    /// `landscape` alias).
    LandscapeLeft,
    /// Landscape with home indicator to the left.
    LandscapeRight,
}

impl MaestroOrientation {
    /// 1:1 forward into the driver-level [`smix_driver::Orientation`].
    pub fn to_driver(self) -> smix_driver::Orientation {
        match self {
            Self::Portrait => smix_driver::Orientation::Portrait,
            Self::PortraitUpsideDown => smix_driver::Orientation::PortraitUpsideDown,
            Self::LandscapeLeft => smix_driver::Orientation::LandscapeLeft,
            Self::LandscapeRight => smix_driver::Orientation::LandscapeRight,
        }
    }
}

/// Maestro yaml `permissions:` action — controls iOS privacy state per bundle.
/// Maestro yaml parity:
/// - `Grant`  ↔ maestro yaml `"allow"` ↔ simctl `privacy grant`
/// - `Revoke` ↔ maestro yaml `"deny"`  ↔ simctl `privacy revoke`
/// - `Reset`  ↔ maestro yaml `"unset"` ↔ simctl `privacy reset`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionAction {
    Grant,
    Revoke,
    Reset,
}

/// Typed shape of maestro yaml `launchApp:` mapping. Adapter assembles this
/// from yaml fields; SDK consumes it in [`App::launch_app_with_options`].
/// Covers maestro `launchApp.permissions / arguments / stopApp`.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchAppOptions {
    pub bundle_id: String,
    pub clear_state: bool,
    pub clear_keychain: bool,
    /// Process-level argv passed via `simctl launch -- <args>`.
    pub arguments: Vec<String>,
    /// Permission directives applied in declaration order BEFORE launch.
    pub permissions: Vec<(SimctlPermission, PermissionAction)>,
    /// App bundle path for clear_state / clear_keychain wipe — mirrors the
    /// `launch_fresh::app_path` parameter; usually populated by the
    /// adapter from `SMIX_APP_PATH_<NORMALIZED_BUNDLE>` env.
    pub app_path: Option<String>,
}

// -------------------- selector helpers (ergonomic factories) --------

/// `text("Login")` shortcut. Mirrors TS `{ text: 'Login' }` shorthand.
#[must_use]
pub fn text<S: Into<String>>(s: S) -> Selector {
    Selector::Text {
        text: Pattern::text(s),
        modifiers: Modifiers::default(),
    }
}

/// `text_regex("^Lo")` shortcut.
#[must_use]
pub fn text_regex<S: Into<String>>(p: S) -> Selector {
    Selector::Text {
        text: Pattern::regex(p),
        modifiers: Modifiers::default(),
    }
}

/// `id("btn-x")` shortcut.
#[must_use]
pub fn id<S: Into<String>>(s: S) -> Selector {
    Selector::Id {
        id: s.into(),
        modifiers: Modifiers::default(),
    }
}

/// `label("Settings")` shortcut.
#[must_use]
pub fn label<S: Into<String>>(s: S) -> Selector {
    Selector::Label {
        label: s.into(),
        modifiers: Modifiers::default(),
    }
}

/// `role(Role::Button)` shortcut.
#[must_use]
pub fn role(r: Role) -> Selector {
    Selector::Role {
        role: r,
        name: None,
        modifiers: Modifiers::default(),
    }
}

/// `role_named(Role::Button, "Submit")` shortcut.
#[must_use]
pub fn role_named<S: Into<String>>(r: Role, name: S) -> Selector {
    Selector::Role {
        role: r,
        name: Some(Pattern::text(name)),
        modifiers: Modifiers::default(),
    }
}

/// `focused()` shortcut.
#[must_use]
pub fn focused() -> Selector {
    Selector::Focused {
        focused: True(true),
    }
}

/// Atomic op in the [`App::launch_fresh`] orchestration plan. Exposed
/// so the plan is testable as a pure function (no `SimctlClient` stub
/// — and a stub wouldn't help much since `SimctlClient` is a ZST).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchFreshOp {
    Terminate,
    Uninstall,
    Install(String),
    KeychainReset,
    Launch,
}

/// Pure planner for [`App::launch_fresh`] — computes the simctl op
/// sequence + warnings from `(clear_state, clear_keychain, app_path)`.
///
/// Maestro `launchApp.clearState` semantic is "wipe app data without
/// removing other apps", and iOS exposes no native per-app data wipe
/// API. The closest aligned host-side path is `simctl uninstall`
/// followed by `simctl install <app_path>`. So `clear_state=true`
/// only triggers a real wipe when `app_path` is supplied (typically
/// by the adapter reading `SMIX_APP_PATH_<BUNDLE_NORMALIZED>`);
/// otherwise it gracefully falls back to the non-clear path
/// (`terminate + launch`) with a warning.
#[must_use]
pub fn plan_launch_fresh_calls(
    clear_state: bool,
    clear_keychain: bool,
    app_path: Option<&str>,
) -> (Vec<LaunchFreshOp>, Vec<String>) {
    let mut ops = vec![LaunchFreshOp::Terminate];
    let mut warnings = Vec::new();
    match (clear_state, app_path) {
        (true, Some(path)) => {
            ops.push(LaunchFreshOp::Uninstall);
            ops.push(LaunchFreshOp::Install(path.to_string()));
        }
        (true, None) => {
            warnings.push(
                "G10 launch_fresh: app_path missing — graceful fallback to non-clear path \
                 (terminate + launch); set SMIX_APP_PATH_<BUNDLE_NORMALIZED> to enable wipe"
                    .to_string(),
            );
        }
        (false, _) => {}
    }
    if clear_keychain {
        ops.push(LaunchFreshOp::KeychainReset);
    }
    ops.push(LaunchFreshOp::Launch);
    (ops, warnings)
}

// -------------------- App ------------------------------------------------

/// Top-level surface for test authors. Mirrors Playwright's `page`
/// deliberately — AI authoring quality is highest when names overlap
/// with corpus the AI was trained on.
///
/// Every method is async — no chaining shortcuts, no fluent builder
/// pattern. One step, one await, one observable side effect (CLAUDE.md
/// §9 #5).
pub struct App {
    /// Sense+act trait stored as `Box<dyn>` for cross-platform
    /// dispatch. iOS impl = `IosDriver`; Android impl = `AndroidDriver`.
    driver: Box<dyn smix_driver::Driver>,
    /// Sim/host control trait stored as `Box<dyn>` for cross-platform
    /// dispatch. iOS impl = `IosDeviceControl`; Android impl =
    /// `AndroidDeviceControl`.
    device: Box<dyn DeviceControl>,
    udid: Option<String>,
    /// Capsule SDK issued-action ledger. Each `tap` / `tap_with_mode` /
    /// `fill` / `tap_at_coord` records an entry before the driver call,
    /// which is reconciled against EventRecorder 1018 focus-change events.
    /// Capacity LRU 1024.
    ledger: IssuedLedger,
}

impl App {
    /// Construct from a fully-wired driver + simctl client. Use this when
    /// you already manage Cell / UDID lifecycle externally.
    ///
    /// Back-compat constructor: still accepts `SimctlDriver` (alias to
    /// `IosDriver`) and `SimctlClient`; internally wraps into
    /// `Box<dyn Driver>` and `Box::new(IosDeviceControl::with_client(...))`.
    pub fn new(driver: SimctlDriver, simctl: SimctlClient) -> Self {
        App {
            driver: Box::new(driver),
            device: Box::new(IosDeviceControl::with_client(simctl)),
            udid: None,
            ledger: IssuedLedger::new(),
        }
    }

    /// Generic constructor for cross-platform tests. Use this when
    /// constructing with non-iOS `Driver` / `DeviceControl` impls.
    pub fn new_with(driver: Box<dyn smix_driver::Driver>, device: Box<dyn DeviceControl>) -> Self {
        App {
            driver,
            device,
            udid: None,
            ledger: IssuedLedger::new(),
        }
    }

    /// Convenience: connect to a runner on `127.0.0.1:{port}` and probe
    /// `GET /health` once. Returns App ready for sense+act calls.
    pub async fn connect_to_runner(port: u16) -> Result<Self, ExpectationFailure> {
        let client = HttpRunnerClient::new(port);
        client.ensure_reachable().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("runner unreachable: {e}"),
                hint: Some(format!("check SmixRunner started on port {port}")),
                ..Default::default()
            })
        })?;
        Ok(App {
            driver: Box::new(SimctlDriver::new(client)),
            device: Box::new(IosDeviceControl::new()),
            udid: None,
            ledger: IssuedLedger::new(),
        })
    }

    /// Connect to an Android Kotlin runner on `127.0.0.1:{port}`
    /// (the host-forwarded port that proxies to the device-side
    /// runner instrumentation). Returns App ready for cross-platform
    /// sense+act calls dispatched via AndroidDriver + AndroidDeviceControl.
    pub async fn connect_to_runner_android(port: u16) -> Result<Self, ExpectationFailure> {
        use smix_driver::AndroidDriver;
        let client = HttpRunnerClient::new(port);
        client.ensure_reachable().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("Android runner unreachable: {e}"),
                hint: Some(format!(
                    "check smix-android-runner instrument is up on port {port}"
                )),
                ..Default::default()
            })
        })?;
        Ok(App {
            driver: Box::new(AndroidDriver::new(client)),
            device: Box::new(AndroidDeviceControl::new()),
            udid: None,
            ledger: IssuedLedger::new(),
        })
    }

    /// Bind a UDID for lifecycle operations (launch/terminate/install/etc.).
    pub fn with_udid<S: Into<String>>(mut self, udid: S) -> Self {
        self.udid = Some(udid.into());
        self
    }

    /// Thread the target bundle id down to the driver, which forwards
    /// it to the runner via the `App-Bundle-Id` HTTP header. The iOS
    /// runner rebinds `XCUIApplication(bundleIdentifier:)` per request
    /// so calls stay pinned to the right app even when something else
    /// briefly claims foreground.
    #[must_use]
    pub fn with_bundle_id<S: Into<String>>(mut self, bundle: S) -> Self {
        let s: String = bundle.into();
        self.driver.set_target_bundle_id(&s);
        self
    }

    /// Enable auto-activate on every request. Runner side
    /// `.activate()`s the resolved target before operating. Costs one
    /// XCUITest activate call per request (~50-100ms); opt-in.
    #[must_use]
    pub fn with_auto_activate(mut self, activate: bool) -> Self {
        self.driver.set_auto_activate(activate);
        self
    }

    /// Force key-event dispatch mode on text-input verbs. Sends
    /// `Input-Dispatch-Mode: key-events` header on every request.
    /// Covers the RN hidden-input pattern where a11y-focus lookup
    /// returns nothing. Also opt-in via `smix run --force-key-events`.
    #[must_use]
    pub fn with_force_key_events(mut self, force: bool) -> Self {
        self.driver.set_force_key_events(force);
        self
    }

    pub fn udid(&self) -> Option<&str> {
        self.udid.as_deref()
    }

    /// Direct access to underlying `Driver` trait object.
    /// Use `app.driver()` for cross-platform calls; downcast to
    /// `IosDriver` only if iOS-specific behavior needed.
    pub fn driver(&self) -> &dyn smix_driver::Driver {
        self.driver.as_ref()
    }

    /// Back-compat `&SimctlClient` accessor. **iOS-only.**
    /// Panics on Android (use `app.device()` instead).
    pub fn simctl(&self) -> &SimctlClient {
        self.device.as_ios_simctl().expect(
            "App::simctl() called on non-iOS App; use app.device() for cross-platform access",
        )
    }

    /// Cross-platform sim/host control trait object.
    pub fn device(&self) -> &dyn DeviceControl {
        self.device.as_ref()
    }

    fn require_udid(&self) -> Result<&str, ExpectationFailure> {
        self.udid.as_deref().ok_or_else(|| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: "App not bound to a UDID; use .with_udid(...) first".into(),
                ..Default::default()
            })
        })
    }

    // ---- lifecycle (simctl-bound, requires UDID) ----------------------

    pub async fn launch(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .launch(udid, bundle_id)
            .await
            .map(|_| ())
            .map_err(simctl_to_failure)
    }

    pub async fn terminate(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .terminate(udid, bundle_id)
            .await
            .map_err(simctl_to_failure)
    }

    pub async fn install(&self, app_path: &str) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .install(udid, app_path)
            .await
            .map_err(simctl_to_failure)
    }

    pub async fn uninstall(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .uninstall(udid, bundle_id)
            .await
            .map_err(simctl_to_failure)
    }

    /// Launch the app with optional state / keychain wipe before launch.
    /// See [`plan_launch_fresh_calls`] for the op sequence semantics.
    /// Returns the warnings produced by the planner (graceful fallback
    /// path is taken when `clear_state=true` but `app_path` is `None`).
    /// The caller should append these warnings to its own collector
    /// (e.g. `RunReport::warnings` in the maestro adapter).
    /// `launch_arguments` is process-level argv (`simctl launch --
    /// <args>`). Empty `&[]` skips argv injection.
    pub async fn launch_fresh(
        &self,
        bundle_id: &str,
        clear_state: bool,
        clear_keychain: bool,
        app_path: Option<&str>,
        launch_arguments: &[String],
    ) -> Result<Vec<String>, ExpectationFailure> {
        let udid = self.require_udid()?;
        let (ops, warnings) = plan_launch_fresh_calls(clear_state, clear_keychain, app_path);
        for op in &ops {
            match op {
                LaunchFreshOp::Terminate => {
                    let _ = self.device.terminate(udid, bundle_id).await;
                }
                LaunchFreshOp::Uninstall => {
                    self.device
                        .uninstall(udid, bundle_id)
                        .await
                        .map_err(simctl_to_failure)?;
                }
                LaunchFreshOp::Install(path) => {
                    self.device
                        .install(udid, path)
                        .await
                        .map_err(simctl_to_failure)?;
                }
                LaunchFreshOp::KeychainReset => {
                    self.device
                        .keychain_reset(udid)
                        .await
                        .map_err(simctl_to_failure)?;
                }
                LaunchFreshOp::Launch => {
                    self.device
                        .launch_with_args(udid, bundle_id, launch_arguments)
                        .await
                        .map(|_| ())
                        .map_err(simctl_to_failure)?;
                }
            }
        }
        Ok(warnings)
    }

    /// Apply a permission action to a bundle.
    /// Maps maestro yaml `permissions: { camera: allow|deny|unset }` to simctl
    /// privacy. **§12.1 three-layer architecture**: sense+act live in
    /// core; the adapter only translates maestro yaml strings to the
    /// `PermissionAction` enum.
    pub async fn set_permission(
        &self,
        bundle_id: &str,
        permission: SimctlPermission,
        action: PermissionAction,
    ) -> Result<(), ExpectationFailure> {
        // Delegate to DeviceControl::set_permission with the cross-platform
        // Permission enum. Round-trip via Permission::from_simctl.
        let udid = self.require_udid()?;
        let xperm = Permission::from_simctl(permission);
        self.device
            .set_permission(udid, bundle_id, xperm, action)
            .await
            .map_err(simctl_to_failure)
    }

    /// Typed launch entry: apply permissions in declaration order, then
    /// dispatch to [`Self::launch_fresh`] (when clear_state /
    /// clear_keychain) or `simctl terminate + launch_with_args`
    /// (otherwise). Maps maestro yaml `launchApp: { ... }` in full
    /// (permissions / arguments / clearState / clearKeychain). Returns
    /// warnings emitted by `launch_fresh` (caller appends to its own
    /// collector).
    pub async fn launch_app_with_options(
        &self,
        opts: &LaunchAppOptions,
    ) -> Result<Vec<String>, ExpectationFailure> {
        let udid = self.require_udid()?;
        for (perm, action) in &opts.permissions {
            self.set_permission(&opts.bundle_id, *perm, *action).await?;
        }
        let warnings = if opts.clear_state || opts.clear_keychain {
            self.launch_fresh(
                &opts.bundle_id,
                opts.clear_state,
                opts.clear_keychain,
                opts.app_path.as_deref(),
                &opts.arguments,
            )
            .await?
        } else {
            // stop+launch path: maestro `launchApp` defaults to
            // stopApp=true — terminate first, then launch_with_args.
            // terminate failure is tolerated (the app may already be
            // dead); launch must succeed.
            let _ = self.device.terminate(udid, &opts.bundle_id).await;
            self.device
                .launch_with_args(udid, &opts.bundle_id, &opts.arguments)
                .await
                .map(|_| ())
                .map_err(simctl_to_failure)?;
            Vec::new()
        };
        Ok(warnings)
    }

    pub async fn open_url(&self, url: &str) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .open_url(udid, url)
            .await
            .map_err(simctl_to_failure)
    }

    /// Deliver an APNS payload to `bundle_id` via `simctl push`.
    /// The payload file must contain a JSON dictionary with at least an
    /// `aps` key (per Apple's spec). Mirrors maestro yaml `sendPush:`
    /// once that command lands upstream — there is no public maestro
    /// yaml `sendPush` today, so this is SDK-only surface.
    pub async fn send_push(
        &self,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .send_push(udid, bundle_id, apns_json_path)
            .await
            .map_err(simctl_to_failure)
    }

    pub async fn screenshot(&self) -> Result<Vec<u8>, ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .screenshot(udid)
            .await
            .map_err(simctl_to_failure)
    }

    /// Register a fixture-side action anchor in the SDK ledger so that
    /// the `capsule_reconcile` window can attribute the
    /// `kAXFirstResponderChangedNotification` (1018) the fixture's
    /// UIKit modal present is about to emit. Use this immediately
    /// before triggering a fixture-owned present path
    /// (UIActivityViewController, UIDocumentPickerViewController,
    /// SpringBoard system popup) that `smix-driver` would otherwise
    /// leave unattributed — without it, the phantom focus change
    /// inflates `unattributed_count`.
    pub fn mark_fixture_action(&self, action_id: &str) {
        self.ledger
            .record_fixture_action(now_ms(), action_id.to_string());
    }

    // ---- sense (driver-bound) -----------------------------------------

    pub async fn tree(&self) -> Result<A11yNode, ExpectationFailure> {
        self.driver.tree(None).await
    }

    pub async fn describe(&self) -> Result<ScreenDescription, ExpectationFailure> {
        // describe() is App-layer aggregation (not on the Driver trait
        // per cross-platform design). Inlined: driver.tree() +
        // collect_visible_summaries.
        let tree = self.driver.tree(None).await?;
        Ok(ScreenDescription {
            screenshot: None,
            elements: collect_visible_summaries(&tree, smix_screen::DEFAULT_VISIBLE_LIMIT),
            front_app: String::new(),
            summary: String::new(),
            captured_at: 0.0,
        })
    }

    pub async fn find_one(
        &self,
        selector: &Selector,
    ) -> Result<Option<A11yNode>, ExpectationFailure> {
        self.driver.find_one(selector, None).await
    }

    pub async fn find_all(&self, selector: &Selector) -> Result<Vec<A11yNode>, ExpectationFailure> {
        self.driver.find_all(selector, None).await
    }

    pub async fn find(&self, selector: &Selector) -> Result<bool, ExpectationFailure> {
        self.driver.find(selector, None).await
    }

    pub async fn system_popups(&self) -> Result<Vec<SystemPopup>, ExpectationFailure> {
        self.driver.system_popups(None).await
    }

    /// Tap a button on a previously enumerated system popup. `popup_id`
    /// and `button_id` round-trip from `system_popups()` — the runner
    /// walks the same scan order so callers don't need to manage an id
    /// map. Returns `Ok(true)` when matched and tapped, `Ok(false)` when
    /// the runner returned 404 not_found (popup or button id stale).
    /// Paired with `system_popups()` to close the sense/act loop on
    /// iOS system popups.
    pub async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        // Anchor the popup-action tap in the SDK ledger so any
        // kAXFirstResponderChangedNotification 1018 the SpringBoard alert
        // dismissal emits attributes to this action (was previously
        // unattributed because system_popup_action skipped record_tap).
        self.ledger.record_tap(
            now_ms(),
            Some(format!(
                "system_popup_action(popup={popup_id} btn={button_id})"
            )),
        );
        self.driver.system_popup_action(popup_id, button_id).await
    }

    // ---- act (driver-bound) -------------------------------------------

    pub async fn tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.ledger
            .record_tap(now_ms(), Some(format!("{selector:?}")));
        self.driver.tap(selector, None).await
    }

    /// Tap a selector via an explicit dispatch mode. Use
    /// `TapMode::DaemonProxySynthesize` for RN Pressable buttons that
    /// don't fire `onPress` with the default `tap()` Apple-native-event
    /// -chain dispatch. All other selectors should use `tap(selector)`
    /// (no mode) — the default host-resolve plus `tap_at_norm_coord`
    /// path is faster and works for non-RN-Pressable elements.
    pub async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: TapMode,
    ) -> Result<(), ExpectationFailure> {
        self.ledger
            .record_tap(now_ms(), Some(format!("{selector:?}")));
        self.driver.tap_with_mode(selector, mode, None).await
    }

    pub async fn fill(&self, selector: &Selector, text: &str) -> Result<(), ExpectationFailure> {
        self.ledger
            .record_fill(now_ms(), Some(format!("{selector:?}")));
        self.driver.fill(selector, text, None).await
    }

    pub async fn clear(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.driver.clear(selector, None).await
    }

    pub async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure> {
        self.driver.press_key(key).await
    }

    pub async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure> {
        self.driver.scroll(selector, direction).await
    }

    pub async fn swipe_once(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        self.driver.swipe_once(direction).await
    }

    pub async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        self.driver.hide_keyboard().await
    }

    pub async fn go_back(&self) -> Result<(), ExpectationFailure> {
        self.driver.back().await
    }

    /// Tap at normalized (nx, ny) coordinates — escape hatch for
    /// coord-based maestro yaml port and other no-a11y-semantic
    /// scenarios. (nx, ny) MUST be in [0, 1] (normalized to viewport).
    ///
    /// **§9 #3 lift (v3.16, escape hatch)**: the Selector surface still
    /// forbids xpath/coord — this method is NOT a Selector, it is the
    /// direct Apple-native-event-chain wire entry. Only `tap` is exposed;
    /// `swipe_at_coord` / `fill_at_coord` / `anchor_at_coord` are
    /// intentionally NOT provided and would require an independent
    /// CLAUDE.md §10 decision.
    ///
    /// Prefer `tap(&selector)` for any path with a11y semantic. Use this
    /// only for yaml-port edge cases (e.g. maestro `point: "X%,Y%"`).
    pub async fn tap_at_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        self.ledger.record_tap_at_coord(now_ms(), nx, ny);
        self.driver.tap_at_norm_coord(nx, ny).await
    }

    /// Tap via `XCUIElement.tap()` over the XCTest gesture-recognizer
    /// chain instead of the default host-HID-at-coord path. The id
    /// selector is resolved runner-side via
    /// `XCUIApplication.descendants(matching: .any)
    /// .matching(identifier:).firstMatch.tap()`.
    ///
    /// **Why this exists**: SwiftUI `.sheet` / `.alert` /
    /// `.confirmationDialog` / `.fullScreenCover` dismiss buttons
    /// present in a separate modal window scene. The default
    /// `tap(&selector)` resolves the button frame and injects an IOKit
    /// event at that coord, but iOS routes the touch to the underlying
    /// scene's hit-target, so SwiftUI's onTap closure for the
    /// modal-window button never fires. `XCUIElement.tap()` operates on
    /// the resolved element handle and reaches the binding regardless
    /// of window topology.
    ///
    /// Use `tap(&selector)` for everything else — the default path is faster
    /// and works on non-modal SwiftUI / UIKit hierarchies.
    pub async fn tap_xcui(&self, id: &str) -> Result<(), ExpectationFailure> {
        self.ledger
            .record_tap(now_ms(), Some(format!("tap_xcui id={id}")));
        self.driver.tap_by_id(id).await
    }

    /// Apple Vision OCR find. Returns the matching text observation's
    /// bounding box (UIKit normalized) or `None`. `locales` are BCP-47
    /// language subtags; empty defaults to the SDK's current locale
    /// (`["en"]` if unset). Covers "lib without testID but with
    /// visible text" scenarios.
    ///
    /// Find a selector's centroid as viewport-normalized `(nx, ny)`.
    /// Used by adapter AnchorRelative dispatch. Returns `None` when
    /// the selector resolves no node / empty frame.
    pub async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        self.driver.find_norm_coord(selector).await
    }

    /// Eval JS against the app-side WKWebView bridge. Returns the JS
    /// result as a JSON Value. Bridge must be running in the target app.
    pub async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure> {
        self.driver.webview_eval(js).await
    }

    pub async fn find_by_text_ocr(
        &self,
        text: &str,
        locales: &[String],
    ) -> Result<Option<OcrFrame>, ExpectationFailure> {
        let owned_default;
        let locales_slice: &[String] = if locales.is_empty() {
            owned_default = vec!["en".to_string()];
            &owned_default
        } else {
            locales
        };
        self.driver
            .find_text_by_ocr(text, locales_slice, "accurate")
            .await
    }

    /// Find by OCR + tap at frame center via IOHID synthesize.
    /// Convenience for the common OCR fallback path (OCR keyword → tap).
    /// Returns `ElementNotFound` when OCR finds no match.
    pub async fn tap_by_text_ocr(
        &self,
        text: &str,
        locales: &[String],
    ) -> Result<(), ExpectationFailure> {
        match self.find_by_text_ocr(text, locales).await? {
            Some(frame) => {
                self.ledger
                    .record_tap(now_ms(), Some(format!("tap_by_text_ocr text={text}")));
                self.driver
                    .tap_at_norm_coord(frame.mid_x(), frame.mid_y())
                    .await
            }
            None => Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("tap_by_text_ocr: OCR found no match for \"{text}\""),
                hint: Some(
                    "Apple Vision OCR returned 0 matching observations; check spelling \
                     / recognition language / surface contrast"
                        .into(),
                ),
                ..Default::default()
            })),
        }
    }

    /// Swipe between two normalized coordinate points — escape hatch for
    /// coord-based maestro yaml port (`swipe: { from: "X%,Y%", to: "X%,Y%" }`).
    /// Both points MUST be in [0, 1].
    ///
    /// **§9 #3 lift (escape hatch)**: companion to
    /// [`Self::tap_at_coord`]. The Selector surface still forbids
    /// xpath/coord — this method is NOT a Selector, it is the direct
    /// Apple-native-event-chain wire entry. Only `tap` and `swipe` coord
    /// forms are exposed; `fill_at_coord` / `anchor_at_coord` /
    /// `hover_at_coord` are intentionally NOT provided and would each
    /// require an independent CLAUDE.md §10 decision.
    pub async fn swipe_at_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure> {
        self.ledger.record_swipe_at_coord(now_ms(), from, to);
        self.driver.swipe_at_norm_coord(from, to).await
    }

    /// Viewport scroll one swipe in the given direction — no selector required.
    /// Maps to maestro yaml `scroll:` (bare, no args, defaults to down).
    ///
    /// Implementation: a single normalized-coord swipe from the viewport
    /// center to one edge (sense+act layer). [`Self::scroll`] is the
    /// scroll-until-visible composite and is orthogonal; `scroll_screen`
    /// is a pure act primitive.
    pub async fn scroll_screen(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        let (from, to) = match direction {
            SwipeDirection::Down => ((0.5, 0.7), (0.5, 0.3)),
            SwipeDirection::Up => ((0.5, 0.3), (0.5, 0.7)),
            SwipeDirection::Left => ((0.7, 0.5), (0.3, 0.5)),
            SwipeDirection::Right => ((0.3, 0.5), (0.7, 0.5)),
        };
        self.swipe_at_coord(from, to).await
    }

    /// Assert that the selector is NOT visible. Dual of
    /// [`Self::assert_visible`]. Maps to maestro yaml `assertNotVisible:`.
    ///
    /// Assertion is a core sense+assertion primitive, not an
    /// adapter-only synthesis. Uses a single non-waiting
    /// [`Self::find`] probe; if the selector matches, raise
    /// `AssertionFailed`.
    /// Wait until the selector is NOT visible. Dual of [`Self::wait_for`].
    /// Polls [`Self::find`] at 250ms intervals; returns Ok the first instant
    /// the element is absent. Returns `AssertionFailed` if the element is
    /// still visible after `timeout` elapses.
    ///
    /// The assertion+sense composite is a core platform capability,
    /// not adapter-only synthesis. Maps to maestro yaml
    /// `extendedWaitUntil: { notVisible: ... , timeout: N }`.
    pub async fn wait_for_not_visible(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<(), ExpectationFailure> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(250);
        loop {
            if !self.find(selector).await? {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::AssertionFailed),
                    message: format!(
                        "wait_for_not_visible: element still visible after {}ms — {}",
                        timeout.as_millis(),
                        describe_selector(selector)
                    ),
                    selector: Some(selector.clone()),
                    ..Default::default()
                }));
            }
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Assert the current sim screenshot matches a recorded baseline
    /// PNG via 64-bit dhash perceptual diff. Maestro
    /// `assertScreenshot: <baseline-path>`.
    ///
    /// **Baseline lifecycle** (same as maestro):
    /// - Baseline missing → write the captured PNG + return
    ///   `Recorded { path }` (auto-record default).
    /// - `SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD=1` env → strict mode:
    ///   missing baseline = `DriverError`.
    /// - Baseline present → dhash(baseline) vs dhash(current) → hamming
    ///   distance; `≤ max_hamming` = `Matched { hamming }`, otherwise
    ///   `AssertionFailed`.
    ///
    /// `max_hamming` typically ≤ 10 (adapter runtime arm pins 5).
    pub async fn assert_screenshot(
        &self,
        baseline_path: &std::path::Path,
        max_hamming: u32,
    ) -> Result<AssertScreenshotOutcome, ExpectationFailure> {
        let png = self.screenshot().await?;
        let strict = std::env::var_os("SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD").is_some();
        assert_screenshot_inner(&png, baseline_path, max_hamming, strict)
    }

    pub async fn assert_not_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        if self.find(selector).await? {
            Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::AssertionFailed),
                message: format!(
                    "expect.toNotBeVisible: element is visible — {}",
                    describe_selector(selector)
                ),
                selector: Some(selector.clone()),
                ..Default::default()
            }))
        } else {
            Ok(())
        }
    }

    /// Write `text` to the iOS Simulator device pasteboard via
    /// `xcrun simctl pbcopy <udid>`. Maps to maestro yaml
    /// `setClipboard: "literal"`.
    ///
    /// Clipboard set is a core act primitive. Uses the simctl host-side
    /// path (device-scoped, explicit UDID) rather than the swift sim-side
    /// UIPasteboard wire — [`SimctlClient::pasteboard_set`] already has
    /// a stable wire, no need to add a new swift route.
    pub async fn set_clipboard(&self, text: &str) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .pasteboard_set(udid, text)
            .await
            .map_err(simctl_to_failure)
    }

    /// Read the current iOS Simulator device pasteboard via
    /// `xcrun simctl pbpaste <udid>`. Returns the raw string (may be empty).
    pub async fn get_clipboard(&self) -> Result<String, ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .pasteboard_get(udid)
            .await
            .map_err(simctl_to_failure)
    }

    /// Paste `text` into the currently-focused input field.
    /// Maps maestro yaml `pasteText: "literal"` (text-bearing form) and
    /// bare `- pasteText` (None form, reads current clipboard first).
    ///
    /// Both forms preserve the clipboard side-effect maestro yaml users
    /// implicitly rely on (literal form writes clipboard so the post-flow
    /// pasteboard mirrors the typed text — same as native "paste from
    /// clipboard" UX).
    pub async fn paste_text(&self, text: Option<&str>) -> Result<(), ExpectationFailure> {
        let to_type = match text {
            Some(t) => {
                self.set_clipboard(t).await?;
                t.to_string()
            }
            None => self.get_clipboard().await?,
        };
        self.fill(&focused(), &to_type).await
    }

    /// Double-tap an element. Maps to maestro yaml
    /// `doubleTapOn: <selector>`. Backed by XCUIElement.doubleTap() on
    /// the swift sim side.
    pub async fn double_tap(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        self.ledger.record_tap(
            now_ms(),
            Some(format!("double:{}", describe_selector(selector))),
        );
        self.driver.double_tap(selector, None).await
    }

    /// Long-press an element for `duration`. Maps to maestro yaml
    /// `longPressOn` (optional `duration:` ms, default 500 on the
    /// adapter side). Backed by XCUIElement.press(forDuration:) on
    /// the swift sim side.
    pub async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
    ) -> Result<(), ExpectationFailure> {
        self.ledger.record_tap(
            now_ms(),
            Some(format!(
                "longpress({}ms):{}",
                duration.as_millis(),
                describe_selector(selector)
            )),
        );
        self.driver.long_press(selector, duration, None).await
    }

    /// Set sim location. Maestro `setLocation: { latitude, longitude }`.
    pub async fn set_location(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .location_set(udid, latitude, longitude)
            .await
            .map_err(simctl_to_failure)
    }

    /// Interpolate sim location along waypoints. Maestro `travel`.
    /// **Fire-and-return**: simctl injects scenario and returns immediately;
    /// sim continues interpolation in background. Caller must explicitly
    /// `waitForAnimationToEnd` / sleep if downstream logic depends on
    /// playback completion.
    pub async fn travel(
        &self,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .location_start(udid, points, speed_mps)
            .await
            .map_err(simctl_to_failure)
    }

    /// Add photos / videos / contacts to the sim library. Maestro
    /// `addMedia: <path>` (scalar) or `addMedia: [paths]` (array;
    /// adapter flattens to Vec).
    pub async fn add_media(&self, paths: &[String]) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        self.device
            .add_media(udid, paths)
            .await
            .map_err(simctl_to_failure)
    }

    /// Start recording the sim display to `path`. Maestro
    /// `startRecording: <path>`. Spawns `xcrun simctl io recordVideo` as
    /// a long-running child; returns immediately. Errors if a recording
    /// is already in progress (call `stop_recording` first — no silent
    /// no-op).
    pub async fn start_recording(&self, path: &str) -> Result<(), ExpectationFailure> {
        // Recording state owned by IosDeviceControl (was on App).
        // Trait method returns Result<(), SimctlError>; double-start
        // surfaces as SimctlError::NonZeroExit (mapped here to
        // ExpectationFailure).
        let udid = self.require_udid()?;
        self.device
            .start_recording(udid, std::path::Path::new(path))
            .await
            .map_err(|e| {
                ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("start_recording: {e}"),
                    suggestions: vec!["Call stop_recording before starting a new one".to_string()],
                    ..Default::default()
                })
            })
    }

    /// Stop the active recording (SIGINT-and-wait simctl child; flushes
    /// mp4 trailer). Maestro `stopRecording`. Errors if no recording is
    /// active — explicit DriverError + hint, not a silent no-op.
    pub async fn stop_recording(&self) -> Result<(), ExpectationFailure> {
        // Delegate to DeviceControl::stop_recording.
        self.device.stop_recording().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("stop_recording: {e}"),
                suggestions: vec![
                    "Add a `- startRecording: <path>` step before this `stopRecording`".to_string(),
                ],
                ..Default::default()
            })
        })
    }

    /// Rotate sim. Maestro `setOrientation: portrait |
    /// portraitUpsideDown | landscapeLeft | landscapeRight`.
    /// Walks `driver.set_orientation` → POST /set-orientation → swift
    /// `XCUIDevice.shared.orientation`.
    pub async fn set_orientation(
        &self,
        orientation: MaestroOrientation,
    ) -> Result<(), ExpectationFailure> {
        self.driver.set_orientation(orientation.to_driver()).await
    }

    /// Batch permission setter. Maestro `setPermissions: { camera:
    /// allow, location: deny, ... }` (top-level command, distinct from
    /// `launchApp.permissions`). Reuses `set_permission` per entry
    /// (sequential apply; fail-fast on first error).
    pub async fn set_permissions(
        &self,
        bundle_id: &str,
        permissions: &[(SimctlPermission, PermissionAction)],
    ) -> Result<(), ExpectationFailure> {
        for (perm, action) in permissions {
            self.set_permission(bundle_id, *perm, *action).await?;
        }
        Ok(())
    }

    /// Read text content from the matched element and write it to the
    /// device pasteboard. Maps to maestro yaml `copyTextFrom:
    /// <selector>`. Field priority follows the maestro iOS driver:
    /// `value → text → label`. All three empty raises
    /// `AssertionFailed` — no silent no-op.
    pub async fn copy_text_from(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        let node = self.find_one(selector).await?.ok_or_else(|| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!(
                    "copy_text_from: no element matched — {}",
                    describe_selector(selector)
                ),
                selector: Some(selector.clone()),
                ..Default::default()
            })
        })?;
        let extracted = node
            .value
            .clone()
            .or_else(|| node.text.clone())
            .or_else(|| node.label.clone())
            .unwrap_or_default();
        if extracted.is_empty() {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::AssertionFailed),
                message: format!(
                    "copy_text_from: matched element carries no extractable text \
                     (value/text/label all empty) — {}",
                    describe_selector(selector)
                ),
                selector: Some(selector.clone()),
                ..Default::default()
            }));
        }
        self.set_clipboard(&extracted).await
    }

    // ---- Capsule helper ----------------------

    /// Start Capsule recording — triggers the UITest runner
    /// `EventRecorder.installSwizzle` registration path (if
    /// `TEST_RUNNER_SMIX_RECORD_ENABLED=1` is set in the env) and
    /// clears the SDK-internal issued-action ledger. **The runner
    /// must be started with `TEST_RUNNER_SMIX_RECORD_ENABLED=1`**;
    /// otherwise swift-side `recordEnabled` is false, `installSwizzle`
    /// is skipped, and `/record/start` returns 404.
    pub async fn start_capsule_recording(&self) -> Result<(), ExpectationFailure> {
        // Capsule recording uses HttpRunnerClient::start_record which
        // is iOS-specific (XCUITest EventRecorder swizzle path).
        // Android impl will use a different mechanism.
        let ios = self.driver.as_ios_driver().ok_or_else(|| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: "start_capsule_recording: iOS-only API".into(),
                ..Default::default()
            })
        })?;
        ios.runner().start_record().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("start_record failed: {e}"),
                hint: Some(
                    "ensure the runner was started with TEST_RUNNER_SMIX_RECORD_ENABLED=1".into(),
                ),
                ..Default::default()
            })
        })?;
        self.ledger.clear();
        // start_capsule_recording is itself an SDK-decision-layer act;
        // record it as an anchor so reconciliation attributes any
        // 1018 focus-change events during fixture lifecycle settle
        // (firstResponder reset after swizzle install, etc.) to this
        // action within the window, avoiding spurious unattributed
        // reports.
        self.ledger.record_capsule_start(now_ms());
        Ok(())
    }

    /// Stop recording and reconcile. `window_ms = None` uses
    /// [`DEFAULT_RECONCILE_WINDOW_MS`] (500 ms). Returned
    /// [`CapsuleReconciliation`] includes the full
    /// `unattributed_events` detail.
    pub async fn stop_capsule_recording_and_reconcile(
        &self,
        window_ms: Option<u64>,
    ) -> Result<CapsuleReconciliation, ExpectationFailure> {
        // iOS-only via Driver::as_ios_driver downcast.
        let ios = self.driver.as_ios_driver().ok_or_else(|| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: "stop_capsule_recording_and_reconcile: iOS-only API".into(),
                ..Default::default()
            })
        })?;
        let events = ios.runner().stop_record().await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("stop_record failed: {e}"),
                ..Default::default()
            })
        })?;
        let issued = self.ledger.get_all();
        Ok(reconcile(
            &issued,
            &events,
            window_ms.unwrap_or(DEFAULT_RECONCILE_WINDOW_MS),
        ))
    }

    pub async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.driver.foreground(bundle_id).await
    }

    pub async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<A11yNode, ExpectationFailure> {
        self.driver.wait_for(selector, timeout, None).await
    }

    // ---- assertion matchers -------------------------------------------

    /// Assert that the selector matches a visible element. Re-uses
    /// `wait_for` semantics (5s default budget) with `NotVisible`
    /// failure code.
    pub async fn assert_visible(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        match self
            .driver
            .wait_for(selector, Duration::from_secs(5), None)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if e.code == FailureCode::Timeout => Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::NotVisible),
                message: format!(
                    "expect.toBeVisible: not visible — {}",
                    describe_selector(selector)
                ),
                selector: Some(selector.clone()),
                visible_elements: e.visible_elements,
                suggestions: e.suggestions,
                ..Default::default()
            })),
            Err(e) => Err(e),
        }
    }

    /// Assert that the matched element has `enabled = true`.
    pub async fn assert_enabled(&self, selector: &Selector) -> Result<(), ExpectationFailure> {
        let node = self.driver.find_one(selector, None).await?.ok_or_else(|| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!(
                    "expect.toBeEnabled: not found — {}",
                    describe_selector(selector)
                ),
                selector: Some(selector.clone()),
                ..Default::default()
            })
        })?;
        if !node.enabled {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::NotEnabled),
                message: format!(
                    "expect.toBeEnabled: disabled — {}",
                    describe_selector(selector)
                ),
                selector: Some(selector.clone()),
                ..Default::default()
            }));
        }
        Ok(())
    }

    /// Assert that the screen contains at least one element whose text /
    /// label / 6-field OR scan matches the literal. Useful for "page
    /// rendered" smoke checks without crafting a full selector tree.
    pub async fn assert_text(&self, literal: &str) -> Result<(), ExpectationFailure> {
        self.assert_visible(&text(literal)).await
    }
}

// -------------------- error mapping -----------------------------------

fn simctl_to_failure(e: SimctlError) -> ExpectationFailure {
    let (code, hint) = match &e {
        SimctlError::Spawn(_) => (
            FailureCode::DriverError,
            Some("xcrun not found — install Xcode command-line tools".into()),
        ),
        SimctlError::NonZeroExit { .. } => (FailureCode::DriverError, None),
        SimctlError::Malformed { .. } => (FailureCode::DriverError, None),
        SimctlError::Timeout { ms, .. } => (
            FailureCode::Timeout,
            Some(format!("subprocess timeout after {ms}ms")),
        ),
    };
    ExpectationFailure::new(FailureInit {
        code: Some(code),
        message: format!("{e}"),
        hint,
        ..Default::default()
    })
}
