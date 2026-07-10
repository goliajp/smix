//! smix-runner-client — HTTP IPC client to `swift-bridge/SmixRunnerCore`.
//! Allows reqwest + tokio + thiserror + tracing deps.
//!
//! Wire-level 1:1 compatibility with the Swift side — any route shape /
//! query param / response body shape change must be done in lockstep with
//! `swift-bridge/Sources/SmixRunnerCore`.
//!
//! # Routes
//!
//! - `GET /health` — connectivity probe (memoized via [`HttpRunnerClient::ensure_reachable`])
//! - `GET /tree?include=` — full a11y tree dump (returns [`A11yNode`])
//! - `GET /system-popups?include=` — list of [`SystemPopup`]
//! - `POST /tap {selector, mode, include?}` — selector → element tap
//! - `POST /tap-at-norm-coord {nx, ny}` — Apple native event chain coord tap
//! - `POST /find {selector, include?}` — boolean existence check
//! - `POST /fill {selector, text, include?}` — fill text into input
//! - `POST /clear {selector, include?}` — clear input
//! - `POST /press-key {key}` — press named key (Return/Tab/etc.)
//! - `POST /scroll {selector, direction, include?}` — scroll-until-visible
//! - `POST /swipe-once {direction}` — single swipe gesture (no probe loop)
//! - `POST /foreground {bundleId}` — bring app to foreground
//! - `POST /hide-keyboard` — swipe-down to dismiss keyboard
//! - `POST /back` — back gesture
//! - `POST /record/start` — begin recording AX notifications
//! - `GET /record/poll` — drain recorded events
//! - `POST /record/stop` — stop recording, drain final events
//!
//! Per CLAUDE.md §9 #4 — never expose raw URL / sleep / xpath surfaces.
//! All operations go through the typed methods below.

#![doc(html_root_url = "https://docs.smix.dev/smix-runner-client")]

use serde::{Deserialize, Serialize};
use smix_input::{KeyName, SwipeDirection};
use smix_screen::{A11yNode, derive_roles_recursive};
use smix_selector::Selector;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;

// -------------------- Error types ----------------------------------------

/// Transport-level failure (network, non-2xx, malformed body). Mirrors
/// TS `RunnerTransportError` 1:1.
#[derive(Debug, Error)]
pub enum RunnerTransportError {
    #[error("runner {endpoint} fetch failed: {source}")]
    FetchFailed {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("runner {endpoint} returned status {status}: {body}")]
    NonSuccessStatus {
        endpoint: String,
        status: u16,
        body: String,
    },
    #[error("runner {endpoint} returned non-JSON body: {source}")]
    NonJsonBody {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("runner {endpoint} returned malformed body: {detail}")]
    MalformedBody { endpoint: String, detail: String },
    #[error("runner {endpoint} unreachable: {message}")]
    Unreachable { endpoint: String, message: String },
    /// The runner returned
    /// `{"ok":false,"error":"snapshot_unavailable"}` (or the enriched
    /// body with `target`/`reason` fields). This is not a network
    /// failure — it means XCUITest could not snapshot the app the client
    /// asked about. Callers that see this after all retries can surface
    /// an AI-readable hint about foreground state.
    #[error("runner {endpoint}: app unavailable (target={target:?}, reason={reason:?})")]
    AppUnavailable {
        endpoint: String,
        target: Option<String>,
        reason: Option<String>,
    },
    /// Client-side rolling-window observation: consecutive requests
    /// have been failing (5xx, unreachable, non-JSON). The runner is
    /// still responding to the transport, but its answers are unsound
    /// — surfacing this instead of silently returning stale bodies is
    /// the whole point of [`HttpRunnerClient::with_liveness_window`].
    #[error(
        "runner degraded: {non_success_recent}/{window} recent requests failed \
         (last_endpoint={last_endpoint}, last_error={last_error})"
    )]
    RunnerDegraded {
        window: usize,
        non_success_recent: usize,
        last_endpoint: String,
        last_error: String,
    },
    /// Client-side observation of a hard runner death. Triggered when a
    /// `is_connect()` error fires AND a `/health` probe times out or
    /// returns a non-2xx within 1 s. Distinct from
    /// [`RunnerTransportError::Unreachable`] which fires on the initial
    /// reachability probe; `RunnerDied` fires mid-session after
    /// `ensure_reachable` had already succeeded.
    #[error("runner died mid-session: last_seen_ms={last_seen_ms}, last_error={last_error}")]
    RunnerDied {
        last_seen_ms: u64,
        last_error: String,
    },
}

/// Client-side rolling window tracking the last N request outcomes.
/// Used by `with_liveness_window` to distinguish "runner is alive and
/// healthily returning some errors" from "runner has silently drifted
/// into a degraded state". See [`RunnerTransportError::RunnerDegraded`]
/// and [`RunnerTransportError::RunnerDied`].
#[derive(Debug, Clone)]
pub(crate) struct LivenessState {
    window: usize,
    /// True = success, false = non-success. Front = oldest.
    outcomes: std::collections::VecDeque<bool>,
    last_endpoint: String,
    last_error: String,
    /// Milliseconds since UNIX epoch of the last successful request.
    last_seen_ms: u64,
    /// True once we've fired a RunnerDied. Prevents re-firing until a
    /// fresh success clears it.
    died: bool,
}

impl LivenessState {
    fn with_window(window: usize) -> Self {
        Self {
            window: window.max(2),
            outcomes: std::collections::VecDeque::with_capacity(window.max(2)),
            last_endpoint: String::new(),
            last_error: String::new(),
            last_seen_ms: 0,
            died: false,
        }
    }

    fn record(&mut self, endpoint: &str, success: bool, error: &str) {
        while self.outcomes.len() >= self.window {
            self.outcomes.pop_front();
        }
        self.outcomes.push_back(success);
        self.last_endpoint = endpoint.to_string();
        if success {
            self.last_error.clear();
            self.last_seen_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            self.died = false;
        } else {
            self.last_error = error.to_string();
        }
    }

    /// If the last `window` outcomes have a majority-non-success shape,
    /// return the degraded error variant. Caller wraps into the outer
    /// [`RunnerTransportError`].
    fn degraded(&self) -> Option<RunnerTransportError> {
        if self.outcomes.len() < self.window {
            return None;
        }
        let bad = self.outcomes.iter().filter(|ok| !**ok).count();
        if bad * 2 > self.window {
            Some(RunnerTransportError::RunnerDegraded {
                window: self.window,
                non_success_recent: bad,
                last_endpoint: self.last_endpoint.clone(),
                last_error: self.last_error.clone(),
            })
        } else {
            None
        }
    }
}

/// Per-request context carried across the wire via the
/// `App-Bundle-Id` and `App-Activate` HTTP headers. Client sets
/// these via `HttpRunnerClient::with_target_bundle_id` /
/// `with_auto_activate`, or per-call by cloning the client with a
/// modified context. The runner side reads them into
/// `SmixRunnerServer.currentContext` (task-local) and passes to
/// `resolveApp()` in every app-touching handler.
///
/// The two-field split is intentional: `bundle_id` is "which app do I
/// want to talk to", `activate` is "should we bring it foreground first".
#[derive(Clone, Debug, Default)]
pub struct RequestContext {
    pub bundle_id: Option<String>,
    pub activate: bool,
}

/// `POST /tap` returned `404 / not_found` — element matched no node.
/// Mirrors TS `TapNotFoundError`.
#[derive(Debug, Error)]
#[error("tap not_found for selector {selector_json}: {body}")]
pub struct TapNotFoundError {
    pub selector_json: String,
    pub body: String,
}

/// `POST /scroll` returned `200 / matched:false / swipes:N` — scroll
/// exhausted N swipes without surfacing the target. Mirrors TS
/// `RunnerScrollNotMatched`.
#[derive(Debug, Error)]
#[error("scroll not_matched after {swipes} swipes for selector {selector_json}")]
pub struct RunnerScrollNotMatched {
    pub selector_json: String,
    pub swipes: u32,
}

/// `POST /swipe-once` returned `200 / ok:false / vanished_during_swipe`.
/// Mirrors TS `RunnerSwipeOnceFailure`.
#[derive(Debug, Error)]
#[error("swipeOnce failed: {detail}")]
pub struct RunnerSwipeOnceFailure {
    pub detail: String,
}

/// Normalized OCR bounding box returned by
/// [`HttpRunnerClient::find_text_by_ocr`]. Coordinates are normalized to
/// `[0, 1]` in UIKit coord space (top-left origin, y-down). Apple Vision's
/// native bbox is bottom-left origin + y-up — the swift handler converts
/// before returning so all consumers see UIKit coords.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrFrame {
    /// Top-left x in `[0, 1]`.
    pub nx: f64,
    /// Top-left y in `[0, 1]`.
    pub ny: f64,
    /// Width in `[0, 1]`.
    pub w: f64,
    /// Height in `[0, 1]`.
    pub h: f64,
}

impl OcrFrame {
    /// Center x in `[0, 1]`.
    #[must_use]
    pub fn mid_x(&self) -> f64 {
        self.nx + self.w * 0.5
    }
    /// Center y in `[0, 1]`.
    #[must_use]
    pub fn mid_y(&self) -> f64 {
        self.ny + self.h * 0.5
    }
}

// -------------------- Wire types ----------------------------------------
//
// Wire types live in the smix-runner-wire stone. The HTTP client
// (this crate) re-exports them so existing consumers don't break.
// Anyone needing the wire shapes without the reqwest+tokio HTTP impl
// should depend on smix-runner-wire directly.

pub use smix_runner_wire::{
    FindRequest, FindResponse, HealthProcessInfo, HealthResponse, HealthTestHostInfo, IncludeScope,
    KeyboardStages, RecordEventsResponse, RecordedEvent, RunnerIncludeOpts, RunnerKeyboardResult,
    RunnerScrollSelector, ScrollResponse, SessionAppLifecycleRequest,
    SessionAppLifecycleResponse, SessionCloseAllResponse, SessionCloseRequest,
    SessionCloseResponse, SessionListResponse, SessionOpenRequest, SessionOpenResponse,
    SessionRelaunchAppRequest, SessionRelaunchAppResponse, SessionRenewActivationRequest,
    SessionRenewActivationResponse, SessionSummary, SimHealthWireState,
    DiagnosticDumpResponse, SubprocessRecord as WireSubprocessRecord, SystemPopup,
    SystemPopupActionRequest, SystemPopupActionResponse, SystemPopupButton, SystemPopupsResponse,
    TapAtNormCoordRequest, TapMode, TapRequest, TapResult, TapStages,
};

// -------------------- Transport retry constants -------------------------

/// Total transport attempts (1 initial + 2 retries) for transient reqwest
/// connection errors. See `send_with_retry` doc.
const TRANSPORT_MAX_ATTEMPTS: u32 = 3;
/// Backoff between transport retry attempts.
const TRANSPORT_BACKOFF_MS: u64 = 100;

/// Classify a non-2xx runner response body. When the body
/// carries `snapshot_unavailable` (bare shape) or the enriched
/// `{"ok":false,"error":"snapshot_unavailable","target":"...","reason":"..."}`
/// shape, lift it to a first-class
/// `RunnerTransportError::AppUnavailable` variant. Otherwise fall
/// through to the generic `NonSuccessStatus`.
fn classify_error_body(endpoint: &str, status: u16, body: &str) -> RunnerTransportError {
    if body.contains("snapshot_unavailable") {
        // Parse enriched body when present. Any parse miss falls back to
        // just the marker string — still surfaces useful hint upstream.
        #[derive(Deserialize)]
        struct Unavailable {
            #[serde(default)]
            target: Option<String>,
            #[serde(default)]
            reason: Option<String>,
        }
        let parsed: Option<Unavailable> = serde_json::from_str(body).ok();
        return RunnerTransportError::AppUnavailable {
            endpoint: endpoint.to_string(),
            target: parsed.as_ref().and_then(|p| p.target.clone()),
            reason: parsed.as_ref().and_then(|p| p.reason.clone()),
        };
    }
    RunnerTransportError::NonSuccessStatus {
        endpoint: endpoint.to_string(),
        status,
        body: body.chars().take(200).collect(),
    }
}

// -------------------- Client --------------------------------------------

/// HTTP client to the swift-side SmixRunnerCore. Async (tokio-based).
/// Constructed once per Cell; `ensure_reachable` memoizes a successful
/// `/health` probe so subsequent calls don't re-probe.
#[derive(Debug)]
pub struct HttpRunnerClient {
    base: String,
    client: reqwest::Client,
    reachable: Arc<AtomicBool>,
    /// Target bundle id, sent as `App-Bundle-Id` header on every
    /// request. `None` = client doesn't say; runner uses its
    /// boot-time default `app`.
    target_bundle_id: Option<String>,
    /// If `true`, sends `App-Activate: true` on every request so the
    /// runner calls `.activate()` on the resolved target before
    /// operating. Auto-recovers from cases where XCUITest's implicit
    /// app-under-test binds to the wrong foreground.
    auto_activate: bool,
    /// Sent as `Input-Dispatch-Mode` header on every request.
    /// `None` = no header sent (runner uses default).
    input_dispatch_mode: Option<InputDispatchMode>,
    /// Sent as `Session-Id` header on every request. Set by
    /// `Session::wrap` after `open_session()`. When present, the
    /// runner short-circuits its per-request `resolveApp()` and reuses
    /// the session's cached `XCUIApplication` binding — this is what
    /// eliminates the activation storm on long-running gates.
    session_id: Option<String>,
    /// Rolling-window state for liveness observability. `None` when
    /// disabled (default). See [`Self::with_liveness_window`].
    liveness: Arc<std::sync::Mutex<Option<LivenessState>>>,
    /// Sim-side health monitor. `None` when disabled (default). See
    /// [`Self::with_sim_health`]. When present, `/health` outcomes
    /// feed `SimHealthMonitor::record_health_ok` / `_fail`, so a
    /// subscriber gets `Degraded` / `Dead` transitions without polling.
    sim_health: Option<smix_sim_health::SimHealthMonitor>,
    /// v1.0.4 §D7 — session state atomic shared with the SDK Session.
    /// Updated on every response by parsing the `X-Sim-Health` header
    /// (values `healthy` | `degraded` | `cycling` | `dead`). Unknown /
    /// missing header leaves the state unchanged.
    session_state: Arc<std::sync::Mutex<Option<Arc<std::sync::atomic::AtomicU8>>>>,
}

/// Input dispatch mode for the `Input-Dispatch-Mode` header.
/// Runner-side interpretation lives in `SmixRunnerCore/FillRoute.swift`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputDispatchMode {
    /// a11y-anchored dispatch (default; XCUIElement.typeText).
    A11y,
    /// Raw key events via IOHID / daemon; skip a11y-focus resolution.
    /// Covers the RN hidden-input case where a11y-focus lookup returns
    /// nothing.
    KeyEvents,
    /// Try a11y first; on ElementNotFound fall back to key-events.
    Auto,
}

impl InputDispatchMode {
    fn header_value(self) -> &'static str {
        match self {
            InputDispatchMode::A11y => "a11y",
            InputDispatchMode::KeyEvents => "key-events",
            InputDispatchMode::Auto => "auto",
        }
    }
}

impl HttpRunnerClient {
    /// Construct a client targeting `http://127.0.0.1:{port}`.
    pub fn new(port: u16) -> Self {
        Self::with_base(format!("http://127.0.0.1:{port}"))
    }

    /// Construct with an explicit base URL (test / non-localhost cases).
    pub fn with_base<S: Into<String>>(base: S) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest::Client::builder default never fails");
        HttpRunnerClient {
            base: base.into(),
            client,
            reachable: Arc::new(AtomicBool::new(false)),
            target_bundle_id: None,
            auto_activate: false,
            input_dispatch_mode: None,
            session_id: None,
            liveness: Arc::new(std::sync::Mutex::new(None)),
            sim_health: None,
            session_state: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// v1.0.4 §D7 — attach the SDK Session's state atomic so the
    /// client updates it on every response by parsing `X-Sim-Health`.
    /// Called by `App::open_session`. External callers rarely need
    /// this directly.
    pub fn attach_session_state(
        &self,
        state: Arc<std::sync::atomic::AtomicU8>,
    ) {
        let mut guard = self
            .session_state
            .lock()
            .expect("session_state mutex must not be poisoned");
        *guard = Some(state);
    }

    fn update_session_state_from_header(&self, header: Option<&str>) {
        let Some(value) = header else {
            return;
        };
        let Some(state) = smix_runner_wire::SimHealthWireState::from_header(value) else {
            return;
        };
        let byte = match state {
            smix_runner_wire::SimHealthWireState::Healthy => 0u8,
            smix_runner_wire::SimHealthWireState::Degraded => 1u8,
            smix_runner_wire::SimHealthWireState::Cycling => 2u8,
            smix_runner_wire::SimHealthWireState::Dead => 3u8,
            _ => return,
        };
        let guard = self
            .session_state
            .lock()
            .expect("session_state mutex must not be poisoned");
        if let Some(atomic) = guard.as_ref() {
            atomic.store(byte, std::sync::atomic::Ordering::Release);
        }
    }

    /// Set the target bundle id sent as `App-Bundle-Id` header on
    /// every subsequent request. Runner rebinds
    /// `XCUIApplication(bundleIdentifier:)` per request if this differs
    /// from its boot-time default.
    #[must_use]
    pub fn with_target_bundle_id<S: Into<String>>(mut self, bundle: S) -> Self {
        self.target_bundle_id = Some(bundle.into());
        self
    }

    /// Mutable variant of [`Self::with_target_bundle_id`]. Used from
    /// the driver-trait pass-through where the driver holds the client
    /// by value and can't easily use the builder shape.
    pub fn set_target_bundle_id<S: Into<String>>(&mut self, bundle: S) {
        self.target_bundle_id = Some(bundle.into());
    }

    /// When set, every subsequent request sends `App-Activate: true`,
    /// causing the runner to `.activate()` the resolved target before
    /// operating.
    #[must_use]
    pub fn with_auto_activate(mut self, activate: bool) -> Self {
        self.auto_activate = activate;
        self
    }

    /// Mutable variant of [`Self::with_auto_activate`].
    pub fn set_auto_activate(&mut self, activate: bool) {
        self.auto_activate = activate;
    }

    /// Set the input dispatch mode header.
    /// Sent as `Input-Dispatch-Mode: <mode>` on every request that
    /// touches text input (currently `/fill` + `/clear`). Runner routes:
    /// - `a11y` (default) — resolve focus via a11y tree, dispatch to
    ///   XCUIElement.typeText
    /// - `key-events` — skip a11y-focus resolution, send raw key events
    ///   via IOHID / daemon
    /// - `auto` — try a11y first, fall back to key-events on
    ///   ElementNotFound
    ///
    /// `--force-key-events` on `smix run` sets this to `key-events`.
    #[must_use]
    pub fn with_input_dispatch_mode(mut self, mode: InputDispatchMode) -> Self {
        self.input_dispatch_mode = Some(mode);
        self
    }

    /// Mutable variant.
    pub fn set_input_dispatch_mode(&mut self, mode: InputDispatchMode) {
        self.input_dispatch_mode = Some(mode);
    }

    /// Accessor for the current target bundle. Used by callers
    /// that want to log or diagnose.
    pub fn target_bundle_id(&self) -> Option<&str> {
        self.target_bundle_id.as_deref()
    }

    /// Accessor for the current auto-activate flag.
    pub fn auto_activate(&self) -> bool {
        self.auto_activate
    }

    /// Attach a session id sent as the `Session-Id` header on every
    /// subsequent request. When the runner sees this header it looks up
    /// the session's cached `XCUIApplication` and skips the per-request
    /// `.activate()`, eliminating the activation storm on long-running
    /// gates. Typically not called directly — use `open_session()` +
    /// a session wrapper instead.
    pub fn set_session_id<S: Into<String>>(&mut self, id: S) {
        self.session_id = Some(id.into());
    }

    /// Clear a previously-set session id, reverting to legacy per-request
    /// `resolveApp()` (now itself rate-limited to at most one `.activate()`
    /// per 5 s per bundle-id).
    pub fn clear_session_id(&mut self) {
        self.session_id = None;
    }

    /// The session id in force for outgoing requests, or `None` if the
    /// legacy per-request rebind path is in use.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Enable liveness observability. The client tracks the last `window`
    /// request outcomes; if a majority are non-success (5xx, unreachable,
    /// connection-refused), subsequent calls surface
    /// [`RunnerTransportError::RunnerDegraded`] instead of returning
    /// silent stale data. On any `is_connect()` failure the client also
    /// probes `/health`; if that fails within 1 s the next call surfaces
    /// [`RunnerTransportError::RunnerDied`].
    ///
    /// Idempotent; safe to call more than once (last window size wins).
    /// Default is disabled (behavior identical to v1.0.1).
    #[must_use]
    pub fn with_liveness_window(self, window: usize) -> Self {
        {
            let mut guard = self
                .liveness
                .lock()
                .expect("liveness mutex must not be poisoned");
            *guard = Some(LivenessState::with_window(window));
        }
        self
    }

    /// Attach a [`smix_sim_health::SimHealthMonitor`] that receives
    /// `/health` outcome observations. Every `health()`, `health_detail()`
    /// and internal `probe_death()` call feeds
    /// `SimHealthMonitor::record_health_ok` / `record_health_fail`, and
    /// subscribers get a `Degraded` / `Dead` transition without polling.
    ///
    /// Additive over `with_liveness_window`; both may be enabled at the
    /// same time — the liveness window handles per-call transport
    /// classification, the sim-health monitor aggregates across the
    /// wider observation surface (screenshot wall time, watched
    /// processes) fed by other crates.
    ///
    /// Since smix 1.0.4.
    #[must_use]
    pub fn with_sim_health(mut self, monitor: smix_sim_health::SimHealthMonitor) -> Self {
        self.sim_health = Some(monitor);
        self
    }

    /// Mutable variant of [`Self::with_sim_health`].
    pub fn set_sim_health(&mut self, monitor: smix_sim_health::SimHealthMonitor) {
        self.sim_health = Some(monitor);
    }

    /// Accessor for the attached sim-health monitor, or `None` if the
    /// caller opted out. Used by driver / SDK layers that need to
    /// subscribe to state transitions or read the current classification.
    pub fn sim_health(&self) -> Option<&smix_sim_health::SimHealthMonitor> {
        self.sim_health.as_ref()
    }

    /// Apply per-request context headers to a reqwest builder.
    /// Every method that constructs a request calls this so the
    /// `App-Bundle-Id` / `App-Activate` / `Session-Id` /
    /// `Input-Dispatch-Mode` headers are sent uniformly.
    fn apply_context(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut b = builder;
        if let Some(bundle) = &self.target_bundle_id {
            b = b.header("App-Bundle-Id", bundle);
        }
        if self.auto_activate {
            b = b.header("App-Activate", "true");
        }
        if let Some(mode) = self.input_dispatch_mode {
            b = b.header("Input-Dispatch-Mode", mode.header_value());
        }
        if let Some(sid) = &self.session_id {
            b = b.header("Session-Id", sid);
        }
        b
    }

    /// Base URL accessor.
    pub fn base(&self) -> &str {
        &self.base
    }

    // ---- low-level helpers ----

    fn url(&self, endpoint: &str, include: Option<IncludeScope>) -> String {
        match include {
            Some(s) => format!("{}{}?include={}", self.base, endpoint, s.query_value()),
            None => format!("{}{}", self.base, endpoint),
        }
    }

    /// Wire-level transport retry for transient reqwest connection
    /// errors (`is_connect()` / `is_request()` — pre-server-receipt failures
    /// raised before any bytes leave the local socket, idempotent-safe for
    /// every route). Server-side `5xx` / `NonJsonBody` are NOT retried here:
    /// those are semantic responses, retried (or not) by callers like
    /// `driver.find`'s 500 retry loop or `driver.wait_for`'s selector poll.
    ///
    /// Budget: 3 attempts total (1 initial + 2 retries) × `TRANSPORT_BACKOFF_MS`
    /// between attempts. Catches sim/runner socket hiccups mid-flow
    /// without inflating happy-path latency: retries only fire on actual failure.
    async fn send_with_retry<F>(
        &self,
        endpoint: &str,
        builder_fn: F,
    ) -> Result<reqwest::Response, RunnerTransportError>
    where
        F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
    {
        // Preflight: if liveness is enabled and previously observed the
        // runner died, short-circuit until the caller resets it (via a
        // successful direct `/health`).
        if let Some(err) = self.preflight_liveness() {
            return Err(err);
        }
        let mut attempts_left = TRANSPORT_MAX_ATTEMPTS;
        loop {
            attempts_left -= 1;
            match builder_fn(&self.client).send().await {
                Ok(res) => {
                    // v1.0.4 §D7 — parse `X-Sim-Health` response header
                    // and propagate to the attached Session state atomic
                    // (if any). Absent header leaves state unchanged.
                    let sim_health_hdr = res
                        .headers()
                        .get("X-Sim-Health")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    self.update_session_state_from_header(sim_health_hdr.as_deref());
                    // Record but don't classify yet — status-level handling
                    // happens in the json_* helpers so the "5xx but recovered
                    // via retry" case is still counted as success.
                    return Ok(res);
                }
                Err(e) => {
                    let retryable = e.is_connect() || e.is_request();
                    if !retryable || attempts_left == 0 {
                        let is_connect = e.is_connect();
                        let err_str = format!("{e}");
                        self.record_outcome(endpoint, false, &err_str);
                        if is_connect && let Some(died) = self.probe_death(endpoint, &err_str).await
                        {
                            return Err(died);
                        }
                        return Err(RunnerTransportError::FetchFailed {
                            endpoint: endpoint.to_string(),
                            source: e,
                        });
                    }
                    tokio::time::sleep(Duration::from_millis(TRANSPORT_BACKOFF_MS)).await;
                }
            }
        }
    }

    fn record_outcome(&self, endpoint: &str, success: bool, err: &str) {
        let mut guard = self
            .liveness
            .lock()
            .expect("liveness mutex must not be poisoned");
        if let Some(state) = guard.as_mut() {
            state.record(endpoint, success, err);
        }
    }

    fn preflight_liveness(&self) -> Option<RunnerTransportError> {
        let guard = self
            .liveness
            .lock()
            .expect("liveness mutex must not be poisoned");
        if let Some(state) = guard.as_ref() {
            if state.died {
                return Some(RunnerTransportError::RunnerDied {
                    last_seen_ms: state.last_seen_ms,
                    last_error: state.last_error.clone(),
                });
            }
            return state.degraded();
        }
        None
    }

    async fn probe_death(&self, endpoint: &str, err_str: &str) -> Option<RunnerTransportError> {
        // Snapshot `last_seen_ms` before probing so the caller can
        // report "last successful contact at X".
        let last_seen_ms = {
            let guard = self
                .liveness
                .lock()
                .expect("liveness mutex must not be poisoned");
            guard.as_ref().map(|s| s.last_seen_ms).unwrap_or(0)
        };
        let url = format!("{}/health", self.base);
        let alive = tokio::time::timeout(Duration::from_millis(1000), self.client.get(&url).send())
            .await
            .ok()
            .and_then(|r| r.ok())
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        if alive {
            return None;
        }
        {
            let mut guard = self
                .liveness
                .lock()
                .expect("liveness mutex must not be poisoned");
            if let Some(state) = guard.as_mut() {
                state.died = true;
                state.last_endpoint = endpoint.to_string();
                state.last_error = err_str.to_string();
            }
        }
        Some(RunnerTransportError::RunnerDied {
            last_seen_ms,
            last_error: err_str.to_string(),
        })
    }

    async fn json_get<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        include: Option<IncludeScope>,
    ) -> Result<T, RunnerTransportError> {
        let url = self.url(endpoint, include);
        let res = self
            .send_with_retry(endpoint, |c| self.apply_context(c.get(&url)))
            .await?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            let err = classify_error_body(endpoint, status.as_u16(), &body);
            self.record_outcome(endpoint, false, &err.to_string());
            return Err(err);
        }
        match res.json::<T>().await {
            Ok(v) => {
                self.record_outcome(endpoint, true, "");
                Ok(v)
            }
            Err(source) => {
                let err = RunnerTransportError::NonJsonBody {
                    endpoint: endpoint.to_string(),
                    source,
                };
                self.record_outcome(endpoint, false, &err.to_string());
                Err(err)
            }
        }
    }

    async fn json_post<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        body: &B,
        include: Option<IncludeScope>,
    ) -> Result<T, RunnerTransportError> {
        let url = self.url(endpoint, include);
        let res = self
            .send_with_retry(endpoint, |c| self.apply_context(c.post(&url).json(body)))
            .await?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            let err = classify_error_body(endpoint, status.as_u16(), &body);
            self.record_outcome(endpoint, false, &err.to_string());
            return Err(err);
        }
        match res.json::<T>().await {
            Ok(v) => {
                self.record_outcome(endpoint, true, "");
                Ok(v)
            }
            Err(source) => {
                let err = RunnerTransportError::NonJsonBody {
                    endpoint: endpoint.to_string(),
                    source,
                };
                self.record_outcome(endpoint, false, &err.to_string());
                Err(err)
            }
        }
    }

    // ---- public methods (mirrors TS RunnerClient surface) ----

    /// `GET /health` — bare alive probe (no memoization). Returns true on 2xx.
    pub async fn health(&self) -> bool {
        let url = self.url("/health", None);
        let ok = match self.client.get(&url).send().await {
            Ok(res) => res.status().is_success(),
            Err(_) => false,
        };
        if let Some(monitor) = &self.sim_health {
            if ok {
                monitor.record_health_ok();
            } else {
                monitor.record_health_fail();
            }
        }
        ok
    }

    /// `GET /health` memoized — first call probes; subsequent are no-ops
    /// after one success. Failure raises [`RunnerTransportError::Unreachable`].
    /// Mirrors TS `ensureReachable`.
    pub async fn ensure_reachable(&self) -> Result<(), RunnerTransportError> {
        if self.reachable.load(Ordering::Acquire) {
            return Ok(());
        }
        if self.health().await {
            self.reachable.store(true, Ordering::Release);
            return Ok(());
        }
        Err(RunnerTransportError::Unreachable {
            endpoint: "/health".into(),
            message: format!("SmixRunner not reachable at {}", self.base),
        })
    }

    /// `GET /health` extended — parses the v1.0.2+ JSON body carrying
    /// uptime / activation counters / open-session count. On pre-v1.0.2
    /// runners returning a bare 200 with no body, returns a default
    /// [`smix_runner_wire::HealthResponse`] with only `ok = true` set.
    pub async fn health_detail(
        &self,
    ) -> Result<smix_runner_wire::HealthResponse, RunnerTransportError> {
        let url = self.url("/health", None);
        let res = self.client.get(&url).send().await.map_err(|source| {
            if let Some(monitor) = &self.sim_health {
                monitor.record_health_fail();
            }
            RunnerTransportError::FetchFailed {
                endpoint: "/health".into(),
                source,
            }
        })?;
        let status = res.status();
        if !status.is_success() {
            if let Some(monitor) = &self.sim_health {
                monitor.record_health_fail();
            }
            let body = res.text().await.unwrap_or_default();
            return Err(RunnerTransportError::NonSuccessStatus {
                endpoint: "/health".into(),
                status: status.as_u16(),
                body,
            });
        }
        if let Some(monitor) = &self.sim_health {
            monitor.record_health_ok();
        }
        // Legacy runners (< v1.0.2) return an empty body; treat parse
        // failure as {ok: true} to keep the "health passed" invariant.
        let text = res.text().await.unwrap_or_default();
        if text.trim().is_empty() {
            return Ok(smix_runner_wire::HealthResponse {
                ok: true,
                ..Default::default()
            });
        }
        Ok(
            serde_json::from_str(&text).unwrap_or(smix_runner_wire::HealthResponse {
                ok: true,
                ..Default::default()
            }),
        )
    }

    /// `POST /session/open` — reserve a runner-side XCUIApplication
    /// binding and (optionally) activate it once. The returned session
    /// id becomes the `Session-Id` header on subsequent requests via
    /// [`Self::set_session_id`] or a session wrapper.
    pub async fn open_session(
        &self,
        req: &smix_runner_wire::SessionOpenRequest,
    ) -> Result<smix_runner_wire::SessionOpenResponse, RunnerTransportError> {
        self.json_post("/session/open", req, None).await
    }

    /// `POST /session/close` — release a previously-opened session.
    /// Idempotent; closing an unknown session returns `ok = true`.
    pub async fn close_session(
        &self,
        req: &smix_runner_wire::SessionCloseRequest,
    ) -> Result<smix_runner_wire::SessionCloseResponse, RunnerTransportError> {
        self.json_post("/session/close", req, None).await
    }

    /// `POST /session/renew-activation` — re-issue `.activate()` on the
    /// session's cached binding, subject to the runner-side per-session
    /// 5 s rate limit. Escape hatch for consumers that detect drift.
    pub async fn renew_session_activation(
        &self,
        req: &smix_runner_wire::SessionRenewActivationRequest,
    ) -> Result<smix_runner_wire::SessionRenewActivationResponse, RunnerTransportError> {
        self.json_post("/session/renew-activation", req, None).await
    }

    /// v1.0.4 §D5 — `POST /session/close-all` — clear every open
    /// session on the runner. Used by `smix runner cycle` and the
    /// runner-side supervisor to drop stale bindings after a cycle.
    /// Idempotent — closing an empty session table returns
    /// `{ok:true, closed:0}`.
    pub async fn close_all_sessions(
        &self,
    ) -> Result<smix_runner_wire::SessionCloseAllResponse, RunnerTransportError> {
        // No request body — send an empty JSON object.
        self.json_post("/session/close-all", &serde_json::json!({}), None)
            .await
    }

    /// v1.0.4 §D14 — `POST /session/relaunch-app` — instruct the runner
    /// to `terminate()` + `launch()` the session's cached
    /// `XCUIApplication` binding IN PLACE. Preserves session id and
    /// XCUITest binding; recovers from a downstream app crash without
    /// cycling the runner.
    pub async fn relaunch_session_app(
        &self,
        req: &smix_runner_wire::SessionRelaunchAppRequest,
    ) -> Result<smix_runner_wire::SessionRelaunchAppResponse, RunnerTransportError> {
        self.json_post("/session/relaunch-app", req, None).await
    }

    /// v1.0.5 §D1 — `POST /session/list` — enumerate every session
    /// currently known to the runner. Useful for diagnostics + the
    /// supervisor + SDK `Session::still_valid()` probe after a cycle.
    pub async fn list_sessions(
        &self,
    ) -> Result<smix_runner_wire::SessionListResponse, RunnerTransportError> {
        self.json_post("/session/list", &serde_json::json!({}), None)
            .await
    }

    /// v1.0.8 §D1 — `POST /session/terminate-app` — cooperative
    /// `XCUIApplication.terminate()` on the session's cached binding
    /// via testmanagerd (NOT `simctl terminate` SIGKILL). Does not
    /// signal `com.apple.ReportCrash`. Paired with
    /// [`Self::launch_session_app`] and a host-side
    /// `SimctlClient::clear_app_sandbox` invocation to implement the
    /// `Session::reset_app_data` orchestration that eliminates the
    /// "Insight quit unexpectedly" system dialog.
    pub async fn terminate_session_app(
        &self,
        req: &smix_runner_wire::SessionAppLifecycleRequest,
    ) -> Result<smix_runner_wire::SessionAppLifecycleResponse, RunnerTransportError> {
        self.json_post("/session/terminate-app", req, None).await
    }

    /// v1.0.8 §D1 — `POST /session/launch-app` — cooperative
    /// `XCUIApplication.launch()` on the session's cached binding.
    /// Companion of [`Self::terminate_session_app`].
    pub async fn launch_session_app(
        &self,
        req: &smix_runner_wire::SessionAppLifecycleRequest,
    ) -> Result<smix_runner_wire::SessionAppLifecycleResponse, RunnerTransportError> {
        self.json_post("/session/launch-app", req, None).await
    }

    /// v1.0.7 §D5 — `POST /diagnostic/dump` — one-shot post-mortem
    /// snapshot of the runner's runtime state. Bundles the recent
    /// subprocess ring buffer, open sessions, sim-health state,
    /// supervisor pid, and uptime. `smix diagnostic dump` calls this
    /// then pretty-prints.
    ///
    /// Legacy runners (v1.0.6 and earlier) return 404; consumers can
    /// gracefully fall back to the client-side ring buffer only.
    pub async fn diagnostic_dump(
        &self,
    ) -> Result<smix_runner_wire::DiagnosticDumpResponse, RunnerTransportError> {
        self.json_post("/diagnostic/dump", &serde_json::json!({}), None)
            .await
    }

    /// `GET /tree?include=` — full a11y tree dump.
    ///
    /// Post-deser pass [`derive_roles_recursive`] fills
    /// `A11yNode.role` from `raw_type` because the Swift `/tree` route
    /// only emits `rawType` on the wire. Without this, `Selector::Role`
    /// would never match real-sim payloads.
    pub async fn get_tree(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<A11yNode, RunnerTransportError> {
        let mut tree: A11yNode = self.json_get("/tree", include).await?;
        derive_roles_recursive(&mut tree);
        Ok(tree)
    }

    /// `GET /system-popups?include=` — list system popups.
    pub async fn system_popups(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<Vec<SystemPopup>, RunnerTransportError> {
        #[derive(Deserialize)]
        struct Envelope {
            popups: Vec<SystemPopup>,
        }
        let env: Envelope = self.json_get("/system-popups", include).await?;
        Ok(env.popups)
    }

    /// `POST /system-popup-action`. Tap a button
    /// on a previously enumerated popup. `popup_id` and `button_id` are
    /// the `Popup.id` / `PopupButton.id` strings returned by
    /// [`Self::system_popups`]; the runner walks the same scan order so
    /// the round-trip does not need a host-side id map.
    ///
    /// Returns `Ok(true)` when the runner matched both ids and dispatched
    /// the tap, `Ok(false)` when the runner returned a 404 not_found
    /// (popup id, button id, or synthesize dispatch missed). Transport
    /// errors and non-2xx-non-404 statuses surface as
    /// [`RunnerTransportError`].
    pub async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, RunnerTransportError> {
        let url = self.url("/system-popup-action", None);
        let body = SystemPopupActionRequest {
            popup_id: popup_id.to_string(),
            button_id: button_id.to_string(),
        };
        let res = self
            .send_with_retry("/system-popup-action", |c| c.post(&url).json(&body))
            .await?;
        let status = res.status();
        if status.as_u16() == 404 {
            return Ok(false);
        }
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(RunnerTransportError::NonSuccessStatus {
                endpoint: "/system-popup-action".to_string(),
                status: status.as_u16(),
                body: body.chars().take(200).collect(),
            });
        }
        let resp: SystemPopupActionResponse =
            res.json()
                .await
                .map_err(|source| RunnerTransportError::NonJsonBody {
                    endpoint: "/system-popup-action".to_string(),
                    source,
                })?;
        Ok(resp.ok)
    }

    /// `POST /tap` — selector → element tap. Errors:
    /// - 404 → [`TapNotFoundError`] returned via `Err(.. Other ..)`. We
    ///   keep the simple `RunnerTransportError + body inspection`
    ///   pattern. Driver layer wraps with ExpectationFailure.
    pub async fn tap(
        &self,
        selector: &Selector,
        mode: TapMode,
        include: Option<IncludeScope>,
    ) -> Result<TapResult, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            selector: &'a Selector,
            mode: TapMode,
        }
        self.json_post("/tap", &Req { selector, mode }, include)
            .await
    }

    /// `POST /tap-at-norm-coord` — Apple native UI event coord tap.
    pub async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req {
            nx: f64,
            ny: f64,
        }
        let _: serde_json::Value = self
            .json_post("/tap-at-norm-coord", &Req { nx, ny }, None)
            .await?;
        Ok(())
    }

    /// `POST /double-tap-at-norm-coord` — double-tap at
    /// viewport-normalized coord. Android-specific endpoint backing
    /// `AndroidDriver::double_tap` after host-resolve. iOS path uses
    /// selector-based `/double-tap`; this exists for Android where the
    /// runner does its own host-resolve via tree dump.
    pub async fn double_tap_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req {
            nx: f64,
            ny: f64,
        }
        let _: serde_json::Value = self
            .json_post("/double-tap-at-norm-coord", &Req { nx, ny }, None)
            .await?;
        Ok(())
    }

    /// `POST /long-press-at-norm-coord` — long-press at coord
    /// with explicit duration. Android-specific.
    pub async fn long_press_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
    ) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req {
            nx: f64,
            ny: f64,
            #[serde(rename = "durationMs")]
            duration_ms: u64,
        }
        let _: serde_json::Value = self
            .json_post(
                "/long-press-at-norm-coord",
                &Req {
                    nx,
                    ny,
                    duration_ms,
                },
                None,
            )
            .await?;
        Ok(())
    }

    /// `POST /input-text` — type text into currently-focused
    /// input. Caller must tap to focus the field first (AndroidDriver
    /// orchestrates). Android-specific.
    pub async fn input_text(&self, text: &str) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            text: &'a str,
        }
        let _: serde_json::Value = self.json_post("/input-text", &Req { text }, None).await?;
        Ok(())
    }

    /// `POST /tap-by-id` — `XCUIElement.tap()` via the XCTest
    /// gesture-recognizer chain. Drives SwiftUI `.sheet` / `.alert` /
    /// `.confirmationDialog` / `.fullScreenCover` dismiss bindings that the
    /// default host-HID-at-coord path can't trigger. Returns
    /// `Ok(true)` when the element was found and tapped, `Ok(false)` when
    /// the runner reported `ok=false` (element not found / NSException
    /// surfaced through `smixGuarded`). Parallel to
    /// `tap_at_norm_coord`; the existing `/tap` envelope stays untouched.
    pub async fn tap_by_id(&self, id: &str) -> Result<bool, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            id: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            ok: bool,
        }
        let resp: Resp = self.json_post("/tap-by-id", &Req { id }, None).await?;
        Ok(resp.ok)
    }

    /// `POST /find-text-by-ocr` — Apple Vision OCR over the
    /// current XCUIScreen screenshot. Returns the matching text
    /// observation's bounding box normalized to `[0, 1]` in UIKit coord
    /// space (top-left origin, y-down). `Ok(None)` when no observation
    /// matches the keyword. Sense layer covering "lib without testID
    /// but with visible text" scenarios.
    ///
    /// `locales` are BCP-47 language subtags (e.g. `["en"]` / `["ja"]`);
    /// empty defaults to `["en"]` on the swift side. `recognition_level`
    /// is `"accurate"` (default, slower + higher accuracy) or `"fast"`.
    pub async fn find_text_by_ocr(
        &self,
        text: &str,
        locales: &[String],
        recognition_level: &str,
    ) -> Result<Option<OcrFrame>, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            text: &'a str,
            locales: &'a [String],
            recognition_level: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            found: bool,
            frame: Option<[f64; 4]>,
        }
        let resp: Resp = self
            .json_post(
                "/find-text-by-ocr",
                &Req {
                    text,
                    locales,
                    recognition_level,
                },
                None,
            )
            .await?;
        if !resp.found {
            return Ok(None);
        }
        match resp.frame {
            Some([nx, ny, w, h]) => Ok(Some(OcrFrame { nx, ny, w, h })),
            None => Ok(None),
        }
    }

    /// Eval JS against app-side WKWebView bridge. Does NOT use the
    /// XCUITest runner — POSTs directly to the app-side debug bridge on
    /// `127.0.0.1:28080/eval` (iOS sim shares host loopback). Returns
    /// the JS eval result as JSON Value or runtime error from the bridge.
    ///
    /// **Scope**: works for any app that exposes the
    /// `SmixWebViewBridge`. Bridge port is fixed at 28080 today.
    pub async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            js: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            result: serde_json::Value,
            error: String,
        }
        let url = "http://127.0.0.1:28080/eval";
        let resp = reqwest::Client::new()
            .post(url)
            .json(&Req { js })
            .send()
            .await
            .map_err(|e| RunnerTransportError::Unreachable {
                endpoint: "webview-bridge".into(),
                message: format!("webview-bridge POST: {e}"),
            })?;
        let status = resp.status();
        let body: Resp = resp
            .json()
            .await
            .map_err(|e| RunnerTransportError::Unreachable {
                endpoint: "webview-bridge".into(),
                message: format!("webview-bridge JSON decode (status {status}): {e}"),
            })?;
        if !body.error.is_empty() {
            return Err(RunnerTransportError::Unreachable {
                endpoint: "webview-bridge".into(),
                message: format!("webview-bridge JS error: {}", body.error),
            });
        }
        Ok(body.result)
    }

    /// `POST /double-tap` — XCUIElement.doubleTap() public API.
    /// Mirrors `/tap` envelope but issues two-tap gesture on the resolved
    /// element. Same as Maestro `doubleTapOn`.
    pub async fn double_tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<TapResult, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            selector: &'a Selector,
        }
        self.json_post("/double-tap", &Req { selector }, include)
            .await
    }

    /// `POST /long-press` — XCUIElement.press(forDuration:) public
    /// API. `duration_ms` comes from maestro yaml `duration:`, default 500.
    /// Same as Maestro `longPressOn`.
    pub async fn long_press(
        &self,
        selector: &Selector,
        duration_ms: u64,
        include: Option<IncludeScope>,
    ) -> Result<TapResult, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            selector: &'a Selector,
            #[serde(rename = "durationMs")]
            duration_ms: u64,
        }
        self.json_post(
            "/long-press",
            &Req {
                selector,
                duration_ms,
            },
            include,
        )
        .await
    }

    /// `POST /set-orientation` — rotate sim via swift
    /// `XCUIDevice.shared.orientation` setter. `orientation` literal:
    /// "portrait" | "portraitUpsideDown" | "landscapeLeft" | "landscapeRight".
    pub async fn set_orientation(&self, orientation: &str) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            orientation: &'a str,
        }
        let _: serde_json::Value = self
            .json_post("/set-orientation", &Req { orientation }, None)
            .await?;
        Ok(())
    }

    /// `POST /swipe-at-norm-coord` — Apple native UI event from-to
    /// swipe. Sibling of [`Self::tap_at_norm_coord`] under §9 #3 escape hatch.
    pub async fn swipe_at_norm_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req {
            #[serde(rename = "fromNx")]
            from_nx: f64,
            #[serde(rename = "fromNy")]
            from_ny: f64,
            #[serde(rename = "toNx")]
            to_nx: f64,
            #[serde(rename = "toNy")]
            to_ny: f64,
        }
        let _: serde_json::Value = self
            .json_post(
                "/swipe-at-norm-coord",
                &Req {
                    from_nx: from.0,
                    from_ny: from.1,
                    to_nx: to.0,
                    to_ny: to.1,
                },
                None,
            )
            .await?;
        Ok(())
    }

    /// `POST /find` — boolean existence quick-probe.
    pub async fn find(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<bool, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            selector: &'a Selector,
        }
        #[derive(Deserialize)]
        struct Resp {
            /// Wire field name. iOS SmixRunner emits `found`.
            #[serde(default)]
            found: bool,
            /// Legacy alias — historical wire used `exists`.
            #[serde(default)]
            exists: bool,
        }
        // Previously read `exists || ok`, but `ok` is the transport-
        // level "runner responded 200" flag that is TRUE for both
        // found and not-found. That short-circuited the `find`
        // predicate to always-true, defeating when-visible logic.
        // SmixRunner emits `found`; historical wire had `exists`.
        // Accept either; do NOT fall back to `ok`.
        let r: Resp = self.json_post("/find", &Req { selector }, include).await?;
        Ok(r.found || r.exists)
    }

    /// `POST /fill` — fill text into focused / matched input.
    pub async fn fill(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
    ) -> Result<RunnerKeyboardResult, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            selector: &'a Selector,
            text: &'a str,
        }
        self.json_post("/fill", &Req { selector, text }, include)
            .await
    }

    /// `POST /clear` — clear focused / matched input.
    pub async fn clear(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<RunnerKeyboardResult, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            selector: &'a Selector,
        }
        self.json_post("/clear", &Req { selector }, include).await
    }

    /// `POST /press-key` — press named key (Return/Tab/Delete/etc.).
    pub async fn press_key(
        &self,
        key: KeyName,
    ) -> Result<RunnerKeyboardResult, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req {
            key: KeyName,
        }
        self.json_post("/press-key", &Req { key }, None).await
    }

    /// `POST /scroll {selector, direction, include?}` — scroll-until-visible.
    pub async fn scroll(
        &self,
        selector: &RunnerScrollSelector,
        direction: SwipeDirection,
        include: Option<IncludeScope>,
    ) -> Result<u32, RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            selector: &'a RunnerScrollSelector,
            direction: SwipeDirection,
        }
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            matched: Option<bool>,
            #[serde(default)]
            swipes: Option<u32>,
        }
        let r: Resp = self
            .json_post(
                "/scroll",
                &Req {
                    selector,
                    direction,
                },
                include,
            )
            .await?;
        let matched = r.matched.unwrap_or(false);
        let swipes = r.swipes.unwrap_or(0);
        if !matched {
            // Driver layer is responsible for converting matched:false
            // to ExpectationFailure(ELEMENT_NOT_FOUND); here we surface
            // via MalformedBody when shape is missing entirely, otherwise
            // return the swipe count for the caller to inspect.
            //
            // We return swipes; driver wraps via RunnerScrollNotMatched.
            return Err(RunnerTransportError::MalformedBody {
                endpoint: "/scroll".into(),
                detail: format!("not_matched after {swipes} swipes"),
            });
        }
        Ok(swipes)
    }

    /// `POST /swipe-once {direction}` — single swipe, no probe.
    pub async fn swipe_once(&self, direction: SwipeDirection) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req {
            direction: SwipeDirection,
        }
        let _: serde_json::Value = self
            .json_post("/swipe-once", &Req { direction }, None)
            .await?;
        Ok(())
    }

    /// `POST /foreground {bundleId}` — bring app to foreground.
    pub async fn foreground(&self, bundle_id: &str) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            #[serde(rename = "bundleId")]
            bundle_id: &'a str,
        }
        let _: serde_json::Value = self
            .json_post("/foreground", &Req { bundle_id }, None)
            .await?;
        Ok(())
    }

    /// `POST /hide-keyboard`.
    pub async fn hide_keyboard(&self) -> Result<(), RunnerTransportError> {
        let _: serde_json::Value = self
            .json_post("/hide-keyboard", &serde_json::json!({}), None)
            .await?;
        Ok(())
    }

    /// `POST /back` — back gesture.
    pub async fn back(&self) -> Result<(), RunnerTransportError> {
        let _: serde_json::Value = self
            .json_post("/back", &serde_json::json!({}), None)
            .await?;
        Ok(())
    }

    // ---- recorder ----

    /// `POST /record/start` — begin recording.
    pub async fn start_record(&self) -> Result<(), RunnerTransportError> {
        let _: serde_json::Value = self
            .json_post("/record/start", &serde_json::json!({}), None)
            .await?;
        Ok(())
    }

    /// `GET /record/poll` — drain recorded events.
    pub async fn poll_record(&self) -> Result<Vec<RecordedEvent>, RunnerTransportError> {
        #[derive(Deserialize)]
        struct Envelope {
            events: Vec<RecordedEvent>,
        }
        let env: Envelope = self.json_get("/record/poll", None).await?;
        Ok(env.events)
    }

    /// `POST /record/stop` — stop recording, drain final events.
    pub async fn stop_record(&self) -> Result<Vec<RecordedEvent>, RunnerTransportError> {
        #[derive(Deserialize)]
        struct Envelope {
            events: Vec<RecordedEvent>,
        }
        let env: Envelope = self
            .json_post("/record/stop", &serde_json::json!({}), None)
            .await?;
        Ok(env.events)
    }
}
