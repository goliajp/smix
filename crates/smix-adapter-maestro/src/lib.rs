//! smix-adapter-maestro — Maestro YAML adapter for smix.
//!
//! Parses Maestro test flow YAML and translates each [`Step`] into a
//! `smix-sdk` action call. See `README.md` for the design rationale.
//!
//! [`parse_flow_yaml`] does single-file parsing; [`parse_flow_file`]
//! adds recursive `runFlow` expansion with cycle detection.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use smix_selector::Selector;
use std::path::PathBuf;

/// v1.0 Phase C2 — one annotation composed onto a screenshot output.
/// Position resolves against pixel/normalized coords or a smix
/// `Selector` (adapter runtime resolves selector → pixel at
/// screenshot time via a11y tree).
///
/// Serialization: externally-tagged with PascalCase (matches
/// `parse_annotation_from_kind`'s wrapper).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AnnotationSpec {
    /// Filled circle overlay.
    Circle {
        /// Center position.
        at: AnnotationPos,
        /// Color spec (named / #hex / rgb() / rgba()).
        #[serde(default = "default_annotation_color")]
        color: String,
        /// Radius in pixels.
        #[serde(default = "default_annotation_radius")]
        radius: i32,
        /// Stroke width in pixels.
        #[serde(default = "default_annotation_stroke")]
        stroke: i32,
    },
    /// Line from → to.
    Line {
        /// Line start position.
        from: AnnotationPos,
        /// Line end position.
        to: AnnotationPos,
        /// Color spec.
        #[serde(default = "default_annotation_color_line")]
        color: String,
        /// Stroke width.
        #[serde(default = "default_annotation_stroke_line")]
        stroke: i32,
    },
    /// Arrow from → to.
    Arrow {
        /// Arrow start position.
        from: AnnotationPos,
        /// Arrow tip position.
        to: AnnotationPos,
        /// Color spec.
        #[serde(default = "default_annotation_color_line")]
        color: String,
        /// Stroke width.
        #[serde(default = "default_annotation_stroke_arrow")]
        stroke: i32,
    },
    /// Text label.
    Text {
        /// Text baseline start position.
        at: AnnotationPos,
        /// Text content.
        content: String,
        /// Color spec.
        #[serde(default = "default_annotation_color_text")]
        color: String,
        /// Font size in pixels.
        #[serde(default = "default_annotation_font_size")]
        size: f32,
    },
    /// Rectangle outline.
    Box {
        /// Top-left position.
        at: AnnotationPos,
        /// Width in pixels.
        width: i32,
        /// Height in pixels.
        height: i32,
        /// Color spec.
        #[serde(default = "default_annotation_color_line")]
        color: String,
        /// Stroke width.
        #[serde(default = "default_annotation_stroke_line")]
        stroke: i32,
    },
}

/// v1.0 Phase C2 — position spec for an annotation. Three shapes:
/// - `{ x: 100, y: 100 }` — absolute pixel
/// - `{ nx: 0.5, ny: 0.5 }` — normalized 0..1 (viewport-relative)
/// - `{ id: "submit-btn" }` — smix Selector — resolved against a11y
///   tree at capture time; center of the matched element used
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnnotationPos {
    /// Absolute pixel position.
    Pixel {
        /// X coordinate in pixels.
        x: i32,
        /// Y coordinate in pixels.
        y: i32,
    },
    /// Normalized 0..1 position (viewport-relative).
    Normalized {
        /// Normalized X (0.0 = left, 1.0 = right).
        nx: f32,
        /// Normalized Y (0.0 = top, 1.0 = bottom).
        ny: f32,
    },
    /// Selector — resolved to element center at render time via a11y tree.
    Selector(Selector),
}

fn default_annotation_color() -> String {
    "red".into()
}
fn default_annotation_color_line() -> String {
    "cyan".into()
}
fn default_annotation_color_text() -> String {
    "white".into()
}
fn default_annotation_radius() -> i32 {
    30
}
fn default_annotation_stroke() -> i32 {
    3
}
fn default_annotation_stroke_line() -> i32 {
    2
}
fn default_annotation_stroke_arrow() -> i32 {
    4
}
fn default_annotation_font_size() -> f32 {
    20.0
}

/// v1.0 Phase C2 — parse an [`AnnotationSpec`] from a yaml annotation
/// entry like `circle: { at: ..., color: ..., radius: ... }`. `kind`
/// is the outer key (`circle`/`arrow`/`text`/`box`/`line`); `body` is
/// the mapping value. Returns human-readable error string on shape
/// mismatch for the parser to wrap into `ParseError::InvalidValue`.
pub(crate) fn parse_annotation_from_kind(
    kind: &str,
    body: &serde_norway::Value,
) -> Result<AnnotationSpec, String> {
    let m = body
        .as_mapping()
        .ok_or_else(|| format!("expected mapping body for `{kind}`, got {body:?}"))?;
    let get_str = |field: &str| -> Option<String> {
        m.get(serde_norway::Value::String(field.into()))?
            .as_str()
            .map(String::from)
    };
    let get_i32 = |field: &str, default: i32| -> Result<i32, String> {
        match m.get(serde_norway::Value::String(field.into())) {
            None => Ok(default),
            Some(v) => v
                .as_i64()
                .map(|n| n as i32)
                .ok_or_else(|| format!("`{field}` must be integer, got {v:?}")),
        }
    };
    let get_f32 = |field: &str, default: f32| -> Result<f32, String> {
        match m.get(serde_norway::Value::String(field.into())) {
            None => Ok(default),
            Some(v) => v
                .as_f64()
                .or_else(|| v.as_i64().map(|n| n as f64))
                .map(|n| n as f32)
                .ok_or_else(|| format!("`{field}` must be number, got {v:?}")),
        }
    };
    let get_pos = |field: &str| -> Result<AnnotationPos, String> {
        let v = m
            .get(serde_norway::Value::String(field.into()))
            .ok_or_else(|| format!("missing `{field}`"))?;
        let vm = v
            .as_mapping()
            .ok_or_else(|| format!("`{field}` must be mapping, got {v:?}"))?;
        if let (Some(x), Some(y)) = (
            vm.get(serde_norway::Value::String("x".into()))
                .and_then(|v| v.as_i64()),
            vm.get(serde_norway::Value::String("y".into()))
                .and_then(|v| v.as_i64()),
        ) {
            return Ok(AnnotationPos::Pixel {
                x: x as i32,
                y: y as i32,
            });
        }
        if let (Some(nx), Some(ny)) = (
            vm.get(serde_norway::Value::String("nx".into()))
                .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64))),
            vm.get(serde_norway::Value::String("ny".into()))
                .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64))),
        ) {
            return Ok(AnnotationPos::Normalized {
                nx: nx as f32,
                ny: ny as f32,
            });
        }
        Err(format!(
            "`{field}` must have {{x, y}} (pixel) or {{nx, ny}} (normalized) — got {v:?}"
        ))
    };

    match kind {
        "circle" => Ok(AnnotationSpec::Circle {
            at: get_pos("at")?,
            color: get_str("color").unwrap_or_else(default_annotation_color),
            radius: get_i32("radius", default_annotation_radius())?,
            stroke: get_i32("stroke", default_annotation_stroke())?,
        }),
        "arrow" => Ok(AnnotationSpec::Arrow {
            from: get_pos("from")?,
            to: get_pos("to")?,
            color: get_str("color").unwrap_or_else(default_annotation_color_line),
            stroke: get_i32("stroke", default_annotation_stroke_arrow())?,
        }),
        "text" => Ok(AnnotationSpec::Text {
            at: get_pos("at")?,
            content: get_str("content").ok_or_else(|| "missing `content`".to_string())?,
            color: get_str("color").unwrap_or_else(default_annotation_color_text),
            size: get_f32("size", default_annotation_font_size())?,
        }),
        "box" => Ok(AnnotationSpec::Box {
            at: get_pos("at")?,
            width: get_i32("width", 100)?,
            height: get_i32("height", 100)?,
            color: get_str("color").unwrap_or_else(default_annotation_color_line),
            stroke: get_i32("stroke", default_annotation_stroke_line())?,
        }),
        "line" => Ok(AnnotationSpec::Line {
            from: get_pos("from")?,
            to: get_pos("to")?,
            color: get_str("color").unwrap_or_else(default_annotation_color_line),
            stroke: get_i32("stroke", default_annotation_stroke_line())?,
        }),
        other => Err(format!("unknown annotation kind `{other}`")),
    }
}

mod annotate_bridge;
mod apps_config;
mod entry;
mod expr;
mod output;
mod parser;
mod runtime;

pub use entry::{FlowArgs, FlowPlatform, OutputFormat, run_flow};

pub use apps_config::{
    AndroidApp, AppEntry, AppsConfig, IosApp, ResolveError, ResolvedApp, resolve_app_into_flow,
};

pub(crate) use expr::{Context as ExprContext, parse_and_eval as expr_eval};

/// v5.2 c4 — yaml expression engine 的 `Value` 类型. 用于 `Adapter::with_output`
/// seeding 调用方构造 output store. 表面 stable subset (Null / Bool /
/// Number / String).
pub use expr::Value as ExprValue;

pub use parser::{
    parse_flow_file, parse_flow_yaml, set_auto_ocr_fallback_override, text_to_pattern,
    visible_to_selector,
};
pub use runtime::{Adapter, AppLike, RunError, RunReport, RunStepReport, StepDebugRecord};

/// A maestro YAML command. Each variant corresponds to one or more
/// `smix-sdk` action calls (the mapping is the c3 job — for now the
/// enum is a structural representation of the parsed yaml node).
///
/// References to the yaml schema:
/// - `tapOn`: `{ "tapOn": "X" }` short or `{ "tapOn": { text, id, index, ... } }` full
/// - `waitForAnimationToEnd`: `{ "waitForAnimationToEnd": null }`
/// - `extendedWaitUntil`: `{ "extendedWaitUntil": { visible: { text, id }, timeout } }`
/// - `assertVisible`: `{ "assertVisible": "X" | { text|id } }`
/// - `inputText`: `{ "inputText": "..." }`
/// - `pressKey`: `{ "pressKey": "back" | "enter" | ... }`
/// - `runFlow`: `{ "runFlow": "path/to/subflow.yaml" }`
///   or `{ "runFlow": { when: { visible }, file } }`
/// - `scrollUntilVisible`: `{ "scrollUntilVisible": { element: {...}, direction, ... } }`
/// - `eraseText`: `{ "eraseText": <n> }`
/// - `swipe`: `{ "swipe": { from, to, ... } }`
/// - `launchApp`: `{ "launchApp": { clearState, clearKeychain, appId } }`
/// - `openLink`: `{ "openLink": "<url>" }`
/// - `stopApp`: `{ "stopApp": null }`
/// v1.0.26 — explicit tap-dispatch override for `tapOn: { dispatch: … }`.
///
/// The default tap path (host-resolve → IOHID native-event synthesize)
/// fires SwiftUI `onTap` and RN Pressable `onPress` reliably. Two
/// runtime-specific cases need a different mechanism, and WHICH taps
/// need it is knowledge only the test author has (per the smix
/// three-layer model the capability lives in core; the decision is the
/// caller's):
///
/// - `xcui` — `XCUIElement.tap()` anchored dispatch. SwiftUI
///   `.sheet` / `.alert` / `.confirmationDialog` / `.fullScreenCover`
///   dismiss BINDINGS don't fire from coord-based taps on iOS 17+;
///   the element-anchored path is the only one that flips the binding.
///   Requires an `id:` selector (the runner resolves by identifier).
/// - `daemonProxy` — XCTRunnerDaemonSession synthesize (v4.0 G8 fix).
///   Bypasses the XCUIElement gesture-recognizer chain so RN
///   `RCTTouchHandler` receives the raw touch; use when a Pressable
///   swallows the default path on an older RN.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TapDispatch {
    /// `XCUIElement.tap()` anchored dispatch (requires `id:` selector).
    Xcui,
    /// XCTRunnerDaemonSession touch synthesize.
    DaemonProxy,
}

/// One parsed yaml step. See the maestro-compat command table in the
/// module docs above for the yaml shapes each variant corresponds to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    /// Tap an element by selector. Maps to `App::tap`.
    TapOn {
        /// Resolved selector — parser converts the yaml short / full form
        /// to a [`smix_selector::Selector`].
        selector: Selector,
        /// `optional: true` swallows the not-found error per maestro semantics.
        #[serde(default)]
        optional: bool,
        /// v1.0.26 — explicit dispatch-mechanism override
        /// (`dispatch: xcui | daemonProxy`). `None` = default routing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dispatch: Option<TapDispatch>,
    },
    /// Tap at a normalized coordinate point (escape hatch for yaml
    /// `tapOn: { point: "X%,Y%" }`). Mirrors the v3.16 §9 #3 lift
    /// `App::tap_at_coord(nx, ny)` decision: `nx`/`ny` ∈ \[0.0, 1.0\].
    TapAtPoint {
        /// Normalized X (0.0 = left, 1.0 = right).
        nx: f64,
        /// Normalized Y (0.0 = top, 1.0 = bottom).
        ny: f64,
    },
    /// v5.21 c1b — Eval JS against fixture-side WKWebView bridge
    /// (a11y-i18n master plan §1 Option A). yaml shape:
    ///   `- webview_eval: "<js expression>"`  (short form)
    ///   `- webview_eval: { js: "...", assert_eq: <expected JSON value> }` (full form)
    /// Adapter dispatches to `App::webview_eval(js)`; on `assert_eq`
    /// presence, compares result and raises NotMatching on mismatch.
    WebViewEval {
        /// JS expression to evaluate (`document.querySelector(...)...` etc).
        js: String,
        /// Optional expected JSON value. If set + matches, step OK; if
        /// set + mismatches, step fails with assertion error. None = just
        /// eval and discard.
        assert_eq: Option<serde_json::Value>,
    },
    /// Wait for animations to settle. **NOT** an XCTest idle-wait —
    /// `SmixQuiescenceSwizzle.m` no-ops XCTest's idle wait for
    /// performance (RN long-running animations would otherwise stall
    /// every operation). This verb is a **fixed sleep** in the smix
    /// implementation, 400 ms by default.
    ///
    /// v1.0.18 — accepts a numeric override:
    /// - `- waitForAnimationToEnd` — 400 ms sleep (default, unchanged)
    /// - `- waitForAnimationToEnd: 500` — 500 ms sleep (integer = ms)
    ///
    /// Use the numeric form when your animation is longer than 400 ms.
    /// Insight round-4 confusion: consumers assumed the swizzle no-op'd
    /// this verb; the truth is it never went through XCTest idle-wait
    /// in the first place. The default value is preserved for maestro
    /// yaml compat.
    WaitForAnimationToEnd {
        /// Sleep duration in milliseconds. Bare yaml form
        /// (`- waitForAnimationToEnd`) parses to 400 ms (maestro-compat
        /// default); numeric form (`- waitForAnimationToEnd: 500`)
        /// overrides.
        duration_ms: u64,
    },
    /// Extended wait until a selector matches the expected visibility.
    /// `expect_visible=true` waits for visible (maestro yaml `visible:` arm);
    /// `expect_visible=false` waits for not visible (maestro yaml
    /// `notVisible:` arm). v5.2 c2 — 双 arm 编译期 dispatch, parse 时确定.
    ExtendedWaitUntil {
        /// Selector that must match the expected visibility state.
        selector: Selector,
        /// Timeout in milliseconds.
        timeout_ms: u64,
        /// `true` ⇒ wait_for (visible); `false` ⇒ wait_for_not_visible.
        #[serde(default = "default_expect_visible")]
        expect_visible: bool,
    },
    /// Assert a selector is currently visible on screen. Maps to a
    /// non-waiting `App::find` + raise on miss.
    AssertVisible {
        /// Selector to verify.
        selector: Selector,
    },
    /// Type literal text into the focused field. Maps to `App::fill` (chunked).
    InputText(String),
    /// Press a hardware / IME key. Maps to `App::press_key`.
    PressKey(String),
    /// Recursively run a referenced yaml. The parser keeps the raw
    /// (potentially relative) path string here; [`parse_flow_file`]
    /// resolves it against the invoking yaml's directory and expands
    /// unconditional invocations inline. Conditional invocations stay
    /// as [`Step::RunFlowConditional`] (runtime evaluation belongs to
    /// the adapter `Adapter::run`, not the parser).
    RunFlow(String),
    /// Conditional `runFlow: { when: { visible }, file, as }`. Parser stores
    /// the path verbatim, the gating selector, and the optional outputs
    /// alias name; runtime decision is c3 (`Adapter::run` checks visibility
    /// before invoking the inner flow), v5.6 c5 adds outputs alias capture.
    RunFlowConditional {
        /// Raw path string from the yaml (relative to invoking file).
        file: String,
        /// Visibility precondition (`when.visible`). `None` would be a
        /// degenerate yaml — current parser still accepts it for
        /// forward-compat, mapping `None` to "run unconditionally".
        #[serde(skip_serializing_if = "Option::is_none")]
        when_visible: Option<Selector>,
        /// v1.0.24 D2 — inverse gate (`when.notVisible`). Runs the
        /// subflow only when the selector is NOT visible. Enables the
        /// idempotency pattern "only enter conditional if the target
        /// state hasn't already been reached" (e.g. `notVisible:
        /// 'qa-bubble'` = enter the gate ceremony only if not already
        /// past it). Exactly one of `when_visible` / `when_not_visible`
        /// should be Some in idiomatic yaml; both Some is a parse
        /// error, both None is unconditional (same as legacy).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when_not_visible: Option<Selector>,
        /// v5.6 c5 — outputs alias: after the subflow runs, read the device
        /// pasteboard (canonical "what the subflow captured via
        /// copyTextFrom") and write it into the parent flow's output map
        /// under this name. None ⇒ no capture (legacy semantics). Mirrors
        /// maestro yaml `runFlow: { file, as: <name> }` capture form.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_name: Option<String>,
    },
    /// v6.8 c1 — maestro `runFlow: { when: { visible }, commands: [...] }`
    /// inline form. The body is a literal list of steps held in-place; no
    /// child yaml file is referenced. `when.visible` gates execution
    /// identically to [`Step::RunFlowConditional`]. Mirrors maestro's
    /// `YamlRunFlow` `commands:` alternative to `file:` (used widely for
    /// short conditional helpers that don't justify a separate file —
    /// e.g. `dismiss-open-in` style SpringBoard popup dismissers).
    /// `file` and `commands` are mutually exclusive at parse time.
    RunFlowInline {
        /// Visibility precondition (`when.visible`). `None` ⇒ unconditional.
        #[serde(skip_serializing_if = "Option::is_none")]
        when_visible: Option<Selector>,
        /// v1.0.24 D2 — inverse gate (`when.notVisible`). See
        /// [`Step::RunFlowConditional::when_not_visible`] for semantics.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when_not_visible: Option<Selector>,
        /// Inline step list. Runtime executes top-to-bottom under the
        /// same warning channel as the parent flow.
        steps: Vec<Step>,
    },
    /// Scroll the screen until a selector becomes visible.
    /// Maps to a `swipe` loop + `find` polling.
    ScrollUntilVisible {
        /// Selector to find.
        selector: Selector,
        /// Direction (`up` / `down` / `left` / `right`).
        direction: String,
    },
    /// Erase N characters from the focused field. Maps to `App::press_key` × N delete.
    EraseText(u32),
    /// Swipe between two points. Maps to `App::swipe`.
    Swipe {
        /// Start point (normalized coords).
        from: (f64, f64),
        /// End point (normalized coords).
        to: (f64, f64),
    },
    /// Launch / relaunch the app under test. v5.2 c2 — maestro yaml 全集
    /// 子参支持: appId / clearState / clearKeychain / permissions /
    /// arguments / stopApp.
    /// `stopApp=true` (default) → terminate + (optional wipe) + launch_with_args;
    /// `stopApp=false` → foreground (resume already-running app).
    LaunchApp {
        /// Bundle id to launch.
        app_id: String,
        /// Whether to wipe MMKV / NSUserDefaults before launching.
        #[serde(default)]
        clear_state: bool,
        /// Whether to wipe the Keychain before launching.
        #[serde(default)]
        clear_keychain: bool,
        /// v5.2 c2 — maestro yaml `permissions: { name: allow|deny|unset }`.
        /// Empty = no permission changes. Applied in mapping iteration order
        /// before launch.
        #[serde(default)]
        permissions: Vec<(String, MaestroPermissionAction)>,
        /// v5.2 c2 — maestro yaml `arguments: [...]` — process-level argv.
        #[serde(default)]
        arguments: Vec<String>,
        /// v5.2 c2 — maestro yaml `stopApp: bool` (default true).
        #[serde(default = "default_stop_app")]
        stop_app: bool,
        /// v1.0.16 — maestro yaml `waitForInteractiveMs: <ms>`. When
        /// set (and stopApp is true — so we go through the cooperative
        /// launch pathway), threads through to
        /// `SessionAppLifecycleRequest.wait_for_interactive_ms` on the
        /// runner. Runner polls the a11y tree at 500 ms cadence with a
        /// snapshot-refresh each iteration (v1.0.16 D1 fix for the
        /// Fabric mount-item-drain race insight diagnosed in
        /// round-2). None = pre-v1.0.16 behaviour (dispatch and return
        /// on foreground reached).
        #[serde(default)]
        wait_for_interactive_ms: Option<u64>,
    },
    /// v1.0.8 §D2 — clear the current session's app data IN PLACE
    /// without launching. Maps to
    /// [`smix_sdk::App::clear_app_data`] (v1.0.11 §D2: the
    /// `_with_launch_options` variant when the yaml supplies args or
    /// env). Pairs with an optional trailing `launchApp: {}` when the
    /// caller wants a fresh instance.
    ///
    /// Replaces `launchApp: { clearState: true }` — the old shape now
    /// emits a deprecation WARN and internally expands to
    /// `ClearAppData + LaunchApp`. Consumers who migrate see the
    /// deprecation notice go away and get the crash-dialog fix
    /// automatically.
    ///
    /// v1.0.11 §D2 — accepts optional `launchArgs` / `launchEnv` to
    /// steer scaffolding shown at cold launch (e.g., Expo dev-launcher
    /// server picker on SDK 57 which stopped auto-navigating on URL
    /// scheme). Bare `- clearAppData` remains valid and equivalent to
    /// empty vec + empty map.
    ClearAppData {
        /// v1.0.11 §D2 — launchArguments forwarded to the cooperative
        /// runner-side launch. Empty vec = pre-v1.0.11 behavior.
        launch_args: Vec<String>,
        /// v1.0.11 §D2 — launchEnvironment forwarded to the cooperative
        /// runner-side launch. Empty map = pre-v1.0.11 behavior.
        launch_env: std::collections::BTreeMap<String, String>,
    },
    /// v1.0.14 Cluster A — URL-scheme-driven app-owned reset.
    /// Distinct from [`Step::ClearAppData`] (which wipes the whole
    /// container including dev-fixture state). resetAppData fires the
    /// supplied URL via `simctl openurl`, then optionally tails the
    /// external metro log for a completion pattern before returning.
    ///
    /// Consumer app is responsible for handling the URL and emitting
    /// the completion signal (e.g., `console.log('[dev]
    /// reset-complete token=...')`). This keeps smix agnostic to
    /// what "reset" means for the app; the app decides.
    ///
    /// yaml shapes both accepted:
    /// ```yaml
    /// - resetAppData: 'insight://dev-mutate?action=reset'
    ///
    /// - resetAppData:
    ///     via: url-scheme
    ///     url: 'insight://dev-mutate?action=reset'
    ///     waitFor:
    ///       logLinePattern: '\[insight-dev\] reset-complete token='
    ///       timeoutMs: 5000
    /// ```
    ResetAppData {
        /// URL scheme string (`simctl openurl <UDID> <url>`).
        url: String,
        /// Completion-signal wait strategy. `None` = fire URL and
        /// return immediately (no wait). See [`ResetAppDataWaitFor`]
        /// for supported variants.
        wait_for: Option<ResetAppDataWaitFor>,
        /// Timeout in ms for the wait_for path. Ignored when
        /// `wait_for` is None. Default 5000 at the parser layer.
        timeout_ms: u64,
    },
    /// Open a URL / deep link in the OS handler. Maps to
    /// `simctl openurl` (c3 mapping).
    OpenLink(String),
    /// Terminate the foreground app. Maps to `simctl terminate`.
    StopApp,
    /// v5.2 c1 — viewport scroll one swipe (default direction = down).
    /// maestro `scroll:` (bare, no args). Maps to `App::scroll_screen`.
    Scroll,
    /// v5.2 c1 — dismiss the on-screen keyboard. maestro `hideKeyboard`.
    /// Maps to `App::hide_keyboard`.
    HideKeyboard,
    /// v5.2 c1 — assert selector is NOT visible. maestro `assertNotVisible`.
    /// Maps to `App::assert_not_visible`.
    AssertNotVisible {
        /// Selector that must NOT be visible.
        selector: Selector,
    },
    /// v5.2 c1 — terminate an explicitly-specified app by bundle id.
    /// maestro `killApp: "com.x"` (independent from launchApp). Maps to
    /// `App::terminate`.
    KillApp {
        /// Bundle id to terminate.
        app_id: String,
    },
    /// v5.2 c1 — wipe an app's MMKV / NSUserDefaults / file storage
    /// independent of launchApp. maestro `clearState: { appId }`.
    /// Maps to `App::launch_fresh(_, clear_state: true, clear_keychain: false, _)`.
    ClearState {
        /// Bundle id to wipe.
        app_id: String,
    },
    /// v5.2 c1 — wipe the Keychain entries for the last launched app
    /// (or top-of-flow appId). maestro `clearKeychain` (bare, no args).
    /// Maps to `App::launch_fresh(_, false, true, _)` with last_bundle.
    ClearKeychain,
    /// v5.2 c1 / v1.0 Phase C2 — capture a screenshot with optional
    /// annotations. maestro `takeScreenshot: "name"` (string) or bare
    /// `- takeScreenshot` (None). smix-native long form:
    /// ```yaml
    /// - takeScreenshot:
    ///     name: hub-form
    ///     annotate:
    ///       - circle: { at: { id: submit }, color: red, radius: 40 }
    ///       - text: { at: { x: 20, y: 20 }, content: "step 1", color: green, size: 20 }
    ///       - arrow: { from: { x: 100, y: 100 }, to: { x: 200, y: 200 }, color: blue }
    /// ```
    TakeScreenshot {
        /// Optional output file name relative to cwd. None = discard bytes.
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        /// v1.0 Phase C2 — annotations composed onto the PNG before
        /// write. Empty = plain screenshot (byte-compat with v0.3.x
        /// bare shorthand). Selector-relative positions resolve
        /// against the a11y tree fetched at capture time.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<AnnotationSpec>,
    },
    /// v5.2 c3 — write literal to device pasteboard. maestro yaml
    /// `setClipboard: "literal"`. Expressions (`${x}`) c4 才支持; c3
    /// 内 expression 显式 `ParseError::InvalidValue` 报"deferred to c4".
    SetClipboard(String),
    /// v5.2 c3 — paste into focused field. `text: Some(literal)` =
    /// `pasteText: "literal"` (writes clipboard then fills);
    /// `text: None` = bare `- pasteText` (reads current clipboard then fills).
    PasteText {
        /// `None` = read clipboard, `Some(s)` = set clipboard + fill `s`.
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// v5.2 c3 — copy element text content into device pasteboard.
    /// maestro yaml `copyTextFrom: <selector>`. Field priority value→text→label.
    CopyTextFrom {
        /// Selector picking the text-bearing element.
        selector: Selector,
    },
    /// v5.2 c3 — double-tap on element. maestro `doubleTapOn: <selector>`.
    /// XCUIElement.doubleTap() public API path.
    DoubleTapOn {
        /// Selector picking the tap target.
        selector: Selector,
    },
    /// v5.2 c3 — long-press on element with optional duration (ms,
    /// default 500). maestro `longPressOn: <selector>` (scalar) or
    /// `longPressOn: { ..., duration: N }`. XCUIElement.press(forDuration:).
    LongPressOn {
        /// Selector picking the press target.
        selector: Selector,
        /// Press duration in milliseconds (default 500 per maestro doc).
        duration_ms: u64,
    },
    /// v5.2 c4 — assert a yaml expression evaluates truthy. maestro
    /// `assertTrue: ${expression}`. Expression source is held raw (含
    /// `${...}` 包裹或裸字面); runtime expand_template + expr engine
    /// 一并处理. 不支持 pattern 显式 DriverError 报错 (§13).
    AssertTrue {
        /// Raw expression source (字面值原文).
        expr: String,
    },
    /// v5.2 c4 — looped subflow. maestro yaml `repeat: { ... }`.
    /// `RepeatMode::While { condition_expr }` evaluates the expression
    /// truthy before each iteration; `RepeatMode::Times(N)` runs fixed
    /// N iterations. v5.5 c5 lifted `repeat.while: { visible: <selector> }`
    /// into [`RepeatMode::WhileVisible`] (was parser-rejected pre-v5.5).
    Repeat {
        /// Loop mode (parser 编译期 dispatch, runtime 无运行时分叉).
        mode: RepeatMode,
        /// Body to run per iteration (recursively parsed).
        commands: Vec<Step>,
    },
    /// v5.2 c4 — retry body on failure up to `max_retries` extra attempts.
    /// Initial + max_retries = max attempts total. Last attempt's
    /// `RunError` propagates if all attempts fail.
    Retry {
        /// Extra retry attempts on top of initial run.
        max_retries: u32,
        /// Body to attempt (recursively parsed).
        commands: Vec<Step>,
    },
    /// v5.2 c4 — maestro `runScript: <source>` (inline literal or file
    /// path verbatim). Parser accepts so yaml files remain portable.
    /// Runtime raises explicit `DriverError` — complete JS runtime
    /// deferred to v6+ (cold plan §不包含, [[priority-quality-perf-over-cost]]
    /// + §13 不静默 noop).
    RunScript {
        /// Raw script source (inline literal or file path verbatim).
        source: String,
    },
    /// v5.2 c4 — maestro `evalScript: <expr>`. Same graceful unsupported
    /// semantics as [`Step::RunScript`].
    EvalScript {
        /// Raw expression source.
        source: String,
    },
    /// v5.2 c5 — maestro `setLocation: { latitude, longitude }`.
    /// Walks `App::set_location` → simctl location set.
    SetLocation {
        /// Latitude in decimal degrees.
        latitude: f64,
        /// Longitude in decimal degrees.
        longitude: f64,
    },
    /// v5.2 c5 — maestro `travel: { points: [...], speed_mps?: <m/s> }`.
    /// Fire-and-return — downstream Step does not block on playback.
    Travel {
        /// Ordered waypoints (lat, lng). ≥2 required (enforced by parser).
        points: Vec<(f64, f64)>,
        /// Optional travel speed in m/s (forwarded to `simctl location
        /// start --speed=...`).
        speed_mps: Option<f64>,
    },
    /// v5.2 c5 — maestro `setPermissions: { camera: allow, ... }`
    /// (top-level command, distinct from `launchApp.permissions` 子参 in
    /// c2). `app_id` is None at parse time — runtime resolves from
    /// `last_bundle`; empty `last_bundle` → explicit DriverError.
    SetPermissions {
        /// Bundle id; `None` ⇒ runtime resolves from `last_bundle`.
        #[serde(skip_serializing_if = "Option::is_none")]
        app_id: Option<String>,
        /// Permission directives in mapping iteration order.
        permissions: Vec<(String, MaestroPermissionAction)>,
    },
    /// v5.2 c5 — maestro `addMedia: <path>` (scalar) or `addMedia: [paths]`
    /// (array). Adapter flattens both into `Vec<String>`.
    AddMedia {
        /// Absolute or relative paths to media files.
        paths: Vec<String>,
    },
    /// v5.2 c5 — maestro `setOrientation: <variant>`. Walks
    /// `App::set_orientation` → driver `/set-orientation` route →
    /// swift `XCUIDevice.shared.orientation`.
    SetOrientation {
        /// Orientation literal aligned with maestro yaml.
        orientation: smix_sdk::MaestroOrientation,
    },
    /// v5.2 c5 — maestro `startRecording: <output-path>`. Spawns
    /// `xcrun simctl io recordVideo` as a long-running child via SDK::App.
    /// Pair with `stopRecording` for clean SIGINT-and-wait shutdown.
    StartRecording {
        /// Output mp4 path (relative or absolute).
        path: String,
    },
    /// v5.2 c5 — maestro `stopRecording` (bare). SIGINT-and-wait the
    /// child started by previous `startRecording`. No prior start →
    /// DriverError.
    StopRecording,
    /// v5.2 c6 — visual regression assertion against a baseline PNG. maestro
    /// `assertScreenshot: "path/to/baseline.png"` (scalar) OR v5.5 c5 mapping
    /// form `{ path, threshold?, mask? }`. Baseline path is resolved relative
    /// to the invoking yaml's base_dir; first run auto-records (writes the
    /// captured screenshot to baseline_path + warns the adapter RunReport),
    /// subsequent runs compute a 64-bit dhash on both and assert hamming
    /// distance ≤ `max_hamming` (None = 5 default). Set env
    /// `SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD=1` to force strict mode.
    ///
    /// v5.5 c5 — mapping form is **surface-only**: `threshold` overrides
    /// the dhash hamming cap; `mask` is parsed into [`MaskRegion`] but
    /// runtime emits a warning and ignores the regions (region-exclusion
    /// is an R2-tier algorithm change deferred to v6+ when SSIM/pHash
    /// stack lands — cold plan §scope).
    AssertScreenshot {
        /// Baseline PNG path relative to the flow's base_dir.
        path: String,
        /// v5.5 c5 — `threshold` field from mapping form, dhash hamming
        /// max distance override. None ⇒ scalar-form default (5).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_hamming: Option<u32>,
        /// v5.5 c5 — `mask` regions (normalized 0..1 bbox) accepted from
        /// mapping form. Runtime emits warn-and-ignore (surface-only).
        /// Empty Vec for scalar form.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mask: Vec<MaskRegion>,
    },
    /// Await a single log signal matching `regex` (optionally
    /// constrained by `level`) within `timeout_ms`, over the specified
    /// `window`.
    ///
    /// yaml surface:
    /// ```yaml
    /// - expect:
    ///     signal:
    ///       regex: "env=qa-mode"
    ///       timeoutMs: 8000
    ///       window:
    ///         sinceStep: 2              # optional; default = SinceRun
    /// ```
    ExpectSignal {
        /// Regex to match against the log line message.
        regex: String,
        /// Optional log-level constraint (debug/info/warn/error/log).
        /// None = any level.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<String>,
        /// Timeout in milliseconds.
        timeout_ms: u64,
        /// Window shape. `SinceRun` (default), `SinceStep(n)`, or
        /// `LastMs(ms)`.
        #[serde(default)]
        window: SignalWindow,
    },
    /// v0.3.0 Phase A — ordered / unordered multi-signal await. See
    /// [`Step::ExpectSignal`] for surface details.
    ///
    /// ```yaml
    /// - expect:
    ///     signals:
    ///       - regex: "launchOverrideConsumed"
    ///       - regex: "autoLoginValidated"
    ///     order: strict            # or "any" (default)
    ///     timeoutMs: 30000
    /// ```
    ExpectSignals {
        /// Ordered list of signal matchers.
        signals: Vec<SignalMatch>,
        /// Ordering semantics: `strict` (in-list order) or `any`.
        #[serde(default)]
        order: SignalOrderKind,
        /// Timeout in milliseconds for all matchers.
        timeout_ms: u64,
        /// Window over which to look.
        #[serde(default)]
        window: SignalWindow,
    },
    /// v0.3.0 Phase A — post-run log-hygiene assertion. Verifies that
    /// no log entries outside the configured allowlist have been
    /// emitted at level >= warn during the flow. Populated by the CLI
    /// flag `--expect-log-clean` or the yaml verb `expectLogClean: true`.
    ExpectLogClean,
    /// Look up `id` in the fixture registry to find
    /// `{testID, signal, timeoutMs}`, open the QA overlay, tap the
    /// chip, and await the declared signal.
    ///
    /// yaml surface (short + long forms):
    /// ```yaml
    /// - fixture: prime-search-history
    /// - fixture:
    ///     id: prime-search-history
    ///     timeoutMs: 12000        # optional override
    /// ```
    Fixture {
        /// Fixture id (looked up in the registry).
        id: String,
        /// Optional yaml-side timeout override (in ms). None → uses
        /// the registry's declared `timeoutMs`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
}

/// v0.3.0 Phase A — sub-field of [`Step::ExpectSignals`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalMatch {
    /// Regex to match against the log line message.
    pub regex: String,
    /// Optional log-level constraint (debug/info/warn/error/log). None = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// v0.3.0 Phase A — window shape for signal await. Mirrors
/// `smix_metro_log::Window` at the yaml level. Runtime translates
/// `SinceStep` to `Window::SinceMs` using the tail's ms cursor
/// captured at each step-end.
// v1.0.14 Cluster A — re-export smix_sdk's ResetAppDataWaitFor so
// the yaml adapter Step type + parser can name it without an extra
// import path indirection. The canonical definition lives in
// smix-sdk since the App impl needs to consume it.
pub use smix_sdk::ResetAppDataWaitFor;

/// Sliding-window spec for `expect.signal` / `expect.signals` verbs.
/// Selects the segment of the metro log tail that a signal search
/// scans against. Restored above `pub enum SignalWindow` in v1.0.14
/// after the earlier ResetAppData placement shuffled its docstring.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalWindow {
    /// Everything since the tail was constructed (runner boot).
    SinceRun,
    /// After step N ended.
    SinceStep {
        /// 1-indexed step number.
        since_step: usize,
    },
    /// Trailing N ms window.
    LastMs {
        /// Milliseconds prior to now.
        last_ms: u64,
    },
}

#[allow(clippy::derivable_impls)]
impl Default for SignalWindow {
    fn default() -> Self {
        SignalWindow::SinceRun
    }
}

/// v0.3.0 Phase A — ordering semantics for [`Step::ExpectSignals`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalOrderKind {
    /// Every signal must be matched at some point in the window;
    /// order in the yaml is documentation-only.
    Any,
    /// Signals must appear in the exact listed sequence.
    Strict,
}

#[allow(clippy::derivable_impls)]
impl Default for SignalOrderKind {
    fn default() -> Self {
        SignalOrderKind::Any
    }
}

/// v5.5 c5 — normalized bbox for `assertScreenshot.mask` regions.
/// Coordinates are 0..1 fractions of the captured screenshot; the same
/// convention maestro's mapping form uses. Surface-only in v5.5: the
/// adapter parses + carries the regions but the runtime currently emits
/// a warning and skips region exclusion (R2-tier algorithm change
/// deferred to v6+ when SSIM/pHash replaces the dhash backbone).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaskRegion {
    /// Top-left x in 0..1 fraction of capture width.
    pub x: f64,
    /// Top-left y in 0..1 fraction of capture height.
    pub y: f64,
    /// Region width in 0..1 fraction of capture width.
    pub width: f64,
    /// Region height in 0..1 fraction of capture height.
    pub height: f64,
}

/// v5.2 c4 — `Step::Repeat` mode. Parser 编译期 dispatch (yaml `repeat.times`
/// xor `repeat.while`), runtime 无运行时分叉.
///
/// v5.5 c5 — added [`RepeatMode::WhileVisible`] for `while: { visible:
/// <selector> }` mapping form (was parser-rejected pre-v5.5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode", content = "value")]
pub enum RepeatMode {
    /// `repeat: { while: "<expr>", commands: [...] }` — evaluate expression
    /// truthy each iteration; loop exits on falsy. Bounded by
    /// `MAX_REPEAT_ITERATIONS` runtime safety valve.
    While {
        /// Expression source (raw; `${...}` wrapping handled).
        condition_expr: String,
    },
    /// v5.5 c5 — `repeat: { while: { visible: <selector> }, commands: [...] }`
    /// mapping form. Loop continues while the selector resolves to a
    /// visible element; exits when not visible. Bounded by
    /// `MAX_REPEAT_ITERATIONS` runtime safety valve.
    WhileVisible {
        /// Element selector evaluated each iteration via App::find_first.
        selector: smix_sdk::Selector,
    },
    /// v5.6 c5 — `repeat: { while: { notVisible: <selector> }, commands: [...] }`
    /// mapping form. Loop continues while the selector does NOT resolve
    /// to a visible element; exits when the element appears. Bounded by
    /// `MAX_REPEAT_ITERATIONS` runtime safety valve. Useful for "wait
    /// until X is loaded" patterns where the body keeps triggering until
    /// the watched element shows up.
    WhileNotVisible {
        /// Element selector evaluated each iteration via App::find.
        selector: smix_sdk::Selector,
    },
    /// `repeat: { times: N, commands: [...] }` — fixed N iterations.
    Times(u32),
}

/// serde default for [`Step::ExtendedWaitUntil::expect_visible`] —
/// pre-v5.2 yaml shapes only had `visible:` arm.
fn default_expect_visible() -> bool {
    true
}

/// serde default for [`Step::LaunchApp::stop_app`] — maestro yaml `stopApp`
/// 字段缺省值是 true (kill + launch path).
fn default_stop_app() -> bool {
    true
}

/// v5.2 c2 — maestro yaml `permissions:` action 字面值.
/// `Allow` → simctl `grant`, `Deny` → simctl `revoke`, `Unset` → simctl `reset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaestroPermissionAction {
    /// maestro yaml `"allow"` — grant the permission.
    Allow,
    /// maestro yaml `"deny"` — revoke the permission.
    Deny,
    /// maestro yaml `"unset"` — reset the permission to "not determined".
    Unset,
}

impl MaestroPermissionAction {
    /// 翻译到 SDK 的 typed enum (cement 层 1:1 forward).
    pub fn to_sdk(self) -> smix_sdk::PermissionAction {
        match self {
            Self::Allow => smix_sdk::PermissionAction::Grant,
            Self::Deny => smix_sdk::PermissionAction::Revoke,
            Self::Unset => smix_sdk::PermissionAction::Reset,
        }
    }
}

/// A parsed maestro YAML flow — a sequence of [`Step`]s plus an `appId`
/// header.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    /// `appId:` top-level key from the yaml header. iOS literal bundle id
    /// or Android package name. Empty when only [`Self::app`] (logical
    /// cross-platform key) was provided — caller resolves via
    /// [`apps_config::AppsConfig`] before dispatch.
    pub app_id: String,
    /// `app:` top-level yaml key (cross-platform logical name, e.g.
    /// `app: demoApp`). Resolved at runtime via `smix-apps.yaml` → the
    /// platform-specific bundle id. None when the yaml only uses the
    /// legacy `appId:` literal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// Ordered step list.
    pub steps: Vec<Step>,
}

/// Adapter parse / dispatch errors.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// YAML deserialization failed.
    #[error("yaml deserialize: {0}")]
    Yaml(#[from] serde_norway::Error),
    /// I/O failure while reading a referenced flow file.
    #[error("io: {0}")]
    Io(String),
    /// Required yaml field missing.
    #[error("missing required field: {0}")]
    MissingField(String),
    /// Invalid yaml value shape.
    #[error("invalid {field}: {reason}")]
    InvalidValue {
        /// Field path or command name where the failure occurred.
        field: String,
        /// Human-readable reason.
        reason: String,
    },
    /// Unsupported command (not in the v3.17-ported 38 yaml subset).
    #[error("unsupported command: {0}")]
    UnsupportedCommand(String),
    /// runFlow cycle detected. `path` is the absolute path that closes
    /// the cycle; `stack` is the full traversal stack (root → ... → path).
    #[error("runFlow cycle detected at {path:?} (stack: {stack:?})")]
    RunFlowCycle {
        /// Absolute path of the file that re-entered the in-progress stack.
        path: PathBuf,
        /// Full traversal stack including the re-entered file at the end.
        stack: Vec<PathBuf>,
    },
}
