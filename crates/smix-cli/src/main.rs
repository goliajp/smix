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
mod script;

use clap::{Parser, Subcommand};
use smix_simctl::registry::{self, RegistryError, SimRegistry};
use smix_simctl::{Appearance, LaunchResult, SimctlClient, SimctlError};
use std::path::PathBuf;
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
    /// Print the runner's high-level ScreenDescription
    /// (title / interactive elements / status bar / etc.).
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
        /// Append an implicit `expect.signal { regex }` step to the end
        /// of each flow. The `--timeout` value is used as the timeout
        /// (default 8000ms).
        #[arg(long = "await-signal")]
        await_signal: Option<String>,
        /// v1.0.4 §B — prepend an implicit `expect.signal { regex,
        /// timeoutMs }` step at the START of the flow, blocking until
        /// the regex is observed in the metro log tail. Symmetric to
        /// `--await-signal`. Requires `--metro-log-url` also set.
        /// Consumers whose visual/perf gates prelaunch the app and
        /// wait for "all systems go" (bootstrap-ready) use this to
        /// avoid a Node-side waitForMetroLogSignal helper.
        #[arg(long = "gate-signal")]
        gate_signal: Option<String>,
        /// v1.0.4 §B — timeout in ms for `--gate-signal`. Default
        /// 60000. Zero disables the timeout (waits forever).
        #[arg(long = "gate-signal-timeout", default_value_t = 60_000)]
        gate_signal_timeout_ms: u64,
        /// Append an implicit `expectLogClean` step to the end of each
        /// flow. Emits an ExpectationFailure if any non-allowlisted log
        /// entry has been observed during the run (allowlist from
        /// `.smix/config.json` `metroLog.allowlist`).
        #[arg(long = "expect-log-clean", default_value_t = false)]
        expect_log_clean: bool,
        /// Metro log source URL, overrides `.smix/config.json`
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
        #[arg(long = "check", default_value_t = false)]
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
    ///     --annotate "circle,at:100,100,color:red,radius:40" \\
    ///     --annotate "arrow,from:10,10,to:200,200,color:blue" \\
    ///     --annotate "text,at:50,50,content:hello,color:green,size:24"
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
    ///   smix authoring suggest 'text: /Sign.*/'
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
        /// Bundle id the runner binds its XCUIApplication to (default:
        /// the runner's built-in default, com.apple.Preferences).
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
        /// v1.0.6 — after `/health` returns 200, spawn a detached
        /// `smix runner supervise` sidecar and record its pid in
        /// `.smix/runner/state.json`. `smix runner down` cascades a
        /// SIGTERM to the sidecar before tearing down xcodebuild.
        /// Sidecar log at `.smix/runner/supervise-<UDID>.log`.
        #[arg(long = "supervise", default_value_t = false)]
        supervise: bool,
    },
    /// Stop the runner (SIGINT-first to avoid the crash-report dialog).
    Down,
    /// v1.0.4 — Cycle the runner: down + up on the same device/port/
    /// bundle. Preserves the per-udid derived-data directory so the
    /// warm re-up finishes in ~3 s. Errors if no runner state.json
    /// exists — use `runner up` for a cold start. See RFC 1.0.4 D5.
    Cycle {
        /// Explicit path to `SmixRunner.xcodeproj`. Same cascade as
        /// `runner up` — see `resolve_runner_project`.
        #[arg(long = "runner-project", env = "SMIX_RUNNER_PROJECT")]
        runner_project: Option<PathBuf>,
    },
    /// v1.0.5 — Attach a supervisor to a running runner: tail its log
    /// and auto-`cycle` on interrupt patterns (`** TEST INTERRUPTED
    /// **` / `SchemeActionResultOperation started unexpectedly`).
    /// Foreground process; SIGINT or SIGTERM cleanly exits. Session
    /// persistence (v1.0.5 D1) preserves consumer session ids across
    /// each cycle. See RFC 1.0.5 D2.
    Supervise {
        /// Explicit path to `SmixRunner.xcodeproj` for the cycle
        /// operation. Same cascade as `runner up`.
        #[arg(long = "runner-project", env = "SMIX_RUNNER_PROJECT")]
        runner_project: Option<PathBuf>,
    },
    /// v1.0.5 — List every session the runner currently tracks.
    /// Reads `POST /session/list`. Useful for post-cycle diagnostics.
    ListSessions,
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
    let simctl = SimctlClient::new();
    match cli.cmd {
        Cmd::Doctor => cmd_doctor(&simctl).await?,
        Cmd::Sim { action } => match action {
            SimAction::List { json } => cmd_sim_list(&simctl, json).await?,
            SimAction::Resolve { device } => {
                println!("{}", resolve_device(&device)?);
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
                    bundle,
                    runner_project,
                    runner_port: port_flag,
                    supervise,
                } => {
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
                RunnerAction::Down => {
                    let port = runner_port();
                    runner::down(&root, port).map_err(CliError::Other)?;
                }
                RunnerAction::Cycle { runner_project } => {
                    let port = runner_port();
                    runner::cycle(&root, port, runner_project.as_deref())
                        .map_err(CliError::Other)?;
                }
                RunnerAction::Supervise { runner_project } => {
                    runner::supervise(&root, runner_project.as_deref())
                        .map_err(CliError::Other)?;
                }
                RunnerAction::ListSessions => {
                    let port = runner_port();
                    let client = smix_runner_client::HttpRunnerClient::new(port);
                    let rt = tokio::runtime::Runtime::new().map_err(|e| {
                        CliError::Other(format!("tokio runtime: {e}"))
                    })?;
                    let resp = rt.block_on(client.list_sessions()).map_err(|e| {
                        CliError::Other(format!("/session/list: {e}"))
                    })?;
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
                                s.session_id,
                                s.bundle_id,
                                s.opened_at_ms,
                                s.last_activated_at_ms,
                            );
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
                    soft,
                    no_capture,
                } => {
                    let udid = resolve_device(&device)?;
                    capsule::up(capsule::UpOptions {
                        root: &root,
                        udid: &udid,
                        runner_port: port,
                        capture_endpoint: &capture_endpoint,
                        bundle: None,
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
            if check {
                let mut fail = 0u8;
                for flow_path in &flows {
                    match std::fs::read_to_string(flow_path) {
                        Ok(yaml) => match smix_adapter_maestro::parse_flow_yaml(&yaml) {
                            Ok(_) => eprintln!("smix run --check: OK  {}", flow_path.display()),
                            Err(e) => {
                                eprintln!("smix run --check: FAIL {}: {e}", flow_path.display());
                                fail = 2;
                            }
                        },
                        Err(e) => {
                            eprintln!("smix run --check: FAIL {}: read: {e}", flow_path.display());
                            fail = 2;
                        }
                    }
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
            let bundle = bundle_id.unwrap_or_else(|| "com.example.app".to_string());
            let port = runner_port.unwrap_or(22087);
            let plat = platform.to_flow();
            let out_fmt = format.to_adapter();

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
                let exit = smix_adapter_maestro::run_flow(smix_adapter_maestro::FlowArgs {
                    flow: flow_path.clone(),
                    udid: udid.clone(),
                    bundle_id: bundle.clone(),
                    runner_port: port,
                    no_launch,
                    platform: plat,
                    apps_config: apps_config.clone(),
                    env_vars: env.clone(),
                    debug_output: per_flow_debug,
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
                })
                .await;
                // Extract per-flow exit code. ExitCode's numeric surface
                // isn't public; use Debug repr as a stable extraction path
                // (the Rust nightly `to_i32` isn't stable). We already own
                // the u8 via the adapter API — see ExitCode::from(u8).
                let code = exit_code_to_u8(exit);
                worst_exit = worst_exit.max(code);
                if fail_fast && code != 0 {
                    eprintln!(
                        "smix run: --fail-fast — aborting batch on first failure (exit={code})"
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
                        if paths.len() > 1 {
                            eprintln!(
                                "smix migrate: rewrote {} ({} renames)",
                                path.display(),
                                report.renamed.len()
                            );
                        }
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
        Ok(ExitCode::from(worst))
    }
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
fn exit_code_to_u8(code: std::process::ExitCode) -> u8 {
    let dbg = format!("{code:?}");
    // e.g. "ExitCode(unix_exit_status(3))"
    dbg.rsplit_once('(')
        .and_then(|(_, tail)| tail.trim_end_matches("))").parse::<u8>().ok())
        .unwrap_or(0)
}

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

    // 3. iOS-only enforcement reminder (CLAUDE.md §9 #1).
    println!("ℹ smix supports iOS Simulator only — real-device automation is");
    println!("  explicitly out of scope per CLAUDE.md §9.");

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
    Simctl(SimctlError),
    Registry(RegistryError),
    Other(String),
}

impl From<SimctlError> for CliError {
    fn from(e: SimctlError) -> Self {
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
