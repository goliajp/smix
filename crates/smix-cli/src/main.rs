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
mod capsule;
mod down;
mod runner;
mod runner_android;
mod runner_state;
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

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Probe environment health: xcrun simctl availability + sim listing.
    Doctor,
    /// Runtime observability commands. `dump` pretty-prints the
    /// runner's recent subprocess ring buffer + open sessions + sim
    /// health so a failed flow can be diagnosed without a new smix
    /// patch.
    Diagnostic {
        #[command(subcommand)]
        action: DiagnosticAction,
    },
    /// Manage simulators. `<DEVICE>` = explicit UDID, or an alias / deviceName
    /// recorded in .smix/sims.json (env SMIX_SIMS_JSON overrides discovery).
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
        #[arg(long)]
        port: Option<u16>,
    },
    /// Boolean existence probe (POST /find). Prints `exists=<bool>`.
    /// Same selector shorthand as `smix tap`.
    Find {
        selector: String,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Poll `/find` every 250ms until the selector resolves or
    /// `--timeout` expires. Mirrors SDK `App::wait_for` semantics; useful in
    /// shell loops driving the runner from outside Rust.
    WaitFor {
        selector: String,
        /// Timeout in seconds (default 5).
        #[arg(long, default_value_t = 5)]
        timeout: u64,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Type text into the matched field. Equivalent to the flow yaml
    /// `inputText:` verb. Selector shorthand same as `smix tap`.
    Fill {
        selector: String,
        #[arg(long)]
        text: String,
        #[arg(long)]
        port: Option<u16>,
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
    },
    /// Scroll until the selector becomes visible. Direction:
    /// `up` / `down` / `left` / `right`.
    Scroll {
        selector: String,
        #[arg(long)]
        direction: String,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Dismiss the soft keyboard if visible.
    HideKeyboard {
        #[arg(long)]
        port: Option<u16>,
    },
    /// Print the runner's current a11y tree. `--json` emits
    /// wire JSON; default emits an indented text outline.
    Tree {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Print the runner's high-level ScreenDescription: the visible
    /// interactive elements aggregated from the current a11y tree.
    Describe {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Print the runner's current SpringBoard system-popup list.
    SystemPopups {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Press a button on a SpringBoard system popup. Both ids come from
    /// `smix system-popups` output (popup `id` + one of its buttons'
    /// `id`). Errors when the popup or button no longer exists.
    SystemPopupAction {
        popup_id: String,
        button_id: String,
        #[arg(long)]
        port: Option<u16>,
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
        /// Force key-event dispatch mode for `inputText`/`fill` verbs.
        /// Bypasses a11y-focus resolution; sends
        /// `Input-Dispatch-Mode: key-events` header. Use for RN apps
        /// with hidden-input patterns where a11y-focus lookup returns
        /// nothing (e.g. offscreen `<TextInput>` behind a visible cell
        /// wrapper).
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
    },
    /// Capture the current a11y tree JSON to a file for baseline use.
    CaptureTree {
        /// Output path for the JSON baseline.
        output: PathBuf,
        /// Runner HTTP port.
        #[arg(long, env = "SMIX_RUNNER_PORT")]
        port: Option<u16>,
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
enum RunnerAction {
    /// Start the runner on a device; blocks until /health answers.
    Up {
        device: String,
        /// Which runner to bring up. `ios` drives xcodebuild + the
        /// XCUITest runner; `android` installs the instrumentation APK,
        /// forwards the port, and `am instrument`s the Kotlin runner.
        #[arg(long, value_enum, default_value_t = RunPlatform::Ios)]
        platform: RunPlatform,
        /// Bundle id the runner binds its XCUIApplication to.
        /// Required: `runner up` refuses to start without one (the
        /// help used to claim a com.apple.Preferences default that the
        /// implementation rejects).
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
    },
    /// Print the UDID a device ref resolves to.
    Resolve { device: String },
    /// Record a simulator in `.smix/sims.json` under an alias, creating
    /// the registry when absent. This is the bootstrap: alias-form
    /// device refs fail on a fresh checkout until a registry exists.
    /// Device name / runtime / device type are read from `simctl list`,
    /// so only the UDID and alias are needed.
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
    },
    /// Boot a simulator.
    Boot { device: String },
    /// Shutdown a simulator.
    Shutdown { device: String },
    /// Erase a simulator's data.
    Erase { device: String },
    /// Take a screenshot (PNG). Pass `-` to write raw PNG to stdout.
    Screenshot { device: String, out: PathBuf },
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
    Terminate { device: String, bundle_id: String },
    /// Install an .app bundle.
    Install { device: String, app_path: PathBuf },
    /// Uninstall an app by bundle id.
    Uninstall { device: String, bundle_id: String },
    /// Open a URL on the simulator.
    Openurl { device: String, url: String },
    /// Set simulator UI appearance (light / dark).
    Appearance {
        device: String,
        #[arg(value_parser = parse_appearance)]
        mode: Appearance,
    },
    /// Reset keychain on a simulator.
    KeychainReset { device: String },
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

/// Resolve a device ref to a UDID. Explicit UDID short-circuits without
/// touching the registry; aliases need a readable .smix/sims.json (env
/// SMIX_SIMS_JSON overrides upward discovery from cwd).
fn resolve_device(device_ref: &str) -> Result<String, CliError> {
    if registry::is_udid(device_ref) {
        return Ok(device_ref.to_ascii_uppercase());
    }
    let path = registry_path()?;
    Ok(SimRegistry::load(&path)?.resolve(device_ref)?)
}

/// Resolve the path to `.smix/sims.json` (env override or upward
/// discovery from cwd). Extracted from [`resolve_device`] so the caller
/// can also load a [`SimRegistry`] to read sim spec fields like `locale`.
/// Returns `Ok(None)` only when an explicit UDID was given upstream and
/// the registry is genuinely absent — the caller passes the UDID through
/// without spec lookup.
fn registry_path() -> Result<PathBuf, CliError> {
    if let Some(p) = std::env::var_os("SMIX_SIMS_JSON") {
        return Ok(PathBuf::from(p));
    }
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::Other(format!("cannot determine cwd: {e}")))?;
    SimRegistry::discover(&cwd).ok_or_else(|| {
        CliError::Other(format!(
            "no .smix/sims.json was found upward from {} — pass an explicit \
             UDID or set SMIX_SIMS_JSON",
            cwd.display()
        ))
    })
}

/// Best-effort `RegisteredSim` lookup. Returns `None` (not an error)
/// when the device was given as a raw UDID with no registry entry for
/// it — `smix sim boot <unregistered-udid>` is legitimate.
fn lookup_registered(device_ref: &str) -> Option<smix_simctl::registry::RegisteredSim> {
    let path = registry_path().ok()?;
    let reg = SimRegistry::load(&path).ok()?;
    reg.lookup(device_ref).cloned()
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
    // to wipe the in-memory ring. Path is $XDG_DATA_HOME/smix or
    // ~/.local/share/smix; best-effort — a missing $HOME is a no-op.
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    {
        let subprocess_ring_path = dir.join("smix/subprocess-ring.json");
        smix_simctl::set_subprocess_ring_persist_path(subprocess_ring_path);
        // resetAppData counter persistence so
        // `smix diagnostic dump` (later, separate process) sees the
        // count from any prior `smix run` invocations.
        let reset_counters_path = dir.join("smix/reset-app-data-counters.json");
        smix_simctl::set_reset_app_data_counters_persist_path(reset_counters_path);
        // Flow-attempts persistence for retry
        // attribution. `smix run` records per-flow attempts here,
        // `smix diagnostic dump` reads back for the `recent flows`
        // section.
        let flow_attempts_path = dir.join("smix/flow-attempts.json");
        smix_simctl::set_flow_attempts_persist_path(flow_attempts_path);
    }

    let simctl = SimctlClient::new();
    match cli.cmd {
        Cmd::Doctor => cmd_doctor(&simctl).await?,
        Cmd::Diagnostic { action } => cmd_diagnostic(action).await?,
        Cmd::Sim { action } => match action {
            SimAction::List { json } => cmd_sim_list(&simctl, json).await?,
            SimAction::Resolve { device } => {
                println!("{}", resolve_device(&device)?);
            }
            SimAction::Register {
                alias,
                udid,
                locale,
                runner_port,
            } => {
                let udid = udid.to_ascii_uppercase();
                if !registry::is_udid(&udid) {
                    return Err(CliError::Other(format!(
                        "--udid {udid:?} is not UDID-form (8-4-4-4-12 hex); \
                         find it via `smix sim list`"
                    )));
                }
                let devices = simctl.list_devices().await?;
                let device = devices
                    .iter()
                    .find(|d| d.udid.eq_ignore_ascii_case(&udid))
                    .ok_or_else(|| {
                        CliError::Other(format!(
                            "simctl knows no device {udid} — check `smix sim list`"
                        ))
                    })?;
                // Env override, discovered registry, else a fresh
                // `.smix/sims.json` in cwd — register is the one verb
                // that must work before the file exists.
                let path = registry_path().unwrap_or_else(|_| PathBuf::from(".smix/sims.json"));
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
                simctl.boot(&udid).await?;
                println!("booted: {udid}");
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
                simctl.shutdown(&udid).await?;
                println!("shutdown: {udid}");
            }
            SimAction::Erase { device } => {
                let udid = resolve_device(&device)?;
                simctl.erase(&udid).await?;
                println!("erased: {udid}");
            }
            SimAction::Screenshot { device, out } => {
                let udid = resolve_device(&device)?;
                let png = simctl.screenshot(&udid).await?;
                if out.as_os_str() == "-" {
                    use std::io::Write;
                    std::io::stdout()
                        .write_all(&png)
                        .map_err(|e| CliError::Other(format!("write stdout: {e}")))?;
                } else {
                    std::fs::write(&out, &png)
                        .map_err(|e| CliError::Other(format!("write {}: {e}", out.display())))?;
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
                let udid = resolve_device(&device)?;
                simctl
                    .install(&udid, &app_path.display().to_string())
                    .await?;
                println!("installed: {} on {udid}", app_path.display());
            }
            SimAction::Uninstall { device, bundle_id } => {
                let udid = resolve_device(&device)?;
                simctl.uninstall(&udid, &bundle_id).await?;
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
            SimAction::KeychainReset { device } => {
                let udid = resolve_device(&device)?;
                simctl.keychain_reset(&udid).await?;
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
        },
        Cmd::Runner { action } => {
            let root = smix_workspace_root()?;
            match action {
                RunnerAction::Up {
                    device,
                    platform,
                    bundle,
                    runner_project,
                    runner_port: port_flag,
                    supervise,
                } => {
                    if platform == RunPlatform::Android {
                        let port = port_flag.unwrap_or(runner_android::DEFAULT_ANDROID_PORT);
                        // The adb serial IS the device id — there is no
                        // registry indirection on this path.
                        runner_android::up(&root, &device, port, 180).map_err(CliError::Other)?;
                        return Ok(std::process::ExitCode::SUCCESS);
                    }
                    // Port priority chain:
                    //   1. `--runner-port` flag / SMIX_RUNNER_PORT env
                    //   2. `.smix/sims.json` `runnerPort` field for this alias
                    //   3. 22087 default (CLI convention)
                    let sims_port = lookup_registered(&device).and_then(|s| s.runner_port);
                    let port = port_flag.or(sims_port).unwrap_or(22087);
                    let udid = resolve_device(&device)?;
                    // Bare `smix runner up` defaults to record_enabled=false;
                    // the capsule path (`capsule::up`) overrides to true
                    // via TEST_RUNNER_SMIX_RECORD_ENABLED=1.
                    runner::up_with_options(
                        &root,
                        &udid,
                        port,
                        bundle.as_deref(),
                        false,
                        runner_project.as_deref(),
                        supervise,
                    )
                    .map_err(CliError::Other)?;
                }
                RunnerAction::Down { platform, device } => {
                    if platform == RunPlatform::Android {
                        let serial = device.ok_or_else(|| {
                            CliError::Other(
                                "runner down --platform android needs --device \
                                 <adb-serial>: an adb command without one acts on \
                                 whichever device is attached"
                                    .to_string(),
                            )
                        })?;
                        let port = std::env::var("SMIX_RUNNER_PORT")
                            .ok()
                            .and_then(|p| p.parse().ok())
                            .unwrap_or(runner_android::DEFAULT_ANDROID_PORT);
                        runner_android::down(&root, &serial, port).map_err(CliError::Other)?;
                        return Ok(std::process::ExitCode::SUCCESS);
                    }
                    let port = runner_port();
                    runner::down(&root, port).map_err(CliError::Other)?;
                }
                RunnerAction::Cycle { runner_project } => {
                    let port = runner_port();
                    runner::cycle(&root, port, runner_project.as_deref())
                        .map_err(CliError::Other)?;
                }
                RunnerAction::Supervise { runner_project } => {
                    runner::supervise(&root, runner_project.as_deref()).map_err(CliError::Other)?;
                }
                RunnerAction::ListSessions => {
                    let port = runner_port();
                    let client = smix_runner_client::HttpRunnerClient::new(port);
                    let rt = tokio::runtime::Runtime::new()
                        .map_err(|e| CliError::Other(format!("tokio runtime: {e}")))?;
                    let resp = rt
                        .block_on(client.list_sessions())
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
                        runner::installed_runner_dir()
                            .unwrap_or_else(|| PathBuf::from("~/.local/share/smix/runner"))
                    });
                    if !force {
                        // Delegate to the same auto-sync used inside
                        // `runner up`. Idempotent when already current.
                        match runner::ensure_installed_runner_synced(&target) {
                            Ok(runner::SyncOutcome::AlreadyCurrent) => {
                                println!(
                                    "runner install: already at v{} — nothing to do (pass --force to re-extract).",
                                    smix_runner_sources::SOURCES_VERSION
                                );
                            }
                            Ok(runner::SyncOutcome::Extracted {
                                previous_version, ..
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
                                println!(
                                    "runner install: extracted {} files at v{} into {}{}.",
                                    report.file_count,
                                    report.version_written,
                                    target.display(),
                                    backup_note
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
                    no_capture,
                } => {
                    let udid = resolve_device(&device)?;
                    capsule::up(capsule::UpOptions {
                        root: &root,
                        udid: &udid,
                        runner_port: port,
                        capture_endpoint: &capture_endpoint,
                        bundle: Some(&bundle),
                        soft,
                        no_capture,
                    })
                    .await
                    .map_err(CliError::Other)?;
                }
                CapsuleAction::Down { device } => {
                    let udid = resolve_device(&device)?;
                    capsule::down(&root, &udid).await.map_err(CliError::Other)?;
                }
            }
        }
        Cmd::Tap { selector, port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_tap(selector, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Find { selector, port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_find(selector, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::WaitFor {
            selector,
            timeout,
            port,
        } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_wait_for(selector, timeout, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Fill {
            selector,
            text,
            port,
        } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_fill(selector, text, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::PressKey { key, port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_press_key(key, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Scroll {
            selector,
            direction,
            port,
        } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_scroll(selector, direction, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::HideKeyboard { port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_hide_keyboard(p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Tree { json, port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_tree(json, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Describe { json, port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_describe(json, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::SystemPopups { json, port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_system_popups(json, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::SystemPopupAction {
            popup_id,
            button_id,
            port,
        } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            act::cmd_system_popup_action(&popup_id, &button_id, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::RunScript { path, port } => {
            let p = port.unwrap_or_else(act::runner_port_from_env);
            script::cmd_run_script(&path, p)
                .await
                .map_err(|e| CliError::Other(e.to_string()))?;
        }
        Cmd::Run {
            flows,
            device,
            bundle_id,
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
            let switches = runner::load_switches();
            let sw_auto_ocr =
                runner::resolve_switch(switches.auto_ocr_fallback, "SMIX_AUTO_OCR_FALLBACK");
            let sw_ai_assertions =
                runner::resolve_switch(switches.enable_ai_assertions, "SMIX_ENABLE_AI_ASSERTIONS");
            let sw_assert_no_autorecord = runner::resolve_switch(
                switches.assert_screenshot_no_autorecord,
                "SMIX_ASSERT_SCREENSHOT_NO_AUTORECORD",
            );
            let sw_launch_reinstall = runner::resolve_switch(
                switches.launch_fresh_force_reinstall,
                "SMIX_LAUNCH_FRESH_FORCE_REINSTALL",
            );
            let warn_if_env = |r: &runner::ResolvedSwitch, env_name: &str, key: &str| {
                if r.source == runner::SwitchSource::Env {
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
            let port = runner_port
                .or_else(|| {
                    device
                        .as_deref()
                        .and_then(lookup_registered)
                        .and_then(|sim| sim.runner_port)
                })
                .unwrap_or(22087);
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
        Cmd::Authoring { action } => {
            let port = act::runner_port_from_env();
            match action {
                AuthoringAction::Suggest {
                    partial,
                    port: p_override,
                } => {
                    return authoring::cmd_suggest(p_override.unwrap_or(port), partial).await;
                }
                AuthoringAction::CaptureTree {
                    output,
                    port: p_override,
                } => {
                    return authoring::cmd_capture_tree(p_override.unwrap_or(port), output).await;
                }
                AuthoringAction::DiffTree {
                    baseline,
                    port: p_override,
                } => {
                    return authoring::cmd_diff_tree(p_override.unwrap_or(port), baseline).await;
                }
                AuthoringAction::Record {
                    output,
                    duration_secs,
                    interval_ms,
                    port: p_override,
                } => {
                    return authoring::cmd_record_session(
                        p_override.unwrap_or(port),
                        duration_secs,
                        interval_ms,
                        output,
                    )
                    .await;
                }
            }
        }
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
    runner::workspace_root(&cwd).ok_or_else(|| {
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

async fn cmd_doctor(simctl: &SimctlClient) -> Result<(), CliError> {
    println!("smix doctor");
    println!("============");

    // 1. xcrun simctl reachable + runtimes listable.
    let runtimes = simctl.list_runtimes().await.map_err(|e| {
        CliError::Other(format!(
            "xcrun simctl unavailable — check Xcode command-line tools install: {e}"
        ))
    })?;
    let avail = runtimes.iter().filter(|r| r.is_available).count();
    println!(
        "✓ xcrun simctl reachable; {} runtimes detected ({} available)",
        runtimes.len(),
        avail
    );

    // 2. Device inventory.
    let devices = simctl.list_devices().await?;
    let avail_dev = devices.iter().filter(|d| d.is_available).count();
    let booted = devices.iter().filter(|d| d.state == "Booted").count();
    println!(
        "✓ {} devices total ({} available, {} booted)",
        devices.len(),
        avail_dev,
        booted
    );

    // 3. iOS-only enforcement reminder.
    println!("ℹ smix supports iOS Simulator only — real-device automation is");
    println!("  explicitly out of scope.");

    Ok(())
}

async fn cmd_sim_list(simctl: &SimctlClient, json: bool) -> Result<(), CliError> {
    let devices = simctl.list_devices().await?;
    if json {
        let out = serde_json::to_string_pretty(&devices)
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

// ---- tests --------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const UDID: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";

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
}
