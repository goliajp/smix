//! Flow execution entry point — shared by `smix run` (in smix-cli) and
//! the legacy `smix-maestro test` binary. Both call [`run_flow`] with
//! the same args; the surface routing stays in their respective binaries.

use crate::{Adapter, AppsConfig, RunError, RunReport, RunStepReport, Step, parse_flow_file};
use smix_driver::Platform as DriverPlatform;
use smix_sdk::App;
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Target platform for a flow run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowPlatform {
    /// iOS Simulator via XCUITest runner.
    Ios,
    /// Android emulator via Kotlin instrumentation runner.
    Android,
}

impl FlowPlatform {
    fn to_driver(self) -> DriverPlatform {
        match self {
            Self::Ios => DriverPlatform::Ios,
            Self::Android => DriverPlatform::Android,
        }
    }
}

/// Output format for `smix run`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Default. Human-readable stderr summary + `to_prompt` failure
    /// block on stderr. stdout is quiet on success.
    #[default]
    Human,
    /// stderr keeps human summary; stdout emits a single top-level
    /// JSON object at exit time containing run report + any terminal
    /// ExpectationFailure. Consumers `JSON.parse(runCmd(...).stdout)`.
    Json,
    /// JUnit XML output. stdout emits a `<testsuite>` with one
    /// `<testcase>` per yaml flow processed; failures carry the smix
    /// error message + failure code in a `<failure>` sub-element.
    Junit,
}

/// All inputs to a single flow run.
pub struct FlowArgs {
    /// Path to the flow yaml file.
    pub flow: PathBuf,
    /// Optional device UDID (iOS) or device id (Android, e.g. `emulator-5554`).
    /// If `None`, falls back to platform default (env / auto-pick).
    pub udid: Option<String>,
    /// Default bundle id / Android package. Yaml header `appId:` /
    /// `app:` overrides per flow.
    pub bundle_id: String,
    /// Runner HTTP port.
    pub runner_port: u16,
    /// Skip the initial `App::foreground` call. Use when the app is
    /// already on screen.
    pub no_launch: bool,
    /// Target platform.
    pub platform: FlowPlatform,
    /// Optional `smix-apps.yaml` cross-platform resolver config.
    pub apps_config: Option<PathBuf>,
    /// Env vars for yaml `${NAME}` interpolation. CLI
    /// `--env KEY=VAL` (repeatable) → this vec. Merged with the
    /// inherited process env by `run_flow` (CLI wins).
    pub env_vars: Vec<(String, String)>,
    /// Output directory for per-step JSON events + run summary. When
    /// Some, `run_flow` writes `run-summary.json` at the end + per-step
    /// files during execution. Matches Maestro's `--debug-output`
    /// semantics.
    pub debug_output: Option<PathBuf>,
    /// Verbose logging. Turns on debug-level tracing for adapter / sdk /
    /// driver crates. Default keeps info.
    pub verbose: bool,
    /// Output format for the final summary / failure. Default `Human`.
    pub format: OutputFormat,
    /// When true, send `App-Activate: true` on every runner request so
    /// XCUITest `.activate()`s the resolved target before operating.
    /// Opt-in via `smix run --activate`.
    pub auto_activate: bool,
    /// Metro log source URL. `ws://...` starts a WebSocket subscriber;
    /// `file://<path>` starts a file tail. None → no metro tail
    /// (`expect.signal` verbs will error if used).
    pub metro_log_url: Option<String>,
    /// Append an implicit `expect.signal { regex }` step at the end of
    /// the flow.
    pub await_signal: Option<String>,
    /// v1.0.4 §B / D9 — prepend an implicit `expect.signal { regex,
    /// timeoutMs }` step at the START of the flow, before any yaml
    /// step runs. Blocks until the regex is observed in the metro log
    /// tail. Symmetric to [`Self::await_signal`] which fires at end
    /// of flow. Requires `metro_log_url` also set — otherwise the
    /// gate-signal step errors with an actionable hint.
    pub gate_signal: Option<String>,
    /// v1.0.4 §B / D9 — timeout (ms) for [`Self::gate_signal`]. Default
    /// 60_000. Zero disables the timeout (waits forever — usually not
    /// what you want in CI).
    pub gate_signal_timeout_ms: u64,
    /// Append an implicit `expectLogClean` step at the end of the flow.
    pub expect_log_clean: bool,
    /// Path to a JSON fixture registry file. When Some,
    /// `- fixture: <id>` yaml verbs resolve against this. None → the
    /// verb errors with an actionable hint.
    pub fixture_registry: Option<std::path::PathBuf>,
    /// Force key-event dispatch mode for text input verbs. Opt-in via
    /// `smix run --force-key-events`.
    pub force_key_events: bool,
    /// Disable auto-annotate on the `--debug-output` fail-PNG. Default
    /// false = annotate. Opt-out via `smix run --no-fail-annotate`.
    pub no_fail_annotate: bool,
}

/// Run a flow file end-to-end. Async because runner / sdk calls are
/// async. Returns an [`ExitCode`] suitable for direct return from
/// `main`. Exit codes:
///
/// - 0  success
/// - 2  yaml parse error
/// - 3  runtime SDK failure
/// - 4  unknown verb / direction
/// - 5  runFlow cycle / file IO
/// - 6  runner unreachable
pub async fn run_flow(args: FlowArgs) -> ExitCode {
    // 1. connect runner — iOS swift smix-runner OR Android Kotlin runner.
    let app = match args.platform {
        FlowPlatform::Ios => App::connect_to_runner(args.runner_port).await,
        FlowPlatform::Android => App::connect_to_runner_android(args.runner_port).await,
    };
    let app = match app {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "error: connect_to_runner({}, platform={:?}) failed: {e}",
                args.runner_port, args.platform
            );
            return ExitCode::from(6);
        }
    };
    let configure = |app: smix_sdk::App| -> smix_sdk::App {
        let app = if let Some(u) = args.udid.as_deref() {
            app.with_udid(u)
        } else {
            app
        };
        // Thread bundle_id + auto_activate through the driver so every
        // runner request carries `App-Bundle-Id` / `App-Activate` /
        // `Session-Id` headers.
        let app = app.with_bundle_id(&args.bundle_id);
        let app = if args.auto_activate {
            app.with_auto_activate(true)
        } else {
            app
        };
        if args.force_key_events {
            app.with_force_key_events(true)
        } else {
            app
        }
    };
    let app = configure(app);

    // v1.0.3 — session lifecycle. `smix run` opens a runner-side
    // session at start-up and closes it on exit. Every request in
    // between carries `Session-Id`, so the runner short-circuits
    // per-request `.activate()` — eliminating the activation storm at
    // the root without any yaml-side change.
    //
    // Runners that don't implement `/session/open` (older v1.0.x) will
    // return non-2xx; on that path we WARN + reconnect + continue with
    // the legacy per-request rebind (which as of v1.0.2 is itself
    // rate-limited to 1 activate / 5 s / bundle-id, so still safe).
    enum AppHolder {
        Session(smix_sdk::Session),
        Loose(smix_sdk::App),
    }
    impl AppHolder {
        fn app(&self) -> &smix_sdk::App {
            match self {
                AppHolder::Session(s) => s.app(),
                AppHolder::Loose(a) => a,
            }
        }
    }
    let holder = match args.platform {
        FlowPlatform::Ios => match app.open_session(&args.bundle_id, args.auto_activate).await {
            Ok(s) => AppHolder::Session(s),
            Err(e) => {
                eprintln!(
                    "WARN: /session/open failed ({e}); falling back to legacy \
                     per-request path (rate-limited to 1 activate / 5 s / bundle-id)"
                );
                let fresh = match App::connect_to_runner(args.runner_port).await {
                    Ok(a) => configure(a),
                    Err(e2) => {
                        eprintln!("error: reconnect after session-open fail: {e2}");
                        return ExitCode::from(6);
                    }
                };
                AppHolder::Loose(fresh)
            }
        },
        FlowPlatform::Android => AppHolder::Loose(app),
    };

    // 2. foreground unless --no-launch (iOS only; Android brings app
    // to foreground via launchApp step).
    if !args.no_launch
        && args.platform == FlowPlatform::Ios
        && let Err(e) = holder.app().foreground(&args.bundle_id).await
    {
        eprintln!("error: foreground({}) failed: {e}", args.bundle_id);
        return ExitCode::from(3);
    }

    // 3. parse yaml.
    let mut flow = match parse_flow_file(&args.flow) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: parse {}: {e}", args.flow.display());
            return ExitCode::from(2);
        }
    };

    // v1.0.4 §B — prepend gate-signal step at the START of the flow.
    // Insertion at index 0 so it runs BEFORE any yaml step, matching
    // the "hold the entire flow until this regex appears" contract.
    // Timeout defaults to 60s at the FlowArgs layer; here we pass it
    // through verbatim.
    if let Some(regex) = &args.gate_signal {
        flow.steps.insert(
            0,
            crate::Step::ExpectSignal {
                regex: regex.clone(),
                level: None,
                timeout_ms: args.gate_signal_timeout_ms,
                window: crate::SignalWindow::SinceRun,
            },
        );
    }

    // Append implicit signal-await / log-clean steps when CLI opted
    // in. These run AFTER the yaml's own steps, acting as post-flow
    // gates.
    if let Some(regex) = &args.await_signal {
        flow.steps.push(crate::Step::ExpectSignal {
            regex: regex.clone(),
            level: None,
            timeout_ms: 8000,
            window: crate::SignalWindow::SinceRun,
        });
    }
    if args.expect_log_clean {
        flow.steps.push(crate::Step::ExpectLogClean);
    }

    // Resolve the cross-platform `app:` logical key via smix-apps.yaml
    // when both are provided.
    if let Some(config_path) = &args.apps_config {
        let cfg = match AppsConfig::from_path(config_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: apps-config {}: {e}", config_path.display());
                return ExitCode::from(2);
            }
        };
        if let Err(e) = crate::resolve_app_into_flow(&mut flow, &cfg, args.platform.to_driver()) {
            eprintln!("error: resolve app: {e}");
            return ExitCode::from(2);
        }
    }

    // Dispatch every step. Per-step progress to stderr.
    let total = flow.steps.len();
    for (idx, step) in flow.steps.iter().enumerate() {
        eprintln!("STEP {}/{}: {}", idx + 1, total, summarize_step(step));
    }

    let base_dir = args
        .flow
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Build the env store: process env as the base + CLI
    // `--env KEY=VAL` overrides. Adapter.env consumes this.
    let mut env_store: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    for (k, v) in &args.env_vars {
        env_store.insert(k.clone(), v.clone());
    }

    // Wire env + debug_output into the Adapter. Adapter writes per-step
    // JSON + on-fail screenshot into debug_output/; entry.rs adds the
    // aggregate run-summary.json at exit (below).
    let mut adapter = Adapter::new(holder.app(), base_dir).with_env(env_store);
    if let Some(dir) = args.debug_output.clone() {
        adapter = adapter.with_debug_output(dir);
    }
    // no_fail_annotate flows into the Adapter for the debug-output
    // fail-PNG auto-annotate opt-out.
    adapter = adapter.with_no_fail_annotate(args.no_fail_annotate);

    // Attach fixture registry if configured.
    if let Some(path) = &args.fixture_registry {
        match smix_fixture::FixtureRegistry::load(path) {
            Ok(reg) => {
                adapter = adapter.with_fixture_registry(reg);
            }
            Err(e) => {
                eprintln!(
                    "WARN: fixture registry {}: {e}; `- fixture:` verbs will error",
                    path.display()
                );
            }
        }
    }

    // Attach metro log tail if configured. Subscriber handle is dropped
    // at scope exit → subscriber task shuts down.
    let _subscriber_guard = if let Some(url) = &args.metro_log_url {
        let tail = smix_metro_log::MetroLogTail::new();
        adapter = adapter.with_metro_tail(tail.clone());
        if let Some(path) = url.strip_prefix("file://") {
            Some(smix_metro_log::FileTailSubscriber::start(
                std::path::PathBuf::from(path),
                tail,
            ))
        } else if url.starts_with("ws://") || url.starts_with("wss://") {
            Some(smix_metro_log::WebSocketSubscriber::start(
                url.clone(),
                tail,
            ))
        } else if url == "-" || url.starts_with("stdin://") {
            // Consumer pipes metro log lines to stdin (e.g.
            // `bun run metro-tail | smix run --metro-log-url -`).
            // Uses the same FileTailSubscriber but polls /dev/stdin.
            Some(smix_metro_log::FileTailSubscriber::start(
                std::path::PathBuf::from("/dev/stdin"),
                tail,
            ))
        } else {
            eprintln!(
                "WARN: --metro-log-url {url} not recognized (expected ws:// | wss:// | file:// | - | stdin://); metro tail not started"
            );
            None
        }
    } else {
        None
    };

    // v1.0.4 §D15 / feedback lifecycle-safe-exit — race the flow
    // execution against SIGINT / SIGTERM. On signal we abandon the
    // in-flight flow (its next .await point drops out), run the
    // session-close cascade below, and exit with the POSIX-conventional
    // 130 (SIGINT) or 143 (SIGTERM). Without this handler, ctrl-C
    // would drop the tokio runtime and orphan the runner-side session
    // until its idle-close timer fired.
    let mut interrupted_by: Option<&'static str> = None;
    let result = {
        let flow_task = adapter.run(&flow);
        tokio::pin!(flow_task);
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        let sigterm_fut = async {
            if let Some(s) = sigterm.as_mut() {
                s.recv().await;
            } else {
                std::future::pending::<Option<()>>().await;
            }
        };
        tokio::pin!(sigterm_fut);
        tokio::select! {
            r = &mut flow_task => r,
            _ = tokio::signal::ctrl_c() => {
                interrupted_by = Some("SIGINT");
                eprintln!("interrupted (SIGINT) — running session-close cascade");
                Err(crate::RunError::Sdk(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: "smix run interrupted by SIGINT".to_string(),
                    ..Default::default()
                })))
            },
            _ = &mut sigterm_fut => {
                interrupted_by = Some("SIGTERM");
                eprintln!("interrupted (SIGTERM) — running session-close cascade");
                Err(crate::RunError::Sdk(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: "smix run interrupted by SIGTERM".to_string(),
                    ..Default::default()
                })))
            },
        }
    };
    // v1.0.4 §D11 — snapshot per-step trace records BEFORE dropping
    // the adapter, so a failed run still surfaces its partial trace
    // in run-summary.json.
    let debug_records: Vec<crate::StepDebugRecord> = adapter.debug_records().to_vec();
    drop(adapter);

    // v1.0.3 — release the session (best-effort). Errors here are
    // warnings only: the flow's result is already captured. The
    // dispatch is a small POST /session/close and the runner's
    // idempotent contract means "already gone" is not an error.
    //
    // v1.0.4 §D15 — best-effort session close under a tight 2 s
    // timeout even on interrupt paths, so ctrl-C actually terminates
    // instead of hanging behind a dead runner.
    if let AppHolder::Session(session) = holder {
        let close_fut = session.close();
        match tokio::time::timeout(std::time::Duration::from_secs(2), close_fut).await {
            Ok(Ok(_app)) => {}
            Ok(Err(e)) => eprintln!("WARN: /session/close failed at exit: {e}"),
            Err(_) => eprintln!(
                "WARN: /session/close timed out after 2s — session may linger \
                 until the runner's idle-close sweep fires"
            ),
        }
    }

    // debug-output + json format handling. debug_output writes a
    // summary JSON at exit; format=Json emits the same to stdout.
    // Both work together (debug_output for the on-disk artifact,
    // format=Json for pipe consumption).
    if let Some(dir) = &args.debug_output
        && let Err(e) = write_debug_output(dir, args.flow.as_path(), &result, &debug_records)
    {
        eprintln!("WARN: debug-output write failed: {e}");
    }

    match result {
        Ok(report) => {
            for w in &report.warnings {
                eprintln!("WARN: {w}");
            }
            // With --format json, stdout is reserved for the single
            // top-level report JSON object. Skip the legacy human/stats
            // summary line (which was already JSON-shaped but under a
            // different schema).
            match args.format {
                OutputFormat::Json => emit_json_success(&args.flow, &report),
                OutputFormat::Junit => emit_junit(&args.flow, Ok(&report)),
                OutputFormat::Human => print_summary(&report),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            match args.format {
                OutputFormat::Json => emit_json_failure(&args.flow, &e),
                OutputFormat::Junit => emit_junit(&args.flow, Err(&e)),
                OutputFormat::Human => {}
            }
            // v1.0.4 §D15 — POSIX-conventional exit codes for signal
            // interrupts override the RunError-based classification.
            match interrupted_by {
                Some("SIGINT") => ExitCode::from(130),
                Some("SIGTERM") => ExitCode::from(143),
                _ => run_error_to_exit(&e),
            }
        }
    }
}

/// Emit a JUnit XML `<testsuite>` with one `<testcase>` per yaml flow.
/// Failures embed the smix error message + failure code in a
/// `<failure>` sub-element.
///
/// The schema follows the surface-common Jenkins/GoCD JUnit variant
/// (widely accepted by CI dashboards, GitHub Actions, GitLab).
fn emit_junit(flow: &Path, outcome: Result<&crate::RunReport, &crate::RunError>) {
    let flow_name = flow
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("smix-flow");
    let (failures, xml_body, warnings_ct) = match outcome {
        Ok(report) => (0, String::new(), report.warnings.len()),
        Err(e) => {
            let msg = xml_escape(&format!("{e}"));
            let kind = xml_escape(&junit_failure_kind(e));
            let cdata = format!("{e}");
            (
                1,
                format!(
                    "      <failure type=\"{kind}\" message=\"{msg}\"><![CDATA[{cdata}]]></failure>\n",
                ),
                0,
            )
        }
    };
    let flow_esc = xml_escape(flow_name);
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuite name=\"smix\" tests=\"1\" failures=\"{failures}\" errors=\"0\" skipped=\"0\">\n"
    ));
    out.push_str(&format!(
        "  <testcase name=\"{flow_esc}\" classname=\"smix.flow\" time=\"0\">\n"
    ));
    out.push_str(&xml_body);
    if warnings_ct > 0 {
        out.push_str(&format!(
            "      <system-err>{warnings_ct} warning(s) emitted</system-err>\n"
        ));
    }
    out.push_str("  </testcase>\n");
    out.push_str("</testsuite>\n");
    println!("{out}");
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn junit_failure_kind(err: &crate::RunError) -> String {
    // Kind categorization for junit `<failure type=...>`. Match on
    // the RunError variant name via Debug for forward-compat with
    // new variants (unknown → smix.error).
    let dbg = format!("{err:?}");
    let leading = dbg.split_once('(').map(|(a, _)| a).unwrap_or(&dbg);
    format!("smix.{}", leading.to_lowercase())
}

// debug-output writer. MVP shape: writes one `run-summary.json` at run
// end. Per-step files + on-fail screenshots are deferred to a future
// increment (they need a coordinated hook inside the adapter's
// run_step loop).
fn write_debug_output(
    dir: &Path,
    flow: &Path,
    result: &Result<crate::RunReport, crate::RunError>,
    debug_records: &[crate::StepDebugRecord],
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let summary = build_summary_json(flow, result.as_ref(), debug_records);
    let path = dir.join("run-summary.json");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&summary).unwrap_or_default(),
    )?;
    Ok(())
}

fn build_summary_json(
    flow: &Path,
    result: Result<&crate::RunReport, &crate::RunError>,
    debug_records: &[crate::StepDebugRecord],
) -> serde_json::Value {
    use serde_json::json;
    match result {
        Ok(report) => json!({
            "flow": flow.display().to_string(),
            "runOutcome": "success",
            "warnings": report.warnings,
            // v1.0.4 §D11 — per-step trace for post-mortem. Empty
            // unless --debug-output was set.
            "steps": debug_records,
        }),
        Err(e) => json!({
            "flow": flow.display().to_string(),
            "runOutcome": "failure",
            "error": e.to_string(),
            // v1.0.4 §D11 — partial trace up to the failing step.
            "steps": debug_records,
        }),
    }
}

fn emit_json_success(flow: &Path, report: &crate::RunReport) {
    // Stdout emission path: no debug_records available at this call
    // site (the CLI aggregate that owns them writes them to disk via
    // write_debug_output). Pass empty slice.
    let payload = build_summary_json(flow, Ok(report), &[]);
    println!("{}", serde_json::to_string(&payload).unwrap_or_default());
}

fn emit_json_failure(flow: &Path, err: &crate::RunError) {
    use serde_json::json;
    // For SDK failures, surface the ExpectationFailure's own
    // structured form when we can. For parse/io/unknown, plain string.
    let payload = if let crate::RunError::Sdk(f) = err {
        json!({
            "flow": flow.display().to_string(),
            "runOutcome": "failure",
            "failure": {
                "code": format!("{:?}", f.code),
                "message": f.message,
                "selector": f.selector.as_ref().map(|s| format!("{s:?}")),
                "suggestions": f.suggestions,
                "visibleCount": f.visible_elements.len(),
            }
        })
    } else {
        build_summary_json(flow, Err(err), &[])
    };
    println!("{}", serde_json::to_string(&payload).unwrap_or_default());
}

fn run_error_to_exit(e: &RunError) -> ExitCode {
    match e {
        RunError::Parse(_) => ExitCode::from(2),
        RunError::Sdk(_) => ExitCode::from(3),
        RunError::UnknownKey(_) | RunError::UnknownDirection(_) => ExitCode::from(4),
        RunError::RunFlowCycle(_) | RunError::Io(_) => ExitCode::from(5),
    }
}

fn summarize_step(step: &Step) -> String {
    match step {
        Step::TapOn { optional, .. } => {
            if *optional {
                "tapOn (optional)".into()
            } else {
                "tapOn".into()
            }
        }
        Step::TapAtPoint { nx, ny } => format!("tapAtPoint ({nx:.3}, {ny:.3})"),
        Step::WebViewEval { js, .. } => {
            let preview = if js.len() > 40 { &js[..40] } else { js };
            format!("webview_eval `{preview}`")
        }
        Step::WaitForAnimationToEnd => "waitForAnimationToEnd".into(),
        Step::ExtendedWaitUntil { timeout_ms, .. } => {
            format!("extendedWaitUntil (timeout {timeout_ms}ms)")
        }
        Step::AssertVisible { .. } => "assertVisible".into(),
        Step::InputText(s) => format!("inputText ({} chars)", s.chars().count()),
        Step::PressKey(k) => format!("pressKey {k}"),
        Step::RunFlow(p) => format!("runFlow {p}"),
        Step::RunFlowConditional { file, .. } => format!("runFlow {file} (conditional)"),
        Step::RunFlowInline {
            steps,
            when_visible,
        } => {
            let cond = if when_visible.is_some() {
                " (conditional)"
            } else {
                ""
            };
            format!("runFlow inline ({} cmds){cond}", steps.len())
        }
        Step::ScrollUntilVisible { direction, .. } => {
            format!("scrollUntilVisible ({direction})")
        }
        Step::EraseText(n) => format!("eraseText ({n} chars)"),
        Step::Swipe { from, to } => format!("swipe {from:?} -> {to:?}"),
        Step::LaunchApp { app_id, .. } => format!("launchApp {app_id}"),
        Step::OpenLink(url) => format!("openLink {url}"),
        Step::StopApp => "stopApp".into(),
        Step::Scroll => "scroll".into(),
        Step::HideKeyboard => "hideKeyboard".into(),
        Step::AssertNotVisible { .. } => "assertNotVisible".into(),
        Step::KillApp { app_id } => format!("killApp {app_id}"),
        Step::ClearState { app_id } => format!("clearState {app_id}"),
        Step::ClearKeychain => "clearKeychain".into(),
        Step::TakeScreenshot { path, annotations } => match (path, annotations.is_empty()) {
            (Some(p), true) => format!("takeScreenshot {p}"),
            (Some(p), false) => format!("takeScreenshot {p} +{} annotate", annotations.len()),
            (None, true) => "takeScreenshot".into(),
            (None, false) => format!("takeScreenshot +{} annotate", annotations.len()),
        },
        Step::SetClipboard(s) => format!("setClipboard ({} chars)", s.chars().count()),
        Step::PasteText { text } => match text {
            Some(s) => format!("pasteText ({} chars)", s.chars().count()),
            None => "pasteText (from clipboard)".into(),
        },
        Step::CopyTextFrom { .. } => "copyTextFrom".into(),
        Step::DoubleTapOn { .. } => "doubleTapOn".into(),
        Step::LongPressOn { duration_ms, .. } => format!("longPressOn ({duration_ms}ms)"),
        Step::AssertTrue { .. } => "assertTrue".into(),
        Step::Repeat { mode, commands } => match mode {
            crate::RepeatMode::Times(n) => format!("repeat × {n} ({} cmds)", commands.len()),
            crate::RepeatMode::While { .. } => format!("repeat while ({} cmds)", commands.len()),
            crate::RepeatMode::WhileVisible { .. } => {
                format!("repeat while visible ({} cmds)", commands.len())
            }
            crate::RepeatMode::WhileNotVisible { .. } => {
                format!("repeat while notVisible ({} cmds)", commands.len())
            }
        },
        Step::Retry {
            max_retries,
            commands,
        } => format!("retry (max {max_retries}, {} cmds)", commands.len()),
        Step::RunScript { .. } => "runScript".into(),
        Step::EvalScript { .. } => "evalScript".into(),
        Step::SetLocation {
            latitude,
            longitude,
        } => format!("setLocation ({latitude}, {longitude})"),
        Step::Travel { points, .. } => format!("travel ({} waypoints)", points.len()),
        Step::SetPermissions { permissions, .. } => {
            format!("setPermissions ({} entries)", permissions.len())
        }
        Step::AddMedia { paths } => format!("addMedia ({} paths)", paths.len()),
        Step::SetOrientation { orientation } => format!("setOrientation {orientation:?}"),
        Step::StartRecording { path } => format!("startRecording {path}"),
        Step::StopRecording => "stopRecording".into(),
        Step::AssertScreenshot {
            path,
            max_hamming,
            mask,
        } => {
            let mut s = format!("assertScreenshot {path}");
            if let Some(c) = max_hamming {
                s.push_str(&format!(" threshold={c}"));
            }
            if !mask.is_empty() {
                s.push_str(&format!(" mask={}regions", mask.len()));
            }
            s
        }
        Step::ExpectSignal {
            regex, timeout_ms, ..
        } => format!("expect.signal /{regex}/ timeoutMs={timeout_ms}"),
        Step::ExpectSignals {
            signals,
            order,
            timeout_ms,
            ..
        } => format!(
            "expect.signals ({} matcher{}, order={:?}) timeoutMs={timeout_ms}",
            signals.len(),
            if signals.len() == 1 { "" } else { "s" },
            order
        ),
        Step::ExpectLogClean => "expect.logClean".into(),
        Step::Fixture { id, timeout_ms } => match timeout_ms {
            Some(t) => format!("fixture {id} (timeoutMs={t})"),
            None => format!("fixture {id}"),
        },
    }
}

fn aggregate(report: &RunReport, total: &mut usize, skipped: &mut usize, expanded: &mut usize) {
    *total += report.steps.len();
    for step in &report.steps {
        if matches!(step, RunStepReport::Skipped { .. }) {
            *skipped += 1;
        }
        if let RunStepReport::ExpandedSubflow { inner: sub, .. } = step {
            *expanded += 1;
            aggregate(sub, total, skipped, expanded);
        }
    }
}

fn print_summary(report: &RunReport) {
    let mut total = 0;
    let mut skipped = 0;
    let mut expanded = 0;
    aggregate(report, &mut total, &mut skipped, &mut expanded);
    let warnings = report.warnings.len();
    let payload = serde_json::json!({
        "ok": true,
        "steps": total,
        "warnings": warnings,
        "skipped": skipped,
        "expanded_subflows": expanded,
    });
    println!("{payload}");
    eprintln!(
        "summary: {total} steps, {warnings} warnings, {skipped} skipped, {expanded} expanded subflows"
    );
}
