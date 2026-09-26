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

/// One annotation composed onto a screenshot output. Position
/// resolves against pixel/normalized coords or a smix `Selector`
/// (adapter runtime resolves selector → pixel at screenshot time via
/// the a11y tree).
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

/// Position spec for an annotation. Three shapes:
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

/// Parse an [`AnnotationSpec`] from a yaml annotation
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
mod emitter;
mod entry;
mod expr;
mod output;
mod parser;
mod runtime;
pub mod selector_support;
mod watch;

pub use emitter::{EmitError, emit_flow_yaml};

pub use entry::{FlowArgs, FlowPlatform, OutputFormat, run_flow, run_flow_code};

pub use apps_config::{
    AndroidApp, AppEntry, AppsConfig, IosApp, ResolveError, ResolvedApp, resolve_app_into_flow,
};

pub(crate) use expr::{Context as ExprContext, parse_and_eval as expr_eval};

/// The yaml expression engine's `Value` type. Callers use it to build
/// the output store passed to `Adapter::with_output`. The exposed
/// surface is a stable subset (Null / Bool / Number / String).
pub use expr::Value as ExprValue;

pub use parser::{
    parse_flow_file, parse_flow_yaml, set_ai_assertions_override, set_auto_ocr_fallback_override,
    text_to_pattern, visible_to_selector,
};
pub use runtime::{Adapter, AppLike, RunError, RunReport, RunStepReport, StepDebugRecord};

/// A maestro YAML command. Each variant corresponds to one or more
/// `smix-sdk` action calls; the enum itself is a structural
/// representation of the parsed yaml node.
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
///
/// Explicit tap-dispatch override for `tapOn: { dispatch: … }`.
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
/// - `daemonProxy` — XCTRunnerDaemonSession synthesize.
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
///
/// `#[non_exhaustive]` because this enum grows with every verb, and
/// without the attribute each new verb is a semver-major change for
/// anyone matching on it outside this crate. The attribute itself is
/// major, so it was added in a release that was major anyway (the one
/// that brought `rememberBounds`). Inside this crate matches stay
/// exhaustive, which is where the compiler's "you forgot the new verb"
/// is worth having.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Step {
    /// Tap an element by selector. Maps to `App::tap`.
    TapOn {
        /// Resolved selector — parser converts the yaml short / full form
        /// to a [`smix_selector::Selector`].
        selector: Selector,
        /// `optional: true` swallows the not-found error per maestro semantics.
        #[serde(default)]
        optional: bool,
        /// Explicit dispatch-mechanism override
        /// (`dispatch: xcui | daemonProxy`). `None` = default routing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dispatch: Option<TapDispatch>,
    },
    /// Tap at a normalized coordinate point (escape hatch for yaml
    /// `tapOn: { point: "X%,Y%" }`). Maps to `App::tap_at_coord(nx,
    /// ny)`: `nx`/`ny` ∈ \[0.0, 1.0\].
    TapAtPoint {
        /// Normalized X (0.0 = left, 1.0 = right).
        nx: f64,
        /// Normalized Y (0.0 = top, 1.0 = bottom).
        ny: f64,
    },
    /// Eval JS against the fixture-side WKWebView bridge. yaml shape:
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
    /// Wait for the screen to stop moving.
    ///
    /// Returns as soon as two sampled frames are still, so a flow pays
    /// only for the animation that actually ran. The number is a
    /// ceiling:
    /// - `- waitForAnimationToEnd` — up to 400 ms (maestro's default)
    /// - `- waitForAnimationToEnd: 500` — up to 500 ms
    /// - `- waitForAnimationToEnd: { timeout: 5000 }` — maestro's form
    ///
    /// Reaching the ceiling is not a failure: the step warns and moves
    /// on, because a screen that never settles is usually a spinner the
    /// flow does not care about.
    ///
    /// **NOT** an XCTest idle-wait — `SmixQuiescenceSwizzle.m` no-ops
    /// XCTest's idle wait for performance (RN long-running animations
    /// would otherwise stall every operation), and this verb never went
    /// through it anyway. Stillness here is measured by comparing
    /// screenshots.
    WaitForAnimationToEnd {
        /// How long to wait for the screen to stop moving, in
        /// milliseconds — a ceiling, not a duration. The step returns as
        /// soon as the screen is still.
        ///
        /// All three yaml forms land here: bare
        /// (`- waitForAnimationToEnd`) → 400 ms, maestro's default;
        /// numeric (`- waitForAnimationToEnd: 500`); and maestro's
        /// mapping form (`- waitForAnimationToEnd: { timeout: 5000 }`).
        ceiling_ms: u64,
    },
    /// Extended wait until a selector matches the expected visibility.
    /// `expect_visible=true` waits for visible (maestro yaml `visible:` arm);
    /// `expect_visible=false` waits for not visible (maestro yaml
    /// `notVisible:` arm). Which arm applies is decided at parse time,
    /// so the runtime has no branch.
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
    /// Type into a specific field: yaml `inputText: { id, text }`.
    /// Maps to `App::fill(selector, text)` — targeted where
    /// [`Step::InputText`] types into whatever holds focus.
    InputTextInto {
        /// The field to fill.
        selector: Selector,
        /// What to type.
        text: String,
    },
    /// Press a hardware / IME key. Maps to `App::press_key`. The name is
    /// read when the flow is, through [`smix_sdk::KeyName::from_name`].
    PressKey(smix_sdk::KeyName),
    /// Navigation back (maestro `back`): iOS navbar-back / edge swipe,
    /// Android KEYCODE_BACK. Not a keyboard key.
    Back,
    /// Recursively run a referenced yaml. The parser keeps the raw
    /// (potentially relative) path string here; [`parse_flow_file`]
    /// resolves it against the invoking yaml's directory and expands
    /// unconditional invocations inline. Conditional invocations stay
    /// as [`Step::RunFlowConditional`] (runtime evaluation belongs to
    /// the adapter `Adapter::run`, not the parser).
    RunFlow(String),
    /// `runFlow: { file, when?, env?, as?, label?, optional? }`. Parser
    /// stores the path verbatim; `Adapter::run` evaluates `when` before
    /// invoking the inner flow and captures the outputs alias.
    RunFlowConditional {
        /// Raw path string from the yaml (relative to invoking file).
        file: String,
        /// Precondition. `None` runs unconditionally.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<FlowCondition>,
        /// Outputs alias: after the subflow runs, read the device
        /// pasteboard (canonical "what the subflow captured via
        /// copyTextFrom") and write it into the parent flow's output map
        /// under this name. None ⇒ no capture. Mirrors maestro yaml
        /// `runFlow: { file, as: <name> }` capture form.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_name: Option<String>,
        /// Variables visible to the subflow only, in yaml order. Values
        /// are expanded against the caller's scope on entry.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<(String, String)>,
        /// `label` / `optional`.
        #[serde(default, skip_serializing_if = "BlockOptions::is_default")]
        opts: BlockOptions,
    },
    /// maestro `runFlow: { when?, commands: [...] }` inline form. The
    /// body is a literal list of steps held in-place; no child yaml file
    /// is referenced. `file` and `commands` are mutually exclusive at
    /// parse time.
    RunFlowInline {
        /// Precondition. `None` runs unconditionally.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<FlowCondition>,
        /// Inline step list. Runtime executes top-to-bottom under the
        /// same warning channel as the parent flow.
        steps: Vec<Step>,
        /// Variables visible to the body only; see
        /// [`Step::RunFlowConditional::env`].
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<(String, String)>,
        /// `label` / `optional`.
        #[serde(default, skip_serializing_if = "BlockOptions::is_default")]
        opts: BlockOptions,
    },
    /// Scroll the screen until a selector becomes visible.
    /// Maps to a `swipe` loop + `find` polling.
    ScrollUntilVisible {
        /// Selector to find.
        selector: Selector,
        /// Direction (`up` / `down` / `left` / `right`).
        direction: String,
        /// `visibilityPercentage` / `centerElement` / `timeout`.
        #[serde(
            default,
            with = "scroll_until_serde",
            skip_serializing_if = "is_default_until"
        )]
        until: smix_driver::ScrollUntil,
        /// `label` / `optional`.
        #[serde(default, skip_serializing_if = "BlockOptions::is_default")]
        opts: BlockOptions,
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
    /// Swipe inside one element's box, by proportion of that box.
    ///
    /// The opposite of the coordinate escape hatch rather than more of
    /// it. `swipe --from/--to` takes a share of the *screen*, which is
    /// what a caller reaches for when there is nothing nameable to
    /// swipe between; a consumer with a nameable timeline still had to
    /// measure it, getting 45.3–50.5% of screen height on Android and
    /// 47.7–53.2% on iOS, taking 49% as the overlap and knowing it would
    /// have to be measured again on a device of a different shape.
    ///
    /// `over:` names the element and the numbers are shares of *its*
    /// box, so the flow stops depending on the geometry. §9 #3 forbids
    /// xpath and coordinates on the selector surface; this puts a
    /// selector where the coordinates used to be.
    SwipeOver {
        /// The element whose box the swipe happens inside.
        selector: Selector,
        /// Start, as a share of the element's box. `(0.5, 0.3)` is
        /// halfway across and three tenths down.
        from: (f64, f64),
        /// End, same frame of reference.
        to: (f64, f64),
    },
    /// Launch / relaunch the app under test. Supports the full maestro
    /// yaml sub-parameter set: appId / clearState / clearKeychain /
    /// permissions / arguments / stopApp.
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
        /// maestro yaml `permissions: { name: allow|deny|unset }`.
        /// Empty = no permission changes. Applied in mapping iteration order
        /// before launch.
        #[serde(default)]
        permissions: Vec<(String, MaestroPermissionAction)>,
        /// maestro yaml `arguments: [...]` — process-level argv.
        #[serde(default)]
        arguments: Vec<String>,
        /// maestro yaml `stopApp: bool` (default true).
        #[serde(default = "default_stop_app")]
        stop_app: bool,
        /// maestro yaml `waitForInteractiveMs: <ms>`. When set (and
        /// stopApp is true — so we go through the cooperative launch
        /// pathway), threads through to
        /// `SessionAppLifecycleRequest.wait_for_interactive_ms` on the
        /// runner. The runner polls the a11y tree at 500 ms cadence
        /// and re-snapshots each iteration: without the refresh, a
        /// Fabric mount-item drain that lands mid-poll is read off a
        /// stale snapshot and the tree looks empty. `None` = dispatch
        /// and return as soon as the app reaches foreground.
        #[serde(default)]
        wait_for_interactive_ms: Option<u64>,
    },
    /// Clear the current session's app data IN PLACE without
    /// launching. Maps to [`smix_sdk::App::clear_app_data`] (the
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
    /// Accepts optional `launchArgs` / `launchEnv` to steer
    /// scaffolding shown at cold launch: the Expo dev-launcher server
    /// picker on SDK 57 stopped auto-navigating on a URL scheme, so
    /// launch args are how the metro target reaches it. Bare
    /// `- clearAppData` remains valid and equivalent to empty vec +
    /// empty map.
    ClearAppData {
        /// launchArguments forwarded to the cooperative runner-side
        /// launch. Empty vec = no arguments.
        launch_args: Vec<String>,
        /// launchEnvironment forwarded to the cooperative runner-side
        /// launch. Empty map = no environment overrides.
        launch_env: std::collections::BTreeMap<String, String>,
    },
    /// URL-scheme-driven app-owned reset.
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
    /// - resetAppData: 'myapp://dev-mutate?action=reset'
    ///
    /// - resetAppData:
    ///     via: url-scheme
    ///     url: 'myapp://dev-mutate?action=reset'
    ///     waitFor:
    ///       logLinePattern: '\[myapp-dev\] reset-complete token='
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
    /// Delete keys from the target app's persisted
    /// user-defaults store (iOS NSUserDefaults via `simctl spawn
    /// defaults delete`; Android unsupported). yaml shape:
    /// `clearUserDefaults: { keys: [k1, k2], bundleId?: <id> }`.
    /// `bundleId` absent ⇒ the flow's resolved app id. Contract is
    /// "ensure keys absent" — already-absent keys succeed. Terminate
    /// the app first (running processes cache defaults in-memory).
    ///
    /// Motivating case: neutralizing expo-dev-launcher's persisted
    /// deep-link replay between terminate and relaunch.
    ClearUserDefaults {
        /// NSUserDefaults keys to delete.
        keys: Vec<String>,
        /// Target defaults domain. `None` ⇒ flow's resolved app id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bundle_id: Option<String>,
    },
    /// Open a URL / deep link in the OS handler. Maps to
    /// `simctl openurl`.
    OpenLink(String),
    /// Terminate the foreground app. Maps to `simctl terminate`.
    StopApp,
    /// Viewport scroll one swipe (default direction = down).
    /// maestro `scroll:` (bare, no args). Maps to `App::scroll_screen`.
    Scroll,
    /// Dismiss the on-screen keyboard. maestro `hideKeyboard`.
    /// Maps to `App::hide_keyboard`.
    HideKeyboard,
    /// Assert selector is NOT visible. maestro `assertNotVisible`.
    /// Maps to `App::assert_not_visible`.
    AssertNotVisible {
        /// Selector that must NOT be visible.
        selector: Selector,
    },
    /// Terminate an explicitly-specified app by bundle id.
    /// maestro `killApp: "com.x"` (independent from launchApp). Maps to
    /// `App::terminate`.
    KillApp {
        /// Bundle id to terminate; `None` (bare `- killApp`) means the
        /// current app, resolved from the last launched bundle.
        app_id: Option<String>,
    },
    /// Wipe an app's MMKV / NSUserDefaults / file storage
    /// independent of launchApp. maestro `clearState: { appId }`.
    /// Maps to `App::launch_fresh(_, clear_state: true, clear_keychain: false, _)`.
    ClearState {
        /// Bundle id to wipe; `None` (bare `- clearState`) means the
        /// current app, resolved from the last launched bundle.
        app_id: Option<String>,
    },
    /// Wipe the Keychain entries for the last launched app
    /// (or top-of-flow appId). maestro `clearKeychain` (bare, no args).
    /// Maps to `App::launch_fresh(_, false, true, _)` with last_bundle.
    ClearKeychain,
    /// Capture a screenshot with optional
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
        /// Annotations composed onto the PNG before write. Empty =
        /// plain screenshot. Selector-relative positions resolve
        /// against the a11y tree fetched at capture time.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<AnnotationSpec>,
        /// `cropOn`: keep only this element's region — how maestro makes
        /// the baseline a cropped `assertScreenshot` compares against.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop_on: Option<Selector>,
    },
    /// Write a literal to the device pasteboard. maestro yaml
    /// `setClipboard: "literal"`.
    SetClipboard(String),
    /// Paste into focused field. `text: Some(literal)` =
    /// `pasteText: "literal"` (writes clipboard then fills);
    /// `text: None` = bare `- pasteText` (reads current clipboard then fills).
    PasteText {
        /// `None` = read clipboard, `Some(s)` = set clipboard + fill `s`.
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Copy element text content into the device pasteboard.
    /// maestro yaml `copyTextFrom: <selector>`. Field priority value→text→label.
    CopyTextFrom {
        /// Selector picking the text-bearing element.
        selector: Selector,
    },
    /// Double-tap on element. maestro `doubleTapOn: <selector>`.
    /// XCUIElement.doubleTap() public API path.
    DoubleTapOn {
        /// Selector picking the tap target.
        selector: Selector,
    },
    /// Tap one element several times, spaced on the event timeline.
    ///
    /// `repeat` around `tapOn` sends a request per tap; at ~400 ms per
    /// synthesise that cannot drive a gesture gated on a short
    /// inter-tap window, and the spacing is whatever the round trip
    /// cost rather than a number the flow states.
    RepeatTap {
        /// Selector picking the tap target.
        selector: Selector,
        /// How many touches.
        times: u32,
        /// Milliseconds between touches; `None` takes the runner default.
        interval_ms: Option<u32>,
        /// Milliseconds each touch is held; `None` takes the default.
        hold_ms: Option<u32>,
    },
    /// Long-press on element with optional duration (ms,
    /// default 500). maestro `longPressOn: <selector>` (scalar) or
    /// `longPressOn: { ..., duration: N }`. XCUIElement.press(forDuration:).
    LongPressOn {
        /// Selector picking the press target.
        selector: Selector,
        /// Press duration in milliseconds (default 500 per maestro doc).
        duration_ms: u64,
        /// Capture frames of the held state while the touch is down.
        capture_during: bool,
    },
    /// Assert a yaml expression evaluates truthy. maestro
    /// `assertTrue: ${expression}`. The expression source is held raw
    /// (either `${...}`-wrapped or a bare literal); the runtime
    /// applies expand_template and the expression engine together.
    /// Unsupported patterns raise an explicit `DriverError` rather
    /// than silently passing.
    AssertTrue {
        /// Raw expression source, verbatim.
        expr: String,
    },
    /// Looped subflow. maestro yaml `repeat: { ... }`.
    ///
    /// A count, a condition, or both — and with both, the loop runs
    /// while the condition holds AND the count is not spent, which is
    /// what maestro's `while (checkCondition() && counter < maxRuns)`
    /// does. They were mutually exclusive here until 10.2, so "up to
    /// five times, while the spinner is up" could not be written.
    ///
    /// Neither is refused at parse time: a loop that says nothing about
    /// when to stop has not been written yet.
    Repeat {
        /// How many iterations at most. `None` means only the condition
        /// decides.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        times: Option<u32>,
        /// What must hold to run another iteration. `None` means only
        /// the count decides. The string form (`while: "<expr>"`) is a
        /// smix extension and lives in `while_expr` below.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        while_: Option<Box<FlowCondition>>,
        /// The smix-only expression form of `while`, when that is what
        /// was written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        while_expr: Option<String>,
        /// Body to run per iteration (recursively parsed).
        commands: Vec<Step>,
        /// `label` / `optional`.
        #[serde(default, skip_serializing_if = "BlockOptions::is_default")]
        opts: BlockOptions,
    },
    /// Retry body on failure up to `max_retries` extra attempts.
    /// Initial + max_retries = max attempts total. Last attempt's
    /// `RunError` propagates if all attempts fail.
    Retry {
        /// Extra retry attempts on top of initial run.
        max_retries: u32,
        /// Body to attempt (recursively parsed).
        commands: Vec<Step>,
    },
    /// maestro `runScript: <source>` (inline literal or file path
    /// verbatim). The parser accepts it so yaml files stay portable
    /// with maestro, but there is no JS runtime behind it: the adapter
    /// runtime raises an explicit `DriverError` rather than silently
    /// treating the script as a no-op.
    RunScript {
        /// Raw script source (inline literal or file path verbatim).
        source: String,
        /// `when:` — maestro's `YamlRunScript` carries one, and smix
        /// refused the whole mapping form until 10.2, so there was no
        /// way to write it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        when: Option<Box<FlowCondition>>,
        /// `env:` — parsed and carried. There is no JS runtime behind
        /// this verb, so nothing reads these yet; dropping them at the
        /// parser would be the silent swallow C4 closed everywhere else.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        env: Vec<(String, String)>,
        /// `label` / `optional`.
        #[serde(default, skip_serializing_if = "BlockOptions::is_default")]
        opts: BlockOptions,
    },
    /// maestro `evalScript: <expr>`. Same graceful unsupported
    /// semantics as [`Step::RunScript`].
    EvalScript {
        /// Raw expression source.
        source: String,
    },
    /// maestro `setLocation: { latitude, longitude }`.
    /// Walks `App::set_location` → simctl location set.
    SetLocation {
        /// Latitude in decimal degrees.
        latitude: f64,
        /// Longitude in decimal degrees.
        longitude: f64,
    },
    /// `clearLocation` — stop simulating a location. A smix verb, not a
    /// maestro one: maestro can set a location and cannot put it back.
    ClearLocation,
    /// maestro `travel: { points: [...], speed_mps?: <m/s> }`.
    /// Fire-and-return — downstream Step does not block on playback.
    Travel {
        /// Ordered waypoints (lat, lng). ≥2 required (enforced by parser).
        points: Vec<(f64, f64)>,
        /// Optional travel speed in m/s (forwarded to `simctl location
        /// start --speed=...`).
        speed_mps: Option<f64>,
    },
    /// maestro `setPermissions: { camera: allow, ... }` (top-level
    /// command, distinct from the `launchApp.permissions`
    /// sub-parameter). `app_id` is None at parse time — runtime resolves from
    /// `last_bundle`; empty `last_bundle` → explicit DriverError.
    SetPermissions {
        /// Bundle id; `None` ⇒ runtime resolves from `last_bundle`.
        #[serde(skip_serializing_if = "Option::is_none")]
        app_id: Option<String>,
        /// Permission directives in mapping iteration order.
        permissions: Vec<(String, MaestroPermissionAction)>,
    },
    /// maestro `addMedia: <path>` (scalar) or `addMedia: [paths]`
    /// (array). Adapter flattens both into `Vec<String>`.
    AddMedia {
        /// Absolute or relative paths to media files.
        paths: Vec<String>,
    },
    /// maestro `setOrientation: <variant>`. Walks
    /// `App::set_orientation` → driver `/set-orientation` route →
    /// swift `XCUIDevice.shared.orientation`.
    SetOrientation {
        /// Orientation literal aligned with maestro yaml.
        orientation: smix_sdk::MaestroOrientation,
    },
    /// maestro `startRecording: <output-path>`. Spawns
    /// `xcrun simctl io recordVideo` as a long-running child via SDK::App.
    /// Pair with `stopRecording` for clean SIGINT-and-wait shutdown.
    StartRecording {
        /// Output mp4 path (relative or absolute).
        path: String,
    },
    /// maestro `stopRecording` (bare). SIGINT-and-wait the
    /// child started by previous `startRecording`. No prior start →
    /// DriverError.
    StopRecording,
    /// Visual regression assertion against a baseline PNG. maestro
    /// `assertScreenshot: "path/to/baseline.png"` (scalar) OR mapping
    /// form `{ path, threshold?, mask? }`. Baseline path is resolved relative
    /// to the invoking yaml's base_dir; first run auto-records (writes the
    /// captured screenshot to baseline_path + warns the adapter RunReport),
    /// subsequent runs compute a 64-bit dhash on both and assert hamming
    /// distance ≤ `max_hamming` (None = 5 default). Set env
    /// `SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD=1` to force strict mode.
    ///
    /// The mapping form reads `path`, `threshold` (the dhash hamming cap)
    /// and `mask` (regions both frames read as one flat value before
    /// hashing, so what changes inside them cannot count). maestro's
    /// `cropOn` / `thresholdPercentage` / `label` / `optional` are
    /// refused by name at parse time.
    AssertScreenshot {
        /// Baseline PNG path relative to the flow's base_dir.
        path: String,
        /// Which comparison, and its bar: `threshold` (the dhash hamming
        /// cap) or `thresholdPercentage` (maestro's share of matching
        /// pixels). A flow names at most one; naming neither is the hash
        /// with its default cap of 5.
        threshold: ScreenshotThreshold,
        /// `mask` regions (0..1 shares of the compared image) left out of
        /// the comparison. Empty for the scalar form.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mask: Vec<MaskRegion>,
        /// `cropOn`: compare only this element's region. The baseline is
        /// then a cropped image too — `takeScreenshot` with the same
        /// `cropOn` makes one, and a missing baseline is recorded cropped.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop_on: Option<Selector>,
    },
    /// `rememberBounds: { <selector>, as: name }` — keep where an element
    /// is, under a name, for `assertBoundsUnchanged` to compare against.
    ///
    /// A smix verb; maestro has none. The rectangle is kept as a value to
    /// compare, never as something a flow can aim at — the flow surface
    /// addresses elements by selector, never by coordinates.
    RememberBounds {
        /// What to measure.
        selector: Selector,
        /// The name `assertBoundsUnchanged.was` refers to.
        name: String,
    },
    /// `assertBoundsUnchanged: { <selector>, was: name, within: dp }` —
    /// the element's rectangle now matches the one remembered under
    /// `was`, every edge within `within` device-independent pixels.
    ///
    /// A smix verb; maestro has none. Asserting "visible" in each state
    /// passes whether or not the layout jumped between them.
    AssertBoundsUnchanged {
        /// What to measure now.
        selector: Selector,
        /// The name a `rememberBounds` stored.
        was: String,
        /// Allowed movement per edge, in device-independent pixels.
        within_dp: f64,
    },
    /// `neverVisible: { <selector>, during: [<steps>] }` — the element is
    /// not on screen at any moment while the inner steps run.
    ///
    /// A smix verb; maestro has none. `assertNotVisible` answers about one
    /// instant, and a loading state that flashed for 200 ms between two
    /// steps is gone by the time any instant after them is asked about.
    /// The inner steps run as they would anywhere else; beside them a
    /// watch asks the same question `assertNotVisible` asks, as fast as
    /// the device answers, and says how often it managed to.
    NeverVisible {
        /// What must not appear.
        selector: Selector,
        /// The steps the watch spans.
        during: Vec<Step>,
        /// `label` / `optional`, read as a `runFlow` block reads them.
        #[serde(default)]
        opts: BlockOptions,
    },
    /// Ask the AI judge whether a plain-language condition holds on the
    /// current screen.
    ///
    /// Unlike every other assert, this one is a judgement rather than a
    /// measurement: the verdict comes from a local `claude` CLI reading a
    /// screenshot, and the same screen may not produce the same answer
    /// twice. Opt-in, and the runtime marks the result as non-deterministic.
    AssertCondition {
        /// The condition, in plain language.
        condition: String,
    },
    /// Have the AI judge read structured fields off the screen into the
    /// output store, for later `assertTrue` expressions.
    ///
    /// Non-deterministic, on the same terms as [`Step::AssertCondition`].
    ExtractWithAI {
        /// Key the extracted object lands under in `output.*`.
        into: String,
        /// Field names to read off the screen.
        fields: Vec<String>,
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
    /// Ordered / unordered multi-signal await. See
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
    /// Post-run log-hygiene assertion. Verifies that
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

/// Sub-field of [`Step::ExpectSignals`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalMatch {
    /// Regex to match against the log line message.
    pub regex: String,
    /// Optional log-level constraint (debug/info/warn/error/log). None = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// Completion-signal wait strategy for [`Step::ResetAppData`].
///
/// Re-exported from `smix-sdk`, where the canonical definition lives
/// because the `App` impl consumes it.
pub use smix_sdk::ResetAppDataWaitFor;

/// Sliding-window spec for `expect.signal` / `expect.signals` verbs.
/// Selects the segment of the metro log tail that a signal search
/// scans against. Mirrors `smix_metro_log::Window` at the yaml level;
/// the runtime translates `SinceStep` to `Window::SinceMs` using the
/// tail's ms cursor captured at each step-end.
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

/// Ordering semantics for [`Step::ExpectSignals`].
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

/// A region `assertScreenshot` leaves out of its comparison, as shares
/// (0..1) of the captured frame. smix's own key — maestro's
/// `assertScreenshot` has `cropOn` instead, which smix refuses by name.
///
/// The SDK's [`smix_sdk::ScreenMask`] under the name this crate has always
/// used: one type for one thing, so the region the parser reads is the
/// region the comparison applies.
pub use smix_sdk::ScreenMask as MaskRegion;

/// The bar an `assertScreenshot` is held to, and so which comparison
/// runs.
///
/// Two comparisons because they measure two different things: a 64-bit
/// perceptual hash (smix's, tolerant of anti-aliasing and encoder noise,
/// blind to small moves) and maestro's share of matching pixels (sees a
/// small move, counts every anti-aliased edge). Neither is a setting of
/// the other, so a flow names one.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScreenshotThreshold {
    /// `threshold:` — the most differing hash bits allowed; `None` is the
    /// default of 5.
    Hash(Option<u32>),
    /// `thresholdPercentage:` — the least share (0..=100) of pixels that
    /// must match, as written: it may carry `${…}`, evaluated when the step
    /// runs, as maestro does.
    Percentage(String),
}

/// The keys a condition mapping (`runFlow.when`, `repeat.while`) may
/// carry. maestro's `YamlCondition` has these five plus `optional`, which
/// never reaches its `Condition` (`YamlFluentCommand.toCondition`), so
/// smix refuses it rather than accept a key that does nothing.
pub const CONDITION_KEYS: &[&str] = &["platform", "visible", "notVisible", "true", "label"];

/// The keys `scrollUntilVisible:` takes: maestro's
/// `YamlScrollUntilVisible` less `speed` and `waitToSettleTimeoutMs`,
/// which smix refuses by name — its single swipe is a fixed gesture on
/// both runners, with no duration to set and no settle wait to tune.
pub const SCROLL_UNTIL_VISIBLE_KEYS: &[&str] = &[
    "element",
    "direction",
    "timeout",
    "visibilityPercentage",
    "centerElement",
    "label",
    "optional",
];

fn is_default_until(u: &smix_driver::ScrollUntil) -> bool {
    *u == smix_driver::ScrollUntil::default()
}

/// [`smix_driver::ScrollUntil`] in the words a flow uses for it:
/// `visibilityPercentage` (1–100), `centerElement`, `timeout` in
/// milliseconds.
mod scroll_until_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use smix_driver::ScrollUntil;
    use smix_host_coord_resolver::Reach;
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Wire {
        visibility_percentage: f64,
        center_element: bool,
        timeout_ms: u64,
    }

    pub fn serialize<S: Serializer>(u: &ScrollUntil, s: S) -> Result<S::Ok, S::Error> {
        Wire {
            visibility_percentage: u.reach.visibility * 100.0,
            center_element: u.reach.center_element,
            timeout_ms: u64::try_from(u.timeout.as_millis()).unwrap_or(u64::MAX),
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ScrollUntil, D::Error> {
        let w = Wire::deserialize(d)?;
        Ok(ScrollUntil {
            reach: Reach {
                visibility: w.visibility_percentage / 100.0,
                center_element: w.center_element,
            },
            timeout: Duration::from_millis(w.timeout_ms),
        })
    }
}

/// A precondition, as maestro's `when:` / `repeat.while:` spell it. Every
/// present field must hold (AND); the runtime checks them in maestro's
/// order — platform, `true`, visible, notVisible — and stops at the first
/// that does not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCondition {
    /// `platform:` — the device platform the block is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<ConditionPlatform>,
    /// `visible:` — holds when the selector is on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<Selector>,
    /// `notVisible:` — holds when the selector is not on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_visible: Option<Selector>,
    /// `true:` — a template (`${…}` expanded at check time) whose result
    /// maestro reads as false when blank, `false`, `undefined`, `null` or
    /// a zero number, and as true otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// `label:` — what reports call the condition instead of its checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// `when.platform` values. maestro's `Platform.fromString` compares
/// ignoring case, and so does the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionPlatform {
    /// `Android`.
    Android,
    /// `iOS`.
    Ios,
    /// `Web` — smix drives no web platform, so this never holds.
    Web,
}

impl ConditionPlatform {
    /// Whether a device of platform `p` is this one. `Web` never is:
    /// smix drives no browser.
    pub fn matches(self, p: smix_driver::Platform) -> bool {
        matches!(
            (self, p),
            (Self::Android, smix_driver::Platform::Android)
                | (Self::Ios, smix_driver::Platform::Ios)
        )
    }

    /// maestro's spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Android => "Android",
            Self::Ios => "iOS",
            Self::Web => "Web",
        }
    }

    /// maestro's spelling of a device platform.
    pub fn name_of(p: smix_driver::Platform) -> &'static str {
        match p {
            smix_driver::Platform::Android => Self::Android.name(),
            smix_driver::Platform::Ios => Self::Ios.name(),
        }
    }
}

/// `label:` and `optional:` on a block (`runFlow`, `repeat`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockOptions {
    /// Name reports use for the block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A failure inside the block is reported as a skip and the flow
    /// continues.
    #[serde(default)]
    pub optional: bool,
}

impl BlockOptions {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// serde default for [`Step::ExtendedWaitUntil::expect_visible`] —
/// a yaml shape with neither arm named means the `visible:` arm.
fn default_expect_visible() -> bool {
    true
}

/// serde default for [`Step::LaunchApp::stop_app`] — maestro yaml
/// defaults `stopApp` to true (kill + launch path).
fn default_stop_app() -> bool {
    true
}

/// maestro yaml `permissions:` action literal.
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
    /// Translate to the SDK's typed enum (1:1 forward).
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
    /// Android launch activity, resolved from `smix-apps.yaml`.
    ///
    /// Not a yaml key: there is nowhere in a flow to write it, because
    /// it is a property of the app rather than of the run. It arrives
    /// here from [`apps_config::resolve_app_into_flow`] and travels on
    /// to the launch options, which is the whole journey it never used
    /// to make — it was read, defaulted, and dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_activity: Option<String>,
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
    /// Unsupported command (not in the supported maestro yaml subset).
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

/// Reading back what `entry::emit_junit` writes.
///
/// Beside the writer on purpose. Three host adapters read this — an XCTest
/// helper, a JUnit rule, and whatever comes next — and a shape that drifts
/// on the writing side has to break the reader in the same commit, not in
/// somebody else's repository a release later.
///
/// Deliberately not an XML library: this is our own document, its shape is
/// ours to keep, and the two host languages each need a reader small enough
/// to be written twice more without a dependency.
pub mod report {
    /// What a host framework needs out of a run, and nothing else.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FlowReport {
        /// The flow's name, as the report names it.
        pub flow: String,
        /// Whether it passed.
        pub passed: bool,
        /// Why not. `None` exactly when `passed`.
        pub failure: Option<String>,
    }

    /// Why a report could not be read.
    ///
    /// Separate from "it failed": a run that never happened and a run that
    /// failed want different things from a reader, and a single value for
    /// both is how an empty string becomes a green test.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ReadError {
        /// Not a smix report at all — usually the CLI never ran.
        NotAReport,
        /// A report, with no flow in it.
        NoFlowInIt,
    }

    /// Parse the JUnit XML `smix run --format junit` writes.
    pub fn parse_junit(xml: &str) -> Result<FlowReport, ReadError> {
        if !xml.contains("<testsuite") {
            return Err(ReadError::NotAReport);
        }
        let Some(flow) = attr(xml, "<testcase", "name") else {
            return Err(ReadError::NoFlowInIt);
        };
        let failure = between(xml, "<![CDATA[", "]]>")
            .or_else(|| attr(xml, "<failure", "message"))
            .map(|m| unescape(&m));
        Ok(FlowReport {
            flow,
            passed: failure.is_none(),
            failure,
        })
    }

    /// One attribute of the first element whose text starts with `tag`.
    fn attr(xml: &str, tag: &str, name: &str) -> Option<String> {
        let start = xml.find(tag)?;
        let rest = &xml[start..];
        let end = rest.find('>')?;
        let head = &rest[..end];
        let key = format!("{name}=\"");
        let at = head.find(&key)? + key.len();
        let close = head[at..].find('"')?;
        Some(head[at..at + close].to_string())
    }

    fn between(xml: &str, open: &str, close: &str) -> Option<String> {
        let a = xml.find(open)? + open.len();
        let b = xml[a..].find(close)?;
        Some(xml[a..a + b].to_string())
    }

    /// Undo the escaping the writer applies.
    ///
    /// A reader that hands `&quot;` to a developer has made the failure
    /// harder to read than the stdout it replaced.
    fn unescape(s: &str) -> String {
        s.replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }
}
