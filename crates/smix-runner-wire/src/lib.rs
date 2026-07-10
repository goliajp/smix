#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-runner-wire — pure wire types for the SmixRunnerCore HTTP IPC.
//!
//! Stone, zero project coupling beyond the smix-screen / smix-selector
//! upstream types it embeds (those are themselves stones).
//!
//! Pairs with [`smix-runner-client`](https://docs.rs/smix-runner-client)
//! which provides the reqwest+tokio HTTP client; this crate is just the
//! types so a consumer can:
//!
//! - Drive their own HTTP client (sync or async, custom transport)
//! - Hand-roll a server side serving the same wire contract
//! - Pin the wire-shape contract independently of the client implementation
//!
//! # Route surface
//!
//! 18 wire endpoints (see the `smix-runner-client` crate for the
//! corresponding method names):
//!
//! - `GET /health` — bare 200/non-200
//! - `GET /tree?include=…` → [`smix_screen::A11yNode`]
//! - `GET /system-popups?include=…` → `Vec<`[`SystemPopup`]`>`
//! - `POST /system-popup-action` `{popupId, buttonId}` → [`SystemPopupActionResponse`]
//! - `POST /tap` → [`TapResult`]
//! - `POST /tap-at-norm-coord` `{nx, ny}` → 200/`{ok}`
//! - `POST /find` `{selector}` → `{exists}` or `{ok}`
//! - `POST /fill` `{selector, text}` → [`RunnerKeyboardResult`]
//! - `POST /clear` `{selector}` → [`RunnerKeyboardResult`]
//! - `POST /press-key` `{key}` → [`RunnerKeyboardResult`]
//! - `POST /scroll` `{selector, direction}` → `{matched, swipes}`
//! - `POST /swipe-once` `{direction}` → `{ok}`
//! - `POST /foreground` `{bundleId}` → `{ok}`
//! - `POST /hide-keyboard` → `{ok}`
//! - `POST /back` → `{ok}`
//! - `POST /record/start` → `{ok}`
//! - `GET /record/poll` → `{events: [`[`RecordedEvent`]`]}`
//! - `POST /record/stop` → `{events: [`[`RecordedEvent`]`]}`

#![doc(html_root_url = "https://docs.smix.dev/smix-runner-wire")]

use serde::{Deserialize, Serialize};
use smix_selector::Selector;
use thiserror::Error;

// -------------------- Errors --------------------------------------------

/// Transport-level failure variants exposed by the HTTP client. The
/// concrete `reqwest::Error` source lives in `smix-runner-client`; the
/// wire stone exposes only the discriminator + endpoint context so
/// non-HTTP transports can reuse the variants.
#[derive(Debug, Error)]
pub enum RunnerTransportErrorKind {
    /// Network / transport-layer fetch error (timeout, DNS, TLS, etc.).
    #[error("runner {endpoint} fetch failed")]
    FetchFailed {
        /// Endpoint path that failed (e.g. `"/tap"`).
        endpoint: String,
    },
    /// Runner returned non-2xx HTTP status.
    #[error("runner {endpoint} returned status {status}: {body}")]
    NonSuccessStatus {
        /// Endpoint path that returned the error.
        endpoint: String,
        /// HTTP status code.
        status: u16,
        /// Raw response body (may be truncated).
        body: String,
    },
    /// Runner returned a body that wasn't valid JSON.
    #[error("runner {endpoint} returned non-JSON body")]
    NonJsonBody {
        /// Endpoint path that returned a non-JSON body.
        endpoint: String,
    },
    /// Runner returned valid JSON but it didn't match the expected schema.
    #[error("runner {endpoint} returned malformed body: {detail}")]
    MalformedBody {
        /// Endpoint path that returned the malformed body.
        endpoint: String,
        /// Schema-mismatch detail (serde error message).
        detail: String,
    },
    /// Runner is unreachable (refused / closed / not listening).
    #[error("runner {endpoint} unreachable: {message}")]
    Unreachable {
        /// Endpoint path attempted.
        endpoint: String,
        /// Reason (e.g. "connection refused").
        message: String,
    },
}

// -------------------- Common wire types ---------------------------------

/// `include` scope query param shared by `/tree` / `/tap` / `/fill` /
/// `/clear` / `/find` / `/scroll` / `/system-popups`. URL-only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerIncludeOpts {
    /// Optional include scope (e.g. system-popups → `AllWindows`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<IncludeScope>,
}

/// `include` scope literal — currently only `all-windows` (system popups
/// pierce the app frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IncludeScope {
    /// Include all windows (system popups, alerts) above the app frame.
    AllWindows,
}

impl IncludeScope {
    /// kebab-case wire string used in the query parameter value.
    pub fn query_value(self) -> &'static str {
        match self {
            IncludeScope::AllWindows => "all-windows",
        }
    }
}

// -------------------- /tap wire shape -----------------------------------

/// `POST /tap` mode discriminator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TapMode {
    /// Runner returns only the matched frame; host normalizes + injects
    /// via `/tap-at-norm-coord` (v1.6 c5 default).
    Resolve,
    /// Runner resolves AND taps (legacy v1.1 path A, host-HID-based).
    ResolveAndTap,
    /// Runner resolves selector then synthesizes the touch event via the
    /// XCTRunnerDaemonSession daemonProxy (v4.0 c3 swift G8 fix —
    /// bypasses the XCUIElement gesture recognizer chain so RN
    /// Pressable `RCTTouchHandler` UIGestureRecognizer receives the
    /// touch and fires the JS-thread `onPress` callback reliably).
    DaemonProxySynthesize,
}

/// `POST /tap` per-stage timing in milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TapStages {
    /// Time spent resolving the selector against the a11y tree.
    #[serde(rename = "resolveMs", default)]
    pub resolve_ms: f64,
    /// Time spent dispatching the tap event itself.
    #[serde(rename = "tapCallMs", default)]
    pub tap_call_ms: f64,
    /// End-to-end wall-clock for the whole tap call.
    #[serde(rename = "totalMs", default)]
    pub total_ms: f64,
    /// Time spent waiting for the element to exist (implicit wait).
    #[serde(rename = "waitExistenceMs", default)]
    pub wait_existence_ms: f64,
    /// Time spent reading the matched element's frame.
    #[serde(rename = "frameReadMs", default)]
    pub frame_read_ms: f64,
}

/// `POST /tap` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TapResult {
    /// Per-stage timing breakdown.
    #[serde(default)]
    pub stages: Option<TapStages>,
    /// Matched element's geometric frame, when resolve succeeded.
    #[serde(default)]
    pub frame: Option<smix_screen::Rect>,
    /// Application window frame (for normalizing the matched frame).
    #[serde(rename = "appFrame", default)]
    pub app_frame: Option<smix_screen::Rect>,
}

/// `POST /tap` request body.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TapRequest {
    /// Selector picking the tap target.
    pub selector: Selector,
    /// Resolve-only vs resolve-and-tap discriminator.
    pub mode: TapMode,
}

/// `POST /tap-at-norm-coord` request body.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TapAtNormCoordRequest {
    /// Normalized x coordinate in `(0, 1)` (app-frame relative).
    pub nx: f64,
    /// Normalized y coordinate in `(0, 1)` (app-frame relative).
    pub ny: f64,
}

// -------------------- /fill /clear /press-key wire shape ----------------

/// Per-stage timing returned by `/fill` / `/clear` / `/press-key`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardStages {
    /// Selector resolve time.
    #[serde(rename = "resolveMs", default)]
    pub resolve_ms: f64,
    /// Time waiting for keyboard appearance after the focus tap.
    #[serde(rename = "keyboardWaitMs", default)]
    pub keyboard_wait_ms: f64,
    /// Time spent typing the characters.
    #[serde(rename = "typingMs", default)]
    pub typing_ms: f64,
    /// End-to-end wall-clock for the whole keyboard operation.
    #[serde(rename = "totalMs", default)]
    pub total_ms: f64,
}

/// `POST /fill` / `POST /clear` / `POST /press-key` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerKeyboardResult {
    /// Tree snapshot after the keyboard operation completed (optional).
    #[serde(default)]
    pub tree: Option<smix_screen::A11yNode>,
    /// Per-stage timing breakdown (optional).
    #[serde(default)]
    pub stages: Option<KeyboardStages>,
}

// -------------------- /find wire shape -----------------------------------

/// `POST /find` request body.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindRequest {
    /// Selector to look up.
    pub selector: Selector,
}

/// `POST /find` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResponse {
    /// Whether the selector matched any element.
    #[serde(default)]
    pub exists: bool,
    /// Whether the resolve subsystem itself succeeded.
    #[serde(default)]
    pub ok: bool,
}

// -------------------- /scroll wire shape --------------------------------

/// Reduced selector shape used by `/scroll` (text-or-id only; complex
/// selectors are host-side-resolved before reaching the runner).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RunnerScrollSelector {
    /// Match by text content.
    Text {
        /// Text to match against.
        text: String,
    },
    /// Match by accessibility identifier.
    Id {
        /// Identifier to match against.
        id: String,
    },
}

/// `POST /scroll` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollResponse {
    /// Whether the selector matched after scrolling (None when unknown).
    #[serde(default)]
    pub matched: Option<bool>,
    /// Number of swipe iterations performed.
    #[serde(default)]
    pub swipes: Option<u32>,
}

// -------------------- /system-popups wire shape -------------------------

/// One system-popup discovered on the screen (alert / sheet / banner).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPopup {
    /// Stable identifier for matching across polls.
    pub id: String,
    /// Discriminator (e.g. `"alert"` / `"sheet"` / `"banner"`).
    #[serde(rename = "type")]
    pub kind: String,
    /// Originator (e.g. bundle id, system framework name).
    pub source: String,
    /// Popup title text.
    #[serde(default)]
    pub title: String,
    /// Popup body text.
    #[serde(default)]
    pub body: String,
    /// Buttons available on the popup.
    #[serde(default)]
    pub buttons: Vec<SystemPopupButton>,
}

/// One button on a [`SystemPopup`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPopupButton {
    /// Stable identifier for the button.
    pub id: String,
    /// Visible button label.
    pub label: String,
    /// Semantic role (`"cancel"` / `"destructive"` / `"default"`).
    pub role: String,
    /// Whether tapping this button performs a destructive action.
    #[serde(default)]
    pub dangerous: bool,
    /// Optional outcome hint (e.g. `"grants location permission"`).
    #[serde(rename = "outcomeHint", default)]
    pub outcome_hint: Option<String>,
}

/// `POST /system-popup-action` request body (v4.2 c2 — G9 act side).
///
/// `popupId` and `buttonId` round-trip from a prior `GET /system-popups`
/// enumerate (`Popup.id` / `PopupButton.id` fields). The runner walks the
/// same scan order on the act path, so callers do not need to maintain an
/// out-of-band id map.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPopupActionRequest {
    /// Popup id from a prior `Popup.id` enumerate.
    pub popup_id: String,
    /// Button id from a prior `PopupButton.id` enumerate.
    pub button_id: String,
}

/// `POST /system-popup-action` response body.
///
/// `ok=true` ⇒ the runner found the popup + button and dispatched a
/// daemonProxy touch; `ok=false` ⇒ neither side matched (either popup id
/// missed, button id missed, or synthesize raised inside the runner).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPopupActionResponse {
    /// Whether the popup-button match + tap dispatch succeeded.
    #[serde(default)]
    pub ok: bool,
    /// Wire-layer error discriminator ("not_found" / "bad_request" / etc.)
    /// emitted by the runner on the non-2xx path. Absent on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `GET /system-popups` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPopupsResponse {
    /// All popups currently on screen (empty when none).
    #[serde(default)]
    pub popups: Vec<SystemPopup>,
}

// -------------------- /record/* wire shape ------------------------------

/// One recorder event captured by `/record/start` → `/record/poll` flow.
///
/// v5.1 c3 S2 校正:swift `EventRecorder` 端实际 emit 的是 `rawCode` field
/// (kAXNotification raw int — 1018 = focus change / 1028 = HID / 4002 = userTesting / ...),
/// 不是 `code`。c2 capstone 初版 jq 用 `.code` 查 events.json 全返 null,
/// 误判 "0 个 1018",修正后真实有 3 × 1018。SDK 端 deserialize 之前同样
/// silent → `code` 字段永远 0。本 struct 字段名按 swift 真实 schema 对齐
/// (`raw_code` + serde camelCase → `rawCode`),并加 `extra` flatten 兜底
/// 把 swift 在 RecordedEvent 顶层平铺的 enrich 字段(`kind` / `frame` /
/// `payloadDescription` / `elementType` / 等)宽松接收。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedEvent {
    /// Numeric event-type discriminator(swift `RecordedEvent.rawCode`)。
    /// 典型值:1018 (kAXFirstResponderChangedNotification) / 1028
    /// (kAXHIDEventReceivedNotification) / 4002 (kAXUserTestingNotification)
    /// / 1006 (kAXAlertNotification) / 1021 (kAXPidStatusChangedNotification)。
    #[serde(default)]
    pub raw_code: i32,
    /// Capture-time timestamp in milliseconds.
    #[serde(default)]
    pub timestamp_ms: f64,
    /// Free-form per-event 顶层 enrich 字段(swift 端平铺 `kind` / `frame` /
    /// `payloadDescription` / `elementType` / `appBundleId` / `payloadClassName`
    /// 等)。reconcile 只读 `raw_code` + `timestamp_ms`,不依赖此字段;
    /// 上层 SDK 想看明细时按需取(类型 = `serde_json::Map`)。
    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `GET /record/poll` / `POST /record/stop` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordEventsResponse {
    /// Events captured since the last poll (chronological).
    #[serde(default)]
    pub events: Vec<RecordedEvent>,
}

// -------------------- /session/* wire shape (v1.0.2) --------------------
//
// Session lifecycle addresses the "activation storm" root cause: pre-v1.0.2
// runners re-bind + `.activate()` an `XCUIApplication` on every request
// whose `App-Activate: true` header is set. Long-running gates (visual /
// perf regression) accumulate thousands of activate calls, exhausting
// XCTest process arbitration on iOS 26.5+ and crashing `test_runForever()`
// mid-run. Sessions replace that with a one-shot lifecycle: open once,
// runner caches the binding + activates at most on transition or via
// explicit renew, close when the client is done.
//
// Wire compat: absent `Session-Id: <id>` header on any request falls
// through to the legacy per-request `resolveApp()` path, now itself
// rate-limited to at most one `.activate()` per 5 s per bundle-id (which
// is enough to keep the recovery-from-drift semantic of the original
// design without producing the storm).

/// `POST /session/open` request body.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOpenRequest {
    /// Bundle id (iOS) / package name (Android) the session is bound to.
    /// Empty string means "use the runner's boot-time default", which is
    /// almost always `com.apple.Preferences` — usable for testing but
    /// probably not what the client wants.
    #[serde(default)]
    pub bundle_id: String,
    /// If true, the runner calls `.activate()` once as part of the open,
    /// synchronously, before returning. Idiomatic for gates that want
    /// the target app foregrounded before the first `/tap` fires.
    #[serde(default)]
    pub activate: bool,
}

/// `POST /session/open` response body.
///
/// The returned `session_id` becomes the value of the `Session-Id`
/// request header on every subsequent request that should share this
/// session's cached app binding.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOpenResponse {
    /// Opaque token; treat as a stable string identifier. UUID today.
    pub session_id: String,
    /// Whether the runner issued an initial `.activate()` on open.
    /// Mirrors the request's `activate` field unless the runner
    /// rate-limited it (e.g. same bundle-id was activated within the
    /// last 5 s from a prior session).
    #[serde(default)]
    pub activated_once: bool,
    /// Server-side epoch millis at open. Consumers pair this with
    /// downstream `sessionUptimeMs` sidecar fields to reconstruct
    /// session timelines.
    #[serde(default)]
    pub server_time_ms: u64,
}

/// `POST /session/close` request body.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCloseRequest {
    /// Session to close. Absent / unknown / already-closed sessions
    /// return 200 with `ok=true` — idempotent.
    pub session_id: String,
}

/// `POST /session/close` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCloseResponse {
    /// True unless the runner failed to remove the session cache entry.
    /// Not a "session was known" flag — closing an unknown session is
    /// idempotent success.
    pub ok: bool,
}

/// `POST /session/renew-activation` request body.
///
/// Client-side escape hatch when the client detects target-app drift
/// (e.g. `/tree` returned `snapshot_unavailable`, or a foreground steal
/// by SpringBoard). The runner re-issues `.activate()` on the cached
/// binding subject to the same 5 s / bundle-id rate limit.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenewActivationRequest {
    /// Session to renew. Unknown session → 404.
    pub session_id: String,
}

/// `POST /session/renew-activation` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenewActivationResponse {
    /// True on success (session known + not-rate-limited).
    pub ok: bool,
    /// Whether the runner actually issued `.activate()` this call, or
    /// suppressed it because the previous activation was within the
    /// rate-limit window. `ok=true, activated=false` is a valid outcome
    /// meaning "session is fresh enough, no-op".
    #[serde(default)]
    pub activated: bool,
}

/// Extended `GET /health` response body (v1.0.2 additive).
///
/// Prior to v1.0.2 the endpoint returned a bare 200 with no body.
/// Consumers that ignore the body get identical behavior; consumers
/// that parse the JSON gain liveness observability.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Runner alive.
    pub ok: bool,
    /// Runner semver (matches the smix-cli that shipped it).
    #[serde(default)]
    pub runner_version: String,
    /// Runner uptime in milliseconds.
    #[serde(default)]
    pub uptime_ms: u64,
    /// Epoch millis of the runner's most recent processed request
    /// (any route). 0 if the runner has served no requests yet.
    #[serde(default)]
    pub last_request_at_ms: u64,
    /// Currently-open session count.
    #[serde(default)]
    pub sessions_open: u32,
    /// Total `.activate()` calls issued since runner boot.
    #[serde(default)]
    pub activations_total: u64,
    /// v1.0.4 — SimRenderServer pid + alive flag observed by the
    /// runner's sim-health sensor. `alive = false` after
    /// `com.apple.display.captureservice` internal assertion trips.
    #[serde(default)]
    pub sim_render_server: HealthProcessInfo,
    /// v1.0.4 — xcodebuild test-host pid + alive flag + total restart
    /// count (S7 auto-restart on `** TEST INTERRUPTED **`).
    #[serde(default)]
    pub xcodebuild_test_host: HealthTestHostInfo,
}

/// v1.0.4 — pid + alive flag for a watched process. Additive to
/// [`HealthResponse`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthProcessInfo {
    /// True when the runner last observed the process alive.
    #[serde(default)]
    pub alive: bool,
    /// Last-known pid (0 if never observed).
    #[serde(default)]
    pub pid: u32,
}

/// v1.0.4 — xcodebuild test-host health with restart accounting.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthTestHostInfo {
    /// True when the runner-side supervisor last saw xcodebuild alive.
    #[serde(default)]
    pub alive: bool,
    /// Last-known pid (0 if unrecorded).
    #[serde(default)]
    pub pid: u32,
    /// Number of times the supervisor has restarted the test-host
    /// (RFC 1.0.4 §D6).
    #[serde(default)]
    pub restart_count: u32,
}

// ---- v1.0.4 D5 / D6 / D7 / D14 additive wire ----------------------------

/// `POST /session/close-all` — v1.0.4 §D5 support for `smix runner
/// cycle`. Runner-side clears every open session and returns the
/// count that was cleared.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCloseAllResponse {
    /// True on success (idempotent — closing an empty session table
    /// is not an error).
    pub ok: bool,
    /// Number of sessions closed.
    #[serde(default)]
    pub closed: u32,
}

/// `POST /session/relaunch-app` request body (v1.0.4 §D14).
///
/// Instructs the runner to `terminate()` + `launch()` the session's
/// cached `XCUIApplication` binding IN PLACE — same test-host process,
/// same session id, same XCUITest binding. Used to recover from a
/// downstream app crash without cycling the entire runner.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRelaunchAppRequest {
    /// Session whose cached binding will be relaunched.
    pub session_id: String,
}

/// `POST /session/relaunch-app` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRelaunchAppResponse {
    /// True on success (session known + relaunch cycle completed).
    pub ok: bool,
    /// Wall-clock milliseconds the terminate+launch cycle took.
    #[serde(default)]
    pub wall_ms: u64,
}

/// v1.0.4 §D7 — Session state exposed to SDK consumers via the
/// `X-Sim-Health` response header on every runner response.
///
/// Additive to v1.0.3 sessions — consumers that ignore the header get
/// v1.0.3 behavior; consumers that read it get `Degraded` / `Dead` /
/// `Cycling` transitions without polling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SimHealthWireState {
    /// All watched signals inside envelope.
    #[serde(rename = "healthy")]
    Healthy,
    /// At least one signal degraded (screenshot p95 slow, /health
    /// stale, etc.).
    #[serde(rename = "degraded")]
    Degraded,
    /// Runner is mid-cycle (supervisor auto-restart in progress).
    /// Callers should back off until the next `Healthy` transition.
    #[serde(rename = "cycling")]
    Cycling,
    /// Runner or a watched subprocess (SimRenderServer / xcodebuild)
    /// is gone. Callers should bail.
    #[serde(rename = "dead")]
    Dead,
}

impl SimHealthWireState {
    /// Parse a case-insensitive wire string; returns `None` on
    /// unknown values so unrecognized future states don't panic.
    pub fn from_header(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "healthy" => Some(Self::Healthy),
            "degraded" => Some(Self::Degraded),
            "cycling" => Some(Self::Cycling),
            "dead" => Some(Self::Dead),
            _ => None,
        }
    }

    /// Header string emitted on the runner side.
    pub fn as_header(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Cycling => "cycling",
            Self::Dead => "dead",
        }
    }
}
