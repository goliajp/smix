//! smix — AI-native iOS Simulator automation CLI (binary entry).
//!
//! `smix sim` is the sole device-control surface — raw `simctl` is not
//! expected in workflows. Every device argument accepts an explicit
//! UDID or an alias recorded in `.smix/sims.json` (resolved
//! deterministically by `smix_simctl::registry`; never against the live
//! simulator set). Unwrapped long-tail subcommands go through
//! `smix sim exec`, which keeps simctl's original argument shape and
//! injects the resolved UDID.

mod act;
mod authoring;
mod bench;
mod capsule;
mod down;
/// Distributed `smix run --nodes`: roster parsing, cross-node flow
/// sharding, readiness gate, ssh fan-out and merged reporting.
mod federation;
#[cfg(test)]
mod guide_gate;
mod init;
mod lease_cmd;
mod parallel;
mod readiness;
mod record_cmd;
mod runner_list;

mod script;

use clap::{Parser, Subcommand};
use smix_simctl::registry::{self, RegistryError, SimRegistry};
use smix_simctl::{Appearance, DeviceControlError, LaunchResult, SimctlClient};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "smix",
    about = "AI-native iOS Simulator + Android emulator automation",
    version,
    long_about = "\
smix — AI-native automation for iOS Simulator + Android emulator.

What smix is:
  · A single tool that owns the full sim/emulator lifecycle (boot →
    capsule → flow → teardown).
  · A pinned-device model: every command takes an explicit DEVICE
    (registry alias from `.smix/sims.json` or raw UDID). There is no
    `--device booted` fallback; ambiguity is a bug, not a feature.
  · A three-layer architecture: sense (tree / find / OCR / popups) and
    act (tap / fill / swipe / press-key) are core flat capabilities;
    decide lives in driver impls.
  · Two yaml dialects:
      - smix flows (read maestro-format yaml, plus smix-native extensions:
        ocrText / anchorRelative / fallback / cross-platform `app:`).
        Run via `smix run flow.yaml`.
      - smix-native run-script (shell-friendly sequential subcommand
        driver). Run via `smix run-script script.yaml`.
  · AI-readable failures: every error carries visibleElements +
    suggestions + code, not just a stack trace.

What smix is NOT:
  · Not a build tool. smix does not build the app under test; you build,
    smix installs + drives.
  · Not a maestro wrapper. We read maestro's yaml format because flow
    files are portable, not because we are bound to its product surface.

Quick start:
  smix sim boot <DEVICE>                # boot a registered sim/emulator
  smix capsule up <DEVICE>               # start runner (XCUITest on iOS,
                                         # Kotlin instrumentation on Android)
  smix run flow.yaml --device <DEVICE>   # execute a flow
  smix find --selector-id <a11y-id>      # ad-hoc probe (one-shot)
  smix tree --json                       # inspect current a11y tree
  smix capsule down <DEVICE>             # teardown

Subcommand categories:
  Environment:
    doctor, sim, runner, capsule, down

  Flow execution:
    run             (maestro-format yaml flow)
    run-script      (smix-native sequential subcommand script)

  Live probes (require a running runner):
    tap, find, wait-for, fill, press-key, scroll, hide-keyboard,
    tree, describe, system-popups

Documentation:
  - Master AI guide:    docs/AI_GUIDE.md
  - Quickstart:         docs/ai-guide/01-quickstart.md
  - CLI reference:      docs/ai-guide/05-cli.md
  - Cookbook:           docs/ai-guide/08-cookbook.md
  - Errors + remedies:  docs/ai-guide/07-errors.md

Sim safety hook:
  Bare `xcrun simctl <verb>` is BLOCKED for mutating verbs (read-only
  `simctl list` is allowed). Use typed `smix sim ...` subcommands or
  `smix sim exec <DEVICE> ...` for passthrough. The hook requires an
  explicit device id — there is no 'booted' / blanket selector.
"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

// A clap top-level command enum: parsed once, held as a single value, never
// stored in bulk. `Run` legitimately carries ~20 flag fields while `Doctor` is
// a unit — the variant-size spread is by design, not the memory bloat this lint
// guards against. Boxing `Run`'s fields into a separate Args struct buys lint
// compliance at the cost of clap-derive machinery for no runtime gain; the
// inline allow is the sanctioned exception (same idiom as `RunError` in
// smix-adapter-maestro, per the workspace lints policy).
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
enum Cmd {
    /// Register a simulator under an alias, creating the `.smix` registry.
    /// This is the bootstrap: alias-form device refs have nothing to
    /// resolve against until one exists.
    Init {
        /// Alias to register under.
        #[arg(long, default_value = "dev")]
        alias: String,
        /// UDID to register. Required when more than one simulator is
        /// available — init does not choose between devices.
        #[arg(long)]
        device: Option<String>,
        /// Path to the `.app` bundle to install on it. The device is
        /// booted first: `simctl install` refuses a shut-down device, and
        /// a freshly registered one is shut down.
        #[arg(long)]
        app: Option<PathBuf>,
    },
    /// Say whether this machine can drive anything yet, and if not, what
    /// to run next. `--json` for the same verdict in machine form.
    Doctor {
        /// Emit the verdict as JSON instead of prose.
        #[arg(long)]
        json: bool,
    },
    /// Perf regression gate: measure the in-process corpus, compare
    /// against the committed baseline, and fail on a >5% slowdown or a
    /// metric that stopped being measured. The absolute `perf_gate`
    /// ceilings catch a spike; this catches slow drift under them.
    Bench {
        /// Overwrite the committed baseline with this run's measurement
        /// instead of comparing against it.
        #[arg(long = "update-baseline", default_value_t = false)]
        update_baseline: bool,
        /// Read the "current" measurement from a JSON file instead of
        /// measuring. For tests and CI reproduction; skips the
        /// machine-sensitive measurement.
        #[arg(long = "current-file")]
        current_file: Option<std::path::PathBuf>,
        /// Baseline JSON to compare against. Defaults to the committed
        /// `crates/smix-cli/bench/baseline.json`.
        #[arg(long = "baseline-file")]
        baseline_file: Option<std::path::PathBuf>,
    },
    /// Runtime observability commands. `dump` pretty-prints the
    /// runner's recent subprocess ring buffer + open sessions + sim
    /// health so a failed flow can be diagnosed without a new smix
    /// patch.
    Diagnostic {
        #[command(subcommand)]
        action: DiagnosticAction,
    },
    /// Manage simulators. `<DEVICE>` = explicit UDID, or an alias /
    /// deviceName in the workspace's `.smix` registry (env SMIX_SIMS_JSON
    /// overrides discovery).
    Sim {
        #[command(subcommand)]
        action: SimAction,
    },
    /// Manage the XCUITest runner session (host-side xcodebuild handle).
    Runner {
        #[command(subcommand)]
        action: RunnerAction,
    },
    /// Tear down every smix-owned residual process and recycle registered
    /// sims (per-UDID; never touches sims outside .smix/sims.json).
    Down,
    /// Inspect and settle the device resource ledger: who holds a device,
    /// what they left open, and closing it gracefully when they are gone.
    Lease {
        #[command(subcommand)]
        action: LeaseAction,
    },
    /// Record the device screen. The recording is written into the device
    /// ledger, so it survives the process that started it and can be
    /// closed gracefully if that process dies.
    Record {
        #[command(subcommand)]
        action: RecordAction,
    },
    /// End-to-end capsule bring-up / tear-down: headless boot, capture,
    /// and runner start with `--record`. The guard rejects a windowed
    /// session by default; pass `--soft` to accept the soft-capsule
    /// fallback.
    Capsule {
        #[command(subcommand)]
        action: CapsuleAction,
    },
    /// Host-resolve and dispatch a tap on the running runner. Reads
    /// `SMIX_RUNNER_PORT` env (default 22087). Selector shorthand:
    /// `id:<a11y-id>` / `text:<plain>` / `label:<acc-label>` / `role:<role>`.
    Tap {
        /// Selector in `<kind>:<value>` shorthand.
        selector: String,
        /// Runner port override (defaults to SMIX_RUNNER_PORT env or 22087).
        /// Which language to read an `ocrText:` selector in — `zh-Hans`,
        /// `ja`, `en`. Repeatable, best first. Left out, the recogniser
        /// works out the language itself; naming the wrong one does not
        /// fail, it misreads.
        #[arg(long = "ocr-locale")]
        ocr_locale: Vec<String>,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
        /// Write a frame to this path the moment the tap returns.
        ///
        /// For UI that does not wait: a control bar that hides itself
        /// after a few seconds outlives neither a second command nor the
        /// turn between two tool calls. One call takes the picture from
        /// the same process that tapped — measured at about 88 ms after
        /// the tap returns, against roughly 325 ms going out to device
        /// tooling.
        ///
        /// A tap that fails writes nothing: a frame taken after a tap
        /// that did not land looks like evidence and is a picture of the
        /// screen nothing happened on.
        #[arg(long = "then-screenshot", value_name = "OUT")]
        then_screenshot: Option<PathBuf>,
    },
    /// Boolean existence probe (POST /find). Prints `exists=<bool>`.
    /// Same selector shorthand as `smix tap`.
    Find {
        selector: String,
        /// Which language to read an `ocrText:` selector in — `zh-Hans`,
        /// `ja`, `en`. Repeatable, best first. Left out, the recogniser
        /// works out the language itself; naming the wrong one does not
        /// fail, it misreads.
        #[arg(long = "ocr-locale")]
        ocr_locale: Vec<String>,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Poll `/find` every 250ms until the selector resolves or
    /// `--timeout` expires. Mirrors SDK `App::wait_for` semantics; useful in
    /// shell loops driving the runner from outside Rust.
    WaitFor {
        selector: String,
        /// Timeout in seconds (default 5).
        #[arg(long, default_value_t = 5)]
        timeout: u64,
        /// Which language to read an `ocrText:` selector in — `zh-Hans`,
        /// `ja`, `en`. Repeatable, best first. Left out, the recogniser
        /// works out the language itself; naming the wrong one does not
        /// fail, it misreads.
        #[arg(long = "ocr-locale")]
        ocr_locale: Vec<String>,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
        /// Wait until the element is gone, instead of until it appears.
        ///
        /// Mirrors `smix_assert_not_visible`, which the MCP server has
        /// and the CLI did not — so a flow could wait for a spinner to
        /// show and not for it to disappear.
        #[arg(long)]
        absent: bool,
    },
    /// Type text into the matched field. Equivalent to the flow yaml
    /// `inputText:` verb. Selector shorthand same as `smix tap`.
    Fill {
        selector: String,
        #[arg(long)]
        text: String,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Issue a hardware / IME key press. Key shorthand: `return`
    /// (alias `enter`), `delete` (alias `backspace`), `tab`, `space`,
    /// `escape` / `esc`, `arrowUp` / `up`, `arrowDown` / `down`,
    /// `arrowLeft` / `left`, `arrowRight` / `right`, `home`, `lock`,
    /// `volumeUp` / `volume-up`, `volumeDown` / `volume-down`.
    PressKey {
        /// KeyName shorthand (see help text).
        key: String,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Swipe once through the content. `direction` names what you
    /// want to see — `down` reveals what is below — not which way the
    /// finger moves. Direction: `up` / `down` / `left` / `right`.
    ///
    /// Mirrors `smix_swipe`, which the MCP server has had since it
    /// existed while the CLI had nothing. Use `scroll` when you want to
    /// stop at an element; this is the one-gesture form.
    Swipe {
        /// `up` / `down` / `left` / `right`.
        direction: String,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Scroll until the selector becomes visible. Direction:
    /// `up` / `down` / `left` / `right`.
    Scroll {
        selector: String,
        #[arg(long)]
        direction: String,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Dismiss the soft keyboard if visible.
    HideKeyboard {
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Print the runner's current a11y tree. `--json` emits
    /// wire JSON; default emits an indented text outline.
    Tree {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
        /// Print the software keyboard's keys.
        ///
        /// They are collapsed by default: a key per letter plus
        /// `Next keyboard`, `Dictate`, shift and delete is around sixty
        /// nodes that are the same sixty on every screen of every app,
        /// and this output is read by an AI paying for each one. The
        /// keyboard node itself always prints, with the number of keys
        /// it holds — so nothing disappears without saying so.
        #[arg(long)]
        keyboard: bool,
    },
    /// Print the runner's high-level ScreenDescription: the visible
    /// interactive elements aggregated from the current a11y tree.
    Describe {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Print the runner's current SpringBoard system-popup list.
    SystemPopups {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Press a button on a SpringBoard system popup. Both ids come from
    /// `smix system-popups` output (popup `id` + one of its buttons'
    /// `id`). Errors when the popup or button no longer exists.
    SystemPopupAction {
        popup_id: String,
        button_id: String,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Sequential script driver. Reads a yaml file describing ordered
    /// smix subcommand invocations (see `crates/smix-cli/src/script.rs`
    /// for the schema). Lightweight shell-friendly alternative to
    /// chaining `smix tap … && smix fill …`. smix-native dialect — NOT
    /// the maestro yaml flow format (for that, use `smix run`).
    RunScript {
        /// Path to the script yaml file.
        path: PathBuf,
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Run a flow file end-to-end. smix flows are written in a yaml
    /// dialect we share with maestro (so existing flows are reusable),
    /// extended with smix-native selectors (ocr / anchor-relative /
    /// fallback) and cross-platform `app:` resolver.
    ///
    /// The runner (`smix capsule up`) must be up first.
    #[command(long_about = "\
Run a flow file end-to-end on the connected sim/emulator.

A smix flow is a yaml document with two parts: a header (app id / logical \
key) and an ordered list of steps. smix accepts the maestro yaml format \
(40 verbs: assertVisible, tapOn, inputText, scroll, runFlow, ...) plus \
smix-native extensions (ocrText / anchorRelative / fallback selectors, \
cross-platform `app:` resolver via smix-apps.yaml).

Prerequisites:
  1. Sim / emulator booted with a known device id (registry alias or UDID)
  2. Runner up (`smix capsule up <DEVICE>`)
  3. App installed + (optionally) launched

Common invocations:
  # iOS (capsule default port 22087)
  smix run --device ios-17 flow.yaml

  # Android (Kotlin runner on adb-forwarded :28080)
  smix run --device emulator-5554 --platform android \\
      --apps-config smix-apps.yaml --runner-port 28080 flow.yaml

  # Skip auto-foreground (app already on screen)
  smix run --device <DEVICE> --no-launch flow.yaml

Exit codes:
  0  success
  2  yaml parse error
  3  runtime SDK failure (sim / app problem mid-flow)
  4  unknown verb / direction
  5  runFlow cycle / file IO
  6  runner unreachable (capsule not up / wrong port)

Documentation: docs/AI_GUIDE.md
")]
    Run {
        /// Path(s) to flow yaml file(s). One or more files can be
        /// listed; the runner is up'd once and reused across all
        /// flows. Per-flow debug-output subdirectory when
        /// `--debug-output` is set (`<dir>/<flow-basename>/step-*.json`).
        /// Exit code = max(per-flow codes). `--fail-fast` aborts the
        /// batch on the first failure.
        #[arg(required = true, num_args = 1..)]
        flows: Vec<PathBuf>,
        /// Device id — registry alias (preferred) or raw UDID. smix is
        /// strict about explicit device id: there is no `--device booted`
        /// fallback. Same `<DEVICE>` form used by `smix sim ...` /
        /// `smix capsule ...`.
        #[arg(long, env = "SMIX_UDID")]
        device: Option<String>,
        /// Additional sims for `--parallel`. Repeatable. The flows are
        /// sharded round-robin across `--device` plus every
        /// `--also-device`; each shard runs as its own single-sim
        /// `smix run`, so a shard is the ordinary sequential path pinned
        /// to one sim. Ignored when `--parallel` is 1.
        #[arg(long = "also-device")]
        also_device: Vec<String>,
        /// Run up to N sims concurrently, sharding the listed flows
        /// across `--device` + `--also-device`. Default 1 = the
        /// single-sim path, byte-identical. Capped at the number of sims
        /// given.
        #[arg(long, default_value_t = 1)]
        parallel: usize,
        /// Distributed run: shard the flows across the nodes in a roster
        /// yaml (each node runs its own simulators; results merge into one
        /// JSON report on stdout, exit = worst of nodes).
        #[arg(long, conflicts_with_all = ["device", "also_device", "parallel"])]
        nodes: Option<PathBuf>,
        /// Bundle id / Android package for `App::foreground` (skipped
        /// with --no-launch). Overridden by `appId:` / `app:` in the
        /// yaml header.
        #[arg(long)]
        bundle_id: Option<String>,
        /// Runner port. iOS default 22087, Android 28080 by convention.
        #[arg(long, env = "SMIX_RUNNER_PORT")]
        runner_port: Option<u16>,
        /// Skip the initial foreground call. Use when the app is
        /// already on screen (e.g. launched via `smix sim launch` or
        /// `adb shell am start`). Saves 3-5s cold-start latency.
        #[arg(long, default_value_t = false)]
        no_launch: bool,
        /// Run with the device's own animation settings instead of
        /// quietening them.
        ///
        /// By default a run asks the device to stop animating first:
        /// Android's three animation scales go to zero, iOS gets Reduce
        /// Motion (XCUITest cannot reach the app's own animation flag,
        /// so that is as far as it goes). Both are read back and the
        /// run refuses if they did not take. Pass this when motion is
        /// the subject — recording a demo, or an `assertScreenshot`
        /// baseline whose frames include a transition.
        #[arg(long, default_value_t = false)]
        animations: bool,
        /// Target platform.
        #[arg(long, value_enum, env = "SMIX_PLATFORM", default_value_t = RunPlatform::Ios)]
        platform: RunPlatform,
        /// Path to `smix-apps.yaml` cross-platform app resolver config.
        /// When the yaml header uses `app: <logicalKey>`, this resolver
        /// maps to platform-specific bundle id / Android package.
        #[arg(long, env = "SMIX_APPS_CONFIG")]
        apps_config: Option<PathBuf>,
        /// Env var for yaml `${NAME}` interpolation. Repeatable:
        /// `--env A=1 --env B=2`. Wins over inherited process env
        /// (which is the fallback). Matches maestro `test -e KEY=VAL`
        /// semantics. VALUE may contain `=`.
        #[arg(long = "env", value_parser = parse_kv_pair, action = clap::ArgAction::Append)]
        env: Vec<(String, String)>,
        /// Directory for debug artifacts. Currently writes
        /// `<dir>/run-summary.json` at exit. Per-step files + on-fail
        /// screenshots ship in a follow-up.
        #[arg(long = "debug-output")]
        debug_output: Option<PathBuf>,
        /// Verbose logging (debug-level tracing on adapter/sdk/driver
        /// crates).
        #[arg(long, default_value_t = false)]
        verbose: bool,
        /// Output format. `human` (default): unchanged. `json`: emits a
        /// single top-level JSON object on stdout at exit summarizing
        /// the run + any terminal ExpectationFailure.
        #[arg(long, value_enum, default_value_t = RunOutputFormat::Human)]
        format: RunOutputFormat,
        /// Send `App-Activate: true` header on every runner request so
        /// the iOS runner calls `.activate()` on the resolved target
        /// before each operation. Auto-recovers from cases where a
        /// briefly-foregrounded other app (Preferences / an OS preview)
        /// latched XCUITest's implicit app-under-test to the wrong
        /// bundle. Costs ~50-100ms per request; opt-in.
        #[arg(long, default_value_t = false)]
        activate: bool,
        /// Batch semantics. Default: run all listed flows sequentially,
        /// exit code = max(per-flow codes). `--fail-fast`: abort the
        /// batch after the first flow that exits non-zero.
        #[arg(long, default_value_t = false)]
        fail_fast: bool,
        /// Per-flow retry count. Default 1 = one attempt only.
        /// `--retry 2` = up to 2 attempts per flow; if the first fails
        /// and the second succeeds, the flow's exit code is that of the
        /// second.
        /// Each attempt is recorded in `~/.local/share/smix/flow-attempts.json`
        /// with status + errorClass + wallMs + any `.ips` that
        /// appeared during the attempt (attribution vs whole-batch).
        /// `smix diagnostic dump` reads that file and surfaces the
        /// attribution table under a `recent flows` section.
        #[arg(long = "retry", default_value_t = 1)]
        retry: u32,
        /// Append an implicit `expect.signal { regex }` step to the end
        /// of each flow. The `--timeout` value is used as the timeout
        /// (default 8000ms).
        #[arg(long = "await-signal")]
        await_signal: Option<String>,
        /// Prepend an implicit `expect.signal { regex, timeoutMs }`
        /// step at the START of the flow, blocking until the regex is
        /// observed in the metro log tail. Symmetric to
        /// `--await-signal`. Requires `--metro-log-url` also set.
        /// Useful when a visual/perf gate prelaunches the app and must
        /// wait for a bootstrap-ready signal before the flow starts.
        #[arg(long = "gate-signal")]
        gate_signal: Option<String>,
        /// Timeout in ms for `--gate-signal`. Default 60000. Zero
        /// disables the timeout (waits forever).
        #[arg(long = "gate-signal-timeout", default_value_t = 60_000)]
        gate_signal_timeout_ms: u64,
        /// Append an implicit `expectLogClean` step to the end of each
        /// flow. Emits an ExpectationFailure if any non-allowlisted log
        /// entry has been observed during the run (allowlist from
        /// `.smix/config.yaml` `metroLog.allowlist`).
        #[arg(long = "expect-log-clean", default_value_t = false)]
        expect_log_clean: bool,
        /// Metro log source URL, overrides `.smix/config.yaml`
        /// `metroLog.url`. Format: `ws://127.0.0.1:8081/logs` for
        /// expo/metro WebSocket, or `file:///path/to/log` for on-disk
        /// tail fallback.
        #[arg(long = "metro-log-url")]
        metro_log_url: Option<String>,
        /// Path to a fixture registry JSON file. Enables the
        /// `- fixture: <id>` yaml verb.
        #[arg(long = "fixture-registry")]
        fixture_registry: Option<PathBuf>,
        /// Type into whatever holds focus, skipping a11y-focus
        /// resolution, for `inputText` / `fill`. For RN apps whose
        /// hidden `<TextInput>` the a11y tree cannot address — the
        /// case where the default path finds nothing to tap.
        #[arg(long = "force-key-events", default_value_t = false)]
        force_key_events: bool,
        /// Disable auto-annotate on `--debug-output` fail-PNG (default:
        /// annotate with a red circle + step summary text label at the
        /// top of the screenshot). Use when downstream tooling expects
        /// raw screenshot pixels.
        #[arg(long = "no-fail-annotate", default_value_t = false)]
        no_fail_annotate: bool,
        /// Parse-only gate. Reads every listed flow yaml, resolves any
        /// `runFlow:` includes, and reports parse / include errors.
        /// Does not connect to a runner, does not need a simulator, and
        /// does not execute any step. Exit 0 on clean parse across
        /// every flow; non-zero on the first error, listing all
        /// remaining flows unparsed. Suitable for CI pre-flight.
        ///
        /// Accepts `--dry-run` as an equivalent alias (idiomatic in
        /// most CLI tools).
        #[arg(long = "check", alias = "dry-run", default_value_t = false)]
        check: bool,
    },
    /// Static maestro → smix yaml codemod. Renames verbs to smix
    /// canonical form (tapOn → tap, extendedWaitUntil → expect +
    /// timeoutMs, retry.max → retry.maxRetries, etc.) and strips
    /// deprecated arg forms. Unknown verbs are preserved verbatim with
    /// a WARN line to stderr.
    ///
    /// Modes:
    ///   smix migrate                        — read stdin, write stdout
    ///   smix migrate flow.yaml              — read file, write stdout
    ///   smix migrate --in-place a.yaml ...  — rewrite files in place
    ///
    /// Comments, copyright headers, and blank lines survive the
    /// rewrite byte-identical (the codemod is line-based; only the
    /// verb and argument-key portions of step lines are modified).
    Migrate {
        /// One or more input yaml paths. When empty, reads from stdin.
        #[arg(num_args = 0..)]
        paths: Vec<PathBuf>,
        /// Rewrite each input file in place. A parse failure on any
        /// one file leaves that file untouched; other files still get
        /// rewritten. Overall exit != 0 if any file failed. Not
        /// allowed when reading from stdin.
        #[arg(long, default_value_t = false)]
        in_place: bool,
    },
    /// Annotate a PNG with circle / arrow / text / box / line
    /// primitives. Mini-DSL per annotation:
    ///
    ///   kind ',' key:value (',' key:value)*
    ///
    /// Examples:
    ///   smix annotate in.png out.png \\
    ///     --annotate "circle,at:100_100,color:red,radius:40" \\
    ///     --annotate "arrow,from:10_10,to:200_200,color:blue" \\
    ///     --annotate "text,at:50_50,content:hello,color:green,size:24"
    ///     --font /path/to/font.ttf
    Annotate {
        /// Input PNG path.
        input: PathBuf,
        /// Output PNG path.
        output: PathBuf,
        /// One or more annotation specs (see mini-DSL above).
        #[arg(long = "annotate", num_args = 1..)]
        annotations: Vec<String>,
        /// PNG compression preset: `fast`, `balanced` (default),
        /// `aggressive`.
        #[arg(long, default_value = "balanced")]
        compression: String,
        /// TTF font path (required for text annotations).
        #[arg(long)]
        font: Option<PathBuf>,
    },
    /// Authoring subcommands. Compose yaml against a live sim:
    /// suggest selectors matching a partial spec, capture or diff
    /// a11y tree baselines for visual gates.
    Authoring {
        #[command(subcommand)]
        action: AuthoringAction,
    },
}

#[derive(Subcommand, Debug)]
enum AuthoringAction {
    /// Generate a flow (maestro yaml or rust test) from recorded IRAction
    /// JSON — the record -> generate glue. The input is the JSON any capture
    /// leg produces (iOS / Android / web), so a recording from any platform
    /// becomes a flow through the one platform-neutral generator.
    Generate {
        /// Recorded IRAction JSON file (a `Vec<IRAction>`).
        input: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = authoring::GenFormat::Maestro)]
        format: authoring::GenFormat,
        /// Output flow file.
        #[arg(long, short)]
        output: PathBuf,
        /// App / bundle id embedded in the generated flow.
        #[arg(long, default_value = "com.example")]
        app_id: String,
        /// Test fn name (rust format only).
        #[arg(long, default_value = "recorded")]
        test_fn_name: String,
    },
    /// Record a live session on a runner and generate a flow. Records for
    /// `--duration` seconds while you drive the app, then generates from the
    /// captured IRAction. Android today (its runner emits IRAction directly).
    TapRecord {
        /// Output flow file.
        #[arg(long, short)]
        output: PathBuf,
        /// Seconds to record while you interact.
        #[arg(long, default_value_t = 10)]
        duration: u64,
        /// Output format.
        #[arg(long, value_enum, default_value_t = authoring::GenFormat::Maestro)]
        format: authoring::GenFormat,
        /// Runner HTTP port (default: the device's registered port, else
        /// 28080 — the Android runner's default).
        #[arg(long)]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port that
        /// device is registered on.
        #[arg(long)]
        device: Option<String>,
        /// App / bundle id embedded in the generated flow.
        #[arg(long, default_value = "com.example")]
        app_id: String,
        /// Test fn name (rust format only).
        #[arg(long, default_value = "recorded")]
        test_fn_name: String,
    },
    /// Suggest selectors matching a partial spec against the current
    /// sim state. Runs against a live runner on `--port`. Examples:
    ///   smix authoring suggest 'id: qa-*'
    ///   smix authoring suggest 'Sign In'
    Suggest {
        /// Partial selector spec.
        partial: String,
        /// Runner HTTP port. Defaults to SMIX_RUNNER_PORT env or 22087.
        #[arg(long, env = "SMIX_RUNNER_PORT")]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Capture the current a11y tree JSON to a file for baseline use.
    CaptureTree {
        /// Output path for the JSON baseline.
        output: PathBuf,
        /// Runner HTTP port.
        #[arg(long, env = "SMIX_RUNNER_PORT")]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Diff the current sim a11y tree against a baseline JSON file
    /// and report structural differences. Exit code 0 = clean,
    /// exit code 2 = diff found.
    DiffTree {
        /// Baseline a11y tree JSON path.
        baseline: PathBuf,
        /// Runner HTTP port.
        #[arg(long, env = "SMIX_RUNNER_PORT")]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
    },
    /// Read a failed flow's on-disk bundle, ask a local `claude` to
    /// propose edits, and write the amended flow. Device-free: consumes
    /// a bundle already on disk (produce it with `smix run
    /// --debug-output <dir> --format json > <dir>/failure.json`); this
    /// subcommand does not run the flow itself.
    Propose {
        /// The failed flow yaml.
        flow: PathBuf,
        /// The on-disk bundle dir (run-summary.json + failure.json + …).
        #[arg(long)]
        bundle: PathBuf,
        /// Output path for the amended flow yaml.
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Session recording. Sample the a11y tree at `--interval-ms`
    /// for `--duration-secs`; write a yaml scaffold with assertVisible
    /// steps for stable-visible IDs.
    Record {
        /// Output yaml scaffold path.
        output: PathBuf,
        /// Total recording duration in seconds. Default 10.
        #[arg(long, default_value_t = 10)]
        duration_secs: u64,
        /// Sampling interval in milliseconds. Default 500.
        #[arg(long, default_value_t = 500)]
        interval_ms: u64,
        /// Runner HTTP port.
        #[arg(long, env = "SMIX_RUNNER_PORT")]
        port: Option<u16>,
        /// Device UDID, or an alias / deviceName in the workspace's
        /// `.smix` registry. Used here only to find the runner port
        /// that device is registered on — it does not change which
        /// simulator or app the call is dispatched to, because the
        /// port already names the runner.
        #[arg(long)]
        device: Option<String>,
        /// Bundle id to write into the recorded flow. Without it the
        /// scaffold names a placeholder, and a flow naming an app that
        /// does not exist fails at its first step — so a recording could
        /// not be run back without an edit.
        #[arg(long)]
        app_id: Option<String>,
    },
}

/// Output-format enum mirroring [`smix_adapter_maestro::OutputFormat`].
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum RunOutputFormat {
    Human,
    Json,
    /// JUnit XML output for CI test-report pipelines.
    Junit,
}

impl RunOutputFormat {
    fn to_adapter(self) -> smix_adapter_maestro::OutputFormat {
        match self {
            Self::Human => smix_adapter_maestro::OutputFormat::Human,
            Self::Json => smix_adapter_maestro::OutputFormat::Json,
            Self::Junit => smix_adapter_maestro::OutputFormat::Junit,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum RunPlatform {
    Ios,
    Android,
}

impl RunPlatform {
    fn to_flow(self) -> smix_adapter_maestro::FlowPlatform {
        match self {
            Self::Ios => smix_adapter_maestro::FlowPlatform::Ios,
            Self::Android => smix_adapter_maestro::FlowPlatform::Android,
        }
    }
}

#[derive(Subcommand, Debug)]
enum CapsuleAction {
    /// Bring up sim + start capture + start runner in record mode.
    Up {
        device: String,
        /// Bundle id the runner binds to. Required — `capsule up` runs
        /// `runner up`, which refuses to start without a target bundle,
        /// so a capsule without this flag could never complete.
        #[arg(long)]
        bundle: String,
        /// Foreground the app instead of relaunching it.
        ///
        /// Bringing the capsule up restarts the target app, dropping
        /// whatever screen had been navigated to — and the next flow
        /// then fails with `ELEMENT_NOT_FOUND` against a splash
        /// screen. Same meaning as `smix run --no-launch`, which has
        /// had it for longer.
        #[arg(long = "no-launch", default_value_t = false)]
        no_launch: bool,
        /// Fail instead of degrading when Simulator.app is on screen.
        ///
        /// For CI, where a window means something is wrong. On a dev
        /// machine `expo run:ios` opens Simulator.app by design, so a
        /// capsule there degrades to soft with a warning rather than
        /// refusing — a condition that is normal for a whole class of
        /// users reads as an error only once before it reads as noise.
        #[arg(long, default_value_t = false)]
        require_hard: bool,
        /// Allow the "soft capsule" fallback when the Simulator UI is
        /// open (otherwise the guard rejects the boot to avoid
        /// contention with a user-visible Simulator session).
        #[arg(long)]
        soft: bool,
        /// Skip the `/api/capture/start` request that starts the HLS
        /// capture pipeline. Set this when the flow itself invokes
        /// `simctl io recordVideo` so the two do not contend for the
        /// "Host recording is already in progress" mutex.
        #[arg(long)]
        no_capture: bool,
    },
    /// Reverse teardown: runner down + capture stop + sim shutdown.
    Down { device: String },
}

#[derive(Subcommand, Debug)]
enum RecordAction {
    /// Start recording to a file.
    Start {
        device: String,
        /// Where to write the mp4.
        #[arg(long)]
        output: PathBuf,
    },
    /// Stop the recording, letting it write its trailer.
    Stop { device: String },
    /// Whether this device is recording, and where it is writing.
    Status { device: String },
}

#[derive(Subcommand, Debug)]
enum LeaseAction {
    /// Every device with a ledger, and whether its holder is still there.
    List,
    /// One device: the holder, what is open, and the verdict.
    Status { device: String },
    /// Close what a dead holder left open, by the graceful path it never
    /// got to take. A live holder is reported, never preempted.
    Reconcile { device: String },
    /// Who booted this device, in one line and an exit code.
    ///
    /// `0` means a ledger says smix booted it, and names the session;
    /// `3` means no ledger says that — somebody else turned it on, or
    /// nobody wrote it down; `1` means the question could not be asked.
    /// Three codes rather than two: a check that answers "safe" when it
    /// means "I do not know" is the shape this cycle keeps finding.
    ///
    /// Not a teardown permission. It answers "did smix boot this",
    /// which is not "did *you* boot this" — a shell script's `smix sim
    /// boot` exits immediately, so the session that booted the device
    /// is never the one asking. A script that shut down whatever this
    /// reported as smix's would take away a device another run is
    /// using. What it is for is the question that could not be answered
    /// on 2026-08-11: a runner was found holding port 22087, the rule
    /// said find its owner before touching it, and the owner was
    /// recorded in a tree nobody was standing in.
    Owner { device: String },
    /// Fold per-checkout ledgers into this machine's.
    ///
    /// Ledgers used to live in whichever `.smix/` was above the working
    /// directory. Adds and never removes; running it twice does nothing
    /// the second time.
    Migrate {
        /// A checkout (or its `.smix/leases`) to read. Repeatable.
        /// Defaults to the one above the working directory.
        #[arg(long = "from", value_name = "DIR")]
        from: Vec<PathBuf>,
        /// Say what would move and move nothing.
        ///
        /// The same decision path as the real thing, split only at the
        /// write — a rehearsal that works the answer out for itself is
        /// answering about itself.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete ledgers that no longer describe anything.
    ///
    /// A ledger that can only be added to stops describing the machine:
    /// a holder that died without releasing, or a boot row for a device
    /// that is off, sits there for ever and every later command has to
    /// reason around it. Says what it deleted and why; keeps everything
    /// else and says why too.
    Prune {
        /// Report what would go, and delete nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
enum RunnerAction {
    /// Start the runner on a device; blocks until /health answers.
    Up {
        /// The device to bring the runner up on.
        ///
        /// `--device` names the same thing and is accepted, because
        /// `runner down` takes it that way. One pair of commands with
        /// two shapes for the same argument is a trip somebody takes
        /// every time — and the guards and guides that suggested the
        /// flag form were, until now, suggesting something this command
        /// answered with "a similar argument exists: '--supervise'".
        #[arg(value_name = "DEVICE", required_unless_present = "device_flag")]
        device: Option<String>,
        /// The device, named rather than positional. See `device`.
        #[arg(long = "device", conflicts_with = "device")]
        device_flag: Option<String>,
        /// Which runner to bring up. `ios` drives xcodebuild + the
        /// XCUITest runner; `android` installs the instrumentation APK,
        /// forwards the port, and `am instrument`s the Kotlin runner.
        #[arg(long, value_enum, default_value_t = RunPlatform::Ios)]
        platform: RunPlatform,
        /// Bundle id the runner binds its XCUIApplication to. iOS only,
        /// and required there: `runner up` refuses to start without one
        /// (the help used to claim a com.apple.Preferences default that
        /// the implementation rejects). On Android it is refused rather
        /// than ignored — that runner takes its target from the
        /// App-Bundle-Id header per request, not at startup.
        #[arg(long)]
        bundle: Option<String>,
        /// Explicit path to `SmixRunner.xcodeproj`. Wins over
        /// `$SMIX_RUNNER_PROJECT` env and the install-shipped default
        /// at `~/.local/share/smix/runner/`. See resolve_runner_project
        /// cascade in runner.rs.
        #[arg(long = "runner-project", env = "SMIX_RUNNER_PROJECT")]
        runner_project: Option<PathBuf>,
        /// Bind the runner to an explicit port. Priority (high → low):
        /// this flag → `.smix/sims.json` `runnerPort` field →
        /// `SMIX_RUNNER_PORT` env → 22087 default. Two sims with
        /// distinct `runnerPort` in sims.json can run their own runner
        /// concurrently without collision.
        #[arg(long = "runner-port", env = "SMIX_RUNNER_PORT")]
        runner_port: Option<u16>,
        /// After `/health` returns 200, spawn a detached
        /// `smix runner supervise` sidecar and record its pid in
        /// `.smix/runner/state.json`. `smix runner down` cascades a
        /// SIGTERM to the sidecar before tearing down xcodebuild.
        /// Sidecar log at `.smix/runner/supervise-<UDID>.log`.
        #[arg(long = "supervise", default_value_t = false)]
        supervise: bool,
        /// Apple developer team id, for a physical device.
        ///
        /// Discovered from this machine's signing identities when
        /// omitted. Needed only when several teams could sign, which is
        /// a question smix refuses to answer for you.
        #[arg(long)]
        team: Option<String>,
        /// Keep the app where it is: foreground it instead of
        /// relaunching it.
        ///
        /// **Reach for this when the runner died mid-session and you
        /// are several screens deep.** Bringing the runner back up
        /// restarts the target app by default, so the navigation is
        /// gone and the next step fails against a splash screen —
        /// somebody re-walked three or four screens a dozen times in an
        /// afternoon before finding this flag, because the text here
        /// described the problem without saying it was also the answer.
        ///
        /// Same meaning as `smix run --no-launch`, which has had it for
        /// longer.
        #[arg(long = "no-launch", default_value_t = false)]
        no_launch: bool,
        /// Cycle a runner of ours whose session has stopped working,
        /// instead of reporting it as already up.
        ///
        /// `/health` says the runner's HTTP server is answering; it
        /// cannot see the app binding, so reinstalling the app leaves a
        /// runner that answers 200 and drives nothing — and this command,
        /// reading only that, used to say "already up" and return
        /// success. Without this flag it now says no and names the fix;
        /// with it, it runs the fix: the same in-place cycle as
        /// `smix runner cycle`, seconds, no xcodebuild restart.
        ///
        /// It does not reach across to a runner recorded for another
        /// device, or to one the store has no record of. Those are
        /// refused with or without it — `runner down --include-unrecorded`
        /// is the sanctioned way through, and it is a separate decision
        /// on purpose.
        #[arg(long = "force", default_value_t = false)]
        force: bool,
    },
    /// Stop the runner (SIGINT-first to avoid the crash-report dialog).
    Down {
        /// Which runner to stop. `android` needs `--device` too: adb
        /// commands must name their device, or they act on whichever
        /// one happens to be attached.
        #[arg(long, value_enum, default_value_t = RunPlatform::Ios)]
        platform: RunPlatform,
        /// Android only: the adb serial (e.g. `emulator-5554`).
        #[arg(long)]
        device: Option<String>,
        /// Also stop a runner on this port that this workspace has no
        /// record of.
        ///
        /// Off by default: an unrecorded runner may belong to another
        /// session, and ending one by accident is how a sweep once took
        /// out somebody else's work. `runner up` refuses such a port for
        /// the same reason — this flag is the sanctioned way through,
        /// and it is a sentence somebody says rather than something that
        /// happens.
        #[arg(long = "include-unrecorded", default_value_t = false)]
        include_unrecorded: bool,
        /// Which runner's port, when it is not the default.
        ///
        /// `runner up` has taken this flag all along and `down` did
        /// not, reading `SMIX_RUNNER_PORT` instead. So the obvious
        /// pairing — up with `--runner-port N`, down with the same —
        /// failed the argument parse, and a teardown written that way
        /// left the runner running. Ours did, in the e2e written to
        /// prove this very release: the check passed, the process
        /// outlived it, and `|| true` on the cleanup line meant nothing
        /// said so.
        #[arg(long)]
        runner_port: Option<u16>,
    },
    /// Hold a port forward open to a physical device. Runs in the
    /// foreground until killed.
    ///
    /// `runner up` spawns this for a physical device; it is documented
    /// because a process that appears in `ps` and in the device ledger
    /// should be findable, not a mystery.
    Forward {
        /// Device UDID.
        device: String,
        /// Port, the same on both sides.
        #[arg(long, default_value_t = 22087)]
        port: u16,
    },
    /// Remove the Android instrumentation package `runner up` installed.
    ///
    /// Android only: the iOS runner is not installed as a standalone
    /// package, it is launched by xcodebuild and leaves nothing behind.
    Uninstall {
        /// Which platform. Only `android` is meaningful here.
        #[arg(long, value_enum, default_value_t = RunPlatform::Android)]
        platform: RunPlatform,
        /// Device serial. Required — an unpinned uninstall would reach
        /// whatever is attached.
        #[arg(long)]
        device: String,
    },
    /// Cycle the runner: down + up on the same device/port/bundle.
    /// Preserves the per-udid derived-data directory so the warm re-up
    /// finishes in ~3 s. Errors if no runner state.json exists — use
    /// `runner up` for a cold start.
    Cycle {
        /// Explicit path to `SmixRunner.xcodeproj`. Same cascade as
        /// `runner up` — see `resolve_runner_project`.
        #[arg(long = "runner-project", env = "SMIX_RUNNER_PROJECT")]
        runner_project: Option<PathBuf>,
    },
    /// Attach a supervisor to a running runner: tail its log and
    /// auto-`cycle` on interrupt patterns (`** TEST INTERRUPTED **` /
    /// `SchemeActionResultOperation started unexpectedly`). Foreground
    /// process; SIGINT or SIGTERM cleanly exits. Session persistence
    /// preserves client session ids across each cycle.
    Supervise {
        /// Explicit path to `SmixRunner.xcodeproj` for the cycle
        /// operation. Same cascade as `runner up`.
        #[arg(long = "runner-project", env = "SMIX_RUNNER_PROJECT")]
        runner_project: Option<PathBuf>,
    },
    /// List every session the runner currently tracks.
    /// Reads `POST /session/list`. Useful for post-cycle diagnostics.
    ListSessions,
    /// Every smix runner on this machine: its port, its device, and
    /// whether the ledgers know about it.
    ///
    /// Not `list-sessions`, which asks one runner what app sessions it
    /// has open. This asks the machine what runners it has at all —
    /// reading the ledgers and the listening sockets and putting them
    /// side by side, because a runner nobody wrote down is exactly the
    /// one you cannot decide about.
    ///
    /// Reads only. It never signals a process or writes a ledger, and
    /// always exits 0: a command meant to be run before touching
    /// anything has to be safe to run.
    List,
    /// Extract the CLI's embedded Swift runner sources
    /// into `~/.local/share/smix/runner/`. Normally auto-invoked by
    /// `smix runner up` when the on-disk `.smix-runner-version` file
    /// is missing or does not match the CLI version; this verb makes
    /// the operation explicit for troubleshooting or first-time setup
    /// on an air-gapped machine. Backs up any pre-existing runner tree
    /// to `~/.local/share/smix/runner.bak-<ts>/` before writing.
    Install {
        /// Destination directory. Defaults to
        /// `$XDG_DATA_HOME/smix/runner/` (falling back to
        /// `~/.local/share/smix/runner/`).
        #[arg(long)]
        path: Option<PathBuf>,
        /// Extract even when the version file already matches the CLI
        /// version. Useful when the on-disk tree has been manually
        /// edited and you want a clean baseline.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
enum SimAction {
    /// List available simulators (Rust port: `xcrun simctl list devices -j`).
    List {
        /// Output as JSON instead of human-readable table.
        #[arg(long)]
        json: bool,
        /// List the devices smix has recorded, with their aliases,
        /// instead of the ones the platform reports.
        ///
        /// These are two different questions and nothing answered the
        /// second one. `smix sim list` asks simctl and adb, so it
        /// returns the same thing from every checkout whether or not
        /// device records are shared — which made it useless as
        /// evidence that they are.
        #[arg(long)]
        registered: bool,
    },
    /// Print the UDID a device ref resolves to.
    Resolve {
        device: String,
    },
    /// Record a device under an alias, creating the registry when
    /// absent. This is the bootstrap: alias-form device refs fail on a
    /// fresh checkout until a registry exists, and a physical device
    /// cannot be addressed at all until it is registered.
    ///
    /// A virtual device is checked against the catalogue its own
    /// platform keeps — `--kind simulator` (the default) against
    /// `simctl`, `--kind emulator` against `adb` — and its name and
    /// runtime are read from there, so only the UDID and alias are
    /// needed. A physical device is taken as given: nothing here can
    /// enumerate the world's phones, which is why registering one is a
    /// deliberate act rather than a lookup.
    Register {
        alias: String,
        #[arg(long)]
        udid: String,
        /// BCP 47 locale to enforce at boot (e.g. `ja-JP`). Optional.
        #[arg(long)]
        locale: Option<String>,
        /// Dedicated runner port for this sim. Optional.
        #[arg(long = "runner-port")]
        runner_port: Option<u16>,
        /// What kind of device this is.
        ///
        /// Defaults to a simulator, which is the case that must keep
        /// working without anyone learning a new flag. A physical value
        /// changes two things: the identifier is taken as given rather
        /// than looked up in simctl (which lists no phones), and
        /// destructive actions are refused on it until
        /// `smix sim allow-destructive` is run once.
        #[arg(long, value_enum, default_value_t = DeviceKindArg::Simulator)]
        kind: DeviceKindArg,
        /// Human-readable name for a physical device. Ignored for
        /// simulators, whose name comes from simctl.
        #[arg(long)]
        name: Option<String>,
    },
    /// Boot a simulator.
    Boot {
        device: String,
    },
    /// Shutdown a simulator.
    Shutdown {
        device: String,
    },
    /// Erase a simulator's data.
    Erase {
        device: String,
    },
    /// Take a screenshot (PNG). Pass `-` to write raw PNG to stdout.
    Screenshot {
        device: String,
        out: PathBuf,
    },
    /// Launch an app by bundle id; prints the pid. Accepts repeatable
    /// `--child-env KEY=VAL` flags to inject `SIMCTL_CHILD_KEY=VAL` envp
    /// onto the simctl process — the launched app reads it back via
    /// `ProcessInfo().environment["KEY"]`. Used to prelaunch an app
    /// before any `openLink` so iOS treats the URL as in-app routing
    /// (sidesteps the SpringBoard "Open in '`<App>`'?" dialog).
    Launch {
        device: String,
        bundle_id: String,
        /// `--child-env KEY=VAL` (repeatable). KEY is the bare name the
        /// app reads; the `SIMCTL_CHILD_` prefix is added automatically.
        /// Already-prefixed keys pass through unchanged.
        #[arg(long = "child-env", value_parser = parse_kv_pair, action = clap::ArgAction::Append)]
        child_env: Vec<(String, String)>,
        /// Process-level launch arguments forwarded after a `--`
        /// separator to `xcrun simctl launch ... -- <args>`. Mirrors
        /// maestro yaml `launchApp.arguments`. Conventionally an
        /// alternating `-key value` shape, but treated as opaque argv.
        #[arg(last = true)]
        launch_args: Vec<String>,
    },
    /// Terminate an app by bundle id.
    Terminate {
        device: String,
        bundle_id: String,
    },
    /// Put an app on a device: an `.app` on a simulator, an `.apk` on an
    /// Android emulator or phone.
    ///
    /// A simulator goes through `simctl`, an emulator or an Android
    /// phone through `adb`. A physical iPhone has no path here and is
    /// refused rather than attempted — installing on one needs
    /// `devicectl` and a provisioning profile, which nothing in smix
    /// wires up.
    ///
    /// adb's own failures are passed through as it reports them: an
    /// `.apk` signed by a different key does not replace the installed
    /// one, and you get adb's words for why rather than smix's guess.
    Install {
        device: String,
        app_path: PathBuf,
    },
    /// Take an app off a device, by bundle id on Apple platforms and by
    /// package name on Android.
    ///
    /// Reaches every kind of device smix can address. On a physical one
    /// it is refused until that device has been opted in with `smix sim
    /// allow-destructive <device>` — taking an app off somebody's phone
    /// removes its data with it.
    Uninstall {
        device: String,
        bundle_id: String,
    },
    /// Open a URL on the simulator.
    Openurl {
        device: String,
        url: String,
    },
    /// Set simulator UI appearance (light / dark).
    Appearance {
        device: String,
        #[arg(value_parser = parse_appearance)]
        mode: Appearance,
    },
    /// Allow destructive actions on a physical device, once.
    ///
    /// Simulators are never gated — they can be erased and rebuilt in a
    /// minute. A phone cannot, so wiping app data, resetting a keychain
    /// or uninstalling on one is refused until this is run. Recorded in
    /// the registry rather than confirmed per command: a confirmation
    /// that has to be typed every time ends up pasted into a script.
    AllowDestructive {
        device: String,
    },
    /// Forget one alias.
    ///
    /// Removes the name, not the device — another alias for the same
    /// device keeps working. The other half of `register`, which
    /// without it could only ever add.
    Unregister {
        alias: String,
    },
    /// Fold per-checkout device registries into this machine's.
    ///
    /// Device records used to live in whichever `.smix/` was above the
    /// working directory, so a machine with four checkouts had four
    /// answers about the same simulators. This copies them into one
    /// place. It adds and never removes: the source registries are left
    /// exactly where they are, and running it twice does nothing the
    /// second time.
    Migrate {
        /// A checkout (or its `.smix`) to read. Repeatable. Defaults to
        /// the one above the working directory.
        ///
        /// There is no index of checkouts on a machine, so the ones
        /// that are not underfoot have to be named. Better than guessing
        /// at a list and quietly missing the tree somebody actually
        /// cares about.
        #[arg(long = "from", value_name = "DIR")]
        from: Vec<PathBuf>,
        /// Say what would move and move nothing.
        #[arg(long)]
        dry_run: bool,
    },
    KeychainReset {
        device: String,
    },
    /// Set the sim's locale (`AppleLanguages` + `AppleLocale`
    /// NSGlobalDomain). By default writes the values but
    /// does NOT reboot; running apps cache locale at process-start so
    /// they'll continue in the old locale until relaunched. Pass
    /// `--reboot` to have smix shut the sim down and boot it back up
    /// so the next app launch picks up the new locale cleanly.
    ///
    /// Note: `.smix/sims.json` `locale:` field is applied at *next
    /// sim boot* (by `smix runner up` / `smix sim boot`); this command
    /// covers the "sim is already booted, want to change locale now"
    /// gap.
    Locale {
        device: String,
        /// BCP-47 tag (e.g. `en`, `en-US`, `ja`, `zh-Hans`).
        lang: String,
        /// Shut the sim down and boot it back up after writing the
        /// locale, so the change is visible to apps launched next.
        #[arg(long)]
        reboot: bool,
    },
    /// Passthrough for simctl subcommands smix has not wrapped yet:
    /// `smix sim exec <DEVICE> <VERB> [ARGS...]` runs
    /// `xcrun simctl <VERB> <UDID> [ARGS...]` with simctl's original
    /// argument shape. If any arg is the literal `{udid}`, the resolved
    /// UDID substitutes there instead of being injected after the verb.
    Exec {
        device: String,
        verb: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Parse `KEY=VAL` clap value. Empty KEY or missing `=` is rejected.
/// KEY is taken verbatim (caller / [`smix_simctl::compose_child_env`]
/// adds `SIMCTL_CHILD_` prefix); VAL may contain `=` characters (only
/// the first `=` splits).
fn parse_kv_pair(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("expected `KEY=VALUE`, got `{s}`"))?;
    if k.is_empty() {
        return Err(format!("empty KEY in `{s}`"));
    }
    Ok((k.to_string(), v.to_string()))
}

fn parse_appearance(s: &str) -> Result<Appearance, String> {
    match s.to_ascii_lowercase().as_str() {
        "light" => Ok(Appearance::Light),
        "dark" => Ok(Appearance::Dark),
        other => Err(format!("expected 'light' or 'dark', got {:?}", other)),
    }
}

/// Resolve a device ref to a UDID.
///
/// An alias needs a readable `.smix/sims.json` (env `SMIX_SIMS_JSON`
/// overrides upward discovery from cwd), so it is registered by
/// construction. A raw UDID used to short-circuit straight through, and
/// that was the hole: an identifier nobody registered went past every
/// gate and reached an executor, where it was stopped only if that
/// executor happened not to recognise it. `simctl` does not recognise a
/// phone, so for a while nothing bad came of it — but `devicectl` does,
/// and it can uninstall. So a raw UDID now has to be one of ours: either
/// registered here, or a simulator the platform itself lists.
fn resolve_device(device_ref: &str) -> Result<String, CliError> {
    if registry::is_udid(device_ref) {
        let udid = device_ref.to_ascii_uppercase();
        let known = if lookup_registered(&udid).is_some() {
            // The class does not matter here — what may be *done* to a
            // registered device is `guard_destructive`'s question.
            smix_lease::Known::Registered(smix_lease::DeviceClass {
                physical: false,
                destructive_opt_in: false,
            })
        } else if simctl_knows(&udid) {
            smix_lease::Known::UnregisteredVirtual
        } else {
            smix_lease::Known::Unknown
        };
        smix_lease::may_address(&udid, known).map_err(|e| CliError::Other(e.to_string()))?;
        return Ok(udid);
    }
    // An adb serial is an identifier too. Without this, `emulator-5554`
    // fell through to the alias lookup and came back "unknown device
    // ref" — from a tool that had just listed it, screenshotted it, and
    // accepted it at `sim register`. Resolving is only about what this
    // reference means and whether it may be addressed; whether a
    // particular verb can act on that kind of device is
    // `guard_sim_verb`'s question, asked separately.
    if registry::is_emulator_serial(device_ref) {
        smix_lease::may_address(device_ref, smix_lease::Known::UnregisteredVirtual)
            .map_err(|e| CliError::Other(e.to_string()))?;
        return Ok(device_ref.to_string());
    }
    let view = load_registry();
    let resolved = view.registry.resolve(device_ref)?;
    note_if_unmigrated(&view, device_ref);
    Ok(resolved)
}

/// Resolve an Android device ref to the serial `adb` will be given.
///
/// The iOS side gets its addressability check inside [`resolve_device`],
/// because every iOS command goes through it. Android had no such place:
/// the serial went straight to `adb`, so "whichever phone happens to be
/// plugged in" was addressable by anyone who typed its serial — the exact
/// shape that put smix's runner on somebody's personal handset on
/// 2026-07-17.
///
/// Case is preserved rather than upper-cased the way UDIDs are: `adb`
/// serials are matched verbatim, and `EMULATOR-5554` is not a device.
fn resolve_android_serial(device_ref: &str) -> Result<String, CliError> {
    let registered = lookup_registered(device_ref);
    let known = match &registered {
        Some(s) => smix_lease::Known::Registered(smix_lease::DeviceClass {
            physical: s.kind.is_physical(),
            destructive_opt_in: s.destructive_opt_in,
        }),
        // Not a heuristic: `adb` is the one naming these. An emulator is
        // `emulator-<port>`; a physical device answers with its hardware
        // serial, which never takes that form. An emulator is virtual,
        // so it needs no registration — the same standing a simulator
        // has, decided by the same function rather than by an early
        // return that skipped past it.
        None if device_ref.starts_with("emulator-") => smix_lease::Known::UnregisteredVirtual,
        None => smix_lease::Known::Unknown,
    };
    smix_lease::may_address(device_ref, known).map_err(|e| CliError::Other(e.to_string()))?;
    // An alias resolves to the serial it was registered with; a serial
    // given directly is already the answer.
    Ok(registered.map_or_else(|| device_ref.to_string(), |s| s.udid))
}

/// Which platform's tooling addresses this device.
///
/// Read from the registry when it is there, and inferred only where the
/// inference is a consequence of an invariant rather than a guess:
/// `resolve_device` has, since C15, let a raw identifier through only
/// when the platform itself claims it — a simulator `simctl` lists or an
/// `emulator-<port>` serial. So an unregistered reference that reached
/// this point is one of those two, and nothing else.
fn device_kind_of(device_ref: &str) -> smix_simctl::registry::DeviceKind {
    classify_device(&load_registry().registry, device_ref)
}

/// What kind of device a ref names, given a registry to ask.
///
/// Split from [`device_kind_of`] so it can be checked against a
/// registry somebody wrote down. As one function it read whatever this
/// machine happened to have registered, and its test passed or failed
/// on that — which it did, the day the device records moved and a
/// registered `emulator-5554` started making the uppercase spelling
/// resolve to it.
fn classify_device(reg: &SimRegistry, device_ref: &str) -> smix_simctl::registry::DeviceKind {
    use smix_simctl::registry::DeviceKind;
    if let Some(sim) = reg.lookup(device_ref) {
        return sim.kind;
    }
    if registry::is_emulator_serial(device_ref) {
        return DeviceKind::Emulator;
    }
    DeviceKind::Simulator
}

/// Does `simctl` list a simulator with this UDID?
///
/// Only ever asked when the registry missed, so the common case pays
/// nothing for it. A substring match over the JSON is enough and cannot
/// collide: a UDID is 36 characters of a shape nothing else in that
/// document has.
///
/// A machine with no `xcrun` answers no, which is the truthful answer —
/// there are no simulators there for the UDID to be one of.
fn simctl_knows(udid: &str) -> bool {
    std::process::Command::new("xcrun")
        .args(["simctl", "list", "devices", "-j"])
        .output()
        .is_ok_and(|out| {
            out.status.success()
                && String::from_utf8_lossy(&out.stdout)
                    .to_ascii_uppercase()
                    .contains(udid)
        })
}

/// Does `adb` list a device with this serial?
///
/// [`simctl_knows`]'s twin, and deliberately the same shape: each virtual
/// device is checked against the catalogue its own platform keeps. Case
/// is preserved — `adb` matches serials verbatim.
fn adb_knows(serial: &str) -> bool {
    std::process::Command::new("adb")
        .arg("devices")
        .output()
        .is_ok_and(|out| {
            out.status.success()
                && String::from_utf8_lossy(&out.stdout)
                    .lines()
                    // `adb devices` prints "<serial>\t<state>"; a device
                    // that is listed as `offline` or `unauthorized` is
                    // known but cannot be driven, and registering an
                    // alias for one would hand out a name that fails
                    // later somewhere else.
                    .any(|l| {
                        l.split('\t').next().is_some_and(|s| s == serial)
                            && l.split('\t').nth(1).is_some_and(|s| s.trim() == "device")
                    })
        })
}

/// Resolve the path to `.smix/sims.json` (env override or upward
/// discovery from cwd). Extracted from [`resolve_device`] so the caller
/// can also load a [`SimRegistry`] to read sim spec fields like `locale`.
/// Returns `Ok(None)` only when an explicit UDID was given upstream and
/// the registry is genuinely absent — the caller passes the UDID through
/// without spec lookup.
/// The device a `sim` verb was pointed at, if it names one.
///
/// Exhaustive for the same reason as [`sim_verb_supports`]: a new verb
/// has to say whether it takes a device before this compiles.
fn sim_action_device(action: &SimAction) -> Option<&str> {
    match action {
        SimAction::List { .. } | SimAction::Migrate { .. } => None,
        SimAction::Unregister { .. } => None,
        SimAction::Register { udid, .. } => Some(udid),
        SimAction::Resolve { device }
        | SimAction::Boot { device }
        | SimAction::Shutdown { device }
        | SimAction::Erase { device }
        | SimAction::Screenshot { device, .. }
        | SimAction::Launch { device, .. }
        | SimAction::Terminate { device, .. }
        | SimAction::Install { device, .. }
        | SimAction::Uninstall { device, .. }
        | SimAction::Openurl { device, .. }
        | SimAction::Appearance { device, .. }
        | SimAction::AllowDestructive { device }
        | SimAction::KeychainReset { device }
        | SimAction::Locale { device, .. }
        | SimAction::Exec { device, .. } => Some(device),
    }
}

/// Which device kinds each `smix sim` verb can actually act on.
///
/// An exhaustive match, deliberately: adding a verb will not compile
/// until somebody says which devices it works on. The alternative is a
/// hand-kept list of "the verbs I remembered to check", which is how
/// `capsule up` came to run `simctl boot` against an emulator and sit
/// there until it timed out 120 seconds later, reporting the timeout
/// rather than the mistake.
///
/// `None` means the verb takes no device, or acts on the registry
/// rather than the device.
fn sim_verb_supports(action: &SimAction) -> Option<&'static [smix_simctl::registry::DeviceKind]> {
    use DeviceKind::{Emulator, PhysicalAndroid, PhysicalIos, Simulator};
    use smix_simctl::registry::DeviceKind;
    const ALL: &[DeviceKind] = &[Simulator, Emulator, PhysicalIos, PhysicalAndroid];
    const SIMCTL: &[DeviceKind] = &[Simulator];
    const APPLE: &[DeviceKind] = &[Simulator, PhysicalIos];
    // Everything that can be handed a payload. A physical iPhone is
    // absent because no path here puts an app on one — `devicectl` would
    // and is not wired — and §9 #1 ③ says a capability that is not
    // available is said out loud rather than attempted into silence.
    const LOADABLE: &[DeviceKind] = &[Simulator, Emulator, PhysicalAndroid];
    Some(match action {
        // No device, or the registry rather than the device.
        SimAction::List { .. }
        | SimAction::Register { .. }
        | SimAction::AllowDestructive { .. }
        | SimAction::Migrate { .. }
        | SimAction::Unregister { .. }
        | SimAction::Resolve { .. } => return None,

        // Dispatches all four itself.
        SimAction::Screenshot { .. } => ALL,

        // Apple device tooling: simctl for a simulator, devicectl for a
        // phone. Neither speaks adb.
        SimAction::KeychainReset { .. } => APPLE,

        // Taking an app off reaches every kind, and on a physical
        // Android device it is the first thing the per-device
        // destructive opt-in ever has to refuse. Registering one prints
        // that the gate exists; until this arm, nothing could reach it —
        // erase and keychain-reset are simctl and Apple, so a registered
        // phone had a gate with nothing behind it.
        SimAction::Uninstall { .. } => ALL,

        // Putting one on: simctl for a simulator, adb for an emulator or
        // an Android phone.
        SimAction::Install { .. } => LOADABLE,

        // simctl and nothing else. An emulator's counterparts exist
        // (`emulator -avd`, `adb shell am start`, `adb shell settings`)
        // but none of them is wired here, and pretending otherwise is
        // how a caller ends up waiting out a 120-second timeout.
        SimAction::Boot { .. }
        | SimAction::Shutdown { .. }
        | SimAction::Erase { .. }
        | SimAction::Launch { .. }
        | SimAction::Terminate { .. }
        | SimAction::Openurl { .. }
        | SimAction::Appearance { .. }
        | SimAction::Locale { .. }
        | SimAction::Exec { .. } => SIMCTL,
    })
}

/// Refuse a verb this device kind has no path for, naming what does.
fn guard_sim_verb(action: &SimAction, device: &str) -> Result<(), CliError> {
    use smix_simctl::registry::DeviceKind;
    let Some(kinds) = sim_verb_supports(action) else {
        return Ok(());
    };
    let kind = device_kind_of(device);
    if kinds.contains(&kind) {
        return Ok(());
    }
    let what = match kind {
        DeviceKind::Simulator => "an iOS Simulator",
        DeviceKind::Emulator => "an Android emulator",
        DeviceKind::PhysicalIos => "a physical iPhone or iPad",
        DeviceKind::PhysicalAndroid => "a physical Android device",
    };
    let alternative = match kind {
        DeviceKind::Emulator | DeviceKind::PhysicalAndroid => {
            "\nAndroid lifecycle goes through adb — `smix runner up <serial> \
             --platform android` brings the device up for driving."
        }
        _ => "",
    };
    Err(CliError::Other(format!(
        "this command runs through simctl, and {device} is {what} — \
         so there is nothing here it could do to it.{alternative}"
    )))
}

/// Where a device fact is written: this machine.
///
/// A simulator's UDID, its runtime version and whether destruction has
/// been allowed on it are facts about the machine, not about the tree
/// you happen to be standing in. This used to walk up from the working
/// directory, which is why four checkouts here each held their own
/// answer — and why on 2026-08-11 a runner on port 22087 was on the
/// books and invisible at the same time: the books were another
/// workspace's.
///
/// `SMIX_SIMS_JSON` still wins, and still means exactly one registry —
/// tests and gates use it to work against a registry of their own, and
/// a machine-level fallback under it would let the real one leak in.
fn registry_path() -> Result<PathBuf, CliError> {
    if let Some(p) = std::env::var_os("SMIX_SIMS_JSON") {
        return Ok(PathBuf::from(p));
    }
    SimRegistry::machine_dir().ok_or_else(|| {
        CliError::Other(
            "no machine-level place to keep device records — neither HOME nor \
             XDG_DATA_HOME is set. Set SMIX_MACHINE_DIR, or point \
             SMIX_SIMS_JSON at a registry."
                .into(),
        )
    })
}

/// The merged registry, from wherever this machine keeps device facts.
///
/// Delegates to the core resolution rather than repeating it: `smix
/// down` and `smix doctor` read the same records, and three callers
/// each deciding where the registry lives is how four checkouts came to
/// hold four answers in the first place.
fn load_registry() -> smix_simctl::registry::MergedRegistry {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    SimRegistry::open_all(&cwd)
}

/// Say so when a device is recorded in a checkout and not on this machine.
///
/// A record only one tree holds is a record the next tree cannot act on.
fn note_if_unmigrated(view: &smix_simctl::registry::MergedRegistry, device_ref: &str) {
    let Some(sim) = view.registry.lookup(device_ref) else {
        return;
    };
    let Some((alias, from)) = view.unmigrated.iter().find(|(alias, _)| {
        view.registry
            .lookup(alias)
            .is_some_and(|s| s.udid.eq_ignore_ascii_case(&sim.udid))
    }) else {
        return;
    };
    eprintln!(
        "note: `{alias}` is recorded in {} and not on this machine — another \
         checkout cannot see it. `smix sim migrate` moves it.",
        from.display()
    );
}

/// Best-effort `RegisteredSim` lookup. Returns `None` (not an error)
/// when the device was given as a raw UDID with no registry entry for
/// it — `smix sim boot <unregistered-udid>` is legitimate.
fn lookup_registered(device_ref: &str) -> Option<smix_simctl::registry::RegisteredSim> {
    load_registry().registry.lookup(device_ref).cloned()
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

async fn run(cli: Cli) -> Result<ExitCode, CliError> {
    // Enable subprocess-ring persistence so
    // `/diagnostic/dump` payloads survive supervisor cycles that used
    // to wipe the in-memory ring. Best-effort — a machine with no home
    // for smix's data is a no-op.
    if let Some(diag_root) = smix_lease::store::machine_root() {
        // One store directory for all three. They used to be three JSON
        // files here; passing the old filenames still worked (the store
        // resolves a `.json` path to its parent) but it read as though
        // smix still wrote them.
        smix_simctl::set_subprocess_ring_persist_path(diag_root.clone());
        // resetAppData counter persistence so
        // `smix diagnostic dump` (later, separate process) sees the
        // count from any prior `smix run` invocations.
        smix_simctl::set_reset_app_data_counters_persist_path(diag_root.clone());
        // Flow-attempts persistence for retry
        // attribution. `smix run` records per-flow attempts here,
        // `smix diagnostic dump` reads back for the `recent flows`
        // section.
        smix_simctl::set_flow_attempts_persist_path(diag_root);
    }

    let simctl = SimctlClient::new();
    match cli.cmd {
        Cmd::Init { alias, device, app } => {
            cmd_init(&simctl, &alias, device.as_deref(), app.as_deref()).await?
        }
        Cmd::Doctor { json } => cmd_doctor(&simctl, json).await?,
        Cmd::Bench {
            update_baseline,
            current_file,
            baseline_file,
        } => {
            let default_baseline = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("bench")
                .join("baseline.json");
            let baseline_path = baseline_file.unwrap_or(default_baseline);
            bench::run(update_baseline, current_file.as_deref(), &baseline_path)
                .map_err(CliError::Other)?;
        }
        Cmd::Diagnostic { action } => cmd_diagnostic(action).await?,
        Cmd::Sim { action } => {
            // One gate for every `sim` verb, before any of them runs.
            // Putting it inside the arms would mean remembering it in
            // each — and the arms that most needed it are exactly the
            // ones nobody remembered.
            // Governance before transport. Both can refuse the same
            // verb on the same device; the difference is what the
            // person is told, and only one of the two is the rule.
            guard_destructive_action(&action)?;
            if let Some(device) = sim_action_device(&action) {
                guard_sim_verb(&action, device)?;
            }
            match action {
                SimAction::List { json, registered } => {
                    if registered {
                        cmd_sim_list_registered(json)?;
                    } else {
                        cmd_sim_list(&simctl, json).await?;
                    }
                }
                SimAction::Migrate { from, dry_run } => cmd_sim_migrate(from, dry_run)?,
                SimAction::Unregister { alias } => {
                    let path = registry_path()?;
                    let sim = SimRegistry::unregister(&path, &alias)
                        .map_err(|e| CliError::Other(e.to_string()))?;
                    println!("forgot `{alias}` -> {} in {}", sim.udid, path.display());
                }
                SimAction::Resolve { device } => {
                    println!("{}", resolve_device(&device)?);
                }
                SimAction::Register {
                    alias,
                    udid,
                    locale,
                    runner_port,
                    kind,
                    name,
                } => {
                    let kind = kind.to_registry();
                    let udid = registry::canonical_identifier(kind, &udid);
                    registry::identifier_fits(kind, &udid)
                        .map_err(|e| CliError::Other(e.to_string()))?;
                    if kind == smix_simctl::registry::DeviceKind::Emulator {
                        if !adb_knows(&udid) {
                            return Err(CliError::Other(format!(
                                "adb lists no running device {udid} — check `adb devices`.\n\
                             An emulator is checked against adb the way a simulator is \
                             checked against simctl; only a physical device is taken as \
                             given, because nothing here can enumerate the world's phones."
                            )));
                        }
                        let path = registry_path()?;
                        let outcome = SimRegistry::register(
                            &path,
                            &alias,
                            smix_simctl::registry::RegisteredSim {
                                device_name: name.unwrap_or_else(|| alias.clone()),
                                udid: udid.clone(),
                                runtime: String::new(),
                                device_type: String::new(),
                                locale,
                                runner_port,
                                kind,
                                destructive_opt_in: false,
                            },
                        )?;
                        let verb = match outcome {
                            smix_simctl::registry::RegisterOutcome::Added => "registered",
                            smix_simctl::registry::RegisterOutcome::Updated => "updated",
                        };
                        println!(
                            "{verb}: {alias} → {udid} (Android emulator) in {}",
                            smix_simctl::registry::store_dir(&path).display()
                        );
                        return Ok(std::process::ExitCode::SUCCESS);
                    }
                    // A physical device is taken as given.
                    //
                    // simctl lists simulators and nothing else, so the
                    // lookup below would refuse every phone that exists. The
                    // identifier is whatever addresses it — a UDID for iOS,
                    // an adb serial for Android — and neither is checked
                    // against a catalogue, because there is no catalogue to
                    // check against. Registration is the deliberate act;
                    // that is the whole point of requiring it.
                    if kind.is_physical() {
                        let path = registry_path()?;
                        let outcome = SimRegistry::register(
                            &path,
                            &alias,
                            smix_simctl::registry::RegisteredSim {
                                device_name: name.unwrap_or_else(|| alias.clone()),
                                udid: udid.clone(),
                                runtime: String::new(),
                                device_type: String::new(),
                                locale,
                                runner_port,
                                kind,
                                // Never on by registration. Allowing
                                // destruction has to be its own decision, or
                                // it is not a decision.
                                destructive_opt_in: false,
                            },
                        )?;
                        let verb = match outcome {
                            smix_simctl::registry::RegisterOutcome::Added => "registered",
                            smix_simctl::registry::RegisterOutcome::Updated => "updated",
                        };
                        println!(
                            "{verb}: {alias} → {udid} (physical device) in {}",
                            smix_simctl::registry::store_dir(&path).display()
                        );
                        println!(
                            "destructive actions are refused on it until \
                         `smix sim allow-destructive {alias}`"
                        );
                        return Ok(std::process::ExitCode::SUCCESS);
                    }
                    // The shape check that used to live here now runs above
                    // for every kind, and names the right world when it
                    // refuses — telling an Android user their serial "is not
                    // UDID-form" described the shape of a thing they were not
                    // registering.
                    let devices = simctl.list_devices().await?;
                    let device = devices
                        .iter()
                        .find(|d| d.udid.eq_ignore_ascii_case(&udid))
                        .ok_or_else(|| {
                            CliError::Other(format!(
                                "simctl knows no device {udid} — check `smix sim list`"
                            ))
                        })?;
                    // The machine registry, which `open_store` creates on
                    // first write — register is the one verb that has to
                    // work before any registry exists. It used to fall
                    // back to `.smix/sims.json` in the working directory
                    // when that resolution failed, which meant a device
                    // record landing somewhere no other tree would ever
                    // read. There is no good place to put it silently.
                    let path = registry_path()?;
                    let outcome = SimRegistry::register(
                        &path,
                        &alias,
                        smix_simctl::registry::RegisteredSim {
                            device_name: device.name.clone(),
                            udid: device.udid.to_ascii_uppercase(),
                            runtime: device.runtime_identifier.clone(),
                            device_type: device.device_type_identifier.clone(),
                            locale,
                            runner_port,
                            // Not a guess: this record is built from what
                            // `simctl list devices` returned, and simctl only
                            // ever lists simulators. A physical device is
                            // registered by a different path.
                            kind: smix_simctl::registry::DeviceKind::Simulator,
                            destructive_opt_in: false,
                        },
                    )?;
                    let verb = match outcome {
                        smix_simctl::registry::RegisterOutcome::Added => "registered",
                        smix_simctl::registry::RegisterOutcome::Updated => "updated",
                    };
                    // The store, not `sims.json` — the file this used to
                    // name is no longer written, and pointing a user at it
                    // sends them to look at stale bytes or nothing at all.
                    println!(
                        "{verb}: {alias} → {} ({}) in {}",
                        device.udid,
                        device.name,
                        smix_simctl::registry::store_dir(&path).display()
                    );
                }
                SimAction::Boot { device } => {
                    let udid = resolve_device(&device)?;
                    // Whether this command is the one that brought the device
                    // up decides, later, whether smix may shut it down. A
                    // device someone else booted is not ours to turn off as
                    // the price of cleaning up after ourselves.
                    let was_up = booted_udids(&simctl).await.contains(&udid);
                    // Wait for the device to finish booting, not just to accept
                    // the boot. `simctl boot` returns while CoreSimulator is
                    // still bringing the render surfaces up, and a device in
                    // that state answers "Booted" to a listing while
                    // `simctl io … screenshot` fails with "Timeout waiting for
                    // screen surfaces" and `recordVideo` produces a zero-byte
                    // file it reports as written. Printing "booted" then is a
                    // statement that is not yet true, and everything the next
                    // command does with the device fails in ways that do not
                    // name this as the cause.
                    simctl
                        .boot_and_wait(&udid, std::time::Duration::from_secs(120))
                        .await?;
                    println!("booted: {udid}");
                    // Not gated on standing in a workspace. It was, and
                    // "no `.smix` above the working directory" meant no
                    // record of who booted this device — a fact about the
                    // machine, withheld because of where somebody's shell
                    // happened to be.
                    if let Ok(leases) = smix_capsule::runner::machine_leases()
                        && let Err(e) = smix_lease::store::record_boot(&leases, &udid, !was_up)
                    {
                        eprintln!("warning: boot not recorded in the device ledger: {e}");
                    }
                    // Registry-driven locale enforcement. When the SimEntry
                    // has a `locale` field, ensure the sim's
                    // NSGlobalDomain AppleLanguages first entry matches; if
                    // it doesn't, write the prefs + shutdown+boot once. This
                    // covers the "sim defaulted to the wrong language" case
                    // where an app was built for a locale different from the
                    // sim's persisted default.
                    if let Some(spec) = lookup_registered(&device)
                        && let Some(desired) = spec.locale.as_ref()
                    {
                        let current = simctl.current_locale(&udid).await.ok().flatten();
                        if current.as_deref() == Some(desired.as_str()) {
                            println!("locale: {desired} ok");
                        } else {
                            eprintln!(
                                "locale: enforcing {desired} (current {})",
                                current.as_deref().unwrap_or("<unset>")
                            );
                            simctl.set_locale(&udid, desired).await?;
                            // Defaults apply at process start — must reboot.
                            simctl.shutdown(&udid).await?;
                            simctl
                                .boot_and_wait(&udid, std::time::Duration::from_secs(60))
                                .await?;
                            println!("locale: {desired} enforced + sim re-booted");
                        }
                    }
                }
                SimAction::Shutdown { device } => {
                    let udid = resolve_device(&device)?;
                    // Already off is the state that was asked for. Failing
                    // here would also mean never clearing the boot row below,
                    // so a device shut down twice would keep a record saying
                    // smix still owes it a shutdown — forever.
                    match simctl.shutdown(&udid).await {
                        Ok(()) => {}
                        Err(smix_simctl::DeviceControlError::NonZeroExit {
                            ref stderr, ..
                        }) if stderr.contains("current state: Shutdown") => {}
                        Err(e) => return Err(e.into()),
                    }
                    // The boot row records who may shut this device down.
                    // Once it is off, the answer is nobody — leaving the row
                    // behind would have a later teardown shut down a device
                    // this process never turned on.
                    if let Ok(leases) = smix_capsule::runner::machine_leases()
                        && let Err(e) = smix_lease::store::drop_resource_kind(
                            &leases,
                            &udid,
                            &smix_lease::Resource::Booted { by_us: true },
                        )
                    {
                        eprintln!("warning: boot row not cleared from the device ledger: {e}");
                    }
                    println!("shutdown: {udid}");
                }
                SimAction::Erase { device } => {
                    let udid = resolve_device(&device)?;
                    simctl.erase(&udid).await?;
                    println!("erased: {udid}");
                }
                SimAction::Screenshot { device, out } => {
                    use smix_simctl::registry::DeviceKind;
                    let udid = resolve_device(&device)?;
                    // Sense is a flat capability: which tool takes the
                    // picture is smix's problem, not the caller's. What is
                    // not flat is a phone — its screen comes from the
                    // runner's XCUIScreen, and there is no device-tooling
                    // path to it at all. §9#1's third constraint says that
                    // has to be said out loud rather than degraded into an
                    // empty file that every later assertion measures.
                    let png = match device_kind_of(&device) {
                        DeviceKind::Simulator => simctl.screenshot(&udid).await?,
                        DeviceKind::Emulator | DeviceKind::PhysicalAndroid => {
                            use smix_sdk::device_control::DeviceControl;
                            smix_sdk::android_device::AndroidDeviceControl::new()
                                .screenshot(&udid)
                                .await
                                .map_err(|e| CliError::Other(e.to_string()))?
                        }
                        DeviceKind::PhysicalIos => {
                            // Through the runner, because Apple exposes no
                            // screen capture for a phone via simctl or
                            // devicectl — but `XCUIScreen` runs inside the
                            // runner and works on both. Until C20 this arm
                            // was a refusal saying so; leaving that in place
                            // once the route existed would have been a
                            // message describing a hole that had been
                            // filled.
                            let port = runner_port();
                            smix_capsule::runner::screenshot(port)
                                .map_err(|e| CliError::Other(e.to_string()))?
                        }
                    };
                    if out.as_os_str() == "-" {
                        use std::io::Write;
                        std::io::stdout()
                            .write_all(&png)
                            .map_err(|e| CliError::Other(format!("write stdout: {e}")))?;
                    } else {
                        std::fs::write(&out, &png).map_err(|e| {
                            CliError::Other(format!("write {}: {e}", out.display()))
                        })?;
                        println!(
                            "screenshot: {udid} → {} ({} bytes)",
                            out.display(),
                            png.len()
                        );
                    }
                }
                SimAction::Launch {
                    device,
                    bundle_id,
                    child_env,
                    launch_args,
                } => {
                    let udid = resolve_device(&device)?;
                    let pairs: Vec<(&str, &str)> = child_env
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();
                    let LaunchResult { pid } = simctl
                        .launch_with_args_and_env(&udid, &bundle_id, &launch_args, &pairs)
                        .await?;
                    println!("launched: {bundle_id} on {udid} (pid {pid})");
                }
                SimAction::Terminate { device, bundle_id } => {
                    let udid = resolve_device(&device)?;
                    simctl.terminate(&udid, &bundle_id).await?;
                    println!("terminated: {bundle_id} on {udid}");
                }
                SimAction::Install { device, app_path } => {
                    use smix_simctl::registry::DeviceKind;
                    let udid = resolve_device(&device)?;
                    // Which tool carries the payload is smix's problem,
                    // not the caller's — the same shape `screenshot`
                    // takes. `guard_sim_verb` has already refused the
                    // kinds with no path, so the arms here are the ones
                    // that have one.
                    match device_kind_of(&device) {
                        DeviceKind::Emulator | DeviceKind::PhysicalAndroid => {
                            use smix_sdk::device_control::DeviceControl;
                            smix_sdk::android_device::AndroidDeviceControl::new()
                                .install(&udid, &app_path.display().to_string())
                                .await
                                .map_err(|e| CliError::Other(e.to_string()))?;
                        }
                        _ => {
                            simctl
                                .install(&udid, &app_path.display().to_string())
                                .await?;
                        }
                    }
                    println!("installed: {} on {udid}", app_path.display());
                }
                SimAction::Uninstall { device, bundle_id } => {
                    use smix_simctl::registry::DeviceKind;
                    let udid = resolve_device(&device)?;
                    let bundle = bundle_id.clone();
                    match device_kind_of(&device) {
                        DeviceKind::Emulator | DeviceKind::PhysicalAndroid => {
                            let control = smix_sdk::android_device::AndroidDeviceControl::new();
                            with_device_lease(&control, &udid, |leased| async move {
                                leased.uninstall(&bundle).await?;
                                Ok(((), leased))
                            })
                            .await?;
                        }
                        _ => {
                            let control = smix_sdk::ios_device::IosDeviceControl::new();
                            with_device_lease(&control, &udid, |leased| async move {
                                leased.uninstall(&bundle).await?;
                                Ok(((), leased))
                            })
                            .await?;
                        }
                    }
                    println!("uninstalled: {bundle_id} on {udid}");
                }
                SimAction::Openurl { device, url } => {
                    let udid = resolve_device(&device)?;
                    simctl.open_url(&udid, &url).await?;
                    println!("opened: {url} on {udid}");
                }
                SimAction::Appearance { device, mode } => {
                    let udid = resolve_device(&device)?;
                    simctl.set_appearance(&udid, mode).await?;
                    println!("appearance: {udid} → {}", mode.as_str());
                }
                SimAction::AllowDestructive { device } => {
                    let path = registry_path()?;
                    match smix_simctl::registry::SimRegistry::allow_destructive(&path, &device) {
                        Ok((alias, already)) => {
                            if already {
                                println!("{alias}: destructive actions were already allowed");
                            } else {
                                println!(
                                    "{alias}: destructive actions allowed — \
                                 erase / uninstall / keychain-reset will now run on it"
                                );
                            }
                        }
                        Err(e) => return Err(CliError::Other(e.to_string())),
                    }
                }
                SimAction::KeychainReset { device } => {
                    let udid = resolve_device(&device)?;
                    let control = smix_sdk::ios_device::IosDeviceControl::new();
                    with_device_lease(&control, &udid, |leased| async move {
                        leased.keychain_reset().await?;
                        Ok(((), leased))
                    })
                    .await?;
                    println!("keychain reset: {udid}");
                }
                SimAction::Locale {
                    device,
                    lang,
                    reboot,
                } => {
                    let udid = resolve_device(&device)?;
                    // Read current locale first — no-op if already desired.
                    let current = simctl.current_locale(&udid).await.ok().flatten();
                    if current.as_deref() == Some(lang.as_str()) {
                        println!("locale already: {lang}");
                        return Ok(ExitCode::SUCCESS);
                    }
                    simctl.set_locale(&udid, &lang).await?;
                    if reboot {
                        println!("locale: written {lang} — rebooting sim to apply");
                        simctl.shutdown(&udid).await?;
                        simctl.boot(&udid).await?;
                        println!("locale: {lang} enforced (sim rebooted)");
                    } else {
                        println!(
                            "locale: written {lang}\n\
                         note: running apps cache locale at process-start — \
                         restart the target app, or re-run with `--reboot` to \
                         cycle the sim so subsequent launches see the new locale."
                        );
                    }
                }
                SimAction::Exec { device, verb, args } => {
                    return cmd_sim_exec(&device, &verb, &args).await;
                }
            }
        }
        Cmd::Runner { action } => {
            let root = smix_workspace_root()?;
            match action {
                RunnerAction::Up {
                    device,
                    device_flag,
                    platform,
                    bundle,
                    runner_project,
                    runner_port: port_flag,
                    supervise,
                    team,
                    no_launch,
                    force,
                } => {
                    // clap has already refused the case where neither
                    // is given, and the case where both are.
                    let device = device
                        .or(device_flag)
                        .expect("clap requires one of the two forms");
                    if platform == RunPlatform::Android {
                        reject_ios_only_up_flags(
                            bundle.is_some(),
                            runner_project.is_some(),
                            supervise,
                        )
                        .map_err(CliError::Other)?;
                        let port =
                            port_flag.unwrap_or(smix_capsule::runner_android::DEFAULT_ANDROID_PORT);
                        // The adb serial is the device id, but it still
                        // has to be a device smix was invited to touch:
                        // this installs an APK.
                        let serial = resolve_android_serial(&device)?;
                        smix_capsule::runner_android::up(&root, &serial, port, 180)
                            .map_err(CliError::Other)?;
                        return Ok(std::process::ExitCode::SUCCESS);
                    }
                    // Port priority chain:
                    //   1. `--runner-port` flag / SMIX_RUNNER_PORT env
                    //   2. `.smix/sims.json` `runnerPort` field for this alias
                    //   3. 22087 default (CLI convention)
                    let sims_port = lookup_registered(&device).and_then(|s| s.runner_port);
                    let port = port_flag.or(sims_port).unwrap_or(22087);
                    let udid = resolve_device(&device)?;
                    // A physical device needs a signing team and a
                    // different destination. Which it is comes from the
                    // registry, not from a guess about the identifier —
                    // §9#1 requires registration precisely so this
                    // question has a recorded answer.
                    let physical_team = match lookup_registered(&device) {
                        Some(sim) if sim.kind.is_physical() => {
                            let facts = smix_capsule::signing::collect_facts(&udid);
                            Some(
                                smix_capsule::signing::resolve_team(team.as_deref(), &facts)
                                    .map_err(|e| CliError::Other(e.to_string()))?,
                            )
                        }
                        _ => None,
                    };
                    // Bare `smix runner up` defaults to record_enabled=false;
                    // the capsule path (`capsule::up`) overrides to true
                    // via TEST_RUNNER_SMIX_RECORD_ENABLED=1.
                    let target = match physical_team.as_deref() {
                        Some(team) => smix_capsule::runner::RunnerTarget::Physical { team },
                        None => smix_capsule::runner::RunnerTarget::Simulator,
                    };
                    // Turn the simulator on here, and write down that we
                    // did. It came up either way — `xcodebuild test
                    // -destination platform=iOS Simulator,id=…` boots it
                    // as a side effect — but then nothing on the machine
                    // could say who turned it on, and `lease owner` exits
                    // 3 for a device that is very much running. That code
                    // is what `pick-dev-sim` reads, so a good simulator
                    // looked busy and the 4.1.0 ship failed on it.
                    //
                    // §9 #9: who booted a device is a fact about the
                    // machine. Letting a subprocess create that fact
                    // without recording it is the same as not having it.
                    if physical_team.is_none() {
                        let simctl = SimctlClient::new();
                        let was_up = booted_udids(&simctl).await.contains(&udid);
                        simctl
                            .boot_and_wait(&udid, std::time::Duration::from_secs(120))
                            .await?;
                        if let Ok(leases) = smix_capsule::runner::machine_leases()
                            && let Err(e) = smix_lease::store::record_boot(&leases, &udid, !was_up)
                        {
                            eprintln!("warning: boot not recorded in the device ledger: {e}");
                        }
                    }
                    smix_capsule::runner::up_on(
                        &root,
                        &udid,
                        port,
                        bundle.as_deref(),
                        runner_project.as_deref(),
                        smix_capsule::runner::UpOptions {
                            supervise,
                            attach_without_relaunch: no_launch,
                            force_recover: force,
                            ..Default::default()
                        },
                        target,
                    )
                    .map_err(CliError::Other)?;
                }
                RunnerAction::Forward { device, port } => {
                    let udid = resolve_device(&device)?;
                    let Some(found) = smix_usbmux::find_by_serial(&udid)
                        .map_err(|e| CliError::Other(e.to_string()))?
                    else {
                        return Err(CliError::Other(format!(
                            "usbmux does not see device {udid}. It is the transport smix \
                             drives through, so its view is the one that counts — a device \
                             `devicectl` calls available can still be off the USB bus.\n\
                             Check the cable, then `smix sim list`."
                        )));
                    };
                    let forward = smix_usbmux::forward(found.device_id, port, port)
                        .map_err(|e| CliError::Other(format!("bind 127.0.0.1:{port}: {e}")))?;
                    println!(
                        "forwarding 127.0.0.1:{} -> {udid}:{port}",
                        forward.local_port()
                    );
                    // Hold it open. The forwarder lives for as long as
                    // this process does, which is the whole reason this
                    // subcommand exists — a listener in the `runner up`
                    // process would die with it.
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(3600));
                    }
                }
                RunnerAction::Uninstall { platform, device } => {
                    if platform != RunPlatform::Android {
                        return Err(CliError::Other(
                            "runner uninstall is Android-only: the iOS runner is launched by \
                             xcodebuild and installs no standalone package"
                                .to_string(),
                        ));
                    }
                    let port = smix_capsule::runner_android::DEFAULT_ANDROID_PORT;
                    let serial = resolve_android_serial(&device)?;
                    smix_capsule::runner_android::uninstall(&serial, port)
                        .map_err(CliError::Other)?;
                }
                RunnerAction::Down {
                    platform,
                    device,
                    include_unrecorded,
                    runner_port: port_flag,
                } => {
                    if platform == RunPlatform::Android {
                        let serial = device.ok_or_else(|| {
                            CliError::Other(
                                "runner down --platform android needs --device \
                                 <adb-serial>: an adb command without one acts on \
                                 whichever device is attached"
                                    .to_string(),
                            )
                        })?;
                        let port = port_flag.unwrap_or_else(|| {
                            std::env::var("SMIX_RUNNER_PORT")
                                .ok()
                                .and_then(|p| p.parse().ok())
                                .unwrap_or(smix_capsule::runner_android::DEFAULT_ANDROID_PORT)
                        });
                        let serial = resolve_android_serial(&serial)?;
                        smix_capsule::runner_android::down(&root, &serial, port)
                            .map_err(CliError::Other)?;
                        return Ok(std::process::ExitCode::SUCCESS);
                    }
                    let port = port_flag.unwrap_or_else(runner_port);
                    if include_unrecorded {
                        smix_capsule::runner::down_including_unrecorded(&root, port)
                    } else {
                        smix_capsule::runner::down(&root, port)
                    }
                    .map_err(CliError::Other)?;
                }
                RunnerAction::Cycle { runner_project } => {
                    let port = runner_port();
                    smix_capsule::runner::cycle(&root, port, runner_project.as_deref())
                        .map_err(CliError::Other)?;
                }
                RunnerAction::Supervise { runner_project } => {
                    smix_capsule::runner::supervise(&root, runner_project.as_deref())
                        .map_err(CliError::Other)?;
                }
                RunnerAction::List => {
                    let leases = smix_capsule::runner::machine_leases().map_err(CliError::Other)?;
                    return Ok(std::process::ExitCode::from(runner_list::run(&leases)?));
                }
                RunnerAction::ListSessions => {
                    let port = runner_port();
                    let client = smix_runner_client::HttpRunnerClient::new(port);
                    // `run` is already inside `#[tokio::main]`; a second
                    // runtime here panics with "Cannot start a runtime
                    // from within a runtime" on every call.
                    let resp = client
                        .list_sessions()
                        .await
                        .map_err(|e| CliError::Other(format!("/session/list: {e}")))?;
                    if resp.sessions.is_empty() {
                        println!("(no open sessions)");
                    } else {
                        println!(
                            "{:<38} {:<40} openedAtMs        lastActivatedAtMs",
                            "sessionId", "bundleId"
                        );
                        for s in &resp.sessions {
                            println!(
                                "{:<38} {:<40} {:<17} {}",
                                s.session_id, s.bundle_id, s.opened_at_ms, s.last_activated_at_ms,
                            );
                        }
                    }
                }
                RunnerAction::Install { path, force } => {
                    let target = path.unwrap_or_else(|| {
                        smix_capsule::runner::installed_runner_dir()
                            .unwrap_or_else(|| PathBuf::from("~/.local/share/smix/runner"))
                    });
                    if !force {
                        // Delegate to the same auto-sync used inside
                        // `runner up`. Idempotent when already current.
                        match smix_capsule::runner::ensure_installed_runner_synced(&target) {
                            Ok(smix_capsule::runner::SyncOutcome::AlreadyCurrent) => {
                                println!(
                                    "runner install: already at v{} — nothing to do (pass --force to re-extract).",
                                    smix_runner_sources::SOURCES_VERSION
                                );
                            }
                            Ok(smix_capsule::runner::SyncOutcome::Extracted {
                                previous_version,
                                ..
                            }) => {
                                let from = previous_version.as_deref().unwrap_or("<none>");
                                println!(
                                    "runner install: extracted v{} into {} (was {}).",
                                    smix_runner_sources::SOURCES_VERSION,
                                    target.display(),
                                    from
                                );
                            }
                            Err(e) => {
                                return Err(CliError::Other(format!(
                                    "runner install: sync failed at {}: {e}",
                                    target.display()
                                )));
                            }
                        }
                    } else {
                        // Force path: unconditional extract with backup.
                        match smix_runner_sources::extract_to(&target, true) {
                            Ok(report) => {
                                let backup_note = report
                                    .backup
                                    .as_ref()
                                    .map(|b| {
                                        format!(" (previous tree backed up to {})", b.display())
                                    })
                                    .unwrap_or_default();
                                // Said out loud rather than done quietly:
                                // deleting a directory the user never
                                // asked about should not be something
                                // they discover from `du`.
                                let pruned_note = if report.pruned_backups.is_empty() {
                                    String::new()
                                } else {
                                    format!(
                                        " Removed {} older backup tree(s), keeping the newest {}.",
                                        report.pruned_backups.len(),
                                        smix_runner_sources::BACKUPS_KEPT
                                    )
                                };
                                println!(
                                    "runner install: extracted {} files at v{} into {}{}.{}",
                                    report.file_count,
                                    report.version_written,
                                    target.display(),
                                    backup_note,
                                    pruned_note
                                );
                            }
                            Err(e) => {
                                return Err(CliError::Other(format!(
                                    "runner install --force: {e}"
                                )));
                            }
                        }
                    }
                }
            }
        }
        Cmd::Down => {
            let root = smix_workspace_root()?;
            down::run(&root, runner_port())
                .await
                .map_err(CliError::Other)?;
        }
        Cmd::Lease { action } => {
            let root = smix_workspace_root()?;
            let leases = smix_capsule::runner::machine_leases().map_err(CliError::Other)?;
            return Ok(std::process::ExitCode::from(
                lease_cmd::run(&root, &leases, action).await?,
            ));
        }
        Cmd::Record { action } => {
            let root = smix_workspace_root()?;
            record_cmd::run(&root, action).await?;
        }
        Cmd::Capsule { action } => {
            let root = smix_workspace_root()?;
            let port = runner_port();
            let capture_endpoint = std::env::var("SMIX_CAPTURE_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:8787".to_string());
            match action {
                CapsuleAction::Up {
                    device,
                    bundle,
                    soft,
                    require_hard,
                    no_capture,
                    no_launch,
                } => {
                    let udid = resolve_device(&device)?;
                    capsule::capsule_supports(device_kind_of(&device), &device)
                        .map_err(CliError::Other)?;
                    capsule::up(capsule::UpOptions {
                        root: &root,
                        udid: &udid,
                        runner_port: port,
                        capture_endpoint: &capture_endpoint,
                        bundle: Some(&bundle),
                        soft,
                        require_hard,
                        no_capture,
                        no_launch,
                    })
                    .await
                    .map_err(CliError::Other)?;
                }
                CapsuleAction::Down { device } => {
                    let udid = resolve_device(&device)?;
                    capsule::capsule_supports(device_kind_of(&device), &device)
                        .map_err(CliError::Other)?;
                    capsule::down(&root, &udid).await.map_err(CliError::Other)?;
                }
            }
        }
        Cmd::Tap {
            selector,
            ocr_locale,
            port,
            device,
            then_screenshot,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            match then_screenshot {
                Some(out) => act::cmd_tap_then_screenshot(selector, p, &out)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?,
                None => act::cmd_tap(selector, p, ocr_locale)
                    .await
                    .map_err(|e| CliError::Other(e.to_string()))?,
            }
        }
        Cmd::Find {
            selector,
            ocr_locale,
            port,
            device,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_find(selector, p, ocr_locale)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::WaitFor {
            selector,
            ocr_locale,
            timeout,
            port,
            device,
            absent,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_wait_for(selector, timeout, p, absent, ocr_locale)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Fill {
            selector,
            text,
            port,
            device,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_fill(selector, text, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::PressKey { key, port, device } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_press_key(key, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Scroll {
            selector,
            direction,
            port,
            device,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_scroll(selector, direction, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Swipe {
            direction,
            port,
            device,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_swipe(direction, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::HideKeyboard { port, device } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_hide_keyboard(p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Tree {
            json,
            port,
            device,
            keyboard,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_tree(json, p, keyboard)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Describe { json, port, device } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_describe(json, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::SystemPopups { json, port, device } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_system_popups(json, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::SystemPopupAction {
            popup_id,
            button_id,
            port,
            device,
        } => {
            let p = runner_dial_port(port, device.as_deref());
            act::cmd_system_popup_action(&popup_id, &button_id, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::RunScript { path, port, device } => {
            let p = runner_dial_port(port, device.as_deref());
            script::cmd_run_script(&path, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Run {
            flows,
            device,
            also_device,
            parallel,
            nodes,
            bundle_id,
            animations,
            runner_port,
            no_launch,
            platform,
            apps_config,
            env,
            debug_output,
            verbose,
            format,
            activate,
            fail_fast,
            retry,
            await_signal,
            gate_signal,
            gate_signal_timeout_ms,
            expect_log_clean,
            metro_log_url,
            fixture_registry,
            force_key_events,
            no_fail_annotate,
            check,
        } => {
            // v2 break #3: resolve the four behavior switches once, here
            // at the CLI edge. Priority: `.smix/config.yaml switches.*` >
            // `SMIX_*` env > default(false). This resolver is the ONLY
            // place these four env names carry weight on the `smix run` /
            // `--check` path; reading one (env source) earns a named
            // deprecation warn. The resolved values are injected into the
            // parser (via FlowArgs → thread-local override) and the sdk
            // (via FlowArgs → App builder) — parser/sdk keep their own env
            // reads solely as the non-CLI fallback.
            let switches = smix_capsule::runner::load_switches();
            let sw_auto_ocr = smix_capsule::runner::resolve_switch(
                switches.auto_ocr_fallback,
                "SMIX_AUTO_OCR_FALLBACK",
            );
            let sw_ai_assertions = smix_capsule::runner::resolve_switch(
                switches.enable_ai_assertions,
                "SMIX_ENABLE_AI_ASSERTIONS",
            );
            let sw_assert_no_autorecord = smix_capsule::runner::resolve_switch(
                switches.assert_screenshot_no_autorecord,
                "SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD",
            );
            let sw_launch_reinstall = smix_capsule::runner::resolve_switch(
                switches.launch_fresh_force_reinstall,
                "SMIX_LAUNCH_FRESH_FORCE_REINSTALL",
            );
            let warn_if_env = |r: &smix_capsule::runner::ResolvedSwitch,
                               env_name: &str,
                               key: &str| {
                if r.source == smix_capsule::runner::SwitchSource::Env {
                    eprintln!(
                        "warning: {env_name} is deprecated; use .smix/config.yaml switches.{key}"
                    );
                }
            };
            if check {
                // `--check` only parses, so only the two parse-time
                // switches matter here. Warn for those, then pin them on
                // the parser override seam right before parse_flow_yaml —
                // synchronous, no tokio, so no thread-migration concern.
                warn_if_env(&sw_auto_ocr, "SMIX_AUTO_OCR_FALLBACK", "autoOcrFallback");
                warn_if_env(
                    &sw_ai_assertions,
                    "SMIX_ENABLE_AI_ASSERTIONS",
                    "enableAiAssertions",
                );
                smix_adapter_maestro::set_auto_ocr_fallback_override(Some(sw_auto_ocr.value));
                smix_adapter_maestro::set_ai_assertions_override(Some(sw_ai_assertions.value));
                // Invoked via either `--check` or its `--dry-run`
                // alias; render prefix neutrally so the output reads
                // correctly for both.
                let mut fail = 0u8;
                let mut step_total: usize = 0;
                for flow_path in &flows {
                    match std::fs::read_to_string(flow_path) {
                        Ok(yaml) => match smix_adapter_maestro::parse_flow_yaml(&yaml) {
                            Ok(flow) => {
                                let n = flow.steps.len();
                                step_total += n;
                                eprintln!(
                                    "smix run: parse OK  {} ({n} step{})",
                                    flow_path.display(),
                                    if n == 1 { "" } else { "s" },
                                );
                            }
                            Err(e) => {
                                eprintln!("smix run: parse FAIL {}: {e}", flow_path.display());
                                fail = 2;
                            }
                        },
                        Err(e) => {
                            eprintln!("smix run: parse FAIL {}: read: {e}", flow_path.display());
                            fail = 2;
                        }
                    }
                }
                if fail == 0 {
                    eprintln!(
                        "smix run: parse OK — {} flow{}, {step_total} total step{}",
                        flows.len(),
                        if flows.len() == 1 { "" } else { "s" },
                        if step_total == 1 { "" } else { "s" },
                    );
                }
                return Ok(std::process::ExitCode::from(fail));
            }

            // Federation lane: shard the flows across the nodes in a
            // roster yaml. Devices come from the roster (`--nodes`
            // conflicts with `--device`/`--also-device`/`--parallel`),
            // the leaves run remotely under `--format json`, and the
            // merged report is one JSON document on stdout — exit is
            // the worst of nodes. Sync/rebuild never happen here: the
            // readiness gate only judges (repair is the sync script's
            // job), and a red gate fast-fails before anything fans out.
            if let Some(nodes_path) = &nodes {
                let yaml = std::fs::read_to_string(nodes_path)
                    .map_err(|e| CliError::Other(format!("read {}: {e}", nodes_path.display())))?;
                let node_specs =
                    federation::parse_nodes(&yaml).map_err(|e| CliError::Other(e.to_string()))?;
                // Flow paths are repo-relative and must exist on every
                // node at the same path; the scheduler repo is the
                // authority, so a flow missing here fails fast before
                // any ssh is dialed.
                for flow in &flows {
                    if !flow.is_file() {
                        return Err(CliError::Other(format!(
                            "flow not found locally: {} — flow paths are repo-relative \
                             and must exist on every node (scheduler repo is the authority)",
                            flow.display()
                        )));
                    }
                }
                // Same CLI-sourced passthrough set as the parallel lane,
                // except `--debug-output`: the remote leaves stage their
                // artifacts under the fixed per-repo dir and the user's
                // directory is the local rsync-pull target instead.
                // Every token is shell-quoted — passthrough rides the
                // remote command string verbatim.
                let mut passthrough: Vec<String> = Vec::new();
                if let Some(b) = &bundle_id {
                    passthrough.push("--bundle-id".into());
                    passthrough.push(b.clone());
                }
                if no_launch {
                    passthrough.push("--no-launch".into());
                }
                if animations {
                    passthrough.push("--animations".into());
                }
                if activate {
                    passthrough.push("--activate".into());
                }
                if verbose {
                    passthrough.push("--verbose".into());
                }
                if fail_fast {
                    passthrough.push("--fail-fast".into());
                }
                if retry != 1 {
                    passthrough.push("--retry".into());
                    passthrough.push(retry.to_string());
                }
                passthrough.push("--platform".into());
                passthrough.push(
                    match platform {
                        RunPlatform::Ios => "ios",
                        RunPlatform::Android => "android",
                    }
                    .into(),
                );
                if let Some(a) = &apps_config {
                    passthrough.push("--apps-config".into());
                    passthrough.push(a.display().to_string());
                }
                if debug_output.is_some() {
                    passthrough.push("--debug-output".into());
                    passthrough.push(federation::FED_ARTIFACT_DIR.into());
                }
                for (k, v) in &env {
                    passthrough.push("--env".into());
                    passthrough.push(format!("{k}={v}"));
                }
                let passthrough: Vec<String> = passthrough
                    .iter()
                    .map(|t| federation::shell_quote(t))
                    .collect();
                let flow_strs: Vec<String> =
                    flows.iter().map(|p| p.display().to_string()).collect();
                let slots = federation::expand_slots(&node_specs);
                let assignments = federation::assign_flows(flow_strs.len(), &slots);
                let merged = federation::run_federation(
                    &node_specs,
                    &assignments,
                    &flow_strs,
                    &passthrough,
                    debug_output.as_deref(),
                )
                .map_err(|e| CliError::Other(e.to_string()))?;
                let doc = serde_json::to_string(&merged)
                    .map_err(|e| CliError::Other(format!("serialize merged report: {e}")))?;
                println!("{doc}");
                return Ok(std::process::ExitCode::from(merged.aggregate_exit));
            }

            // Parallel multi-sim. Shard the flows across `--device` +
            // `--also-device`, each shard a child `smix run` pinned to
            // one sim — so a shard reuses the sequential single-sim path
            // verbatim rather than re-implementing it concurrently. Only
            // when >1 sim is actually in play; `--parallel 1` (default)
            // or a single device falls through to the path below,
            // byte-identical.
            let all_devices: Vec<String> = device
                .iter()
                .cloned()
                .chain(also_device.iter().cloned())
                .collect();
            let sim_count = parallel::effective_sim_count(parallel, all_devices.len());
            if sim_count > 1 {
                let devices = &all_devices[..sim_count];
                let flow_strs: Vec<String> =
                    flows.iter().map(|p| p.display().to_string()).collect();
                let buckets = parallel::shard_flows(flow_strs.len(), sim_count);
                let shards: Vec<(String, Vec<String>)> = buckets
                    .iter()
                    .enumerate()
                    .map(|(i, idxs)| {
                        (
                            devices[i].clone(),
                            idxs.iter().map(|&j| flow_strs[j].clone()).collect(),
                        )
                    })
                    .collect();
                // CLI-sourced flags a child needs; behaviour switches and
                // the flow's own appId are inherited via config/env and
                // yaml. runner_port is per-sim (skipped → each child
                // resolves its own); await/gate signals are batch
                // coordination, not per-shard; --parallel/--also-device
                // never recurse.
                let mut passthrough: Vec<String> = Vec::new();
                if let Some(b) = &bundle_id {
                    passthrough.push("--bundle-id".into());
                    passthrough.push(b.clone());
                }
                if no_launch {
                    passthrough.push("--no-launch".into());
                }
                if animations {
                    passthrough.push("--animations".into());
                }
                if activate {
                    passthrough.push("--activate".into());
                }
                if verbose {
                    passthrough.push("--verbose".into());
                }
                if fail_fast {
                    passthrough.push("--fail-fast".into());
                }
                if retry != 1 {
                    passthrough.push("--retry".into());
                    passthrough.push(retry.to_string());
                }
                passthrough.push("--platform".into());
                passthrough.push(
                    match platform {
                        RunPlatform::Ios => "ios",
                        RunPlatform::Android => "android",
                    }
                    .into(),
                );
                if let Some(a) = &apps_config {
                    passthrough.push("--apps-config".into());
                    passthrough.push(a.display().to_string());
                }
                if let Some(d) = &debug_output {
                    passthrough.push("--debug-output".into());
                    passthrough.push(d.display().to_string());
                }
                for (k, v) in &env {
                    passthrough.push("--env".into());
                    passthrough.push(format!("{k}={v}"));
                }
                let exe = std::env::current_exe()
                    .map_err(|e| CliError::Other(format!("cannot find smix binary: {e}")))?;
                let code = parallel::run_parallel(&exe, &shards, &passthrough);
                return Ok(std::process::ExitCode::from(code));
            }

            // The verbose flag sets SMIX_LOG=debug for this process
            // only. tracing_subscriber (initialized in whichever binary
            // set it up) will pick it up.
            if verbose && std::env::var_os("SMIX_LOG").is_none() {
                // SAFETY: process is single-threaded here (before any
                // adapter/sdk async setup). setting env is safe.
                unsafe { std::env::set_var("SMIX_LOG", "debug") };
            }
            // Resolve device alias if registry has it; else pass raw.
            let udid = device
                .as_deref()
                .map(|d| resolve_device(d).unwrap_or_else(|_| d.to_string()));
            // Announce the run before it starts, and hold the device for
            // as long as this command lasts — every flow, every retry.
            // Released when this binding goes out of scope.
            let _run_lease = hold_run_lease(udid.as_deref())?;
            // No placeholder default: run_flow resolves the app from the
            // flow's own appId when the flag is absent. The literal
            // com.example.app default here once made the quickstart form
            // undriveable. `attribution_bundle` below is best-effort for
            // .ips crash attribution only.
            let bundle = bundle_id.clone();
            // Same priority chain as `runner up`: flag/env → the
            // registry's per-sim runnerPort → 22087. `smix run` used to
            // skip the registry, so a sim registered on a dedicated port
            // got its runner bound there and then dialed on 22087.
            let port = run_port(runner_port, || {
                device
                    .as_deref()
                    .and_then(lookup_registered)
                    .and_then(|sim| sim.runner_port)
            });
            let plat = platform.to_flow();
            let out_fmt = format.to_adapter();
            // The run path consumes all four switches. Warn once for any
            // sourced from a deprecated env var, then inject Some(value)
            // into every flow's FlowArgs below.
            warn_if_env(&sw_auto_ocr, "SMIX_AUTO_OCR_FALLBACK", "autoOcrFallback");
            warn_if_env(
                &sw_ai_assertions,
                "SMIX_ENABLE_AI_ASSERTIONS",
                "enableAiAssertions",
            );
            warn_if_env(
                &sw_assert_no_autorecord,
                "SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD",
                "assertScreenshotNoAutorecord",
            );
            warn_if_env(
                &sw_launch_reinstall,
                "SMIX_LAUNCH_FRESH_FORCE_REINSTALL",
                "launchFreshForceReinstall",
            );

            // Batch invocation. When N flows are listed, iterate;
            // exit = max(per-flow codes). Per-flow debug-output subdir
            // keyed by flow basename.
            let multi_flow = flows.len() > 1;
            let mut worst_exit: u8 = 0;
            for (idx, flow_path) in flows.iter().enumerate() {
                // Per-flow debug-output subdir when running multiple
                // flows. Single-flow batches keep the raw dir for
                // backwards byte-compat.
                let per_flow_debug = debug_output.as_ref().map(|d| {
                    if multi_flow {
                        let stem = flow_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("flow")
                            .to_string();
                        d.join(stem)
                    } else {
                        d.clone()
                    }
                });
                if multi_flow {
                    eprintln!(
                        "smix run: [{}/{}] {}",
                        idx + 1,
                        flows.len(),
                        flow_path.display()
                    );
                }
                // Per-flow retry loop + attempt attribution.
                // Retry only fires on non-zero exit. Each attempt records
                // status + errorClass (best-effort from exit code) + wallMs
                // + any new `.ips` for the target bundle appearing between
                // attempt start and end. `flow-attempts.json` persistence
                // + `smix diagnostic dump` overlay surface the attribution
                // table.
                let attribution_bundle: Option<String> = bundle.clone().or_else(|| {
                    smix_adapter_maestro::parse_flow_file(flow_path)
                        .ok()
                        .map(|f| f.app_id)
                        .filter(|a| !a.is_empty())
                });
                let max_attempts = retry.max(1);
                let mut attempts: Vec<smix_runner_wire::FlowAttempt> = Vec::new();
                let flow_name = flow_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| flow_path.display().to_string());
                let mut final_code: u8 = 1;
                for attempt_index in 0..max_attempts {
                    if attempt_index > 0 {
                        eprintln!("smix run: retry #{attempt_index} for flow {flow_name}");
                    }
                    let ips_before = ips_snapshot(attribution_bundle.as_deref());
                    let started = std::time::Instant::now();
                    let code =
                        smix_adapter_maestro::run_flow_code(smix_adapter_maestro::FlowArgs {
                            flow: flow_path.clone(),
                            udid: udid.clone(),
                            bundle_id: bundle.clone(),
                            runner_port: port,
                            animations,
                            no_launch,
                            platform: plat,
                            apps_config: apps_config.clone(),
                            env_vars: env.clone(),
                            debug_output: per_flow_debug.clone(),
                            verbose,
                            format: out_fmt,
                            auto_activate: activate,
                            metro_log_url: metro_log_url.clone(),
                            await_signal: await_signal.clone(),
                            gate_signal: gate_signal.clone(),
                            gate_signal_timeout_ms,
                            expect_log_clean,
                            fixture_registry: fixture_registry.clone(),
                            force_key_events,
                            no_fail_annotate,
                            auto_ocr_fallback: Some(sw_auto_ocr.value),
                            ai_assertions: Some(sw_ai_assertions.value),
                            assert_screenshot_no_autorecord: Some(sw_assert_no_autorecord.value),
                            launch_fresh_force_reinstall: Some(sw_launch_reinstall.value),
                        })
                        .await;
                    let wall_ms = started.elapsed().as_millis() as u64;
                    let ips_after = ips_snapshot(attribution_bundle.as_deref());
                    let new_ips: Option<String> =
                        ips_after.iter().find(|p| !ips_before.contains(*p)).cloned();
                    // The adapter's real exit-code table (entry.rs
                    // run_error_to_exit): 2 parse, 3 expectation/SDK,
                    // 4 unknown verb, 5 cycle/IO, 6 unreachable. The old
                    // mapping here invented 1=expectation and 2=timeout,
                    // so parse errors were attributed as timeouts and
                    // real expectation failures as bare EXIT_3.
                    let (status, error_class) = match code {
                        0 => ("ok".to_string(), None),
                        2 => ("error".to_string(), Some("PARSE_ERROR".to_string())),
                        3 => ("error".to_string(), Some("EXPECTATION_FAILURE".to_string())),
                        4 => ("error".to_string(), Some("UNKNOWN_VERB".to_string())),
                        5 => ("error".to_string(), Some("FLOW_IO_ERROR".to_string())),
                        6 => ("error".to_string(), Some("RUNNER_UNREACHABLE".to_string())),
                        130 | 143 => ("interrupted".to_string(), Some("SIGNAL".to_string())),
                        n => ("error".to_string(), Some(format!("EXIT_{n}"))),
                    };
                    let mut a = smix_runner_wire::FlowAttempt::default();
                    a.attempt_index = attempt_index;
                    a.status = status;
                    a.error_class = error_class;
                    a.ips_generated = new_ips;
                    a.wall_ms = wall_ms;
                    attempts.push(a);
                    final_code = code;
                    if code == 0 {
                        break;
                    }
                }
                // Persist per-flow attempts so `smix diagnostic dump`
                // (later, separate process) can render the attribution.
                // Wire type doesn't implement the trait; convert to local
                // shape here (thin adapter).
                struct AttemptView<'a>(&'a smix_runner_wire::FlowAttempt);
                impl<'a> smix_simctl::FlowAttemptShape for AttemptView<'a> {
                    fn attempt_index(&self) -> u32 {
                        self.0.attempt_index
                    }
                    fn status(&self) -> &str {
                        &self.0.status
                    }
                    fn error_class(&self) -> Option<&str> {
                        self.0.error_class.as_deref()
                    }
                    fn ips_generated(&self) -> Option<&str> {
                        self.0.ips_generated.as_deref()
                    }
                    fn wall_ms(&self) -> u64 {
                        self.0.wall_ms
                    }
                }
                let views: Vec<AttemptView> = attempts.iter().map(AttemptView).collect();
                smix_simctl::record_flow_attempts(&flow_name, &views);
                worst_exit = worst_exit.max(final_code);
                if fail_fast && final_code != 0 {
                    eprintln!(
                        "smix run: --fail-fast — aborting batch on first failure (exit={final_code})"
                    );
                    break;
                }
            }
            return Ok(ExitCode::from(worst_exit));
        }
        Cmd::Migrate { paths, in_place } => {
            return cmd_migrate(paths, in_place).await;
        }
        Cmd::Annotate {
            input,
            output,
            annotations,
            compression,
            font,
        } => {
            return cmd_annotate(input, output, annotations, compression, font).await;
        }
        Cmd::Authoring { action } => match action {
            AuthoringAction::Suggest {
                partial,
                port,
                device,
            } => {
                return authoring::cmd_suggest(runner_dial_port(port, device.as_deref()), partial)
                    .await;
            }
            AuthoringAction::Generate {
                input,
                format,
                output,
                app_id,
                test_fn_name,
            } => {
                return authoring::cmd_generate(input, format, output, app_id, test_fn_name).await;
            }
            AuthoringAction::TapRecord {
                output,
                duration,
                format,
                port,
                device,
                app_id,
                test_fn_name,
            } => {
                // The run_port ladder, with the Android runner's 28080 as
                // the final rung: tap-record is Android-only today.
                let port = port
                    .or_else(|| {
                        device
                            .as_deref()
                            .and_then(lookup_registered)
                            .and_then(|sim| sim.runner_port)
                    })
                    .unwrap_or(28080);
                return authoring::cmd_tap_record(
                    port,
                    duration,
                    format,
                    output,
                    app_id,
                    test_fn_name,
                )
                .await;
            }
            AuthoringAction::CaptureTree {
                output,
                port,
                device,
            } => {
                return authoring::cmd_capture_tree(
                    runner_dial_port(port, device.as_deref()),
                    output,
                )
                .await;
            }
            AuthoringAction::DiffTree {
                baseline,
                port,
                device,
            } => {
                return authoring::cmd_diff_tree(
                    runner_dial_port(port, device.as_deref()),
                    baseline,
                )
                .await;
            }
            AuthoringAction::Propose {
                flow,
                bundle,
                output,
            } => {
                return authoring::cmd_propose(flow, bundle, output).await;
            }
            AuthoringAction::Record {
                output,
                duration_secs,
                interval_ms,
                port,
                device,
                app_id,
            } => {
                return authoring::cmd_record_session(
                    runner_dial_port(port, device.as_deref()),
                    duration_secs,
                    interval_ms,
                    output,
                    app_id,
                )
                .await;
            }
        },
    }
    Ok(ExitCode::SUCCESS)
}

/// CLI wrapper around `smix_annotate::Annotator`.
async fn cmd_annotate(
    input: PathBuf,
    output: PathBuf,
    annotations: Vec<String>,
    compression: String,
    font: Option<PathBuf>,
) -> Result<ExitCode, CliError> {
    use smix_annotate::{Annotator, Compression};
    let png = std::fs::read(&input)
        .map_err(|e| CliError::Other(format!("read {}: {e}", input.display())))?;
    let mut ann = Annotator::new(&png)
        .map_err(|e| CliError::Other(format!("decode {}: {e}", input.display())))?;
    if let Some(fp) = &font {
        let font_bytes = std::fs::read(fp)
            .map_err(|e| CliError::Other(format!("read font {}: {e}", fp.display())))?;
        ann = ann.font(font_bytes);
    }
    for spec in &annotations {
        let a = parse_annotation_spec(spec)
            .map_err(|e| CliError::Other(format!("annotation `{spec}`: {e}")))?;
        ann = ann.add(a);
    }
    let comp = match compression.to_lowercase().as_str() {
        "fast" => Compression::Fast,
        "balanced" => Compression::Balanced,
        "aggressive" => Compression::Aggressive,
        other => {
            return Err(CliError::Other(format!(
                "unknown compression preset `{other}`"
            )));
        }
    };
    ann = ann.compression(comp);
    let bytes = ann
        .render()
        .map_err(|e| CliError::Other(format!("render: {e}")))?;
    std::fs::write(&output, bytes)
        .map_err(|e| CliError::Other(format!("write {}: {e}", output.display())))?;
    eprintln!("smix annotate: wrote {}", output.display());
    Ok(ExitCode::SUCCESS)
}

/// Parse one annotation spec from the mini-DSL:
///   kind ',' key:value (',' key:value)*
fn parse_annotation_spec(spec: &str) -> Result<smix_annotate::Annotation, String> {
    use smix_annotate::{Annotation, Color, Position};
    let parts: Vec<&str> = spec.split(',').collect();
    let kind = parts
        .first()
        .ok_or_else(|| "empty spec".to_string())?
        .trim();
    let mut kv = std::collections::BTreeMap::new();
    for part in &parts[1..] {
        let (k, v) = part
            .split_once(':')
            .ok_or_else(|| format!("expected key:value, got `{part}`"))?;
        kv.insert(k.trim(), v.trim());
    }
    let get_color = |default: Color| -> Result<Color, String> {
        Ok(match kv.get("color") {
            Some(s) => Color::parse(s).map_err(|e| e.to_string())?,
            None => default,
        })
    };
    let get_pos = |key_prefix: &str, default_key: &str| -> Result<Position, String> {
        let key = if kv.contains_key(key_prefix) {
            key_prefix
        } else {
            default_key
        };
        let v = kv.get(key).ok_or_else(|| format!("missing `{key}`"))?;
        // v is either "X,Y" absolute — but comma already split. Use
        // `at:X_Y` (underscore or pipe separator) instead of `x=X;y=Y`.
        let parts: Vec<&str> = v.split(['_', '|']).collect();
        if parts.len() != 2 {
            return Err(format!(
                "position `{v}` — expected `X_Y` (underscore or pipe separator)"
            ));
        }
        let x: i32 = parts[0]
            .parse()
            .map_err(|_| format!("bad x `{}`", parts[0]))?;
        let y: i32 = parts[1]
            .parse()
            .map_err(|_| format!("bad y `{}`", parts[1]))?;
        Ok(Position::pixel(x, y))
    };
    match kind {
        "circle" => {
            let at = get_pos("at", "at")?;
            let color = get_color(Color::RED)?;
            let radius: i32 = kv
                .get("radius")
                .map(|s| s.parse().unwrap_or(30))
                .unwrap_or(30);
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(3))
                .unwrap_or(3);
            Ok(Annotation::circle(at)
                .color(color)
                .radius(radius)
                .stroke(stroke)
                .build())
        }
        "arrow" => {
            let from = get_pos("from", "from")?;
            let to = get_pos("to", "to")?;
            let color = get_color(Color::BLUE)?;
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(4))
                .unwrap_or(4);
            Ok(Annotation::arrow(from, to)
                .color(color)
                .stroke(stroke)
                .build())
        }
        "text" => {
            let at = get_pos("at", "at")?;
            let content = kv
                .get("content")
                .ok_or_else(|| "missing `content`".to_string())?
                .to_string();
            let color = get_color(Color::WHITE)?;
            let size: f32 = kv
                .get("size")
                .map(|s| s.parse().unwrap_or(24.0))
                .unwrap_or(24.0);
            Ok(Annotation::text(at, content)
                .color(color)
                .size(size)
                .build())
        }
        "box" => {
            let at = get_pos("at", "at")?;
            let width: i32 = kv
                .get("width")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "missing `width`".to_string())?;
            let height: i32 = kv
                .get("height")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "missing `height`".to_string())?;
            let color = get_color(Color::YELLOW)?;
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(2))
                .unwrap_or(2);
            Ok(Annotation::box_(at, width, height)
                .color(color)
                .stroke(stroke)
                .build())
        }
        "line" => {
            let from = get_pos("from", "from")?;
            let to = get_pos("to", "to")?;
            let color = get_color(Color::CYAN)?;
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(2))
                .unwrap_or(2);
            Ok(Annotation::line(from, to)
                .color(color)
                .stroke(stroke)
                .build())
        }
        other => Err(format!(
            "unknown annotation kind `{other}` (expected circle/arrow/text/box/line)"
        )),
    }
}

/// Thin wrapper around `smix_migrate::Migrator`. Three input modes
/// (stdin / file→stdout / in-place batch); unified stderr WARN for
/// unknown verbs; per-file exit-code aggregation.
async fn cmd_migrate(paths: Vec<PathBuf>, in_place: bool) -> Result<ExitCode, CliError> {
    use std::io::{Read, Write};
    let migrator = smix_migrate::Migrator::default();

    // stdin mode
    if paths.is_empty() {
        if in_place {
            eprintln!("smix migrate: --in-place requires at least one path");
            return Ok(ExitCode::from(2));
        }
        let mut buf = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("smix migrate: failed to read stdin: {e}");
            return Ok(ExitCode::from(2));
        }
        match migrator.migrate(&buf) {
            Ok((out, report)) => {
                warn_unknown(&report.unknown_verbs, "<stdin>");
                warn_unknown_selector_keys(&report.unknown_selector_keys, "<stdin>");
                print!("{out}");
                std::io::stdout().flush().ok();
                Ok(ExitCode::SUCCESS)
            }
            Err(e) => {
                eprintln!("smix migrate: <stdin>: {e}");
                Ok(ExitCode::from(2))
            }
        }
    } else {
        let mut worst: u8 = 0;
        let mut totals = Totals::default();
        for path in &paths {
            let input = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("smix migrate: read {}: {e}", path.display());
                    worst = worst.max(2);
                    continue;
                }
            };
            match migrator.migrate(&input) {
                Ok((out, report)) => {
                    warn_unknown(&report.unknown_verbs, &path.display().to_string());
                    warn_unknown_selector_keys(
                        &report.unknown_selector_keys,
                        &path.display().to_string(),
                    );
                    if in_place {
                        // Atomic-ish rewrite. Write to sibling
                        // `.smix-migrate.tmp` then rename, so a process
                        // kill mid-write doesn't corrupt the original
                        // file.
                        let tmp = path.with_extension("smix-migrate.tmp");
                        if let Err(e) = std::fs::write(&tmp, &out) {
                            eprintln!("smix migrate: write tmp {}: {e}", tmp.display());
                            worst = worst.max(3);
                            continue;
                        }
                        if let Err(e) = std::fs::rename(&tmp, path) {
                            eprintln!("smix migrate: rename {}: {e}", tmp.display());
                            worst = worst.max(3);
                            continue;
                        }
                        eprintln!(
                            "smix migrate: rewrote {} — {}",
                            path.display(),
                            describe_changes(&report)
                        );
                        totals.files += 1;
                        totals.renames += report.renamed.len();
                        totals.unknown += report.unknown_verbs.len();
                        totals.subflows += report.subflow_refs;
                    } else {
                        print!("{out}");
                        std::io::stdout().flush().ok();
                    }
                }
                Err(e) => {
                    eprintln!("smix migrate: {}: {e}", path.display());
                    worst = worst.max(2);
                }
            }
        }
        if in_place {
            totals.report(paths.len());
        }
        Ok(ExitCode::from(worst))
    }
}

/// What a migrate run did, across every file it touched.
#[derive(Default)]
struct Totals {
    files: usize,
    renames: usize,
    unknown: usize,
    subflows: usize,
}

impl Totals {
    fn report(&self, asked_for: usize) {
        if asked_for > 1 {
            eprintln!(
                "smix migrate: {} of {} file(s) rewritten, {} rename(s)",
                self.files, asked_for, self.renames
            );
        }
        // The two things a caller has to act on. Neither is a failure, and
        // both are invisible once the command exits, so they are said out
        // loud rather than left in the diff.
        if self.unknown > 0 {
            eprintln!(
                "smix migrate: {} verb(s) had no smix equivalent and were left as they were \
                 — the flow will not parse until you replace them",
                self.unknown
            );
        }
        if self.subflows > 0 {
            eprintln!(
                "smix migrate: {} runFlow reference(s) point at files this run did not open \
                 — migrate those too",
                self.subflows
            );
        }
    }
}

/// One file's changes, for the line printed as it is rewritten.
fn describe_changes(report: &smix_migrate::MigrateReport) -> String {
    if report.renamed.is_empty() {
        return format!("no changes, {} step(s)", report.step_count);
    }
    let mut counts: Vec<(&str, &str, usize)> = Vec::new();
    for rename in &report.renamed {
        match counts.iter_mut().find(|c| c.0 == rename.from) {
            Some(c) => c.2 += 1,
            None => counts.push((rename.from, rename.to, 1)),
        }
    }
    let detail: Vec<String> = counts
        .iter()
        .map(|(from, to, n)| {
            if *n == 1 {
                format!("{from} → {to}")
            } else {
                format!("{from} → {to} ×{n}")
            }
        })
        .collect();
    format!(
        "{} rename(s) in {} step(s): {}",
        report.renamed.len(),
        report.step_count,
        detail.join(", ")
    )
}

fn warn_unknown(unknown: &[String], src: &str) {
    if !unknown.is_empty() {
        eprintln!(
            "smix migrate: WARN {}: unknown verb(s) preserved verbatim: {}",
            src,
            unknown.join(", ")
        );
    }
}

/// Selector keys v2 refuses. Distinct from unknown verbs: those are
/// preserved and may still be smix-native, while these WILL fail to
/// parse — so the wording says so rather than "preserved verbatim".
fn warn_unknown_selector_keys(keys: &[String], src: &str) {
    if !keys.is_empty() {
        eprintln!(
            "smix migrate: WARN {}: selector key(s) v2 does not accept: {} \
             — flows using them fail to parse; remove them or use a \
             supported selector",
            src,
            keys.join(", ")
        );
    }
}

/// UDIDs currently reporting `Booted`.
///
/// Asked before a boot so the ledger can record whether this process is
/// the one that brought the device up.
async fn booted_udids(simctl: &smix_simctl::SimctlClient) -> Vec<String> {
    simctl
        .list_devices()
        .await
        .map(|ds| {
            ds.into_iter()
                .filter(|d| d.state == "Booted")
                .map(|d| d.udid.to_uppercase())
                .collect()
        })
        .unwrap_or_default()
}

/// Take the device's lease for one destructive command, run it, give it back.
///
/// The two commands that reach this — `sim uninstall` and
/// `sim keychain-reset` — take data away, and until now either would run
/// happily against a device somebody else's session was driving. The gate
/// is the same one the SDK uses; what is new is that the CLI goes through
/// it rather than around it via `SimctlClient`'s inherent methods.
///
/// Any settling of a previous holder's abandoned session is printed
/// rather than swallowed: a command that quietly tore down someone's
/// leftovers and then did its own destructive work would leave the person
/// unable to tell the two apart afterwards.
async fn with_device_lease<'a, F, Fut, T>(
    // Whatever drives the device, not one platform's driver.
    // `Leased::acquire` has always taken `&dyn DeviceControl`; this
    // named a concrete iOS type only because iOS was the only caller,
    // and that is what would have made the Android arm build a second
    // way of taking a lease rather than use this one.
    control: &'a dyn smix_sdk::device_control::DeviceControl,
    udid: &str,
    body: F,
) -> Result<T, CliError>
where
    F: FnOnce(smix_sdk::leased::Leased<'a>) -> Fut,
    Fut: std::future::Future<Output = Result<(T, smix_sdk::leased::Leased<'a>), CliError>>,
{
    let root = smix_workspace_root()?;
    let leases = smix_capsule::runner::machine_leases().map_err(CliError::Other)?;
    let leased = smix_sdk::leased::Leased::acquire(
        control,
        &root,
        &leases,
        udid,
        &smix_capsule::reconcile::Reconciler,
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    for report in leased.settled() {
        println!("settled first: {}", report.line);
    }
    let (out, leased) = body(leased).await?;
    leased
        .release()
        .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(out)
}

/// Holds a device lease for as long as a `smix run` is in progress.
///
/// A run is the longest thing the CLI does to a device, and until now it
/// announced nothing: `smix sim uninstall` aimed at the same device would
/// proceed happily alongside it, and whichever landed second won.
///
/// Released on drop. Drop cannot report a failure, so a failed release is
/// printed rather than returned — and it is not serious: an unreleased
/// lease is found by the next command, which sees the holder is gone and
/// settles it. Releasing just makes the device free sooner.
struct RunLease {
    leases: smix_lease::store::LeaseDir,
    device_id: String,
    inherited: Vec<smix_lease::Resource>,
}

impl Drop for RunLease {
    fn drop(&mut self) {
        // Keep what was inherited: an adopted runner and its forwarder
        // belong to the ledger after this run as much as before it.
        if let Err(e) = smix_lease::store::drop_process_rows_except(
            &self.leases,
            &self.device_id,
            &self.inherited,
        ) {
            eprintln!(
                "warning: device lease not released for {}: {e}",
                self.device_id
            );
        }
    }
}

/// Take the device for this run, settling an abandoned session first.
///
/// `None` device means the platform picks one later, and a lease cannot
/// be taken on a device nobody has named yet — the run proceeds
/// unannounced, exactly as it did before leases existed.
fn hold_run_lease(udid: Option<&str>) -> Result<Option<RunLease>, CliError> {
    let Some(udid) = udid else {
        return Ok(None);
    };
    // A run outside a workspace still gets a lease: the ledger is the
    // machine's. `root` only decides where a dead holder's build
    // products would be settled, and cwd is the honest answer when
    // there is no `.smix` above it.
    let root = smix_workspace_root()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let Ok(leases) = smix_capsule::runner::machine_leases() else {
        return Ok(None);
    };
    let control = smix_sdk::ios_device::IosDeviceControl::new();
    let leased = smix_sdk::leased::Leased::acquire(
        &control,
        &root,
        &leases,
        udid,
        &smix_capsule::reconcile::Reconciler,
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    for report in leased.settled() {
        println!("settled first: {}", report.line);
    }
    let inherited = leased.inherited().to_vec();
    Ok(Some(RunLease {
        leases,
        device_id: udid.to_string(),
        inherited,
    }))
}

/// Refuse a destructive action on a physical device that has not opted in.
///
/// Reads the class out of the registry and hands the one bit that matters
/// to `smix_lease::may_destroy`, which is where the rule lives. An
/// unregistered ref is not gated here — resolution already refused it, and
/// a second refusal with a different reason would be confusing.
/// Whether this verb can take something away from a device somebody
/// carries.
///
/// Named here, once, so the test that requires every variant to be
/// classified reads the same judgement the runtime uses. `exec` is on
/// the list because it runs an arbitrary command: whatever the worst
/// thing that command can do is, this verb can do it.
fn is_destructive(action: &SimAction) -> bool {
    // Exhaustive, for the reason `sim_verb_supports` is: a new verb has
    // to say whether it destroys something before this compiles. As a
    // `matches!` it listed four verbs and called everything else safe,
    // and `exec` — which runs an arbitrary command on the device — was
    // absent from that list for as long as it existed. A gate whose
    // default answer is "harmless" is one nobody has to remember to
    // update, which is the same as not having it.
    match action {
        SimAction::Erase { .. }
        | SimAction::Uninstall { .. }
        | SimAction::KeychainReset { .. }
        | SimAction::Exec { .. } => true,

        SimAction::List { .. }
        | SimAction::Register { .. }
        | SimAction::Resolve { .. }
        | SimAction::Migrate { .. }
        | SimAction::Unregister { .. }
        | SimAction::AllowDestructive { .. }
        | SimAction::Boot { .. }
        | SimAction::Shutdown { .. }
        | SimAction::Screenshot { .. }
        | SimAction::Launch { .. }
        | SimAction::Terminate { .. }
        | SimAction::Install { .. }
        | SimAction::Openurl { .. }
        | SimAction::Appearance { .. }
        | SimAction::Locale { .. } => false,
    }
}

/// The governance gate, for every verb, before any of them runs.
///
/// It used to be called inside three match arms, and the comment above
/// `guard_sim_verb` had already explained why that fails: "putting it
/// inside the arms would mean remembering it in each — and the arms
/// that most needed it are exactly the ones nobody remembered." `exec`
/// was one nobody remembered.
///
/// Before `guard_sim_verb`, deliberately. Both refuse a destructive
/// verb on a physical device, but they answer different questions —
/// this one says the rule and how to lift it, the other says simctl
/// cannot reach that device. A reader told the second learns a plumbing
/// detail and goes looking for another route; a reader told the first
/// learns the rule. The device was safe either way only because simctl
/// happens not to reach phones, which the guard's own comment noted
/// "stopped holding the day a `devicectl` path appeared".
fn guard_destructive_action(action: &SimAction) -> Result<(), CliError> {
    if !is_destructive(action) {
        return Ok(());
    }
    let Some(device) = sim_action_device(action) else {
        return Ok(());
    };
    guard_destructive(device)
}

fn guard_destructive(device_ref: &str) -> Result<(), CliError> {
    let reg = load_registry().registry;
    let Some(sim) = reg.lookup(device_ref) else {
        // Not registered, and still here — so `resolve_device` let it
        // through, which it only does for a simulator the platform
        // lists. Those were never gated: erasing one costs a minute of
        // rebuilding.
        //
        // This used to be the gate's blind spot rather than its
        // conclusion: an unregistered device of *any* kind was waved
        // through, and what stopped a phone was `simctl` refusing to
        // recognise it downstream. That is a property of the executor,
        // not of this guard, and it stopped holding the day a
        // `devicectl` path appeared.
        return Ok(());
    };
    smix_lease::may_destroy(
        device_ref,
        smix_lease::DeviceClass {
            physical: sim.kind.is_physical(),
            destructive_opt_in: sim.destructive_opt_in,
        },
    )
    .map_err(|e| CliError::Other(e.to_string()))
}

/// CLI spelling of [`smix_simctl::registry::DeviceKind`].
///
/// A separate type because clap's `ValueEnum` derive belongs to the CLI,
/// not to the registry — the registry is read by things that have no
/// command line.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
enum DeviceKindArg {
    Simulator,
    Emulator,
    PhysicalIos,
    PhysicalAndroid,
}

impl DeviceKindArg {
    fn to_registry(self) -> smix_simctl::registry::DeviceKind {
        use smix_simctl::registry::DeviceKind as K;
        match self {
            DeviceKindArg::Simulator => K::Simulator,
            DeviceKindArg::Emulator => K::Emulator,
            DeviceKindArg::PhysicalIos => K::PhysicalIos,
            DeviceKindArg::PhysicalAndroid => K::PhysicalAndroid,
        }
    }
}

fn runner_port() -> u16 {
    std::env::var("SMIX_RUNNER_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(22087)
}

/// Extract the numeric exit code from a `std::process::ExitCode`.
///
/// `ExitCode` has no public conversion back to `u8` (Rust chose "opaque so
/// platforms can widen later" for the stability guarantee), but the internal
/// `impl Debug` prints `ExitCode(unix_exit_status(N))` on Unix. Parse it back.
/// For the batch-invocation path we only need to compare codes; the parsed u8
/// is fed straight into `ExitCode::from(u8)` for the process exit. Success
/// (Debug "ExitCode(unix_exit_status(0))") maps to 0.
/// smix workspace root = nearest ancestor with a `.smix/` dir (env
/// SMIX_WORKSPACE overrides discovery).
fn smix_workspace_root() -> Result<PathBuf, CliError> {
    if let Some(p) = std::env::var_os("SMIX_WORKSPACE") {
        return Ok(PathBuf::from(p));
    }
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::Other(format!("cannot determine cwd: {e}")))?;
    smix_capsule::runner::workspace_root(&cwd).ok_or_else(|| {
        CliError::Other(format!(
            "no .smix/ workspace found upward from {} — cd into the smix \
             workspace or set SMIX_WORKSPACE",
            cwd.display()
        ))
    })
}

// ---- subcommand impls --------------------------------------------------

/// Build the simctl argv for an exec passthrough: `{udid}` placeholder
/// substitution when present, otherwise UDID injected right after the verb
/// (simctl's device position for every device-taking subcommand).
/// Catch a udid passed where the alias already implied one.
///
/// `smix sim exec insight openurl <UDID> "insight://…"` shifts the URL
/// into simctl's device slot, and simctl answers
/// `Simulator device failed to open <UDID>. (OSStatus error -50)` —
/// which names neither the mistake nor the fix. The device is part of
/// `sim exec`'s own shape, so a second one is always a mistake.
///
/// Only for verbs whose arity is known. A passthrough that guessed at
/// every verb would refuse the ones it had not heard of, and the point
/// of a passthrough is the verbs nobody thought about.
fn exec_arity_complaint(verb: &str, args: &[String]) -> Option<String> {
    const DEVICE_IMPLIED: &[&str] = &["openurl", "launch", "terminate", "install", "uninstall"];
    if !DEVICE_IMPLIED.contains(&verb) {
        return None;
    }
    let looks_like_udid =
        |a: &String| a.len() == 36 && a.split('-').map(str::len).eq([8, 4, 4, 4, 12]);
    let stray = args.iter().position(looks_like_udid)?;
    Some(format!(
        "`{verb}` takes no device argument — `sim exec <ALIAS|UDID>` already named one, \
         and simctl would read `{}` as the device and everything after it shifted by one. \
         Drop it: `smix sim exec <ALIAS> {verb} {}`",
        args[stray],
        args.iter()
            .enumerate()
            .filter(|(i, _)| *i != stray)
            .map(|(_, a)| a.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

fn exec_argv(verb: &str, udid: &str, args: &[String]) -> Vec<String> {
    let mut argv = vec![verb.to_string()];
    if args.iter().any(|a| a == "{udid}") {
        argv.extend(args.iter().map(|a| {
            if a == "{udid}" {
                udid.to_string()
            } else {
                a.clone()
            }
        }));
    } else {
        argv.push(udid.to_string());
        argv.extend(args.iter().cloned());
    }
    argv
}

async fn cmd_sim_exec(device: &str, verb: &str, args: &[String]) -> Result<ExitCode, CliError> {
    let udid = resolve_device(device)?;
    if let Some(complaint) = exec_arity_complaint(verb, args) {
        return Err(CliError::Other(complaint));
    }
    let argv = exec_argv(verb, &udid, args);
    // exec(2), not spawn: the caller's pid becomes simctl itself, so shell
    // job control (`& ... kill -INT $!`) reaches simctl directly — required
    // for recordVideo, whose output is only finalized on a clean SIGINT.
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("xcrun")
        .arg("simctl")
        .args(&argv)
        .exec();
    Err(CliError::Other(format!("exec xcrun simctl: {err}")))
}

#[derive(Subcommand, Debug)]
enum DiagnosticAction {
    /// Print everything smix has persisted, as JSON.
    ///
    /// The state used to be JSON files you could `cat`; it is an
    /// embedded store now, and this is what keeps it as readable as it
    /// was. A value that is not valid JSON is shown as hex rather than
    /// stopping the dump — this is what you run when something is
    /// already wrong.
    Store {
        /// Which store: the workspace's `.smix`, or another path.
        #[arg(long, default_value = ".smix")]
        root: PathBuf,
    },
    /// Pretty-print the runner's runtime observability snapshot:
    /// recent subprocess argvs + exit codes + timings, open sessions,
    /// sim-health state, supervisor pid, uptime. Calls
    /// `POST /diagnostic/dump` on the runner. When the runner is too
    /// old to serve that route, falls back to the client-side ring
    /// buffer only.
    Dump {
        /// JSON output instead of the human table.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Path to an external metro log file. If set, the dump tails
        /// the last N lines of this file (see `--metro-log-tail-lines`)
        /// into a `metro log tail` section (and into
        /// `runner.metroLogTail` on the JSON payload). Covers the case
        /// where metro was started externally (e.g. redirected to a log
        /// file), which smix's own log-gate would otherwise skip.
        #[arg(long = "metro-log")]
        metro_log: Option<PathBuf>,
        /// Number of trailing lines to read from `--metro-log`.
        /// Ignored when `--metro-log` is unset.
        #[arg(long = "metro-log-tail-lines", default_value_t = 200)]
        metro_log_tail_lines: usize,
    },
}

/// Snapshot the current set of `.ips` filenames under
/// `~/Library/Logs/DiagnosticReports/` matching the target bundle id
/// (or every `.ips` entry when `bundle_id` is None).
/// Used before / after each flow attempt so retry attribution can
/// diff the sets and attribute any new `.ips` to the attempt that
/// generated it.
///
/// Best-effort: returns empty on unreadable directory. Cost per call:
/// a single readdir + up to ~30 filename comparisons on a typical
/// developer machine.
fn ips_snapshot(bundle_id: Option<&str>) -> std::collections::HashSet<String> {
    let mut set: std::collections::HashSet<String> = Default::default();
    let Some(home) = std::env::var_os("HOME") else {
        return set;
    };
    let dir = std::path::PathBuf::from(home).join("Library/Logs/DiagnosticReports");
    let Ok(read_dir) = std::fs::read_dir(&dir) else {
        return set;
    };
    // Match logic: when `bundle_id` is set, look for the last
    // component after the final `.` — bundle-id-shaped names
    // (com.foo.bar) match against the .ips filename prefix in
    // heuristic-match mode. When None, include EVERY `.ips` file:
    // callers use this snapshot as a before/after diff around a flow
    // run, so time-bounding does the relevance filtering — any crash
    // report that appears during the flow window is a candidate
    // regardless of process name.
    let bundle_leaf = bundle_id
        .and_then(|b| b.rsplit('.').next())
        .map(|s| s.to_lowercase());
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.ends_with(".ips") {
            continue;
        }
        let interesting = match &bundle_leaf {
            Some(leaf) => name.contains(leaf),
            None => true,
        };
        if interesting {
            set.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    set
}

/// Read the last `n` lines from `path`. Seeks
/// from EOF backward in 8 KB chunks, splitting on `\n`, until it has
/// gathered `n` lines or reached BOF. Handles the "file smaller than
/// one chunk" and "file has no trailing newline" cases. Returns
/// oldest → newest ordered lines with newlines stripped.
///
/// Not tokio — this is called from a sync context (dump command is
/// sync-shaped inside an async fn) and the operation is one-shot at
/// dump time. For streaming tail during a run, use
/// `smix_metro_log::subscriber::FileTailSubscriber` which handles the
/// growing-file case.
fn tail_lines(path: &Path, n: usize) -> std::io::Result<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let end = f.seek(SeekFrom::End(0))?;
    if end == 0 || n == 0 {
        return Ok(Vec::new());
    }
    let chunk_size: u64 = 8192;
    let mut pos = end;
    let mut buf: Vec<u8> = Vec::new();
    let mut line_count = 0usize;
    // Read backward until we have n+1 newlines (so we can drop the
    // partial line at the start) or we hit BOF.
    while pos > 0 && line_count <= n {
        let read_from = pos.saturating_sub(chunk_size);
        let read_len = (pos - read_from) as usize;
        pos = read_from;
        f.seek(SeekFrom::Start(read_from))?;
        let mut chunk = vec![0u8; read_len];
        f.read_exact(&mut chunk)?;
        chunk.append(&mut buf);
        buf = chunk;
        line_count = buf.iter().filter(|&&b| b == b'\n').count();
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    if lines.len() > n {
        lines = lines.split_off(lines.len() - n);
    }
    Ok(lines)
}

async fn cmd_diagnostic(action: DiagnosticAction) -> Result<(), CliError> {
    match action {
        DiagnosticAction::Store { root } => {
            let store = smix_store::Store::open(&root)
                .map_err(|e| CliError::Other(format!("open store at {}: {e}", root.display())))?;
            let dumped = store
                .dump_json()
                .map_err(|e| CliError::Other(format!("dump store: {e}")))?;
            println!("{dumped}");
        }
        DiagnosticAction::Dump {
            json,
            metro_log,
            metro_log_tail_lines,
        } => {
            let port = runner_port();
            let client = smix_runner_client::HttpRunnerClient::new(port);
            let mut resp = match client.diagnostic_dump().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "warning: /diagnostic/dump unreachable ({e}); \
                         showing client-side ring buffer only"
                    );
                    smix_runner_wire::DiagnosticDumpResponse::default()
                }
            };
            // CLI-side metro log tail. Read at
            // dump time from the file path (not from the runner) so
            // it works even when the runner never saw the log tail
            // and doesn't require the runner to have been booted
            // with a subscriber. See smix-metro-log FileTailSubscriber
            // for the runtime path used by `smix run`'s
            // `expect.signal` / `expect.signals` verbs.
            if let Some(ref path) = metro_log {
                match tail_lines(path, metro_log_tail_lines) {
                    Ok(lines) => resp.metro_log_tail = lines,
                    Err(e) => {
                        eprintln!(
                            "warning: --metro-log {} unreadable ({e}); \
                             metro log tail will be empty in dump",
                            path.display()
                        );
                    }
                }
            }
            // Overlay CLI-side resetAppData
            // counters onto the wire response before display. The
            // runner never sees resetAppData dispatches (they're
            // host-side simctl-openurl calls), so the wire counters
            // for these fields arrive as 0; we merge from the
            // CLI-persisted store.
            let reset_counters = smix_simctl::reset_app_data_counters_snapshot();
            resp.session_counters.reset_app_data_total = reset_counters.reset_app_data_total;
            resp.session_counters.reset_app_data_timed_out =
                reset_counters.reset_app_data_timed_out;
            // Overlay per-flow retry attribution from the
            // CLI-persisted store. Runner side never sees flow-level
            // retry (it's CLI-orchestrated), so wire arrives empty and
            // we merge from disk here.
            let recent_flows = smix_simctl::recent_flow_attempts();
            resp.recent_flows = recent_flows
                .into_iter()
                .map(|f| {
                    let mut rec = smix_runner_wire::FlowAttemptRecord::default();
                    rec.flow_name = f.flow_name;
                    rec.attempts = f
                        .attempts
                        .into_iter()
                        .map(|a| {
                            let mut w = smix_runner_wire::FlowAttempt::default();
                            w.attempt_index = a.attempt_index;
                            w.status = a.status;
                            w.error_class = a.error_class;
                            w.ips_generated = a.ips_generated;
                            w.wall_ms = a.wall_ms;
                            w
                        })
                        .collect();
                    rec
                })
                .collect();
            let client_side = smix_simctl::recent_subprocesses();

            if json {
                let payload = serde_json::json!({
                    "runner": resp,
                    "clientSubprocesses": client_side.iter().map(|r| serde_json::json!({
                        "argv": r.argv,
                        "exitCode": r.exit_code,
                        "wallMs": r.wall_ms,
                        "stderrHead": r.stderr_head,
                        "timestampMs": r.timestamp.duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64).unwrap_or(0),
                    })).collect::<Vec<_>>(),
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&payload).unwrap_or_default()
                );
                return Ok(());
            }

            println!("=== runner runtime snapshot ===");
            println!("uptime:         {}ms", resp.uptime_ms);
            println!("sim health:     {}", resp.sim_health);
            if let Some(pid) = resp.supervisor_pid {
                println!("supervisor pid: {pid}");
            } else {
                println!("supervisor pid: (none)");
            }
            println!();
            println!("=== open sessions ({}) ===", resp.sessions.len());
            for s in &resp.sessions {
                println!(
                    "  {:<38} {:<40} openedAtMs={} lastActivatedAtMs={}",
                    s.session_id, s.bundle_id, s.opened_at_ms, s.last_activated_at_ms
                );
            }
            println!();
            // Surface the always-emitted counter fields so callers
            // can numerically check "did the observability actually
            // reach this workload" without dropping into `--json`.
            let ac = &resp.alive_cache;
            println!("=== app-alive cache counters ===");
            println!(
                "  wired={} markDead={} markAlive={} suppressHit={} suppressMiss={}",
                ac.wired,
                ac.mark_dead_total,
                ac.mark_alive_total,
                ac.suppress_hit_total,
                ac.suppress_miss_total,
            );
            println!(
                "  reprobeAttempted={} reprobeSucceeded={} reprobeInvalidatedEarly={} reprobeExhaustedWindow={}",
                ac.reprobe_attempted_total,
                ac.reprobe_succeeded_total,
                ac.reprobe_invalidated_early,
                ac.reprobe_exhausted_window,
            );
            let sc = &resp.session_counters;
            println!();
            println!("=== session lifecycle counters (cumulative, survive close) ===");
            println!(
                "  opened={} closed={} relaunch={} terminate={} launch={}",
                sc.opened_total,
                sc.closed_total,
                sc.relaunch_app_total,
                sc.terminate_app_total,
                sc.launch_app_total,
            );
            println!(
                "  terminate: viaXCUIApplication={} viaFallback={}  # fallback>0 = cooperative terminate failed → potential .ips writes",
                sc.terminate_app_via_xcuiapplication, sc.terminate_app_via_fallback,
            );
            println!(
                "  launch:    reachedForeground={} timedOutBeforeForeground={}  # timedOut>0 → next call may fire during launch → bug_type 309",
                sc.launch_app_reached_foreground, sc.launch_app_timed_out_before_foreground,
            );
            // resetAppData + interactive fingerprint.
            println!(
                "  resetAppData: total={} timedOut={}  # timedOut>0 → URL scheme fired but reset-complete log-line never arrived",
                sc.reset_app_data_total, sc.reset_app_data_timed_out,
            );
            println!(
                "  interactive: reachedInteractive={} timedOutBeforeInteractive={}  # timedOut>0 → process foreground but a11y tree unusable (splash / dev-launcher / sparse annotation)",
                sc.launch_app_reached_interactive, sc.launch_app_timed_out_before_interactive,
            );
            // Top-level lastInteractiveNamedIds sample.
            if !resp.last_interactive_named_ids.is_empty() {
                println!(
                    "  lastInteractiveNamedIds ({}): {}",
                    resp.last_interactive_named_ids.len(),
                    resp.last_interactive_named_ids.join(", "),
                );
            } else {
                println!(
                    "  lastInteractiveNamedIds: []  # no launch has completed with a non-empty sample yet",
                );
            }
            println!();
            // External metro log tail. Only printed
            // when the user passed `--metro-log <path>` to this dump
            // command; the runner doesn't buffer for us.
            if !resp.metro_log_tail.is_empty() {
                println!(
                    "=== metro log tail (last {} of file) ===",
                    resp.metro_log_tail.len()
                );
                for line in &resp.metro_log_tail {
                    println!("  {}", line);
                }
                println!();
            }
            // Retry-attribution roll-up.
            if !resp.recent_flows.is_empty() {
                println!("=== recent flows (retry attribution) ===");
                for flow in &resp.recent_flows {
                    println!("  flow: {}", flow.flow_name);
                    for attempt in &flow.attempts {
                        let err = attempt
                            .error_class
                            .as_deref()
                            .map(|c| format!(" errorClass={c}"))
                            .unwrap_or_default();
                        let ips = attempt
                            .ips_generated
                            .as_deref()
                            .map(|p| format!(" ipsGenerated={p}"))
                            .unwrap_or_default();
                        println!(
                            "    attempt #{} status={} wallMs={}{}{}",
                            attempt.attempt_index, attempt.status, attempt.wall_ms, err, ips,
                        );
                    }
                }
                println!();
            }
            println!(
                "=== runner-side subprocesses (last {} of {}) ===",
                resp.recent_subprocesses.len().min(20),
                resp.recent_subprocesses.len(),
            );
            for r in resp.recent_subprocesses.iter().rev().take(20) {
                let code = r
                    .exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".into());
                let head = if r.stderr_head.is_empty() {
                    String::new()
                } else {
                    format!(" err={:?}", r.stderr_head)
                };
                println!(
                    "  {:>13}ms  code={:>3}  {}{}",
                    r.wall_ms,
                    code,
                    r.argv.join(" "),
                    head
                );
            }
            println!();
            println!(
                "=== client-side subprocesses (last {} of {}) ===",
                client_side.len().min(20),
                client_side.len(),
            );
            for r in client_side.iter().rev().take(20) {
                let code = r
                    .exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".into());
                let head = if r.stderr_head.is_empty() {
                    String::new()
                } else {
                    format!(" err={:?}", r.stderr_head)
                };
                println!(
                    "  {:>13}ms  code={:>3}  simctl {}{}",
                    r.wall_ms,
                    code,
                    r.argv.join(" "),
                    head
                );
            }
        }
    }
    Ok(())
}

async fn cmd_init(
    simctl: &SimctlClient,
    alias: &str,
    device: Option<&str>,
    app: Option<&Path>,
) -> Result<(), CliError> {
    // Make this tree a workspace first, before anything here can refuse.
    //
    // `.smix/` is what `runner up` walks up to find, and where a run's
    // traces, cells and runner state go — checkout-scoped facts, which
    // the move of device records to the machine did not touch. It used
    // to appear as a side effect of writing the registry into it; once
    // the registry went elsewhere, nothing created it, and on a machine
    // that had never run smix `init` reported success while the next
    // command answered "no .smix/ workspace found upward". CI found it
    // on a clean runner; here every tree already had one from before.
    //
    // Before the device is resolved rather than after, because which
    // simulator you name is a separate question from whether this tree
    // is a place smix can work in. An `init` that cannot find your
    // device should still leave you somewhere to run the next one.
    let workspace = std::env::current_dir()
        .map_err(|e| CliError::Other(format!("cannot determine cwd: {e}")))?
        .join(".smix");
    std::fs::create_dir_all(&workspace)
        .map_err(|e| CliError::Other(format!("create {}: {e}", workspace.display())))?;

    let devices = simctl.list_devices().await?;
    let candidates: Vec<init::Candidate> = devices
        .iter()
        .filter(|d| d.is_available)
        .map(|d| init::Candidate {
            udid: d.udid.clone(),
            name: d.name.clone(),
        })
        .collect();

    // The same resolution `smix sim register` uses. Init writing
    // anywhere else would produce a registry the rest of smix does not
    // read, which is worse than not writing one.
    let path = registry_path()?;
    // Every alias this machine already answers to, not just the ones in
    // the book being written: an alias that collides with another
    // checkout's is a name that means two devices depending on where you
    // stand, which is the thing this scope move exists to end.
    let existing: Vec<String> = load_registry().registry.sims().keys().cloned().collect();

    let plan = match init::plan_init(&candidates, alias, device, &existing) {
        Ok(plan) => plan,
        Err(refusal) => {
            return Err(CliError::Other(format!(
                "{}\n{}",
                refusal.reason, refusal.remedy
            )));
        }
    };

    let sim = devices
        .iter()
        .find(|d| d.udid == plan.udid)
        .map(|d| registry::RegisteredSim {
            udid: d.udid.to_ascii_uppercase(),
            device_name: d.name.clone(),
            runtime: d.runtime_identifier.clone(),
            device_type: d.device_type_identifier.clone(),
            runner_port: None,
            locale: None,
            // Same reason as `sim register`: this came from simctl, which
            // lists simulators and nothing else.
            kind: registry::DeviceKind::Simulator,
            destructive_opt_in: false,
        })
        .ok_or_else(|| CliError::Other(format!("device {} vanished mid-init", plan.udid)))?;
    SimRegistry::register(&path, &plan.alias, sim)
        .map_err(|e| CliError::Other(format!("register: {e}")))?;

    println!(
        "registered `{}` -> {} in {}",
        plan.alias,
        plan.udid,
        path.display()
    );

    // Registering a device is half of what someone arrives with; the other
    // half is an app. Installing it here is what lets the next command
    // carry a real bundle id instead of a placeholder to fill in.
    let bundle = match app {
        Some(app_path) => {
            // simctl refuses to install on a shut-down device, and a device
            // that was just registered is shut down. Booting is part of
            // installing here, not a step to leave someone to discover from
            // a raw CoreSimulator error code.
            if let Err(e) = simctl.boot(&plan.udid).await
                && !e.to_string().contains("current state: Booted")
            {
                return Err(CliError::Other(format!("boot {}: {e}", plan.udid)));
            }
            let id = bundle_id_of(app_path)?;
            simctl
                .install(&plan.udid, &app_path.display().to_string())
                .await
                .map_err(|e| CliError::Other(format!("install {}: {e}", app_path.display())))?;
            println!("installed {} ({id})", app_path.display());
            Some(id)
        }
        None => None,
    };

    println!();
    match bundle {
        Some(id) => println!("next: smix capsule up {} --bundle {id}", plan.alias),
        None => println!(
            "next: smix capsule up {} --bundle <your.bundle.id>",
            plan.alias
        ),
    }
    println!("      boots the device and starts the runner that carries every tap and query");
    Ok(())
}

/// The bundle id declared by an `.app`.
///
/// Read rather than asked for: it is already inside the bundle, and making
/// someone supply it is asking them to retype something they have no
/// reason to know by heart.
fn bundle_id_of(app: &Path) -> Result<String, CliError> {
    let plist = app.join("Info.plist");
    if !plist.exists() {
        return Err(CliError::Other(format!(
            "{} has no Info.plist — is it an .app bundle?",
            app.display()
        )));
    }
    let out = std::process::Command::new("plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(&plist)
        .output()
        .map_err(|e| CliError::Other(format!("plutil: {e}")))?;
    if !out.status.success() {
        return Err(CliError::Other(format!(
            "{} declares no CFBundleIdentifier",
            plist.display()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether the capture server is answering on its default endpoint.
///
/// A plain TCP connect: `capsule up` only needs the process to be there,
/// and asking for a health route would couple doctor to a surface that is
/// not this module's business.
fn capture_server_reachable() -> bool {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    let addr: SocketAddr = ([127, 0, 0, 1], 8787).into();
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

async fn cmd_doctor(simctl: &SimctlClient, json: bool) -> Result<(), CliError> {
    // Gather, then judge. The judging is `readiness::assess`, a pure
    // function with its ordering under test — which is the only way the
    // "what do I run next" answer stays correct as checks are added.
    let simctl_facts = match simctl.list_runtimes().await {
        Ok(runtimes) => {
            let devices = simctl.list_devices().await.unwrap_or_default();
            Some(readiness::SimctlFacts {
                available_runtimes: runtimes.iter().filter(|r| r.is_available).count(),
                available_devices: devices.iter().filter(|d| d.is_available).count(),
            })
        }
        // Not an error to report and abort on: "simctl does not run" is
        // the most useful thing doctor can say, and it can only say it by
        // continuing.
        Err(_) => None,
    };

    // Every device this machine knows, not every device this tree
    // knows. A doctor that reports on the checkout answers a question
    // nobody asked: the simulators are the machine's.
    let reg = load_registry().registry;
    let registry = (!reg.sims().is_empty()).then(|| readiness::RegistryFacts {
        aliases: reg.sims().len(),
        first_alias: reg.sims().keys().next().cloned(),
    });

    let port: u16 = std::env::var("SMIX_RUNNER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(22087);

    // Existence checked before opening, because `Store::open` creates
    // what it cannot find — and a health check that brings a store into
    // being has changed the thing it was asked to report on. A workspace
    // with no `.smix` yet simply has no store fact.
    let downgradeable = std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(".smix"))
        .filter(|dir| dir.join("kv").is_dir())
        .and_then(|dir| smix_store::Store::open(&dir).ok())
        .and_then(|store| store.downgradeable_to_kevy3());

    let verdict = readiness::assess(&readiness::Facts {
        simctl: simctl_facts,
        registry,
        // health-not-a-decider: doctor prints whether a runner is
        // answering. It concludes nothing and drives nothing.
        runner_up: smix_capsule::runner::health_ok(port),
        capture_server_up: capture_server_reachable(),
        downgradeable,
    });

    if json {
        let out = serde_json::to_string_pretty(&verdict)
            .map_err(|e| CliError::Other(format!("serialize: {e}")))?;
        println!("{out}");
        return Ok(());
    }

    println!("smix doctor");
    println!("============");
    for check in &verdict.checks {
        let mark = match check.status {
            readiness::Status::Ok => "\u{2713}",
            readiness::Status::Blocked => "\u{2717}",
            readiness::Status::Skipped => "\u{2022}",
        };
        println!("{mark} {}", check.detail);
    }
    println!("{}", readiness::PLATFORM_NOTE);
    match &verdict.next {
        Some(next) => {
            println!();
            println!("next: {}", next.command);
            println!("      {}", next.reason);
        }
        None => println!("\nready — nothing left to set up"),
    }
    Ok(())
}

/// One Android device as `adb devices -l` reports it.
struct AndroidDevice {
    serial: String,
    state: String,
    model: String,
    release: String,
}

/// Parse `adb devices -l` output.
///
/// Split out from the call so the parsing is testable without a device:
/// the shape of that output is the only thing worth asserting, and it
/// is not worth an emulator to assert it.
fn parse_adb_devices(list: &str) -> Vec<(String, String, String)> {
    list.lines()
        .skip_while(|l| l.starts_with("List of devices"))
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?.to_string();
            let state = fields.next()?.to_string();
            let model = fields
                .find_map(|f| f.strip_prefix("model:"))
                .unwrap_or("")
                .to_string();
            Some((serial, state, model))
        })
        .collect()
}

/// Every Android device adb can see, with its OS release.
///
/// A machine with no adb has no Android devices, which is the truthful
/// answer rather than an error — the same way `simctl_knows` answers no
/// on a machine with no Xcode.
fn android_devices() -> Vec<AndroidDevice> {
    let Ok(out) = std::process::Command::new("adb")
        .args(["devices", "-l"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_adb_devices(&String::from_utf8_lossy(&out.stdout))
        .into_iter()
        .map(|(serial, state, model)| {
            let release = std::process::Command::new("adb")
                .args([
                    "-s",
                    &serial,
                    "shell",
                    "getprop",
                    "ro.build.version.release",
                ])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            AndroidDevice {
                serial,
                state,
                model,
                release,
            }
        })
        .collect()
}

/// List every device smix can drive — simulators and Android both.
///
/// This listed simulators only, which read as "smix has no Android
/// devices" to anyone who ran it with an emulator attached, right after
/// `sim register --kind emulator` had accepted that same emulator. A
/// catalogue command that omits half the catalogue is worse than one
/// that does not exist: it answers the question wrongly instead of
/// sending you to ask elsewhere.
///
/// The JSON stays an array and every element gains a `platform` field.
/// Android entries are not dressed in simctl's clothes — there is no
/// runtime identifier on an emulator, and inventing one would make the
/// listing agree with a schema by lying about the device.
/// The devices smix has records for, and where each record lives.
fn cmd_sim_list_registered(json: bool) -> Result<(), CliError> {
    let view = load_registry();
    let machine = SimRegistry::machine_dir();
    if json {
        let rows: Vec<serde_json::Value> = view
            .registry
            .all()
            .map(|(alias, sim)| {
                serde_json::json!({
                    "alias": alias,
                    "udid": sim.udid,
                    "name": sim.device_name,
                    "kind": sim.kind,
                    "runtime": sim.runtime,
                    "destructiveOptIn": sim.destructive_opt_in,
                    // Where the record is, not just what it says. The
                    // question this command exists to answer is whether
                    // two checkouts see the same devices, and a row
                    // that does not say where it came from cannot
                    // answer it.
                    "scope": match view.unmigrated.get(alias) {
                        Some(p) => serde_json::json!({"checkout": p.display().to_string()}),
                        None => serde_json::json!({"machine": machine.as_ref().map(|m| m.display().to_string())}),
                    },
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows)
                .map_err(|e| CliError::Other(format!("serialize: {e}")))?
        );
        return Ok(());
    }
    if view.registry.sims().is_empty() {
        println!("no devices recorded — `smix sim register <alias> --udid <udid>`");
        return Ok(());
    }
    println!("{:<20} {:<40} {:<10} SCOPE", "ALIAS", "UDID", "KIND");
    for (alias, sim) in view.registry.all() {
        let scope = match view.unmigrated.get(alias) {
            Some(p) => p.display().to_string(),
            None => "machine".to_string(),
        };
        println!(
            "{:<20} {:<40} {:<10} {scope}",
            alias,
            sim.udid,
            format!("{:?}", sim.kind).to_lowercase()
        );
    }
    if !view.unmigrated.is_empty() {
        println!();
        println!(
            "{} record(s) live in a checkout and are invisible from anywhere \
             else — `smix sim migrate` folds them in",
            view.unmigrated.len()
        );
    }
    Ok(())
}

/// Fold per-checkout device registries into this machine's.
fn cmd_sim_migrate(from: Vec<PathBuf>, dry_run: bool) -> Result<(), CliError> {
    let into = registry_path()?;
    let sources: Vec<PathBuf> = if from.is_empty() {
        let cwd = std::env::current_dir()
            .map_err(|e| CliError::Other(format!("cannot determine cwd: {e}")))?;
        SimRegistry::discover(&cwd).into_iter().collect()
    } else {
        from.iter()
            .map(|p| {
                if p.ends_with(".smix") {
                    p.clone()
                } else {
                    p.join(".smix")
                }
            })
            .collect()
    };
    if sources.is_empty() {
        println!(
            "nothing to migrate — no .smix registry above {}. \
             Name the checkouts to read with --from <dir>.",
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .display()
        );
        return Ok(());
    }
    // Migrating a book into itself would read every row and write it
    // back, which is harmless but reports every device as "already
    // there" and reads like the migration did nothing. Say which one it
    // is instead.
    let sources: Vec<PathBuf> = sources
        .into_iter()
        .filter(|p| {
            if p == &into {
                println!("{} is already the machine registry — skipped", p.display());
                false
            } else {
                true
            }
        })
        .collect();

    // A rehearsal reads the same books and applies the same merge
    // rules; only the write is skipped. Working the answer out a second
    // way would produce a report about the second way.
    let report = if dry_run {
        SimRegistry::migrate_dry_run(&into, &sources)
    } else {
        SimRegistry::migrate(&into, &sources)
    }
    .map_err(|e| CliError::Other(format!("migrate: {e}")))?;

    for (path, why) in &report.unreadable {
        eprintln!("  {} could not be read: {why}", path.display());
    }
    for path in &report.empty {
        println!("  {} held no devices", path.display());
    }
    for alias in &report.added {
        println!("  + {alias}");
    }
    for alias in &report.narrowed {
        println!("  ~ {alias} — destructive consent narrowed to the stricter answer");
    }
    println!(
        "{} device(s) {} in {} ({} already there)",
        report.added.len(),
        if dry_run {
            "would be recorded"
        } else {
            "now recorded"
        },
        into.display(),
        report.unchanged.len()
    );
    if !report.added.is_empty() && !dry_run {
        // The source is deliberately left in place, so the next reader
        // still finds it and still calls it unmigrated. Saying so beats
        // having somebody wonder why the note did not go away.
        println!(
            "the source registries are untouched — delete them yourself once \
             every tree you use has been migrated"
        );
    }
    Ok(())
}

async fn cmd_sim_list(simctl: &SimctlClient, json: bool) -> Result<(), CliError> {
    let devices = simctl.list_devices().await?;
    let android = android_devices();
    if json {
        let mut all: Vec<serde_json::Value> = Vec::new();
        for d in &devices {
            let mut v =
                serde_json::to_value(d).map_err(|e| CliError::Other(format!("serialize: {e}")))?;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("platform".into(), "ios".into());
            }
            all.push(v);
        }
        for a in &android {
            all.push(serde_json::json!({
                "platform": "android",
                "udid": a.serial,
                "name": if a.model.is_empty() { a.serial.clone() } else { a.model.clone() },
                "state": a.state,
                "release": a.release,
            }));
        }
        let out = serde_json::to_string_pretty(&all)
            .map_err(|e| CliError::Other(format!("serialize: {e}")))?;
        println!("{}", out);
        return Ok(());
    }
    // Compact human-readable table.
    println!("{:<40} {:<28} {:<10} RUNTIME", "UDID", "NAME", "STATE");
    for d in &devices {
        let runtime_short = d
            .runtime_identifier
            .rsplit('.')
            .next()
            .unwrap_or(d.runtime_identifier.as_str());
        println!(
            "{:<40} {:<28} {:<10} {runtime_short}",
            d.udid, d.name, d.state
        );
    }
    for a in &android {
        let name = if a.model.is_empty() {
            a.serial.as_str()
        } else {
            a.model.as_str()
        };
        let release = if a.release.is_empty() {
            "Android".to_string()
        } else {
            format!("Android-{}", a.release)
        };
        println!("{:<40} {:<28} {:<10} {release}", a.serial, name, a.state);
    }
    Ok(())
}

// ---- errors -----------------------------------------------------------

#[derive(Debug)]
enum CliError {
    Simctl(DeviceControlError),
    Registry(RegistryError),
    Other(String),
}

impl From<DeviceControlError> for CliError {
    fn from(e: DeviceControlError) -> Self {
        CliError::Simctl(e)
    }
}

impl From<RegistryError> for CliError {
    fn from(e: RegistryError) -> Self {
        CliError::Registry(e)
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Simctl(e) => write!(f, "{e}"),
            CliError::Registry(e) => write!(f, "{e}"),
            CliError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for CliError {}

/// The port `smix run` dials: explicit flag (or SMIX_RUNNER_PORT via
/// clap's env binding), else the registry's per-sim `runnerPort`, else
/// the convention.
///
/// A function rather than an inline chain so the priority can be
/// asserted. `smix run` used to skip the registry entirely, so a sim
/// registered on a dedicated port had its runner bound there and then
/// dialled on 22087; the chain has been right since that was fixed and
/// unwatched ever since.
///
/// The registry lookup is a closure because reading it is a disk touch
/// worth skipping when the caller already said which port to use.
fn run_port(flag: Option<u16>, registered: impl FnOnce() -> Option<u16>) -> u16 {
    flag.or_else(registered).unwrap_or(act::DEFAULT_RUNNER_PORT)
}

/// The port a single-shot verb dials.
///
/// The same ladder [`run_port`] walks, with the env rung read here
/// rather than bound by clap: these verbs spell the flag `--port`, and
/// giving that one an `env =` would silently widen what
/// `SMIX_RUNNER_PORT` means for anyone passing `--port` explicitly
/// alongside it.
///
/// They had no `--device` at all until the guide gate asked, so the
/// registry rung was unreachable from them: in a workspace with a sim
/// registered on 22088, `smix run` dialled 22088 and `smix tap`
/// dialled 22087, and nothing said so.
fn runner_dial_port(flag: Option<u16>, device: Option<&str>) -> u16 {
    run_port(flag.or_else(act::runner_port_from_env_opt), || {
        device
            .and_then(lookup_registered)
            .and_then(|sim| sim.runner_port)
    })
}

/// Refuse `runner up` flags that only the iOS path implements.
///
/// The Android branch reads `--runner-port` and nothing else. It used to
/// accept the other three silently: `--bundle` went nowhere (the Android
/// runner is `am instrument`-hosted and learns its target from the
/// `App-Bundle-Id` header on each request, not at startup), so its help
/// text — "Required: `runner up` refuses to start without one" — was
/// false on this platform; `--runner-project` points at an .xcodeproj;
/// and `--supervise` promises a sidecar that `runner supervise` has no
/// Android path for at all, which is the worst of the three, because a
/// user who asked for supervision and got none is told nothing.
///
/// Same species as the four defects fixed the same week — a parameter
/// accepted and dropped — but one branch deep, where a scan for "clap
/// fields nobody reads" cannot see it: every one of these IS read, on
/// the other platform.
fn reject_ios_only_up_flags(
    bundle: bool,
    runner_project: bool,
    supervise: bool,
) -> Result<(), String> {
    let offenders: Vec<&str> = [
        (bundle, "--bundle"),
        (runner_project, "--runner-project"),
        (supervise, "--supervise"),
    ]
    .into_iter()
    .filter_map(|(given, name)| given.then_some(name))
    .collect();

    if offenders.is_empty() {
        return Ok(());
    }

    Err(format!(
        "runner up --platform android does not implement {}: {} iOS-only. \
         Drop the flag — the Android runner takes its target app from the \
         App-Bundle-Id header per request, builds no Xcode project, and has \
         no supervise sidecar. Only --runner-port applies on this platform.",
        offenders.join(" / "),
        if offenders.len() == 1 {
            "it is"
        } else {
            "they are"
        },
    ))
}

// ---- tests --------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const UDID: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";

    #[test]
    fn a_device_ref_is_classed_by_the_registry_then_by_shape() {
        use smix_simctl::registry::{DeviceKind, RegisteredSim};
        let mut reg = SimRegistry::default();
        reg.insert(
            "phone",
            RegisteredSim {
                device_name: "a phone".into(),
                kind: DeviceKind::PhysicalIos,
                destructive_opt_in: false,
                udid: "00008120-000000000000000E".into(),
                runtime: String::new(),
                device_type: String::new(),
                locale: None,
                runner_port: None,
            },
        );
        // Registered wins over shape: that is the whole ordering.
        assert_eq!(
            classify_device(&reg, "00008120-000000000000000E"),
            DeviceKind::PhysicalIos
        );
        // Unregistered and emulator-shaped: adb names these, so the
        // shape is a fact rather than a heuristic.
        assert_eq!(classify_device(&reg, "emulator-5554"), DeviceKind::Emulator);
        // Unregistered and not emulator-shaped. Falling back to
        // Simulator is a consequence of C15, not a guess: a raw
        // identifier only gets past `resolve_device` when the platform
        // itself claims it, and the emulator case was just handled.
        assert_eq!(classify_device(&reg, UDID), DeviceKind::Simulator);
        // The uppercase spelling is not what adb answers to, so it is
        // not an emulator serial.
        assert_eq!(
            classify_device(&reg, "EMULATOR-5554"),
            DeviceKind::Simulator
        );
    }

    /// A device you can register and drive, you can also load.
    ///
    /// v4.0 made a physical Android device registrable, addressable and
    /// drivable, and left it impossible to put an app on: `Install`
    /// routed to simctl alone, while `smix-adb` had carried the call
    /// that does it the whole time. The device guard refuses the bare
    /// form and names smix as the way through, so the two pointed at
    /// each other — and a consumer moved all eight copies of that guard
    /// aside to get a build onto a phone.
    ///
    /// Shape only: `sim_verb_supports` reads nothing but its argument. A
    /// test that consulted the registry would pass or fail on whatever
    /// this machine happens to have registered, which is how the
    /// classification test went red the day device records moved.
    #[test]
    fn payload_verbs_reach_android() {
        use smix_simctl::registry::DeviceKind::{
            Emulator, PhysicalAndroid, PhysicalIos, Simulator,
        };
        let install = sim_verb_supports(&SimAction::Install {
            device: String::new(),
            app_path: std::path::PathBuf::new(),
        })
        .expect("install takes a device");
        assert!(install.contains(&Emulator), "install: {install:?}");
        assert!(install.contains(&PhysicalAndroid), "install: {install:?}");
        assert!(install.contains(&Simulator), "install: {install:?}");
        // No devicectl path is wired for it, and §9 #1 ③ says an
        // unavailable capability is loud rather than silently attempted.
        assert!(
            !install.contains(&PhysicalIos),
            "install claims a physical iPhone, and nothing here can put an \
             app on one: {install:?}"
        );

        let uninstall = sim_verb_supports(&SimAction::Uninstall {
            device: String::new(),
            bundle_id: String::new(),
        })
        .expect("uninstall takes a device");
        for kind in [Simulator, PhysicalIos, Emulator, PhysicalAndroid] {
            assert!(
                uninstall.contains(&kind),
                "uninstall dropped {kind:?}: {uninstall:?}"
            );
        }
    }

    #[test]
    fn adb_devices_output_parses_to_serial_state_and_model() {
        let out = "List of devices attached\n\
                   emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:1\n\
                   R5CT10ABCDE            unauthorized usb:337641472X transport_id:2\n\
                   \n";
        let got = parse_adb_devices(out);
        assert_eq!(
            got,
            vec![
                (
                    "emulator-5554".to_string(),
                    "device".to_string(),
                    "sdk_gphone64_arm64".to_string()
                ),
                (
                    "R5CT10ABCDE".to_string(),
                    "unauthorized".to_string(),
                    String::new()
                ),
            ]
        );
    }

    #[test]
    fn adb_knows_answers_from_adbs_own_list() {
        // A serial of a shape nothing answers to. This half runs
        // everywhere, including machines with no adb at all — a missing
        // binary is truthfully "no such device".
        assert!(!adb_knows("NOSUCHSERIAL0001"));

        // The other half needs hardware, so it asserts only when there
        // is some: every serial adb reports as `device` must come back
        // true. Skipping quietly would leave a function that returns
        // `false` for everything looking exactly as healthy as one that
        // works.
        let Ok(out) = std::process::Command::new("adb").arg("devices").output() else {
            return;
        };
        if !out.status.success() {
            return;
        }
        let listed: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .filter(|(_, state)| state.trim() == "device")
            .map(|(serial, _)| serial.to_string())
            .collect();
        for serial in listed {
            assert!(
                adb_knows(&serial),
                "adb lists {serial} as a device but adb_knows says no"
            );
        }
    }

    // tail_lines behavior lock-ins. Small chunk
    // reads deliberately (not just 1 huge chunk) so the "read
    // backward in 8 KB chunks" logic is exercised for files smaller,
    // equal, and larger than one chunk.

    #[test]
    fn tail_lines_returns_last_n_when_file_larger_than_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let content = (0..5000)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, format!("{content}\n")).unwrap();
        let tail = tail_lines(&path, 3).unwrap();
        assert_eq!(tail, vec!["line-4997", "line-4998", "line-4999"]);
    }

    #[test]
    fn tail_lines_returns_all_when_file_smaller_than_n() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.log");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let tail = tail_lines(&path, 10).unwrap();
        assert_eq!(tail, vec!["one", "two", "three"]);
    }

    #[test]
    fn tail_lines_handles_no_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("noeol.log");
        std::fs::write(&path, "alpha\nbeta").unwrap();
        let tail = tail_lines(&path, 5).unwrap();
        assert_eq!(tail, vec!["alpha", "beta"]);
    }

    #[test]
    fn tail_lines_returns_empty_for_zero_n() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("any.log");
        std::fs::write(&path, "content\n").unwrap();
        assert!(tail_lines(&path, 0).unwrap().is_empty());
    }

    #[test]
    fn tail_lines_returns_empty_for_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.log");
        std::fs::write(&path, b"").unwrap();
        assert!(tail_lines(&path, 100).unwrap().is_empty());
    }

    #[test]
    fn tail_lines_survives_utf8_split_across_chunk_boundary() {
        // Craft a file where a multibyte utf-8 sequence straddles our
        // 8192-byte chunk boundary. Uses "😀" (4 bytes) placed at
        // offsets that land on the boundary. String::from_utf8_lossy
        // must produce a valid string even if one chunk has partial
        // bytes.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("utf8.log");
        // 8189 bytes of ASCII + one 😀 (4 bytes) so the 😀 starts at
        // offset 8189 = the first chunk read covers bytes 0..8192,
        // which cuts the emoji in half.
        let prefix = "a".repeat(8189);
        let content = format!("{prefix}😀\ntail-line\n");
        std::fs::write(&path, content).unwrap();
        let tail = tail_lines(&path, 1).unwrap();
        assert_eq!(tail, vec!["tail-line"]);
    }

    #[test]
    fn authoring_propose_parses_flow_bundle_out() {
        let cli = Cli::try_parse_from([
            "smix",
            "authoring",
            "propose",
            "corrupt.yaml",
            "--bundle",
            "bundle-dir",
            "-o",
            "amended.yaml",
        ])
        .unwrap();
        let Cmd::Authoring {
            action:
                AuthoringAction::Propose {
                    flow,
                    bundle,
                    output,
                },
        } = cli.cmd
        else {
            panic!("expected authoring propose")
        };
        assert_eq!(flow, std::path::PathBuf::from("corrupt.yaml"));
        assert_eq!(bundle, std::path::PathBuf::from("bundle-dir"));
        assert_eq!(output, std::path::PathBuf::from("amended.yaml"));
    }

    #[test]
    fn exec_parses_hyphen_args_verbatim() {
        let cli = Cli::try_parse_from([
            "smix",
            "sim",
            "exec",
            "02",
            "status_bar",
            "override",
            "--time",
            "9:41",
        ])
        .unwrap();
        let Cmd::Sim {
            action: SimAction::Exec { device, verb, args },
        } = cli.cmd
        else {
            panic!("expected sim exec");
        };
        assert_eq!(device, "02");
        assert_eq!(verb, "status_bar");
        assert_eq!(args, ["override", "--time", "9:41"]);
    }

    /// The mistake that produced an opaque OSStatus -50.
    #[test]
    fn a_second_device_argument_is_named_as_the_mistake() {
        let complaint = exec_arity_complaint(
            "openurl",
            &[
                "74CAF762-06F0-4687-9D29-E737A435AEAF".to_string(),
                "insight://x".to_string(),
            ],
        )
        .expect("a udid passed to openurl is always a mistake");
        assert!(
            complaint.contains("takes no device argument"),
            "{complaint}"
        );
        assert!(
            complaint.contains("smix sim exec <ALIAS> openurl insight://x"),
            "it has to show the corrected command: {complaint}"
        );
    }

    /// A verb this does not know about is passed straight through.
    ///
    /// Guessing at arity for every verb would refuse the ones nobody
    /// thought about, and those are what a passthrough is for.
    #[test]
    fn an_unknown_verb_is_not_second_guessed() {
        assert!(
            exec_arity_complaint(
                "spawn",
                &["74CAF762-06F0-4687-9D29-E737A435AEAF".to_string()]
            )
            .is_none()
        );
    }

    #[test]
    fn an_ordinary_call_is_not_complained_about() {
        assert!(exec_arity_complaint("openurl", &["insight://x".to_string()]).is_none());
    }

    #[test]
    fn exec_argv_injects_udid_after_verb() {
        let argv = exec_argv(
            "push",
            UDID,
            &["com.example.app".into(), "payload.json".into()],
        );
        assert_eq!(argv, ["push", UDID, "com.example.app", "payload.json"]);
    }

    #[test]
    fn exec_argv_substitutes_placeholder_instead_of_injecting() {
        let argv = exec_argv(
            "spawn",
            UDID,
            &[
                "-s".into(),
                "{udid}".into(),
                "launchctl".into(),
                "list".into(),
            ],
        );
        assert_eq!(argv, ["spawn", "-s", UDID, "launchctl", "list"]);
    }

    // `--child-env KEY=VAL` repeatable flag on `sim launch` composes
    // `SIMCTL_CHILD_*` envp at dispatch time.
    #[test]
    fn sim_launch_parses_repeated_child_env_flags() {
        let cli = Cli::try_parse_from([
            "smix",
            "sim",
            "launch",
            "02",
            "com.example.app",
            "--child-env",
            "SMIX_PERF_RECEIVER_URL=http://127.0.0.1:9999",
            "--child-env",
            "LAUNCH_FORCE_PUSH=true",
        ])
        .expect("parse sim launch with --child-env x2");
        let Cmd::Sim {
            action:
                SimAction::Launch {
                    device,
                    bundle_id,
                    child_env,
                    launch_args,
                },
        } = cli.cmd
        else {
            panic!("expected sim launch");
        };
        assert_eq!(device, "02");
        assert_eq!(bundle_id, "com.example.app");
        assert_eq!(
            child_env,
            vec![
                (
                    "SMIX_PERF_RECEIVER_URL".to_string(),
                    "http://127.0.0.1:9999".to_string(),
                ),
                ("LAUNCH_FORCE_PUSH".to_string(), "true".to_string()),
            ]
        );
        assert!(launch_args.is_empty());
    }

    // Trailing launch arguments after `--` go to simctl as
    // `xcrun simctl launch ... -- <args>`; ProcessInfo.arguments reads
    // them. Mirrors maestro yaml launchApp.arguments.
    #[test]
    fn sim_launch_parses_trailing_launch_args_after_double_dash() {
        let cli = Cli::try_parse_from([
            "smix",
            "sim",
            "launch",
            "02",
            "com.example.app",
            "--child-env",
            "K=V",
            "--",
            "-uitestV2Root",
            "YES",
        ])
        .expect("parse trailing args");
        let Cmd::Sim {
            action:
                SimAction::Launch {
                    launch_args,
                    child_env,
                    ..
                },
        } = cli.cmd
        else {
            panic!("expected sim launch");
        };
        assert_eq!(launch_args, vec!["-uitestV2Root", "YES"]);
        assert_eq!(child_env.len(), 1);
    }

    #[test]
    fn sim_launch_without_child_env_yields_empty_vec() {
        let cli = Cli::try_parse_from(["smix", "sim", "launch", "02", "com.example.app"])
            .expect("parse bare launch");
        let Cmd::Sim {
            action: SimAction::Launch { child_env, .. },
        } = cli.cmd
        else {
            panic!("expected sim launch");
        };
        assert!(child_env.is_empty());
    }

    #[test]
    fn sim_launch_rejects_child_env_without_equals() {
        let err = Cli::try_parse_from([
            "smix",
            "sim",
            "launch",
            "02",
            "com.example.app",
            "--child-env",
            "NOEQUALS",
        ])
        .expect_err("must reject KEY without =");
        let msg = format!("{err}");
        assert!(
            msg.contains("KEY=VALUE") || msg.contains("="),
            "expected error to hint KEY=VALUE shape; got: {msg}"
        );
    }

    #[test]
    fn sim_launch_rejects_child_env_with_empty_key() {
        let err = Cli::try_parse_from([
            "smix",
            "sim",
            "launch",
            "02",
            "com.example.app",
            "--child-env",
            "=just_value",
        ])
        .expect_err("must reject empty KEY");
        let msg = format!("{err}");
        assert!(msg.contains("empty KEY"), "msg: {msg}");
    }

    #[test]
    fn parse_kv_pair_allows_equals_in_value() {
        let (k, v) = super::parse_kv_pair("URL=http://h:9999/p=q&r=s").expect("parse");
        assert_eq!(k, "URL");
        assert_eq!(v, "http://h:9999/p=q&r=s");
    }

    #[test]
    fn every_device_subcommand_accepts_alias_ref() {
        // Parse-level guarantee that the surface is alias-first: no
        // subcommand should reject a non-UDID device string at parse time.
        for argv in [
            vec!["smix", "sim", "boot", "02"],
            vec!["smix", "sim", "shutdown", "ios-17"],
            vec!["smix", "sim", "erase", "02"],
            vec!["smix", "sim", "screenshot", "02", "/tmp/x.png"],
            vec!["smix", "sim", "launch", "02", "com.example.app"],
            vec!["smix", "sim", "terminate", "02", "com.example.app"],
            vec!["smix", "sim", "install", "02", "/tmp/App.app"],
            vec!["smix", "sim", "uninstall", "02", "com.example.app"],
            vec!["smix", "sim", "openurl", "02", "https://example.com"],
            vec!["smix", "sim", "appearance", "02", "dark"],
            vec!["smix", "sim", "keychain-reset", "02"],
            vec!["smix", "sim", "resolve", "02"],
        ] {
            Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?} failed to parse: {e}"));
        }
    }

    #[test]
    fn android_runner_up_takes_only_the_port_flag() {
        assert!(reject_ios_only_up_flags(false, false, false).is_ok());

        for (bundle, project, supervise, expected) in [
            (true, false, false, "--bundle"),
            (false, true, false, "--runner-project"),
            (false, false, true, "--supervise"),
        ] {
            let err = reject_ios_only_up_flags(bundle, project, supervise)
                .expect_err("an iOS-only flag must be refused, not dropped");
            assert!(err.contains(expected), "{expected} unnamed in: {err}");
            assert!(
                err.contains("--runner-port"),
                "the refusal must say what DOES work: {err}"
            );
        }
    }

    #[test]
    fn every_dropped_flag_is_named_at_once() {
        // One flag per run would make a user re-run three times to
        // discover three problems.
        let err = reject_ios_only_up_flags(true, true, true).expect_err("all three are iOS-only");
        for flag in ["--bundle", "--runner-project", "--supervise"] {
            assert!(err.contains(flag), "{flag} unnamed in: {err}");
        }
        assert!(err.contains("they are"), "plural form expected in: {err}");
    }
    /// `--animations` exists on `smix run` and is off by default.
    ///
    /// The default is the whole change: a run quietens the device
    /// unless this is passed. Read off the clap tree rather than the
    /// help text, because the text is generated from the tree and
    /// checking it would be checking the rendering.
    #[test]
    fn the_animations_flag_is_off_by_default() {
        use clap::CommandFactory;
        let cli = Cli::command();
        let run = cli
            .get_subcommands()
            .find(|c| c.get_name() == "run")
            .expect("`smix run` still exists");
        let arg = run
            .get_arguments()
            .find(|a| a.get_id() == "animations")
            .expect("`smix run --animations` exists");
        assert_eq!(
            arg.get_default_values(),
            ["false"],
            "the flag stopped defaulting to false, so a run no longer \
             quietens the device unless asked"
        );
    }

    /// The port `smix run` dials, in priority order.
    ///
    /// Extracted so the chain can be asserted rather than only read.
    /// It was already correct — and recorded as broken for three days,
    /// because nothing anywhere would have gone red either way.
    #[test]
    fn run_port_flag_wins() {
        assert_eq!(run_port(Some(22099), || Some(23000)), 22099);
    }

    #[test]
    fn run_port_falls_back_to_registry() {
        assert_eq!(run_port(None, || Some(22099)), 22099);
    }

    #[test]
    fn run_port_defaults_to_22087() {
        assert_eq!(run_port(None, || None), 22087);
    }

    /// Laziness is behaviour, not an implementation detail: with an
    /// explicit port there is no reason to read the registry off disk.
    /// A refactor to something eager would be invisible without this.
    #[test]
    fn run_port_skips_registry_lookup_when_flag_present() {
        let consulted = std::cell::Cell::new(false);
        let port = run_port(Some(22099), || {
            consulted.set(true);
            Some(23000)
        });
        assert_eq!(port, 22099);
        assert!(
            !consulted.get(),
            "registry was read despite an explicit port"
        );
    }
}
