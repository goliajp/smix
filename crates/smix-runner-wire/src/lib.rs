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
    /// via `/tap-at-norm-coord`. This is the default.
    Resolve,
    /// Runner resolves AND taps (legacy host-HID-based path).
    ResolveAndTap,
    /// Runner resolves selector then synthesizes the touch event via the
    /// XCTRunnerDaemonSession daemonProxy. This bypasses the XCUIElement
    /// gesture recognizer chain so RN Pressable `RCTTouchHandler`
    /// UIGestureRecognizer receives the touch and fires the JS-thread
    /// `onPress` callback reliably.
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

/// `POST /system-popup-action` request body.
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
/// The Swift `EventRecorder` emits the notification discriminator as
/// `rawCode`, not `code`. Field names here mirror that schema exactly
/// (`raw_code` + serde camelCase → `rawCode`); a mismatch deserializes
/// silently to `0` rather than erroring, so the naming is load-bearing.
///
/// `extra` is a flatten catch-all for the enrich fields Swift splats onto
/// the top level of `RecordedEvent` (`kind` / `frame` /
/// `payloadDescription` / `elementType` / …), so new ones are accepted
/// without a wire break.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedEvent {
    /// Numeric event-type discriminator (Swift `RecordedEvent.rawCode`),
    /// carrying the raw kAXNotification int. Typical values: 1018
    /// (kAXFirstResponderChangedNotification) / 1028
    /// (kAXHIDEventReceivedNotification) / 4002 (kAXUserTestingNotification)
    /// / 1006 (kAXAlertNotification) / 1021 (kAXPidStatusChangedNotification).
    #[serde(default)]
    pub raw_code: i32,
    /// Capture-time timestamp in milliseconds.
    #[serde(default)]
    pub timestamp_ms: f64,
    /// Free-form per-event enrich fields that the Swift side splats onto
    /// the top level (`kind` / `frame` / `payloadDescription` /
    /// `elementType` / `appBundleId` / `payloadClassName` / …).
    /// Reconciliation reads only `raw_code` + `timestamp_ms` and never
    /// depends on this; consumers wanting detail can read it on demand.
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

// -------------------- /session/* wire shape ------------------------------
//
// Session lifecycle addresses the "activation storm" root cause: without a
// session the runner re-binds + `.activate()`s an `XCUIApplication` on every
// request whose `App-Activate: true` header is set. Long-running gates (visual /
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

/// Extended `GET /health` response body.
///
/// The body is purely additive over the legacy bare-200 response.
/// Wire schema versions this build speaks.
///
/// The schema is versioned apart from the runner, because they are not the
/// same thing. A runner that has been up since before a CLI upgrade, and an
/// SDK released on its own cadence, both talk to a wire whose shape may not
/// have moved at all. Tying them to the runner's semver made every version
/// difference an error, including the ones that did not matter.
///
/// Add a version here when the shape changes, and keep the old one for as
/// long as it is still spoken.
pub const WIRE_SCHEMA_SUPPORTED: &[u32] = &[1, 2];

/// The newest schema both ends speak, or `None` when they share none.
///
/// Highest-common rather than exact-match: two builds that both know schema
/// 2 agree on schema 2, whatever their versions say.
#[must_use]
pub fn negotiate_wire_schema(ours: &[u32], theirs: &[u32]) -> Option<u32> {
    ours.iter()
        .filter(|v| theirs.contains(v))
        .copied()
        .max()
}

/// What a runner says about the wire it speaks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSchemaInfo {
    /// Every schema the runner speaks.
    #[serde(default)]
    pub supports: Vec<u32>,
    /// The one it settled on with the client that asked, when the client
    /// said what it speaks. Absent from a runner too old to have been asked.
    #[serde(default)]
    pub negotiated: Option<u32>,
}

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
    /// SimRenderServer pid + alive flag observed by the runner's
    /// sim-health sensor. `alive = false` after
    /// `com.apple.display.captureservice` internal assertion trips.
    #[serde(default)]
    pub sim_render_server: HealthProcessInfo,
    /// xcodebuild test-host pid + alive flag + total restart count
    /// (auto-restart on `** TEST INTERRUPTED **`).
    #[serde(default)]
    pub xcodebuild_test_host: HealthTestHostInfo,
    /// The wire this runner speaks. Empty from a runner that predates the
    /// question — which means "unknown", not "none".
    #[serde(default)]
    pub wire_schema: WireSchemaInfo,
}

/// Pid + alive flag for a watched process. Additive to
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

/// xcodebuild test-host health with restart accounting.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthTestHostInfo {
    /// True when the runner-side supervisor last saw xcodebuild alive.
    #[serde(default)]
    pub alive: bool,
    /// Last-known pid (0 if unrecorded).
    #[serde(default)]
    pub pid: u32,
    /// Number of times the supervisor has restarted the test-host.
    #[serde(default)]
    pub restart_count: u32,
}

/// `POST /session/close-all` — backs `smix runner cycle`. Runner-side
/// clears every open session and returns the count that was cleared.
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

/// `POST /session/relaunch-app` request body.
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

/// Request body shared by `POST /session/terminate-app` and
/// `POST /session/launch-app`.
///
/// Carries launch-argument / launch-environment injection so consumers
/// can bypass scaffolding like Expo dev-launcher server pickers that
/// vanish on clearAppData (SDK 57 stops auto-navigating on URL scheme).
// Not `#[non_exhaustive]` — consumers construct these directly. Wire
// evolution is handled by `#[serde(default)]` on each field, so
// adding a field is source-compatible for anyone using
// `..Default::default()` in their struct literal (idiomatic pattern
// in the SDK). The response variant DOES carry `#[non_exhaustive]`
// because consumers only READ it, not construct.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAppLifecycleRequest {
    /// Session whose cached `XCUIApplication` binding will be acted on.
    pub session_id: String,
    /// launchArguments passed to `XCUIApplication.launch()`. Empty vec
    /// leaves whatever the runner already had cached on
    /// `entry.app.launchArguments` (empty by default).
    /// Ignored by `/session/terminate-app`.
    #[serde(default)]
    pub args: Vec<String>,
    /// launchEnvironment passed to `XCUIApplication.launch()`. Empty map
    /// leaves the runner's cached environment untouched. Ignored by
    /// `/session/terminate-app`.
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// After `.launch()` dispatches, poll `XCUIApplication.state` at
    /// 250 ms cadence until `.runningForeground` (or timeout). Prevents
    /// callers from firing a subsequent terminateApp before the process
    /// has signalled launchd ready — the root cause of
    /// `bug_type: 309 exec_terminated_before_ready` `.ips` writes.
    /// `None` = fire-and-return semantics; `Some(0)` = wait one
    /// iteration then return terminal state (useful for tests).
    /// Default: `Some(15000)` at the SDK layer.
    #[serde(default)]
    pub wait_for_foreground_ms: Option<u64>,
    /// After `.runningForeground` is observed (or immediately when
    /// `wait_for_foreground_ms == None`), poll the a11y tree at 500 ms
    /// cadence looking for ≥ `minIdentifierCount` descendants with a
    /// non-empty `accessibilityIdentifier` NOT in the interactive-probe
    /// ignore list. Fires `launchAppReachedInteractive` when found; on
    /// timeout fires `launchAppTimedOutBeforeInteractive`. Both counters
    /// live on [`SessionLifecycleCounters`].
    ///
    /// `None` = fire-and-return (no interactive polling). Consumer config
    /// `.smix/config.yaml interactiveProbe: { minIdentifierCount, ignore: [...] }`
    /// forwards to the runner via `TEST_RUNNER_SMIX_INTERACTIVE_PROBE_JSON`
    /// at boot; defaults are `minIdentifierCount: 3` and
    /// `ignore: ["SplashScreenLogo"]`.
    #[serde(default)]
    pub wait_for_interactive_ms: Option<u64>,
}

/// Response for `POST /session/terminate-app` and
/// `POST /session/launch-app`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SessionAppLifecycleResponse {
    /// True on success.
    pub ok: bool,
    /// Wall-clock milliseconds the terminate or launch took.
    #[serde(default)]
    pub wall_ms: u64,
    /// Wall-clock milliseconds spent inside the wait-for-foreground
    /// loop. Zero when the caller passed `wait_for_foreground_ms: 0`
    /// or when the runner does not implement the wait.
    #[serde(default)]
    pub waited_ms: u64,
    /// Final observed `XCUIApplication.state` at the moment the runner
    /// returned. Wire-encoded as the raw `Int` value XCTest uses:
    /// `0` unknown, `1` notRunning, `2` runningBackgroundSuspended,
    /// `3` runningBackground, `4` runningForeground. Zero when the
    /// caller opted out or the runner does not implement the wait.
    #[serde(default)]
    pub terminal_state: u8,
    /// Set when the runner observed cooperative
    /// termination (`XCUIApplication.terminate()` observed
    /// `.notRunning` before timeout). `false` on `terminate-app` when
    /// the runner had to fall back to a hard signal.
    /// Only meaningful on `terminate-app` responses; `false` on
    /// `launch-app`.
    #[serde(default)]
    pub terminated_cooperatively: bool,
    /// Set on `launch-app` responses when the
    /// interactive-probe (≥ `minIdentifierCount` non-empty ax-ids
    /// outside the ignore list) fired inside `wait_for_interactive_ms`.
    /// `false` when the caller didn't set `wait_for_interactive_ms`
    /// OR when polling timed out without observing the interactive
    /// fingerprint.
    #[serde(default)]
    pub reached_interactive: bool,
    /// Sample of up to 8 `accessibilityIdentifier`
    /// values observed at the moment `reached_interactive` fired.
    /// Non-normative; useful for consumers debugging "did my
    /// interactive probe fire on the right screen". Empty when
    /// `reached_interactive == false` or the caller didn't set
    /// `wait_for_interactive_ms`.
    #[serde(default)]
    pub interactive_named_ids: Vec<String>,
}

/// One entry in `POST /session/list` response.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// Session id.
    pub session_id: String,
    /// Bundle id the session's XCUIApplication is bound to.
    pub bundle_id: String,
    /// Epoch millis of open time.
    #[serde(default)]
    pub opened_at_ms: u64,
    /// Epoch millis of last `.activate()` on this session (renew or
    /// open). Distinct from the runner-internal `lastAccessedAt` used
    /// by the idle-close sweep.
    #[serde(default)]
    pub last_activated_at_ms: u64,
}

/// `POST /session/list` response body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponse {
    /// Every session currently known to the runner.
    #[serde(default)]
    pub sessions: Vec<SessionSummary>,
}

/// One entry in the subprocess ring buffer.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubprocessRecord {
    /// argv passed to the invoked binary.
    pub argv: Vec<String>,
    /// Exit code; `None` when the process failed to spawn.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Wall-clock milliseconds.
    #[serde(default)]
    pub wall_ms: u64,
    /// First 256 bytes of stderr.
    #[serde(default)]
    pub stderr_head: String,
    /// Epoch millis when the invocation completed.
    #[serde(default)]
    pub timestamp_ms: u64,
}

/// App-alive cache observability counters.
/// Always emitted (even when the runner was booted without an
/// `appAliveCache` — `wired: false` sentinel + all-zero counters).
/// Consumers use `wired` to distinguish "workload never fired the
/// re-probe path" from "runner version predates this observability".
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AliveCacheCounters {
    /// `false` sentinel — either the runner omits the field entirely
    /// or it was booted without a cache. When `true`, the field values
    /// reflect an actual live cache.
    #[serde(default)]
    pub wired: bool,
    /// Total calls to `AppAliveCache::markDead`.
    #[serde(default)]
    pub mark_dead_total: u64,
    /// Total calls to `AppAliveCache::markAlive` (explicit + re-probe-success).
    #[serde(default)]
    pub mark_alive_total: u64,
    /// `isSuppressed` returned true — a call was short-circuited.
    #[serde(default)]
    pub suppress_hit_total: u64,
    /// `isSuppressed` returned false — a call was allowed through.
    #[serde(default)]
    pub suppress_miss_total: u64,
    /// Re-probe Task spawned.
    #[serde(default)]
    pub reprobe_attempted_total: u64,
    /// Re-probe observed target return to running.
    #[serde(default)]
    pub reprobe_succeeded_total: u64,
    /// Re-probe was invalidated mid-flight by external markAlive.
    #[serde(default)]
    pub reprobe_invalidated_early: u64,
    /// Re-probe ran all 6 iterations without recovery.
    #[serde(default)]
    pub reprobe_exhausted_window: u64,
}

/// Cumulative session lifecycle counters.
///
/// Unlike the instantaneous `sessions: Vec<SessionSummary>` view,
/// these are advanced on every mutation and survive `close`. The split
/// `terminate_app_via_xcuiapplication` / `terminate_app_via_fallback`
/// counters answer "did clearAppData actually use the cooperative
/// pathway or fall back to SIGKILL".
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SessionLifecycleCounters {
    /// Total `POST /session/open` outcomes.
    #[serde(default)]
    pub opened_total: u64,
    /// Total `POST /session/close` outcomes (idempotent — includes closes
    /// of unknown sessions).
    #[serde(default)]
    pub closed_total: u64,
    /// Total `POST /session/relaunch-app` outcomes.
    #[serde(default)]
    pub relaunch_app_total: u64,
    /// Total `POST /session/terminate-app` outcomes.
    #[serde(default)]
    pub terminate_app_total: u64,
    /// Terminate calls that observed the target `.notRunning` before
    /// the XCUIApplication timeout — cooperative pathway succeeded.
    #[serde(default)]
    pub terminate_app_via_xcuiapplication: u64,
    /// Terminate calls where XCUIApplication timed out and the runner
    /// fell back to a hard-kill pathway. `> 0` is the smoking gun for
    /// a non-cooperative teardown.
    #[serde(default)]
    pub terminate_app_via_fallback: u64,
    /// Total `POST /session/launch-app` outcomes.
    #[serde(default)]
    pub launch_app_total: u64,
    /// Launch calls that observed `.runningForeground` inside the
    /// requested `wait_for_foreground_ms` window.
    #[serde(default)]
    pub launch_app_reached_foreground: u64,
    /// Launch calls that timed out waiting for foreground.
    #[serde(default)]
    pub launch_app_timed_out_before_foreground: u64,
    /// Total `resetAppData` verbs dispatched (URL-scheme JS-wipe
    /// pattern, separate from `clearAppData`'s container-wipe pathway).
    #[serde(default)]
    pub reset_app_data_total: u64,
    /// resetAppData calls where the completion
    /// signal (log-line pattern match on the metro log tail) never
    /// arrived inside the timeout window. `> 0` = the URL fired but
    /// the app didn't emit the expected `[dev] reset-complete` line.
    #[serde(default)]
    pub reset_app_data_timed_out: u64,
    /// Launch calls that observed the
    /// interactive fingerprint (`≥ minIdentifierCount` non-ignored
    /// ax-ids in the a11y tree) inside `wait_for_interactive_ms`.
    /// Distinct from `launch_app_reached_foreground` which only
    /// tracks process state, not tree probeability.
    #[serde(default)]
    pub launch_app_reached_interactive: u64,
    /// Launch calls that timed out waiting for the interactive
    /// fingerprint. Non-zero = "process foreground but tree unusable"
    /// — typically a splash screen whose descendants are all unnamed.
    #[serde(default)]
    pub launch_app_timed_out_before_interactive: u64,
}

/// Per-flow attempt attribution. Populated by
/// `smix run --retry <N>` (default 1) as each flow completes; carries
/// through `/diagnostic/dump` so consumers can attribute a `.ips` to
/// a specific retry rather than a whole batch.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct FlowAttemptRecord {
    /// Flow name (yaml basename or explicit id).
    #[serde(default)]
    pub flow_name: String,
    /// Ordered attempts. Retry #0 is the first try; retry #1 is
    /// second; etc.
    #[serde(default)]
    pub attempts: Vec<FlowAttempt>,
}

/// One attempt within a flow (see [`FlowAttemptRecord`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct FlowAttempt {
    /// Zero-based retry index. `0` = first try.
    #[serde(default)]
    pub attempt_index: u32,
    /// Overall outcome: "ok" / "timeout" / "error" / "crashed".
    #[serde(default)]
    pub status: String,
    /// Free-form error class code when non-ok (e.g., "TIMEOUT",
    /// "DRIVER_ERROR", "APP_UNAVAILABLE"). Empty on success.
    #[serde(default)]
    pub error_class: Option<String>,
    /// If a `.ips` file appeared under `~/Library/Logs/DiagnosticReports/`
    /// during this attempt's window, its filename. Consumers grep for
    /// this to tie a crash-report to a specific retry.
    #[serde(default)]
    pub ips_generated: Option<String>,
    /// Wall-clock milliseconds this attempt ran.
    #[serde(default)]
    pub wall_ms: u64,
}

/// `POST /diagnostic/dump` response body.
///
/// Snapshot of the runner's runtime state: recent subprocess
/// invocations, currently-open sessions, sim health state, supervisor
/// pid, plus the always-emitted app-alive cache + cumulative session
/// lifecycle counters that answer "is the observability instrumented?"
/// as a numeric question, not a grep for a log line that may or may
/// not survive a supervisor cycle.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DiagnosticDumpResponse {
    /// Recent `xcrun simctl` invocations recorded by the runner's
    /// subprocess ring buffer. Ordered oldest → newest, capped at 128.
    #[serde(default)]
    pub recent_subprocesses: Vec<SubprocessRecord>,
    /// Every session currently known to the runner (mirrors
    /// `/session/list`; embedded here for one-shot dump).
    #[serde(default)]
    pub sessions: Vec<SessionSummary>,
    /// Coarse sim-health classification at dump time.
    #[serde(default)]
    pub sim_health: String,
    /// Pid of the supervisor sidecar, when spawned via
    /// `smix runner up --supervise`.
    #[serde(default)]
    pub supervisor_pid: Option<u32>,
    /// Runner wall-clock uptime in milliseconds.
    #[serde(default)]
    pub uptime_ms: u64,
    /// Always-present. `wired=false` when the runner didn't have an
    /// `AppAliveCache` wired at boot (opt-out) or doesn't implement
    /// this observability. When `wired=true`, the counter
    /// fields reflect an actual live cache — even all-zero is
    /// meaningful ("workload didn't fire the re-probe path").
    #[serde(default)]
    pub alive_cache: AliveCacheCounters,
    /// Cumulative session lifecycle counters. Advance on every
    /// mutation, survive close, and are persisted alongside the
    /// session-persistence file so `smix runner cycle` doesn't wipe
    /// them.
    #[serde(default)]
    pub session_counters: SessionLifecycleCounters,
    /// Trailing N lines from the external
    /// metro log the CLI was told to tail (via `--metro-log <path>`
    /// on `smix run` or `smix diagnostic dump`). Empty when the CLI
    /// was not given a log path OR when the file was unreadable at
    /// dump time. Default N is 200 lines; consumer overrides with
    /// `--metro-log-tail-lines <N>`. Purely additive — a consumer
    /// ignoring this field sees no behavior change.
    #[serde(default)]
    pub metro_log_tail: Vec<String>,
    /// Retry-attribution roll-up. Populated by
    /// `smix run` when it has driven at least one flow. Each entry
    /// carries per-attempt status + errorClass + any `.ips` generated
    /// so consumers can tell "flow X passed on retry 2 after retry 1
    /// crashed" from "flow X passed on the first try". Empty when no
    /// flow context available at dump time.
    #[serde(default)]
    pub recent_flows: Vec<FlowAttemptRecord>,
    /// Most-recent `interactiveNamedIds` sample observed
    /// across all `launchApp` completions since runner boot. Survives
    /// session teardown so post-batch triage can see WHICH ax-ids
    /// triggered `reachedInteractive` on the last observed launch,
    /// not just the count. Empty when no launch this run.
    ///
    /// Per-session values remain on `sessions[n].interactiveNamedIds`
    /// but go with the session at teardown. This top-level field is
    /// the last-values-standing for post-mortem observation.
    #[serde(default)]
    pub last_interactive_named_ids: Vec<String>,
}

/// Session state exposed to SDK consumers via the `X-Sim-Health`
/// response header on every runner response.
///
/// Purely additive — consumers that ignore the header are unaffected;
/// consumers that read it get `Degraded` / `Dead` / `Cycling`
/// transitions without polling.
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
