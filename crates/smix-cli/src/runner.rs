//! `smix runner up/down` — XCUITest runner (SmixRunner) lifecycle.
//!
//! The runner is an XCUITest bundle kept alive by `test_runForever`; the
//! host-side `xcodebuild test` process IS the session. A leftover
//! xcodebuild keeps the device's testmanagerd automation slot occupied,
//! blocking every other XCUITest client on that sim — so the process
//! handle lives in `.smix/runner/state.json` and teardown is a product
//! responsibility, not a script convention.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persisted handle for the host-side xcodebuild process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerState {
    pub pid: u32,
    pub udid: String,
    pub port: u16,
    pub log: PathBuf,
    /// Target bundle the runner's XCUIApplication is bound to (None =
    /// runner default, com.apple.Preferences).
    #[serde(default)]
    pub bundle: Option<String>,
    /// Pid of the supervisor sidecar spawned when `runner up`
    /// was invoked with `--supervise`. `None` means no sidecar was
    /// started. `runner down` cascades a SIGTERM to this pid before
    /// tearing down xcodebuild.
    #[serde(default)]
    pub supervisor_pid: Option<u32>,
}

/// Env pairs for the xcodebuild process. Xcode forwards `TEST_RUNNER_*`
/// variables into the XCUITest runner process (prefix stripped); a
/// positional `NAME=VALUE` arg would be a build setting and never reach
/// the runner.
///
/// `record_enabled` sets `TEST_RUNNER_SMIX_RECORD_ENABLED=1`, which
/// activates the swift `EventRecorder.installSwizzle` path plus the
/// `/record/*` routes. The capsule bring-up sets this to true; the
/// bare `smix runner up` path leaves it false for backward compat.
///
/// `port` is forwarded via `TEST_RUNNER_SMIX_RUNNER_PORT=<port>` so
/// Xcode surfaces `SMIX_RUNNER_PORT` inside the swift runner process
/// and its `RunnerPortResolver` binds FlyingFox to the requested port.
/// This is what lets multiple concurrent runners share one host
/// without colliding on the default 22087 port.
pub fn runner_env(bundle: Option<&str>, record_enabled: bool, port: u16) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(b) = bundle {
        env.push((
            "TEST_RUNNER_SMIX_RUNNER_TARGET_BUNDLE".to_string(),
            b.to_string(),
        ));
    }
    if record_enabled {
        env.push((
            "TEST_RUNNER_SMIX_RECORD_ENABLED".to_string(),
            "1".to_string(),
        ));
    }
    env.push(("TEST_RUNNER_SMIX_RUNNER_PORT".to_string(), port.to_string()));
    // Forward the CLI's compile-time version to the runner so
    // `HealthRoute.responseDetail` can echo it back on `GET /health`.
    // Rust `env!("CARGO_PKG_VERSION")` inside the CLI binary matches
    // `smix-runner-sources::SOURCES_VERSION` because the workspace pins
    // them together. The client then compares this echo against its own
    // CARGO_PKG_VERSION and refuses boot on mismatch — without this,
    // CLI-vs-runner drift makes CLI patches silently no-op against
    // stale Swift sources.
    env.push((
        "TEST_RUNNER_SMIX_RUNNER_VERSION".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    ));
    // Forward `.smix/config.yaml interactiveProbe:` (JSON-encoded) to
    // the runner so the `launchApp` handler's interactive-fingerprint
    // probe knows the configured minIdentifierCount + ignore-list.
    // Missing config → env unset → Swift falls back to bundled
    // defaults.
    if let Some(json) = load_interactive_probe_env() {
        env.push((
            "TEST_RUNNER_SMIX_INTERACTIVE_PROBE_JSON".to_string(),
            json,
        ));
    }
    env
}

/// Read `.smix/config.yaml` looking for the `interactiveProbe:` key.
/// Returns a JSON-encoded string when present, `None` when file absent
/// OR key absent OR file unreadable. The runner side falls back to
/// bundled defaults in either case.
///
/// Yaml → JSON conversion goes via `serde_norway` into a
/// `serde_json::Value` — deliberately no explicit schema on this
/// crate's side, so the `interactiveProbe` mapping can grow without
/// smix-cli needing an update.
fn load_interactive_probe_env() -> Option<String> {
    let root = workspace_root(&std::env::current_dir().ok()?)?;
    let config_path = root.join(".smix/config.yaml");
    let text = std::fs::read_to_string(&config_path).ok()?;
    let root_value: serde_json::Value = serde_norway::from_str(&text).ok()?;
    let probe = root_value.get("interactiveProbe")?;
    serde_json::to_string(probe).ok()
}

/// Walk up from `start` to the directory containing `.smix/` — the smix
/// workspace root (same anchor the sim registry lives under).
pub fn workspace_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join(".smix").is_dir() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// argv for the runner session (after the `xcodebuild` word itself).
pub fn xcodebuild_argv(project: &Path, udid: &str) -> Vec<String> {
    // Per-udid `-derivedDataPath` avoids DerivedData contention when
    // multiple `capsule up` invocations share the default Xcode
    // DerivedData root (~/Library/Developer/Xcode/DerivedData): the same
    // project + scheme running under two concurrent xcodebuilds hits an
    // "Xcode3CommandLineBuildTool ... operation queue" lock, and the
    // second sim can hang for 5min+ before failing. Isolating each sim
    // under .smix/runner/derived-data-<udid>/ sidesteps the lock.
    let derived = format!(".smix/runner/derived-data-{udid}");
    vec![
        "test".into(),
        "-project".into(),
        project.display().to_string(),
        "-scheme".into(),
        "SmixRunner".into(),
        "-destination".into(),
        format!("platform=iOS Simulator,id={udid}"),
        "-derivedDataPath".into(),
        derived,
    ]
}

/// Bare HTTP GET /health against `localhost:<port>`; true on a 200 line.
pub fn health_ok(port: u16) -> bool {
    read_health_bytes(port, 64)
        .map(|(status_ok, _body)| status_ok)
        .unwrap_or(false)
}

/// Read `runnerVersion` from the `GET /health` body. Returns
/// `Some("<ver>")` when the runner emits the extended body (runners
/// carrying `SmixRunnerServer.swift`'s `responseDetail` wiring);
/// `None` for older runners that still return the legacy `{"ok":true}`
/// shape, or when the socket read failed / the body wasn't parseable
/// JSON. `None` MUST NOT be treated as a version-mismatch — it's the
/// "runner too old to tell me" signal, and the CLI keeps booting.
pub fn health_runner_version(port: u16) -> Option<String> {
    let (ok, body) = read_health_bytes(port, 4096).ok()?;
    if !ok {
        return None;
    }
    // Extract just the JSON body (after the blank line separator).
    let body_start = body
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)?;
    let json_bytes = &body[body_start..];
    let value: serde_json::Value = serde_json::from_slice(json_bytes).ok()?;
    let v = value.get("runnerVersion")?.as_str()?;
    if v.is_empty() { None } else { Some(v.to_string()) }
}

/// The wire schemas the running runner says it speaks.
///
/// Empty when the runner predates the question or the read failed — which
/// means "it did not say", not "it speaks none". The two are different and
/// only one of them is a reason to stop.
pub fn health_wire_schemas(port: u16) -> Vec<u32> {
    let Ok((ok, body)) = read_health_bytes(port, 4096) else {
        return Vec::new();
    };
    if !ok {
        return Vec::new();
    }
    let Some(start) = body.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body[start..]) else {
        return Vec::new();
    };
    value
        .get("wireSchema")
        .and_then(|w| w.get("supports"))
        .and_then(serde_json::Value::as_array)
        .map(|xs| {
            xs.iter()
                .filter_map(serde_json::Value::as_u64)
                .filter_map(|n| u32::try_from(n).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Shared HTTP GET /health primitive. Returns `(status_is_200, raw_response_bytes)`
/// on connection success, `Err(())` on IO failure. Callers pick apart
/// the byte buffer to answer specific questions.
fn read_health_bytes(port: u16, cap: usize) -> Result<(bool, Vec<u8>), ()> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(1))
        .map_err(|_| ())?;
    s.set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| ())?;
    s.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .map_err(|_| ())?;
    let mut buf = vec![0u8; cap];
    let mut total = 0usize;
    while total < cap {
        match s.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }
    buf.truncate(total);
    let status_ok = std::str::from_utf8(&buf)
        .ok()
        .map(|s| s.contains(" 200"))
        .unwrap_or(false);
    Ok((status_ok, buf))
}

fn state_path(root: &Path) -> PathBuf {
    root.join(".smix/runner/state.json")
}

fn read_state(root: &Path) -> Option<RunnerState> {
    let text = std::fs::read_to_string(state_path(root)).ok()?;
    serde_json::from_str(&text).ok()
}

/// `ps -p <pid> -o command=` — None if the pid is gone.
fn pid_command(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let cmd = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if cmd.is_empty() { None } else { Some(cmd) }
}

fn signal(pid: u32, sig: &str) {
    let _ = std::process::Command::new("kill")
        .args([sig, &pid.to_string()])
        .status();
}

fn tail_log(log: &Path, lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(log) else {
        return String::new();
    };
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// Resolve the SmixRunner.xcodeproj path via a 4-step cascade. First
/// match wins:
///
/// 1. `override` — explicit `--runner-project <path>` from CLI. Wins.
/// 2. `$SMIX_RUNNER_PROJECT` env — semi-explicit.
/// 3. Install-shipped default (with auto-sync — see below):
///    - `$XDG_DATA_HOME/smix/runner/` when set
///    - `~/.local/share/smix/runner/` (macOS + Linux XDG fallback)
/// 4. `<root>/swift-bridge/SmixRunner.xcodeproj` — smix-dev-repo fallback
///    so `cd smix; cargo run --bin smix -- runner up ...` still works
///    from a fresh checkout.
///
/// The install-shipped step is auto-syncing. Before returning the
/// install-shipped path, we compare the on-disk version file
/// (`~/.local/share/smix/runner/.smix-runner-version`) against the CLI
/// version. On drift OR missing, we extract the embedded
/// `smix-runner-sources` tarball, preserving the previous tree as a
/// timestamped backup. Without this, `cargo install smix` would ship
/// only the Rust binary and the Swift runner project would silently
/// stay frozen at whatever revision first landed on disk.
///
/// Returns the first existing path, or the last candidate's error
/// (which prints as "runner project missing: `<path>`") so users see the
/// most-likely-intended location.
pub fn resolve_runner_project(
    root: &Path,
    override_path: Option<&Path>,
) -> Result<PathBuf, String> {
    // Explicit override (either --runner-project or $SMIX_RUNNER_PROJECT)
    // is a *strict* override: if set, that path MUST exist; we do NOT
    // silently fall back. This matches unix `--config` conventions —
    // when a user tells you where to look, believe them.
    let explicit_flag = override_path.map(Path::to_path_buf);
    let explicit_env = std::env::var_os("SMIX_RUNNER_PROJECT").map(PathBuf::from);
    if let Some(p) = explicit_flag.or(explicit_env) {
        if p.exists() {
            return Ok(p);
        }
        return Err(format!(
            "runner project missing: {}\n\
             explicit override (--runner-project or $SMIX_RUNNER_PROJECT) \
             does not point to an existing SmixRunner.xcodeproj — \
             fix the path, or unset the override to fall back to \
             install-shipped / repo-local defaults.",
            p.display()
        ));
    }

    // Auto-sync install-shipped sources on version drift. Runs before
    // the existence check so a first-run consumer with an
    // empty ~/.local/share/smix/ gets sources extracted transparently.
    if let Some(installed_dir) = installed_runner_dir() {
        match ensure_installed_runner_synced(&installed_dir) {
            Ok(SyncOutcome::AlreadyCurrent) => {}
            Ok(SyncOutcome::Extracted { previous_version, .. }) => {
                let from = previous_version.as_deref().unwrap_or("<none>");
                eprintln!(
                    "smix-runner: synced runner sources → {} (was {}) at {}",
                    smix_runner_sources::SOURCES_VERSION,
                    from,
                    installed_dir.display()
                );
            }
            Err(err) => {
                // Don't fail the whole resolve — fall through to the
                // repo-local candidate. A dev running from the repo
                // still works; a consumer without $HOME hits the same
                // "runner project missing" error they'd have hit before
                // auto-sync existed.
                eprintln!(
                    "smix-runner: auto-sync failed at {}: {err}",
                    installed_dir.display()
                );
            }
        }
    }

    // No explicit override — try install-shipped, then repo-local.
    let candidates: Vec<PathBuf> = std::iter::empty()
        .chain(installed_runner_project())
        .chain(std::iter::once(
            root.join("swift-bridge/SmixRunner.xcodeproj"),
        ))
        .collect();

    for cand in &candidates {
        if cand.exists() {
            return Ok(cand.clone());
        }
    }

    let last = candidates
        .last()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("<no candidates — this should not happen>"));
    let attempted = candidates
        .iter()
        .map(|p| format!("  - {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "runner project missing: {}\n\
         tried:\n{attempted}\n\
         fix: (a) `smix runner install` to populate ~/.local/share/smix/runner/, \
         or (b) pass `--runner-project <path>` on `smix runner up`, \
         or (c) set $SMIX_RUNNER_PROJECT",
        last.display()
    ))
}

/// Install-shipped runner *project* path — the SmixRunner.xcodeproj
/// under [`installed_runner_dir`]. Returns `None` when `$HOME` is unset.
fn installed_runner_project() -> Option<PathBuf> {
    installed_runner_dir().map(|d| d.join("SmixRunner.xcodeproj"))
}

/// Install-shipped runner *directory* (parent of SmixRunner.xcodeproj).
/// Follows XDG basedir when `$XDG_DATA_HOME` is set; falls back to
/// `~/.local/share/smix/runner/` on macOS + Linux. Returns `None` when
/// `$HOME` is unset (rare).
pub(crate) fn installed_runner_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("smix/runner"))
}

/// Outcome of an [`ensure_installed_runner_synced`] call.
#[derive(Debug)]
pub(crate) enum SyncOutcome {
    /// The on-disk `.smix-runner-version` already matched the CLI
    /// version — no extract performed.
    AlreadyCurrent,
    /// Sources were extracted. Callers should emit an info banner.
    Extracted {
        /// Version string previously on disk (if any).
        previous_version: Option<String>,
        /// Backup path where the previous tree was moved, `None` when
        /// the destination was empty.
        #[allow(dead_code)]
        backup: Option<PathBuf>,
    },
}

/// Ensure `dir` contains runner sources whose `.smix-runner-version`
/// matches the CLI's [`smix_runner_sources::SOURCES_VERSION`]. Extracts
/// the embedded tarball on mismatch or missing, backing up any prior
/// contents. Idempotent: a second call with the same version is a
/// cheap file read.
///
/// This is what keeps the Swift sources in step with the CLI: they are
/// baked into the CLI binary and re-materialise on every `smix runner
/// up` when the CLI version has moved forward (typically after
/// `cargo install smix` / `brew upgrade smix`).
pub(crate) fn ensure_installed_runner_synced(
    dir: &Path,
) -> Result<SyncOutcome, smix_runner_sources::ExtractError> {
    let previous = smix_runner_sources::read_installed_version(dir)?;
    if previous.as_deref() == Some(smix_runner_sources::SOURCES_VERSION) {
        return Ok(SyncOutcome::AlreadyCurrent);
    }
    // Version drift OR missing → extract with force. `force=true` is
    // safe: extract_to backs up any existing tree to a timestamped
    // sibling directory before writing, so a consumer's local
    // modifications (rare — the install dir is meant to be
    // CLI-managed) are preserved for post-mortem inspection.
    let report = smix_runner_sources::extract_to(dir, true)?;
    Ok(SyncOutcome::Extracted {
        previous_version: previous,
        backup: report.backup,
    })
}

/// Bring the runner up on `udid`. Blocks until `/health` answers 200 or
/// the timeout (env `SMIX_RUNNER_UP_TIMEOUT_SECS`, default 300 — first
/// run includes a full Swift build) expires.
///
/// `runner_project` — optional explicit path to `SmixRunner.xcodeproj`.
/// When `None`, uses [`resolve_runner_project`] cascade against `root`.
pub fn up(
    root: &Path,
    udid: &str,
    port: u16,
    bundle: Option<&str>,
    record_enabled: bool,
    runner_project: Option<&Path>,
) -> Result<(), String> {
    up_with_options(root, udid, port, bundle, record_enabled, runner_project, false)
}

/// [`up`] extended with the `--supervise` sidecar flag. When
/// `supervise = true`, after `/health` returns 200 spawn a detached
/// `smix runner supervise` process, record its pid in state.json, and
/// return. `runner down` cascades a SIGTERM to that pid before
/// tearing down xcodebuild.
///
/// `up_with_options(_, _, _, _, _, _, false)` is equivalent to [`up`].
pub fn up_with_options(
    root: &Path,
    udid: &str,
    port: u16,
    bundle: Option<&str>,
    record_enabled: bool,
    runner_project: Option<&Path>,
    supervise: bool,
) -> Result<(), String> {
    // Refuse to boot without --bundle unless the caller explicitly
    // opts in via SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1. The runner's
    // built-in default `com.apple.Preferences` silently latches every
    // `/tree` call to Preferences and every `takeScreenshot` to the
    // wrong app, which surfaces as baffling "empty tree" results —
    // so it's explicit-or-error.
    match bundle {
        Some(b) => {
            println!("[runner] target bundle-id: {b}");
        }
        None => {
            let bypass = std::env::var("SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if !bypass {
                return Err(
                    "no --bundle passed; the runner would latch to the \
                     built-in default (com.apple.Preferences) and every \
                     subsequent /tree call would report Preferences as the \
                     app.\n\n\
                     fix: pass --bundle <your-app-bundle-id>, e.g.\n\
                       smix runner up <device> --bundle com.example.app\n\n\
                     to keep the legacy default (v1.0.3 behavior), export\n\
                       SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1\n\
                     — but expect empty a11y trees until you re-attach a \
                     real target."
                        .to_string(),
                );
            }
            eprintln!(
                "[runner] warning: no --bundle passed and \
                 SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1 set; latching to \
                 default com.apple.Preferences"
            );
        }
    }
    if health_ok(port) {
        match read_state(root) {
            Some(st) if st.udid == udid && st.bundle.as_deref() == bundle => {
                println!("runner already up: udid={udid} port={port} pid={}", st.pid);
                return Ok(());
            }
            Some(st) => {
                return Err(format!(
                    "port {port} already serves a runner recorded for udid={} \
                     bundle={:?} — run `smix runner down` first",
                    st.udid, st.bundle
                ));
            }
            None => {
                return Err(format!(
                    "port {port} already serves /health but no \
                     .smix/runner/state.json records it — not killing blindly; \
                     investigate (pgrep -fl xcodebuild), then `smix runner down`"
                ));
            }
        }
    }

    let project = resolve_runner_project(root, runner_project)?;
    let runner_dir = root.join(".smix/runner");
    std::fs::create_dir_all(&runner_dir).map_err(|e| format!("mkdir .smix/runner: {e}"))?;
    let log = runner_dir.join(format!("runner-{udid}.log"));
    let log_file =
        std::fs::File::create(&log).map_err(|e| format!("create {}: {e}", log.display()))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("clone log handle: {e}"))?;

    let mut cmd = std::process::Command::new("xcodebuild");
    cmd.args(xcodebuild_argv(&project, udid))
        .envs(runner_env(bundle, record_enabled, port))
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_err);
    // Own process group so the session outlives this CLI invocation and a
    // ctrl-C on smix doesn't tear the runner down implicitly.
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("spawn xcodebuild: {e}"))?;
    let pid = child.id();

    let st = RunnerState {
        pid,
        udid: udid.to_string(),
        port,
        log: log.clone(),
        bundle: bundle.map(str::to_string),
        supervisor_pid: None,
    };
    std::fs::write(
        state_path(root),
        serde_json::to_string_pretty(&st).expect("state serializes"),
    )
    .map_err(|e| format!("write state.json: {e}"))?;

    let timeout_secs: u64 = std::env::var("SMIX_RUNNER_UP_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    // Detect cold vs warm rebuild by inspecting whether the per-udid
    // derived-data dir is already populated. Cold rebuilds after a
    // version bump can take 5-10 min (full swift stdlib copy + linker +
    // code sign). Print an explicit banner so callers know to budget the
    // wait and don't set a spawnSync timeout too aggressively.
    let derived_dir = root.join(format!(".smix/runner/derived-data-{udid}"));
    let is_cold = !derived_dir.is_dir()
        || std::fs::read_dir(&derived_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
    if is_cold {
        println!(
            "runner starting: udid={udid} port={port} pid={pid} \
             — COLD REBUILD expected up to 10 minutes (first run after \
             upgrade compiles the XCUITest bundle for smix {}). \
             Log: {}. Timeout {timeout_secs}s.",
            env!("CARGO_PKG_VERSION"),
            log.display()
        );
    } else {
        println!(
            "runner starting: udid={udid} port={port} pid={pid} \
             (warm rebuild ~3 s expected; log {}, timeout {timeout_secs}s)",
            log.display()
        );
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    // Heartbeat every 30 s during a cold rebuild so anyone watching
    // stdout sees progress instead of a stall.
    let started_at = std::time::Instant::now();
    let mut last_heartbeat = started_at;
    while std::time::Instant::now() < deadline {
        if is_cold && last_heartbeat.elapsed() >= std::time::Duration::from_secs(30) {
            let elapsed_s = started_at.elapsed().as_secs();
            println!("runner up: xcodebuild still working ({elapsed_s}s elapsed)");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            last_heartbeat = std::time::Instant::now();
        }
        if let Ok(Some(status)) = child.try_wait() {
            let _ = std::fs::remove_file(state_path(root));
            return Err(format!(
                "xcodebuild exited early ({status}) — log tail:\n{}",
                tail_log(&log, 25)
            ));
        }
        if health_ok(port) {
            // Version-mismatch gate. Ask the runner what version it
            // thinks it is; if it disagrees with the CLI, refuse boot
            // with an actionable message. This is the last line of
            // defense against CLI-vs-runner drift silently no-oping CLI
            // patches — if `ensure_installed_runner_synced` failed for
            // any reason (unwritable XDG dir, custom
            // SMIX_RUNNER_PROJECT, stale supervisor cache), this check
            // catches it before the user runs into a mysterious 404.
            let cli_version = env!("CARGO_PKG_VERSION");
            // Ask about the wire before asking about the version. A runner
            // that has been up across a CLI upgrade is not a problem unless
            // the shape between them moved, and demanding identical semvers
            // called every such runner broken.
            let theirs = health_wire_schemas(port);
            if !theirs.is_empty() {
                match smix_runner_wire::negotiate_wire_schema(
                    smix_runner_wire::WIRE_SCHEMA_SUPPORTED,
                    &theirs,
                ) {
                    Some(schema) => {
                        let v = health_runner_version(port).unwrap_or_default();
                        println!(
                            "runner up: http://localhost:{port}/health = 200 \
                             (runner v{v}, wire schema {schema})"
                        );
                        return Ok(());
                    }
                    None => {
                        let _ = std::fs::remove_file(state_path(root));
                        signal(pid, "-TERM");
                        let ours = smix_runner_wire::WIRE_SCHEMA_SUPPORTED;
                        return Err(format!(
                            "no wire schema in common: this CLI speaks {ours:?} and the \
                             running SmixRunner speaks {theirs:?}. Nothing they could say \
                             to each other would mean the same thing. Fix: \
                             `smix runner install --force` to re-extract the runner \
                             sources this CLI ships with, then retry `smix runner up`."
                        ));
                    }
                }
            }
            match health_runner_version(port) {
                Some(v) if v == cli_version => {
                    println!(
                        "runner up: http://localhost:{port}/health = 200 (runner v{v})"
                    );
                }
                Some(v) => {
                    let _ = std::fs::remove_file(state_path(root));
                    signal(pid, "-TERM");
                    return Err(format!(
                        "runner version mismatch: CLI is v{cli_version} but the \
                         running SmixRunner reports v{v}. This means the on-disk \
                         runner project used by xcodebuild is out of sync with the \
                         installed CLI — the v1.0.4-v1.0.9 distribution gap the \
                         v1.0.10 auto-sync closes. Fix: `smix runner install --force` \
                         to re-extract the embedded runner sources, then retry \
                         `smix runner up`. If you're using an explicit \
                         --runner-project / $SMIX_RUNNER_PROJECT, either update \
                         that path to a v{cli_version} runner or drop the override."
                    ));
                }
                None => {
                    // Older runner (legacy `{\"ok\":true}` body).
                    // Don't refuse boot — that would break every user
                    // who has an older runner they haven't re-installed.
                    // Warn instead. On next `runner install`/upgrade the
                    // warning goes away.
                    eprintln!(
                        "runner up: warning — runner /health returned legacy body \
                         (no `runnerVersion` field). This runner predates v1.0.10 \
                         and cannot self-report its version. If you see missing \
                         routes (e.g. `/session/open` 404), run \
                         `smix runner install --force` to sync sources to v{cli_version}."
                    );
                    println!("runner up: http://localhost:{port}/health = 200 (legacy body)");
                }
            }
            // Sidecar mode.
            if supervise {
                match spawn_supervisor(root, runner_project) {
                    Ok(sup_pid) => {
                        // Rewrite state.json with the supervisor pid.
                        if let Some(mut current) = read_state(root) {
                            current.supervisor_pid = Some(sup_pid);
                            let _ = std::fs::write(
                                state_path(root),
                                serde_json::to_string_pretty(&current)
                                    .expect("state serializes"),
                            );
                            println!(
                                "runner supervise: spawned pid={sup_pid} \
                                 (log: .smix/runner/supervise-{udid}.log)"
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "runner supervise: spawn failed: {e} — runner \
                             is up but no sidecar attached"
                        );
                    }
                }
            }
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    signal(pid, "-INT");
    let _ = std::fs::remove_file(state_path(root));
    Err(format!(
        "runner did not become healthy within {timeout_secs}s — sent SIGINT; log tail:\n{}",
        tail_log(&log, 25)
    ))
}

/// Tear the runner down. SIGINT first — xcodebuild cancels the XCUITest
/// session cleanly via testmanagerd; a hard kill SIGABRTs the runner app
/// and macOS pops a crash-report dialog that steals user focus.
///
/// If state.json records a supervisor pid, cascade a SIGTERM to it
/// BEFORE tearing down xcodebuild. Otherwise the sidecar
/// would flap into a `TEST INTERRUPTED` trigger the moment we send
/// SIGINT to xcodebuild and try to re-cycle a runner we just killed.
pub fn down(root: &Path, port: u16) -> Result<(), String> {
    let mut acted = false;
    if let Some(st) = read_state(root) {
        // Supervisor teardown first. Skip when we are
        // the supervisor calling down() (avoid killing ourselves
        // mid-cycle — the re-entrant case).
        if let Some(sup_pid) = st.supervisor_pid
            && sup_pid != std::process::id()
            && let Some(cmd) = pid_command(sup_pid)
            && (cmd.contains("smix") || cmd.contains("supervise"))
        {
            println!("stopping supervisor: pid={sup_pid}");
            signal(sup_pid, "-TERM");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while pid_command(sup_pid).is_some() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            if pid_command(sup_pid).is_some() {
                eprintln!("supervisor pid {sup_pid} ignored SIGTERM for 5s — SIGKILL");
                signal(sup_pid, "-9");
            }
        }
        match pid_command(st.pid) {
            Some(cmd) if cmd.contains("xcodebuild") => {
                println!("stopping runner: pid={} udid={}", st.pid, st.udid);
                signal(st.pid, "-INT");
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                while pid_command(st.pid).is_some() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                if pid_command(st.pid).is_some() {
                    eprintln!(
                        "warning: pid {} ignored SIGINT for 30s — escalating to \
                         SIGKILL (expect a macOS crash-report dialog from the \
                         runner app)",
                        st.pid
                    );
                    signal(st.pid, "-9");
                }
                acted = true;
            }
            Some(other) => {
                eprintln!(
                    "stale handle: pid {} is now {:?} (not xcodebuild) — \
                     dropping state without killing",
                    st.pid, other
                );
            }
            None => {
                println!("runner pid {} already gone — dropping stale handle", st.pid);
            }
        }
        let _ = std::fs::remove_file(state_path(root));
    }

    // Fallback: sessions started outside `smix runner up` (no handle).
    let swept = std::process::Command::new("pkill")
        .args(["-INT", "-f", "xcodebuild.*SmixRunner"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if swept {
        println!("swept unrecorded xcodebuild SmixRunner session(s)");
        acted = true;
    }

    if acted {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while health_ok(port) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    if health_ok(port) {
        return Err(format!(
            "port {port} still answers /health after teardown — inspect \
             `pgrep -fl xcodebuild`"
        ));
    }
    println!("runner down: port {port} closed");
    Ok(())
}

/// `smix runner cycle`.
///
/// Reads the current runner state, tears the runner down (SIGINT +
/// wait), and brings it back up on the SAME device + port + bundle. The
/// per-udid derived-data directory (`.smix/runner/derived-data-<udid>/`)
/// is preserved by both [`down`] and [`up`], so the second `xcodebuild
/// test-without-building` boots in ~3 s instead of the ~15 s cold path.
///
/// When the XCTest test-host observes `** TEST INTERRUPTED **`, the
/// safest recovery is to cycle. This verb exposes cycle explicitly, and
/// is also invoked internally by the runner supervisor.
///
/// Errors if no state.json exists — cycle only cycles known runners;
/// use `smix runner up` for a cold start.
pub fn cycle(
    root: &Path,
    port: u16,
    runner_project: Option<&Path>,
) -> Result<(), String> {
    let st = read_state(root).ok_or_else(|| {
        "no runner state.json — cycle only cycles a known runner; \
         run `smix runner up <device> [--bundle <id>]` for a cold start"
            .to_string()
    })?;
    let udid = st.udid.clone();
    let bundle = st.bundle.clone();
    let cycle_port = st.port;
    // Carry the supervise flag across the cycle so the
    // sidecar re-attaches to the new xcodebuild after `up` returns.
    // Otherwise `runner cycle` from inside a supervisor-managed runner
    // would silently drop supervision.
    let had_supervisor = st.supervisor_pid.is_some();
    if cycle_port != port {
        eprintln!(
            "note: state.json port {cycle_port} differs from --runner-port {port}; \
             cycling on state.json's {cycle_port}"
        );
    }
    println!(
        "cycling runner: udid={udid} port={cycle_port} bundle={bundle:?}"
    );
    down(root, cycle_port)?;
    up_with_options(
        root,
        &udid,
        cycle_port,
        bundle.as_deref(),
        false,
        runner_project,
        had_supervisor,
    )
}

/// Collect ±`context_size` lines surrounding the first occurrence of
/// `match_line` inside the log file. Best-effort: returns empty on
/// file-read failure, or on partial matches (log rotated between
/// trigger + read). Emitted inside the supervisor's `RunnerCycled` JSON
/// event so callers get cycle-cascade classification data without
/// needing a separate `grep` pass.
fn collect_log_context(
    log_path: &Path,
    match_line: &str,
    context_size: usize,
) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(log_path) else {
        return Vec::new();
    };
    let trimmed = match_line.trim();
    let lines: Vec<&str> = text.lines().collect();
    let Some(idx) = lines.iter().position(|l| l.contains(trimmed)) else {
        return Vec::new();
    };
    let start = idx.saturating_sub(context_size);
    let end = (idx + context_size + 1).min(lines.len());
    lines[start..end].iter().map(|l| l.to_string()).collect()
}

/// Spawn the supervisor as a detached child process after
/// `runner up --supervise`. Redirects stdout/stderr to
/// `.smix/runner/supervise-<UDID>.log`. Uses its own process group so
/// a ctrl-C on the CLI doesn't tear the supervisor down. Returns the
/// child pid on success.
fn spawn_supervisor(root: &Path, runner_project: Option<&Path>) -> Result<u32, String> {
    let st = read_state(root)
        .ok_or_else(|| "internal: no state.json to attach supervisor to".to_string())?;
    let udid = st.udid.clone();
    let runner_dir = root.join(".smix/runner");
    std::fs::create_dir_all(&runner_dir).map_err(|e| format!("mkdir .smix/runner: {e}"))?;
    let log = runner_dir.join(format!("supervise-{udid}.log"));
    let log_file = std::fs::File::create(&log)
        .map_err(|e| format!("create {}: {e}", log.display()))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("clone supervise log handle: {e}"))?;

    let self_exe = std::env::current_exe()
        .map_err(|e| format!("current_exe: {e}"))?;
    let mut cmd = std::process::Command::new(&self_exe);
    cmd.arg("runner").arg("supervise");
    if let Some(p) = runner_project {
        cmd.arg("--runner-project").arg(p);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_err);
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let child = cmd.spawn().map_err(|e| format!("spawn supervise: {e}"))?;
    Ok(child.id())
}

/// Host-side XCTest supervisor.
///
/// Tails the runner log at `.smix/runner/runner-<UDID>.log` and looks
/// for interrupt patterns (`** TEST INTERRUPTED **`,
/// `SchemeActionResultOperation started unexpectedly`). On match:
/// invokes [`cycle`] to tear the runner down and bring it back up on
/// the same device/port/bundle. Session persistence preserves the
/// client's `Session-Id` across the cycle.
///
/// Backoff: at most one cycle per 60 s (a spurious hit during boot is
/// common). If 5 cycles fire inside 10 minutes the supervisor exits
/// non-zero so a monitoring layer can escalate.
///
/// Runs foreground; SIGINT / SIGTERM to the supervisor cleanly shuts
/// it down. `smix runner down` invoked separately still tears the
/// runner itself down.
pub fn supervise(
    root: &Path,
    runner_project: Option<&Path>,
) -> Result<(), String> {
    let st = read_state(root).ok_or_else(|| {
        "no runner state.json — supervise attaches to a known runner; \
         run `smix runner up <device> --bundle <id>` first"
            .to_string()
    })?;
    let log_path = st.log.clone();
    let port = st.port;
    println!(
        "smix runner supervise: attached\n  udid={} port={} log={}",
        st.udid,
        st.port,
        log_path.display()
    );

    let mut position: u64 = std::fs::metadata(&log_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let mut last_cycle_at: Option<std::time::Instant> = None;
    let mut cycle_times: Vec<std::time::Instant> = Vec::new();
    let interrupt_patterns: &[&str] = &[
        "** TEST INTERRUPTED **",
        "SchemeActionResultOperation started unexpectedly",
    ];
    let cycle_cooldown = std::time::Duration::from_secs(60);
    let storm_window = std::time::Duration::from_secs(600);
    let storm_threshold = 5;

    // Health-unreachable trigger. The log-marker triggers only fire
    // when xcodebuild prints a recognizable death banner, but a runner
    // can die with NO marker at all (e.g. warm derived-data reuse after
    // a downgrade sync), which the supervisor would otherwise sit
    // through. Probe GET /health every ~10 s; 3 consecutive failures
    // (~30 s unreachable) is a cycle trigger through the same cooldown +
    // storm accounting as the log markers.
    let health_probe_every = 20; // × 500 ms sleep = ~10 s cadence
    let health_fail_threshold = 3;
    let mut loop_ticks: u64 = 0;
    let mut health_consecutive_fails: u32 = 0;

    fn probe_health(port: u16) -> bool {
        use std::io::{Read, Write};
        let addr = format!("127.0.0.1:{port}");
        let timeout = std::time::Duration::from_secs(3);
        let Ok(mut stream) = std::net::TcpStream::connect_timeout(
            &match addr.parse() {
                Ok(a) => a,
                Err(_) => return false,
            },
            timeout,
        ) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        if stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .is_err()
        {
            return false;
        }
        let mut buf = [0u8; 64];
        match stream.read(&mut buf) {
            Ok(n) if n > 0 => {
                // Any HTTP response line counts as alive; /health
                // never legitimately errors on a healthy runner.
                buf[..n].starts_with(b"HTTP/1.1 200") || buf[..n].starts_with(b"HTTP/1.0 200")
            }
            _ => false,
        }
    }

    let mut carry = String::new();
    loop {
        // Sleep between polls; keeps CPU low.
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Periodic health probe (see above).
        loop_ticks += 1;
        if loop_ticks.is_multiple_of(health_probe_every) {
            if probe_health(port) {
                health_consecutive_fails = 0;
            } else {
                health_consecutive_fails += 1;
                eprintln!(
                    "supervise: /health unreachable ({health_consecutive_fails}/{health_fail_threshold})"
                );
                if health_consecutive_fails >= health_fail_threshold {
                    health_consecutive_fails = 0;
                    let now = std::time::Instant::now();
                    let in_cooldown = last_cycle_at
                        .map(|prev| now.duration_since(prev) < cycle_cooldown)
                        .unwrap_or(false);
                    if in_cooldown {
                        eprintln!(
                            "supervise: health trigger within {:?} of last cycle — skipping (cooldown)",
                            cycle_cooldown
                        );
                    } else {
                        cycle_times.retain(|t| now.duration_since(*t) < storm_window);
                        if cycle_times.len() >= storm_threshold {
                            return Err(format!(
                                "supervise: {} cycles inside {:?} — bailing so a monitoring \
                                 layer can escalate",
                                cycle_times.len(),
                                storm_window
                            ));
                        }
                        use std::io::Write;
                        let mut out = std::io::stdout().lock();
                        let _ = writeln!(
                            out,
                            r#"{{"event":"RunnerCycled","reasonMatched":"health-unreachable x{}","context":[],"atMs":{}}}"#,
                            health_fail_threshold,
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        );
                        let _ = out.flush();
                        match cycle(root, port, runner_project) {
                            Ok(()) => {
                                cycle_times.push(now);
                                last_cycle_at = Some(now);
                                position = 0;
                                carry.clear();
                            }
                            Err(e) => {
                                return Err(format!("supervise: cycle failed: {e}"));
                            }
                        }
                    }
                }
            }
        }

        let meta = match std::fs::metadata(&log_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let current_len = meta.len();
        if current_len < position {
            // Log rotated — reset to start of file.
            position = 0;
            carry.clear();
        }
        if current_len == position {
            continue;
        }
        // Read new bytes.
        use std::io::{Read, Seek, SeekFrom};
        let mut f = match std::fs::File::open(&log_path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        if f.seek(SeekFrom::Start(position)).is_err() {
            continue;
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            // Non-UTF8 chunk — skip and advance.
            position = current_len;
            continue;
        }
        position = current_len;
        carry.push_str(&buf);
        // Match line-by-line so an interrupt marker split across chunks
        // still fires correctly on the reassembled line.
        let mut lines: Vec<&str> = carry.lines().collect();
        // If the last line doesn't end with \n the current position may
        // land mid-line — keep it as carry for the next iter.
        let keep_last = !carry.ends_with('\n');
        let tail = if keep_last { lines.pop().unwrap_or("").to_string() } else { String::new() };
        for line in &lines {
            let matched = interrupt_patterns.iter().any(|p| line.contains(p));
            if !matched {
                continue;
            }
            let now = std::time::Instant::now();
            if last_cycle_at.is_some_and(|prev| now.duration_since(prev) < cycle_cooldown) {
                eprintln!(
                    "supervise: interrupt hit within {:?} of last cycle — \
                     skipping (cooldown)",
                    cycle_cooldown
                );
                continue;
            }
            // Storm check: prune expired timestamps, then check count.
            cycle_times.retain(|t| now.duration_since(*t) < storm_window);
            if cycle_times.len() >= storm_threshold {
                return Err(format!(
                    "supervise: {} cycles inside {:?} — bailing so a monitoring \
                     layer can escalate",
                    cycle_times.len(),
                    storm_window
                ));
            }
            // Flush after every JSON event so anything parsing
            // supervisor stdout sees the event immediately even when
            // the outer flow crashes fast right after.
            //
            // Attach the surrounding ±5 lines of runner log context so
            // the cycle can be classified without a separate grep.
            // Context is best-effort — if the log file has been rotated
            // between the trigger and the read we still emit the event
            // with an empty context.
            let context: Vec<String> = collect_log_context(&log_path, line, 5);
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            let context_json = context
                .iter()
                .map(|l| format!("{:?}", l))
                .collect::<Vec<_>>()
                .join(",");
            let _ = writeln!(
                out,
                r#"{{"event":"RunnerCycled","reasonMatched":{:?},"context":[{}],"atMs":{}}}"#,
                line.trim(),
                context_json,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            );
            let _ = out.flush();
            match cycle(root, port, runner_project) {
                Ok(()) => {
                    cycle_times.push(now);
                    last_cycle_at = Some(now);
                    // After cycle succeeds the log path is truncated
                    // (up recreates the file). Reset our position.
                    position = 0;
                    carry.clear();
                    break;
                }
                Err(e) => {
                    return Err(format!("supervise: cycle failed: {e}"));
                }
            }
        }
        carry = tail;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const UDID: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";

    #[test]
    fn xcodebuild_argv_targets_explicit_udid() {
        let argv = xcodebuild_argv(Path::new("/repo/swift-bridge/SmixRunner.xcodeproj"), UDID);
        assert_eq!(argv[0], "test");
        assert!(argv.contains(&"SmixRunner".to_string()));
        assert!(
            argv.iter()
                .any(|a| a == &format!("platform=iOS Simulator,id={UDID}"))
        );
        assert!(!argv.iter().any(|a| a.contains("name=")));
        // Per-udid derivedDataPath keeps concurrent runners from
        // contending on the shared Xcode DerivedData lock.
        let derived_pos = argv
            .iter()
            .position(|a| a == "-derivedDataPath")
            .expect("argv missing -derivedDataPath");
        assert_eq!(
            argv[derived_pos + 1],
            format!(".smix/runner/derived-data-{UDID}")
        );
    }

    #[test]
    fn runner_state_round_trips() {
        let st = RunnerState {
            pid: 4242,
            udid: UDID.into(),
            port: 22087,
            log: PathBuf::from("/tmp/runner.log"),
            bundle: Some("com.example.app".into()),
            supervisor_pid: None,
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: RunnerState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, st);
    }

    #[test]
    fn runner_env_uses_test_runner_prefix() {
        let env = runner_env(Some("com.example.app"), false, 22087);
        let map: std::collections::HashMap<String, String> = env.iter().cloned().collect();
        assert_eq!(
            map.get("TEST_RUNNER_SMIX_RUNNER_TARGET_BUNDLE")
                .map(String::as_str),
            Some("com.example.app")
        );
        assert_eq!(
            map.get("TEST_RUNNER_SMIX_RUNNER_PORT").map(String::as_str),
            Some("22087")
        );
        let env_no_bundle = runner_env(None, false, 22090);
        // The version is unconditionally forwarded.
        assert_eq!(env_no_bundle.len(), 2);
        assert!(
            env_no_bundle
                .iter()
                .any(|(k, v)| k == "TEST_RUNNER_SMIX_RUNNER_PORT" && v == "22090")
        );
    }

    #[test]
    fn runner_env_forwards_cli_version_for_health_echo() {
        // The CLI's own version reaches the runner via
        // TEST_RUNNER_SMIX_RUNNER_VERSION so /health can echo it and
        // the client can refuse boot on mismatch.
        let env = runner_env(None, false, 22087);
        let map: std::collections::HashMap<String, String> = env.into_iter().collect();
        assert_eq!(
            map.get("TEST_RUNNER_SMIX_RUNNER_VERSION").map(String::as_str),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn runner_env_with_record_adds_enabled_var() {
        let env = runner_env(Some("com.example.app"), true, 22087);
        let map: std::collections::HashMap<String, String> = env.into_iter().collect();
        assert_eq!(
            map.get("TEST_RUNNER_SMIX_RUNNER_TARGET_BUNDLE")
                .map(String::as_str),
            Some("com.example.app")
        );
        assert_eq!(
            map.get("TEST_RUNNER_SMIX_RECORD_ENABLED")
                .map(String::as_str),
            Some("1")
        );
    }

    // Auto-sync regression tests. These lock in the behavior that
    // closes the CLI-vs-runner distribution gap: on version drift OR
    // missing version file, ensure_installed_runner_synced MUST extract
    // the embedded tarball; on matching version it MUST be a no-op.

    #[test]
    fn ensure_installed_runner_synced_extracts_on_missing_version_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = ensure_installed_runner_synced(dir.path()).expect("sync");
        matches!(outcome, SyncOutcome::Extracted { previous_version: None, .. })
            .then_some(())
            .expect("expected first-run extract with no previous version");
        assert!(
            dir.path().join(".smix-runner-version").exists(),
            "version file must be written"
        );
        assert!(
            dir.path().join("SmixRunner.xcodeproj/project.pbxproj").exists(),
            "xcodeproj must land on disk after sync"
        );
    }

    #[test]
    fn ensure_installed_runner_synced_reextracts_on_stale_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Simulate a consumer whose runner tree was populated by an
        // earlier CLI. The stale sentinel version 0.0.0-stale MUST NOT
        // survive the sync call.
        fs::write(dir.path().join(".smix-runner-version"), "0.0.0-stale\n")
            .expect("seed stale version");
        fs::write(dir.path().join("stale-marker.txt"), b"old contents")
            .expect("seed stale marker");

        let outcome = ensure_installed_runner_synced(dir.path()).expect("sync");
        match outcome {
            SyncOutcome::Extracted {
                previous_version,
                backup,
            } => {
                assert_eq!(previous_version.as_deref(), Some("0.0.0-stale"));
                let backup = backup.expect("backup path present");
                assert!(backup.exists(), "backup dir must exist");
                assert!(
                    backup.join("stale-marker.txt").exists(),
                    "backup must preserve prior tree contents"
                );
            }
            SyncOutcome::AlreadyCurrent => panic!("stale must not be treated as current"),
        }
        // Fresh sources landed; stale marker is NOT in the new tree.
        assert!(!dir.path().join("stale-marker.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".smix-runner-version"))
                .unwrap()
                .trim(),
            smix_runner_sources::SOURCES_VERSION
        );
    }

    #[test]
    fn ensure_installed_runner_synced_is_noop_when_current() {
        let dir = tempfile::tempdir().expect("tempdir");
        // First call: extracts.
        ensure_installed_runner_synced(dir.path()).expect("first sync");
        // Second call: same version file → no-op.
        let outcome = ensure_installed_runner_synced(dir.path()).expect("second sync");
        matches!(outcome, SyncOutcome::AlreadyCurrent)
            .then_some(())
            .expect("second call must be AlreadyCurrent, not Extracted");
    }

    #[test]
    fn workspace_root_walks_up_to_smix_dir() {
        let root = std::env::temp_dir().join(format!("smix-runner-ws-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".smix")).unwrap();
        let nested = root.join("crates/smix-cli");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(workspace_root(&nested).unwrap(), root);
        let outside =
            std::env::temp_dir().join(format!("smix-runner-no-ws-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        assert!(workspace_root(&outside).is_none());
    }
}
