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
    /// Host-side `xcodebuild` pid. Checked against `ps` before signalling:
    /// a recorded pid that has been recycled belongs to someone else now.
    pub pid: u32,
    /// Simulator the session drives.
    pub udid: String,
    /// Port the runner's HTTP server answers on.
    pub port: u16,
    /// Where `xcodebuild`'s output is being written.
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
pub fn runner_env(
    bundle: Option<&str>,
    record_enabled: bool,
    port: u16,
    attach_without_relaunch: bool,
) -> Vec<(String, String)> {
    let mut env = Vec::new();
    // `XCUIApplication.activate()` foregrounds an app that is already
    // running instead of restarting it, and still starts one that is
    // not — which is what "attach" has to mean for a consumer who
    // navigated somewhere before bringing the runner up. The runner has
    // resolved this mode since `LaunchModeResolver` was written; until
    // now nothing set the variable, so `launch` was the only reachable
    // behaviour.
    if attach_without_relaunch {
        env.push((
            "TEST_RUNNER_SMIX_RUNNER_LAUNCH_MODE".to_string(),
            "activate".to_string(),
        ));
    }
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
        env.push(("TEST_RUNNER_SMIX_INTERACTIVE_PROBE_JSON".to_string(), json));
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
    let root_value = read_config_yaml(&root)?;
    let probe = root_value.get("interactiveProbe")?;
    serde_json::to_string(probe).ok()
}

/// Read `.smix/config.yaml` under `root` as a schemaless
/// `serde_json::Value`. `None` when the file is absent OR unreadable OR
/// not valid yaml. Deliberately no explicit schema so the config can
/// grow keys without smix-cli needing an update — `interactiveProbe`
/// and `switches` both read their slice off the same parsed value.
fn read_config_yaml(root: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(root.join(".smix/config.yaml")).ok()?;
    serde_norway::from_str(&text).ok()
}

/// The four v2 behavior switches as declared under `.smix/config.yaml`'s
/// `switches:` block. Each is `None` when the key is absent (schemaless:
/// a missing block or missing/non-bool key leaves the field `None`).
/// `None` means "config said nothing" — the CLI resolver then falls
/// through to the `SMIX_*` env var, then to the default.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SwitchesConfig {
    /// Fall back to OCR when the accessibility tree yields nothing.
    pub auto_ocr_fallback: Option<bool>,
    /// Allow assertions that ask a model to judge the screen.
    pub enable_ai_assertions: Option<bool>,
    /// Refuse to record a screenshot baseline as a side effect of a
    /// comparison that found none.
    pub assert_screenshot_no_autorecord: Option<bool>,
    /// Reinstall the app on `launchFresh` rather than clearing its data.
    pub launch_fresh_force_reinstall: Option<bool>,
}

/// Read `.smix/config.yaml`'s `switches:` block from the workspace root
/// anchored at the current dir. A missing file / missing block yields an
/// all-`None` [`SwitchesConfig`].
pub fn load_switches() -> SwitchesConfig {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| workspace_root(&cwd))
        .and_then(|root| read_config_yaml(&root))
        .map(|v| switches_from_value(&v))
        .unwrap_or_default()
}

/// Pull the `switches:` block off an already-parsed config value.
/// Schemaless: each key is read via `Value::as_bool`, so a non-bool or
/// absent key stays `None`.
fn switches_from_value(root: &serde_json::Value) -> SwitchesConfig {
    let block = root.get("switches");
    let get = |key: &str| {
        block
            .and_then(|b| b.get(key))
            .and_then(serde_json::Value::as_bool)
    };
    SwitchesConfig {
        auto_ocr_fallback: get("autoOcrFallback"),
        enable_ai_assertions: get("enableAiAssertions"),
        assert_screenshot_no_autorecord: get("assertScreenshotNoAutorecord"),
        launch_fresh_force_reinstall: get("launchFreshForceReinstall"),
    }
}

/// Where a resolved switch value came from. Drives the CLI's named
/// deprecation warn: only `Env` warns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchSource {
    /// `.smix/config.yaml switches.*` supplied the value.
    Config,
    /// The legacy `SMIX_*` env var supplied it (deprecated → CLI warns).
    Env,
    /// Neither config nor env set it; fell through to the default.
    Default,
}

/// A resolved switch: the effective `bool` plus where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedSwitch {
    /// The effective value.
    pub value: bool,
    /// Which layer supplied it — config, environment, or the default.
    pub source: SwitchSource,
}

/// Resolve one switch with priority `config > SMIX_* env > default(false)`.
///
/// `config` is the `switches.*` value (from [`load_switches`]); `Some`
/// wins outright. Otherwise the legacy `env_name` is consulted — present
/// (any value) resolves to its truthiness and marks the source `Env` so
/// the CLI can emit a named deprecation warn. Absent env resolves to
/// `false`/`Default`. This resolver is the ONLY place the four `SMIX_*`
/// names are read on the `smix run` / `--check` path; parser and sdk keep
/// their own env reads solely as the non-CLI (`None`-injection) fallback.
pub fn resolve_switch(config: Option<bool>, env_name: &str) -> ResolvedSwitch {
    if let Some(value) = config {
        return ResolvedSwitch {
            value,
            source: SwitchSource::Config,
        };
    }
    match std::env::var(env_name) {
        Ok(raw) => ResolvedSwitch {
            value: matches!(raw.as_str(), "1" | "true" | "TRUE" | "yes"),
            source: SwitchSource::Env,
        },
        Err(_) => ResolvedSwitch {
            value: false,
            source: SwitchSource::Default,
        },
    }
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

/// What kind of thing the runner is being built for.
///
/// An enum rather than a pair of optional arguments, so the combination
/// that makes no sense — a simulator with a signing team — cannot be
/// written. A simulator build is unsigned; a device build is nothing
/// without a team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerTarget<'a> {
    /// An iOS Simulator. Unsigned, and `platform=iOS Simulator`.
    Simulator,
    /// A physical device: signed by `team`, and `platform=iOS`.
    Physical {
        /// Apple developer team id.
        team: &'a str,
    },
}

/// argv for the runner session (after the `xcodebuild` word itself).
/// The 2.x face: a simulator destination, which is what every caller
/// meant before physical devices existed.
#[must_use]
pub fn xcodebuild_argv(project: &Path, udid: &str) -> Vec<String> {
    xcodebuild_argv_for(project, udid, RunnerTarget::Simulator)
}

/// The xcodebuild invocation for a runner build, with the device world
/// said explicitly — `platform=iOS,id=…` plus the signing team for a
/// phone, the simulator destination otherwise.
#[must_use]
pub fn xcodebuild_argv_for(project: &Path, udid: &str, target: RunnerTarget<'_>) -> Vec<String> {
    // Per-udid `-derivedDataPath` avoids DerivedData contention when
    // multiple `capsule up` invocations share the default Xcode
    // DerivedData root (~/Library/Developer/Xcode/DerivedData): the same
    // project + scheme running under two concurrent xcodebuilds hits an
    // "Xcode3CommandLineBuildTool ... operation queue" lock, and the
    // second sim can hang for 5min+ before failing. Isolating each sim
    // under .smix/runner/derived-data-<udid>/ sidesteps the lock.
    let derived = format!(".smix/runner/derived-data-{udid}");
    let mut argv = vec![
        "test".into(),
        "-project".into(),
        project.display().to_string(),
        "-scheme".into(),
        "SmixRunner".into(),
        "-destination".into(),
        // The two platform strings are not interchangeable, and mixing
        // them up fails in a way that names neither: xcodebuild goes
        // looking for a simulator with a phone's udid, finds nothing,
        // and reports a destination error that says nothing about
        // signing or about the device being physical.
        match target {
            RunnerTarget::Simulator => format!("platform=iOS Simulator,id={udid}"),
            RunnerTarget::Physical { .. } => format!("platform=iOS,id={udid}"),
        },
        "-derivedDataPath".into(),
        derived,
    ];
    if let RunnerTarget::Physical { team } = target {
        // Both, together, are what makes an unconfigured checkout build
        // for a phone: the team says who signs, and the flag lets Xcode
        // create or update the profile rather than failing on a device
        // it has not seen. Proven on 2026-08-06 — a bare checkout plus
        // these two produced a signed device build.
        argv.push(format!("DEVELOPMENT_TEAM={team}"));
        argv.push("-allowProvisioningUpdates".into());
    }
    argv
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
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
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
    let Some(start) = body
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
    else {
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

/// Probe a live runner for the in-process soft-cycle and report the
/// outcome. `POST /soft-cycle` asks the surviving XCUITest host to bounce
/// its FlyingFox server (via the in-process restart signal) and rebind
/// the app; a `GET /health` afterwards confirms the server came back on
/// the same port. The whole path costs one app relaunch (~seconds)
/// instead of the ~36 s SIGINT-teardown + xcodebuild respawn a hard cycle
/// pays.
///
/// Only a reachable runner can be soft-cycled: if `/health` does not
/// answer up front the host is dead or wedged and the caller must hard
/// cycle. An older runner that never learned the route answers 404 →
/// [`SoftCycleProbe::Unsupported`], also a hard fallback.
fn try_soft_cycle(port: u16) -> smix_runner_client::SoftCycleProbe {
    use smix_runner_client::SoftCycleProbe;
    if !health_ok(port) {
        return SoftCycleProbe::Unreachable;
    }
    let start = std::time::Instant::now();
    match post_soft_cycle(port) {
        Ok((200, _body)) => {
            // The bounce briefly drops the listening socket; confirm the
            // reborn server answers before declaring recovery.
            if wait_health_back(port, std::time::Duration::from_secs(15)) {
                SoftCycleProbe::Recovered {
                    wall_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                }
            } else {
                SoftCycleProbe::Failed(
                    "/soft-cycle answered 200 but /health did not return after the bounce"
                        .to_string(),
                )
            }
        }
        Ok((404, _)) | Ok((400, _)) => SoftCycleProbe::Unsupported,
        Ok((status, _)) => SoftCycleProbe::Failed(format!("/soft-cycle returned status {status}")),
        Err(()) => {
            // The response was cut mid-flight. If the server nonetheless
            // came back, the bounce did happen — treat it as recovered;
            // otherwise it genuinely failed.
            if wait_health_back(port, std::time::Duration::from_secs(15)) {
                SoftCycleProbe::Recovered {
                    wall_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                }
            } else {
                SoftCycleProbe::Failed(
                    "/soft-cycle connection dropped and /health did not return".to_string(),
                )
            }
        }
    }
}

/// `POST /soft-cycle` over raw loopback TCP. Returns `(status, body)`.
/// The read timeout is generous because the handler performs the app
/// relaunch before it writes the response.
fn post_soft_cycle(port: u16) -> Result<(u16, Vec<u8>), ()> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2))
        .map_err(|_| ())?;
    s.set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|_| ())?;
    s.write_all(b"POST /soft-cycle HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .map_err(|_| ())?;
    let mut buf = Vec::with_capacity(512);
    s.read_to_end(&mut buf).map_err(|_| ())?;
    if buf.is_empty() {
        return Err(());
    }
    let status = parse_http_status(&buf).ok_or(())?;
    Ok((status, buf))
}

/// `GET /screenshot` over raw loopback TCP. Returns the PNG bytes.
///
/// The only way to photograph a physical iPhone: Apple exposes no screen
/// capture for one through `simctl` or `devicectl`, but `XCUIScreen` runs
/// inside the runner and works on device and simulator alike.
///
/// Raw TCP rather than an HTTP crate, which is what every other call to
/// the runner in this file does — one request to loopback does not earn a
/// dependency.
///
/// # Errors
///
/// The three failures are told apart because their fixes are not alike:
/// no runner (bring one up), a runner that refused (the device or the
/// session), and a body that is not a picture (worth saying rather than
/// writing a file nobody can open).
pub fn screenshot(port: u16) -> Result<Vec<u8>, ScreenshotError> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2))
        .map_err(|_| ScreenshotError::NoRunner { port })?;
    s.set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|_| ScreenshotError::NoRunner { port })?;
    s.write_all(b"GET /screenshot HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .map_err(|_| ScreenshotError::NoRunner { port })?;
    let mut buf = Vec::with_capacity(1 << 20);
    s.read_to_end(&mut buf)
        .map_err(|e| ScreenshotError::Truncated(e.to_string()))?;
    let status = parse_http_status(&buf).ok_or(ScreenshotError::NoRunner { port })?;
    let body = http_body(&buf).unwrap_or_default();
    if status != 200 {
        return Err(ScreenshotError::Refused {
            status,
            detail: String::from_utf8_lossy(body).trim().to_string(),
        });
    }
    // A zero-byte 200 is the failure this whole path exists to not have:
    // written to disk it becomes a file every later step treats as a
    // picture of the screen.
    if body.is_empty() {
        return Err(ScreenshotError::Empty);
    }
    Ok(body.to_vec())
}

/// Why a screenshot could not be fetched from the runner.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScreenshotError {
    /// Nothing is answering on the port.
    #[error(
        "no runner is answering on port {port}, and a physical device has no other way to \
         be seen — Apple exposes no screen capture for one through simctl or devicectl.\n\
         Bring it up first:\n  smix runner up <device> --bundle <id>"
    )]
    NoRunner {
        /// The port that was dialed.
        port: u16,
    },
    /// The runner answered, and said no.
    #[error(
        "the runner refused to capture the screen ({status}): {detail}\n\
         It is up, so this is the device or the session rather than the connection."
    )]
    Refused {
        /// HTTP status it answered with.
        status: u16,
        /// Whatever it said about why.
        detail: String,
    },
    /// 200 with nothing in it.
    #[error(
        "the runner answered 200 with an empty body — a zero-byte file is not a \
         screenshot, so nothing was written"
    )]
    Empty,
    /// The connection died mid-body.
    #[error("the screenshot body did not arrive in full: {0}")]
    Truncated(String),
}

/// The bytes after the blank line that ends an HTTP response's headers.
fn http_body(buf: &[u8]) -> Option<&[u8]> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| &buf[at + 4..])
}

/// Parse the numeric status from an HTTP response's status line
/// (`HTTP/1.1 200 OK`).
fn parse_http_status(buf: &[u8]) -> Option<u16> {
    let line_end = buf
        .windows(2)
        .position(|w| w == b"\r\n")
        .unwrap_or(buf.len());
    let line = std::str::from_utf8(&buf[..line_end]).ok()?;
    line.split_whitespace().nth(1)?.parse::<u16>().ok()
}

/// Poll `GET /health` until it answers 200 or the deadline passes.
fn wait_health_back(port: u16, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if health_ok(port) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Forget the iOS runner record. Reported rather than discarded: a
/// stale record makes the next `up` believe a runner is already there.
fn clear_state(root: &Path) {
    if let Err(e) = crate::runner_state::clear(root, crate::runner_state::Platform::Ios) {
        eprintln!("runner: {e}");
    }
}

/// The iOS runner's record. `None` is "no runner"; a record that
/// cannot be read is reported, not swallowed — the `.ok()?` this
/// replaces turned a damaged record into "no runner", and `up` would
/// then start a second one beside the first.
fn read_state(root: &Path) -> Option<RunnerState> {
    match crate::runner_state::read(root, crate::runner_state::Platform::Ios) {
        Ok(state) => state,
        Err(e) => {
            eprintln!("runner: {e}");
            None
        }
    }
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

/// The simulator UDID a runner-app process belongs to, read off its
/// executable path.
///
/// A runner app lives under
/// `…/CoreSimulator/Devices/<UDID>/data/Containers/Bundle/…`, so the
/// path is what ties a listening socket back to a sim. The process
/// holding the port is the runner app inside the simulator, not the
/// `xcodebuild` that started the session, and the two are not related by
/// parentage — the app is a child of the sim's own launchd. The path is
/// the link between them.
fn udid_from_device_path(cmd: &str) -> Option<String> {
    let rest = cmd.split("/Devices/").nth(1)?;
    let udid = rest.split('/').next()?;
    // A UDID is 8-4-4-4-12 hex. Anything else under Devices/ is not one,
    // and guessing would hand a sweep the wrong sim.
    let shape: Vec<usize> = udid.split('-').map(str::len).collect();
    let hex = udid.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    if shape == [8, 4, 4, 4, 12] && hex {
        Some(udid.to_string())
    } else {
        None
    }
}

/// Whether a command line is an `xcodebuild` session driving `udid`.
///
/// `runner_up` puts the sim in `-destination platform=iOS Simulator,
/// id=<UDID>`, which is the only place the session names its device —
/// the port arrives as an environment variable, and macOS does not let
/// one process read another's environment.
fn xcodebuild_drives_udid(cmd: &str, udid: &str) -> bool {
    cmd.contains("xcodebuild") && cmd.contains(&format!("id={udid}"))
}

/// PIDs listening on `port`, as reported by `lsof`.
fn listener_pids(port: u16) -> Vec<u32> {
    let Ok(out) = std::process::Command::new("lsof")
        .args(["-nP", "-t", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

/// PIDs of `xcodebuild` runner sessions driving the sim that holds
/// `port`, with no state handle of our own to go on.
///
/// This used to be `pkill -f "xcodebuild.*SmixRunner"`, which matches on
/// the process name and so reached every runner on the machine. `smix`
/// supports several at once — `--parallel` and federation both depend on
/// it — and a teardown pinned to one port killed a resident runner on
/// another, belonging to someone else.
///
/// A session that is not answering on this port is not found here, and
/// that is the trade: an unrecorded, wedged runner now has to be stopped
/// by hand rather than by a sweep that could not tell whose it was.
fn unrecorded_sessions_on(port: u16) -> Vec<u32> {
    let udids: Vec<String> = listener_pids(port)
        .into_iter()
        .filter_map(pid_command)
        .filter_map(|cmd| udid_from_device_path(&cmd))
        .collect();
    if udids.is_empty() {
        return Vec::new();
    }
    let Ok(out) = std::process::Command::new("pgrep")
        .args(["-f", "xcodebuild.*SmixRunner"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .filter(|pid| {
            pid_command(*pid)
                .is_some_and(|cmd| udids.iter().any(|u| xcodebuild_drives_udid(&cmd, u)))
        })
        .collect()
}

/// What to do about runner sessions this workspace has no record of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unrecorded {
    /// Nothing is holding the port but us.
    None,
    /// Something is, and nobody said to take it down.
    Reported(Vec<u32>),
    /// Something is, and somebody said so.
    Taken(Vec<u32>),
}

/// Decide what a teardown may do to a runner it did not start.
///
/// The ledger is the authority: what is not written down is not this
/// workspace's to end. `runner up` has said so since C6 — it refuses a
/// port held by a runner the store has no record of, rather than killing
/// blindly. `down` did the opposite, quietly, and that is the shape of
/// the 2026-07 incident where a sweep took out another session's runner.
///
/// The fix is not "never" — `up`'s refusal points at `runner down` as the
/// way out, and a guard that leaves someone with no path gets worked
/// around rather than obeyed. It is "not silently": the default reports,
/// and taking it down is something a person says out loud.
#[must_use]
pub fn decide_unrecorded(pids: Vec<u32>, consented: bool) -> Unrecorded {
    if pids.is_empty() {
        Unrecorded::None
    } else if consented {
        Unrecorded::Taken(pids)
    } else {
        Unrecorded::Reported(pids)
    }
}

/// What a teardown says when it will not end a runner it did not start.
///
/// Its own function so the wording can be asserted rather than grepped
/// for: the sentence *is* the deliverable here — a refusal that does not
/// say whose the process might be, or how to proceed, is how a guard
/// turns into an obstacle and then into something people route around.
#[must_use]
pub fn unrecorded_refusal(port: u16, pids: &[u32]) -> String {
    let list = pids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "port {port} is held by a runner this workspace has no record of \
         (pid {list}), and it is still running.\n\
         It may belong to another session — check before ending it:\n  \
         ps -o lstart=,command= -p {list}\n\
         If it should go, say so:\n  \
         smix runner down --include-unrecorded"
    )
}

pub(crate) fn signal(pid: u32, sig: &str) {
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
            Ok(SyncOutcome::Extracted {
                previous_version, ..
            }) => {
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
pub fn installed_runner_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("smix/runner"))
}

/// Outcome of an [`ensure_installed_runner_synced`] call.
#[derive(Debug)]
pub enum SyncOutcome {
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
pub fn ensure_installed_runner_synced(
    dir: &Path,
) -> Result<SyncOutcome, smix_runner_sources::ExtractError> {
    let previous = smix_runner_sources::read_installed_version(dir)?;
    // Compared against version *and* digest. Version alone held only
    // across releases: change a Swift source between two of them and the
    // stamp still matched, so the tree was left alone and the device
    // kept running the old runner while the repo showed the new. What
    // that looks like from the outside is a route that 404s — a bug in
    // the caller, apparently.
    if smix_runner_sources::stamp_is_current(previous.as_deref()) {
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

/// What `runner up` should do beyond starting the process.
///
/// These were trailing positional bools; a third one would have made
/// `up(_, _, _, _, false, _, false, true)` the call site.
#[derive(Clone, Copy, Debug, Default)]
pub struct UpOptions {
    /// Let the runner record events (`capsule up` wants this; bare
    /// `runner up` does not).
    pub record_enabled: bool,
    /// Spawn the `runner supervise` sidecar once `/health` answers.
    pub supervise: bool,
    /// Foreground the target app instead of relaunching it.
    ///
    /// Bringing the runner up restarts the app, which drops whatever
    /// screen had been navigated to — and then reports the next flow's
    /// failure as `ELEMENT_NOT_FOUND` against a splash screen.
    pub attach_without_relaunch: bool,
}

/// Bring the runner up on `udid`. Blocks until `/health` answers 200 or
/// the timeout (env `SMIX_RUNNER_UP_TIMEOUT_SECS`, default 300 — first
/// run includes a full Swift build) expires.
///
/// `runner_project` — optional explicit path to `SmixRunner.xcodeproj`.
/// When `None`, uses [`resolve_runner_project`] cascade against `root`.
///
/// With `opts.supervise`, after `/health` returns 200 spawn a detached
/// `smix runner supervise` process, record its pid in state.json, and
/// return. `runner down` cascades a SIGTERM to that pid before tearing
/// down xcodebuild.
pub fn up(
    root: &Path,
    udid: &str,
    port: u16,
    bundle: Option<&str>,
    runner_project: Option<&Path>,
    opts: UpOptions,
) -> Result<(), String> {
    // The 2.x face: every caller that predates physical devices meant a
    // simulator, and keeps meaning one without changing a line.
    up_on(
        root,
        udid,
        port,
        bundle,
        runner_project,
        opts,
        RunnerTarget::Simulator,
    )
}

/// [`up`], with the device world said explicitly.
///
/// A separate name rather than a field on [`UpOptions`]: the options
/// struct is externally constructible, so a new field would have broken
/// every existing `UpOptions { .. }` literal — and cost the struct its
/// `Copy` — to say something only the physical path says. The target
/// carries the signing team, which only exists in that world.
#[allow(clippy::too_many_arguments)]
pub fn up_on(
    root: &Path,
    udid: &str,
    port: u16,
    bundle: Option<&str>,
    runner_project: Option<&Path>,
    opts: UpOptions,
    target: RunnerTarget<'_>,
) -> Result<(), String> {
    let UpOptions {
        record_enabled,
        supervise,
        attach_without_relaunch,
    } = opts;
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
                return Err("no --bundle passed; the runner would latch to the \
                     built-in default (com.apple.Preferences) and every \
                     subsequent /tree call would report Preferences as the \
                     app.\n\n\
                     fix: pass --bundle <your-app-bundle-id>, e.g.\n\
                       smix runner up <device> --bundle com.example.app\n\n\
                     to keep the legacy default (v1.0.3 behavior), export\n\
                       SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1\n\
                     — but expect empty a11y trees until you re-attach a \
                     real target."
                    .to_string());
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
                // Naming the command that actually resolves it. This
                // used to end at `smix runner down`, which since C17
                // reports the same session and stops — so the advice
                // sent people in a circle, twice past the one fact that
                // mattered: it might be somebody else's.
                return Err(format!(
                    "port {port} already serves /health but the store has no \
                     record of that runner — not killing blindly.\n\
                     See whose it is:\n  pgrep -fl xcodebuild\n\
                     If it should go:\n  smix runner down --include-unrecorded\n\
                     If it should stay, bring this one up elsewhere:\n  \
                     SMIX_RUNNER_PORT=<other> smix runner up …"
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
    cmd.args(xcodebuild_argv_for(&project, udid, target))
        .envs(runner_env(
            bundle,
            record_enabled,
            port,
            attach_without_relaunch,
        ))
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_err);
    // Own process group so the session outlives this CLI invocation and a
    // ctrl-C on smix doesn't tear the runner down implicitly.
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    // The pipe first, on a phone.
    //
    // The runner listens on the *device's* loopback, so without this the
    // health probe below dials a port on the Mac that nothing is on. The
    // failure would read as "the runner never became healthy", which is
    // both wrong and the most expensive kind of wrong — it sends whoever
    // reads it into the runner logs rather than at the missing pipe.
    if matches!(target, RunnerTarget::Physical { .. }) {
        let fwd_pid = spawn_forwarder(root, udid, port)?;
        println!("runner up: port forward {port} -> device (pid {fwd_pid})");
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
    crate::runner_state::write(root, crate::runner_state::Platform::Ios, &st)?;
    record_runner_lease(root, udid, port, pid)?;

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
    // Said once, the moment it is known — not saved for the timeout.
    // Whoever is watching can act on it while the wait is still running,
    // and xcodebuild picks up where it left off once they do.
    let mut announced_block = false;
    while std::time::Instant::now() < deadline {
        if !announced_block && let Some(blocked) = device_preflight_block(&log) {
            println!("runner up: waiting on the device — {blocked}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            announced_block = true;
        }
        if is_cold && last_heartbeat.elapsed() >= std::time::Duration::from_secs(30) {
            let elapsed_s = started_at.elapsed().as_secs();
            println!("runner up: xcodebuild still working ({elapsed_s}s elapsed)");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            last_heartbeat = std::time::Instant::now();
        }
        if let Ok(Some(status)) = child.try_wait() {
            clear_state(root);
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
            // Set when the wire-schema branch has already announced the
            // runner, so the version branch below does not say it twice.
            let mut wire_reported = false;
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
                        // Deliberately not returning here.
                        //
                        // It used to, and that quietly disabled
                        // `--supervise` for every runner new enough to
                        // report a wire schema — which is all of them. The
                        // sidecar block lives at the end of this branch,
                        // and returning from the middle skipped it: the
                        // flag was accepted, nothing was spawned, and
                        // nothing said so.
                        wire_reported = true;
                    }
                    None => {
                        clear_state(root);
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
                    if !wire_reported {
                        println!("runner up: http://localhost:{port}/health = 200 (runner v{v})");
                    }
                }
                Some(v) => {
                    clear_state(root);
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
                        record_supervisor_lease(root, udid, sup_pid);
                        // Rewrite state.json with the supervisor pid.
                        if let Some(mut current) = read_state(root) {
                            current.supervisor_pid = Some(sup_pid);
                            // Not discarded: losing the supervisor pid
                            // means `runner down` never cascades SIGTERM
                            // to the sidecar, and the sidecar outlives
                            // the runner it was watching.
                            if let Err(e) = crate::runner_state::write(
                                root,
                                crate::runner_state::Platform::Ios,
                                &current,
                            ) {
                                eprintln!("runner supervise: {e}");
                            }
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
    clear_state(root);
    // Lead with the reason when the device itself said one. A timeout
    // whose log ends in "Unlock panda's iphone to Continue" is not a
    // timeout anyone needs 25 lines of build transcript to understand.
    if let Some(blocked) = device_preflight_block(&log) {
        return Err(format!(
            "the device never became ready, so the runner never started: {blocked}\n\
             xcodebuild waits for this rather than failing, so it sat for \
             {timeout_secs}s — sent SIGINT. Clear it and run the same command again."
        ));
    }
    Err(format!(
        "runner did not become healthy within {timeout_secs}s — sent SIGINT; log tail:\n{}",
        tail_log(&log, 25)
    ))
}

/// What the device is waiting for, if `xcodebuild` said so.
///
/// A locked phone does not fail a build — it parks it. `xcodebuild`
/// writes `Run Destination Preflight: The destination is not ready` and
/// then waits, so a caller polling `/health` sees nothing at all until
/// the timeout, and the one sentence that would have taken a second to
/// act on ("Unlock panda's iphone to Continue") stays in a file nobody
/// was told to open.
fn device_preflight_block(log: &Path) -> Option<String> {
    let text = std::fs::read_to_string(log).ok()?;
    if !text.contains("Run Destination Preflight") {
        return None;
    }
    // `NSLocalizedRecoverySuggestion=` carries the sentence written for
    // a person; the quoted error description is the terse version.
    let suggestion = text.find("NSLocalizedRecoverySuggestion=").map(|at| {
        let rest = &text[at + "NSLocalizedRecoverySuggestion=".len()..];
        let end = rest.find(", DVT").or_else(|| rest.find('\n')).unwrap_or(0);
        rest[..end].trim().to_string()
    });
    suggestion
        .filter(|s| !s.is_empty())
        .or_else(|| Some("the destination is not ready".to_string()))
}

/// Tear the runner down. SIGINT first — xcodebuild cancels the XCUITest
/// session cleanly via testmanagerd; a hard kill SIGABRTs the runner app
/// and macOS pops a crash-report dialog that steals user focus.
///
/// If state.json records a supervisor pid, cascade a SIGTERM to it
/// BEFORE tearing down xcodebuild. Otherwise the sidecar
/// would flap into a `TEST INTERRUPTED` trigger the moment we send
/// SIGINT to xcodebuild and try to re-cycle a runner we just killed.
/// Stop the runner this workspace recorded. A runner on the port that
/// the store has no record of is reported, not ended — it may belong to
/// another session, and [`down_including_unrecorded`] is the sanctioned
/// way through when it should go.
pub fn down(root: &Path, port: u16) -> Result<(), String> {
    down_with(root, port, false)
}

/// [`down`], and also end any runner on the port the store has no
/// record of. The consent lives in the name: only a person typing
/// `--include-unrecorded` is in a position to know the unrecorded
/// session should go, so only the CLI's flag path calls this.
pub fn down_including_unrecorded(root: &Path, port: u16) -> Result<(), String> {
    down_with(root, port, true)
}

fn down_with(root: &Path, port: u16, consent: bool) -> Result<(), String> {
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
        // The forwarder, before the ledger row that knows about it is
        // dropped. It is its own process on purpose — `up` exits and the
        // pipe must not — which means nothing dies with the runner: the
        // one that outlived a passing e2e by five hours was found still
        // forwarding to a phone whose runner was long gone, and the
        // teardown check ("ledger empty, /health silent") could not see
        // it precisely because a forwarder with no runner behind it
        // answers nothing.
        if let Ok(facts) = smix_lease::store::collect_facts(root, &st.udid)
            && let Some(held) = facts.existing
        {
            for r in &held.lease.resources {
                if let smix_lease::Resource::PortForward {
                    local_port, proc, ..
                } = r
                {
                    println!("stopping port forward on {local_port}: pid={}", proc.pid);
                    match crate::reconcile::stop_port_forward(*local_port, proc) {
                        crate::reconcile::Outcome::Failed(line) => eprintln!("  {line}"),
                        outcome => println!("  {}", outcome.line()),
                    }
                }
            }
        }
        clear_state(root);
        forget_runner_lease(root, &st.udid);
    }

    // Sessions started outside `smix runner up`, narrowed to whoever
    // actually holds this port. Not ours to end on our own say-so.
    match decide_unrecorded(unrecorded_sessions_on(port), consent) {
        Unrecorded::None => {}
        Unrecorded::Taken(pids) => {
            for pid in pids {
                println!("stopping unrecorded runner session on port {port}: pid={pid}");
                signal(pid, "-INT");
                acted = true;
            }
        }
        Unrecorded::Reported(pids) => return Err(unrecorded_refusal(port, &pids)),
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
pub fn cycle(root: &Path, port: u16, runner_project: Option<&Path>) -> Result<(), String> {
    let st = read_state(root).ok_or_else(|| {
        "no runner recorded — cycle only cycles a known runner; \
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
            "note: the recorded port {cycle_port} differs from --runner-port {port}; \
             cycling on state.json's {cycle_port}"
        );
    }
    println!("cycling runner: udid={udid} port={cycle_port} bundle={bundle:?}");

    // Try the in-process soft-cycle first: if the XCUITest host is alive
    // and answering /health, it can bounce its server + relaunch the app
    // in seconds without the ~36 s SIGINT-teardown + xcodebuild respawn.
    // Anything else (host dead, wedged, or an older runner that never
    // learned /soft-cycle) falls back to the byte-identical hard cycle,
    // preserving the N=1 / no-supervisor contract.
    match smix_runner_client::soft_cycle_plan(try_soft_cycle(cycle_port)) {
        smix_runner_client::CyclePlan::Soft { wall_ms } => {
            println!(
                "runner soft-cycled: recovered in {wall_ms}ms on port {cycle_port} \
                 (host survived, no xcodebuild respawn)"
            );
            Ok(())
        }
        smix_runner_client::CyclePlan::HardFallback { reason } => {
            println!("soft-cycle unavailable ({reason}); hard-cycling via xcodebuild");
            // No: a cycle restarts the runner this state file names. If
            // something else holds the port, the restart is not what is
            // wanted anyway — better to say so than to clear the way by
            // ending somebody else's session.
            down(root, cycle_port)?;
            up(
                root,
                &udid,
                cycle_port,
                bundle.as_deref(),
                runner_project,
                UpOptions {
                    supervise: had_supervisor,
                    ..Default::default()
                },
            )
        }
    }
}

/// Collect ±`context_size` lines surrounding the first occurrence of
/// `match_line` inside the log file. Best-effort: returns empty on
/// file-read failure, or on partial matches (log rotated between
/// trigger + read). Emitted inside the supervisor's `RunnerCycled` JSON
/// event so callers get cycle-cascade classification data without
/// needing a separate `grep` pass.
fn collect_log_context(log_path: &Path, match_line: &str, context_size: usize) -> Vec<String> {
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
/// Spawn a process whose only job is to hold a port forwarder open.
///
/// The forwarder is a listener living inside a process, and `runner up`
/// exits as soon as the runner is healthy — so a forwarder started here
/// would die seconds later, taking the only route to the device's runner
/// with it. Same shape as the supervisor sidecar, and for the same
/// reason: what must outlive the command has to be its own process.
///
/// Returns the child's pid so the ledger can record something a later
/// teardown can find and stop.
fn spawn_forwarder(root: &Path, udid: &str, port: u16) -> Result<u32, String> {
    let runner_dir = root.join(".smix/runner");
    std::fs::create_dir_all(&runner_dir).map_err(|e| format!("mkdir .smix/runner: {e}"))?;
    let log = runner_dir.join(format!("forward-{udid}.log"));
    let log_file =
        std::fs::File::create(&log).map_err(|e| format!("create {}: {e}", log.display()))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("clone forward log handle: {e}"))?;

    let self_exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let mut cmd = std::process::Command::new(&self_exe);
    cmd.arg("runner")
        .arg("forward")
        .arg(udid)
        .arg("--port")
        .arg(port.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_err);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group: a Ctrl-C in the terminal that ran
        // `runner up` should not take the forwarder with it, any more
        // than it takes the runner.
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("spawn forwarder: {e}"))?;
    let pid = child.id();

    // Wait for the port to answer, and treat not answering as a failure.
    //
    // This used to wait and then report success either way, which is
    // worse than not waiting at all: on 2026-08-06 the forwarder died
    // immediately — it could not resolve the device it was handed — and
    // `runner up` printed `port forward 22097 -> device (pid 30199)`
    // about a process that was already gone, wrote a ledger row for it,
    // and let xcodebuild run for two minutes against a pipe that never
    // existed. A check that cannot fail is decoration.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut ready = false;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "the port forwarder exited immediately ({status}). It is a \
                 `smix runner forward {udid} --port {port}` of its own, and its \
                 output is in {}:\n{}",
                log.display(),
                tail_for_error(&log)
            ));
        }
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if !ready {
        let _ = child.kill();
        return Err(format!(
            "the port forwarder never accepted a connection on {port} within 10s. \
             Its output is in {}:\n{}",
            log.display(),
            tail_for_error(&log)
        ));
    }

    record_forward_lease(root, udid, port, pid);
    Ok(pid)
}

/// Last few lines of a log, for putting inside an error message.
///
/// The forwarder's own failure is written there and nowhere else; an
/// error that names a path a person then has to go and open is a worse
/// error than one that quotes it.
fn tail_for_error(path: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return "  (its log could not be read)".to_string();
    };
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.starts_with("kevy:") && !l.trim().is_empty())
        .collect();
    let start = lines.len().saturating_sub(5);
    lines[start..]
        .iter()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Record the forwarder in the device's ledger.
fn record_forward_lease(root: &Path, udid: &str, port: u16, pid: u32) {
    use smix_lease::store;
    let proc = store::identify(pid).unwrap_or(smix_lease::ProcIdentity {
        pid,
        started_at: String::new(),
        cmd: format!("smix runner forward {udid}"),
    });
    if let Err(e) = store::add_resource(
        root,
        udid,
        smix_lease::Resource::PortForward {
            local_port: port,
            device_port: port,
            proc,
        },
    ) {
        eprintln!("warning: port forward not recorded in the device ledger: {e}");
    }
}

fn spawn_supervisor(root: &Path, runner_project: Option<&Path>) -> Result<u32, String> {
    let st = read_state(root)
        .ok_or_else(|| "internal: no state.json to attach supervisor to".to_string())?;
    let udid = st.udid.clone();
    let runner_dir = root.join(".smix/runner");
    std::fs::create_dir_all(&runner_dir).map_err(|e| format!("mkdir .smix/runner: {e}"))?;
    let log = runner_dir.join(format!("supervise-{udid}.log"));
    let log_file =
        std::fs::File::create(&log).map_err(|e| format!("create {}: {e}", log.display()))?;
    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("clone supervise log handle: {e}"))?;

    let self_exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
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
pub fn supervise(root: &Path, runner_project: Option<&Path>) -> Result<(), String> {
    let st = read_state(root).ok_or_else(|| {
        "no runner recorded — supervise attaches to a known runner; \
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

    let mut position: u64 = std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
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
        let tail = if keep_last {
            lines.pop().unwrap_or("").to_string()
        } else {
            String::new()
        };
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

/// Record the runner in the device's lease ledger.
///
/// `smix runner up` returns as soon as the runner is healthy, so the
/// process that wrote this row is gone within seconds while the
/// `xcodebuild` it started keeps the device for hours. The row is what
/// lets the next command tell "someone is working here" from "someone
/// was killed here", and it is what gives that xcodebuild a graceful
/// teardown if nobody ever comes back for it.
///
/// A ledger that cannot be written is not shrugged off: the whole
/// mechanism is the ledger, and a session running without one is exactly
/// the state that leaves crash dialogs behind.
/// Record the supervisor sidecar in the device's ledger.
///
/// The sidecar outlives the command that spawned it and will restart a
/// runner it finds dead. A teardown that does not know about it stops
/// the runner and watches it come back — so the row exists to make the
/// sidecar something a later process can find and stop first.
///
/// Reported rather than propagated: the runner is already up and healthy
/// by this point, and failing the whole bring-up over bookkeeping would
/// throw away a working session.
fn record_supervisor_lease(root: &Path, udid: &str, pid: u32) {
    use smix_lease::store;
    let proc = store::identify(pid).unwrap_or(smix_lease::ProcIdentity {
        pid,
        started_at: String::new(),
        cmd: "smix runner supervise".to_string(),
    });
    if let Err(e) = store::add_resource(root, udid, smix_lease::Resource::Supervisor { proc }) {
        eprintln!("warning: supervisor not recorded in the device ledger: {e}");
    }
}

fn record_runner_lease(root: &Path, udid: &str, port: u16, pid: u32) -> Result<(), String> {
    use smix_lease::store;
    let proc = store::identify(pid).unwrap_or(smix_lease::ProcIdentity {
        pid,
        // An empty start time matches nothing, so a runner that died
        // between spawn and probe reads as gone rather than as a live
        // holder nobody can displace.
        started_at: String::new(),
        cmd: format!("xcodebuild test … id={udid}"),
    });
    store::add_resource(root, udid, smix_lease::Resource::Runner { port, proc })
        .map_err(|e| e.to_string())
}

/// Drop the runner row after a clean teardown, and the whole ledger with
/// it when nothing else is open.
///
/// Failures here are reported, not propagated: teardown already
/// succeeded, and refusing to admit that because the bookkeeping failed
/// would leave callers retrying a stop that already happened.
fn forget_runner_lease(root: &Path, udid: &str) {
    use smix_lease::store;
    // Both rows, because `down` stops both: the supervisor first, then
    // the runner. Leaving the supervisor row behind would have the next
    // reconcile try to stop a sidecar that is already gone.
    for sample in [
        smix_lease::Resource::Runner {
            port: 0,
            proc: store::identify_self(),
        },
        smix_lease::Resource::Supervisor {
            proc: store::identify_self(),
        },
        smix_lease::Resource::PortForward {
            local_port: 0,
            device_port: 0,
            proc: store::identify_self(),
        },
    ] {
        if let Err(e) = store::drop_resource_kind(root, udid, &sample) {
            eprintln!("runner down: lease ledger not updated: {e}");
        }
    }
}

#[cfg(test)]
#[allow(unsafe_code)] // env mutation: the thing under test
mod tests {
    use super::{ScreenshotError, Unrecorded, decide_unrecorded, http_body, unrecorded_refusal};

    #[test]
    fn an_http_body_starts_after_the_blank_line() {
        let resp = b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\n\r\n\x89PNG\r\n";
        assert_eq!(http_body(resp), Some(&b"\x89PNG\r\n"[..]));
        // A body may itself contain \r\n\r\n; only the first one ends the
        // headers, and a PNG is full of bytes that could be anything.
        let with_blanks = b"HTTP/1.1 200 OK\r\n\r\nAA\r\n\r\nBB";
        assert_eq!(http_body(with_blanks), Some(&b"AA\r\n\r\nBB"[..]));
        assert_eq!(http_body(b"no headers here"), None);
    }

    #[test]
    fn screenshot_failure_names_a_different_fix_each() {
        // They read the same to someone holding an empty file — "no
        // screenshot" — and the things to do about them are unalike.
        let no_runner = ScreenshotError::NoRunner { port: 22087 }.to_string();
        assert!(no_runner.contains("smix runner up"), "{no_runner}");
        assert!(no_runner.contains("22087"), "{no_runner}");

        let refused = ScreenshotError::Refused {
            status: 503,
            detail: "XCUIScreen returned no PNG representation".into(),
        }
        .to_string();
        assert!(refused.contains("503"), "{refused}");
        assert!(
            refused.contains("device or the session"),
            "a refusal must not read as a connection problem: {refused}"
        );

        let empty = ScreenshotError::Empty.to_string();
        assert!(
            empty.contains("nothing was written"),
            "the caller must know no file appeared: {empty}"
        );
    }

    #[test]
    fn the_refusal_says_whose_it_might_be_and_how_to_proceed() {
        // Three things, and dropping any one turns a guard into an
        // obstacle: which process, that it may not be yours, and the
        // way through. A refusal missing the last one gets routed
        // around rather than obeyed.
        let msg = unrecorded_refusal(22087, &[4242]);
        assert!(msg.contains("4242"), "names no process: {msg}");
        assert!(
            msg.contains("another session"),
            "does not raise whose: {msg}"
        );
        assert!(
            msg.contains("smix runner down --include-unrecorded"),
            "no way through: {msg}"
        );
        // And it must not claim to have done anything.
        for done in ["skipped", "ignored", "stopped", "closed"] {
            assert!(!msg.contains(done), "reads as handled ({done}): {msg}");
        }
    }

    #[test]
    fn nothing_unrecorded_is_nothing_to_do() {
        assert_eq!(decide_unrecorded(vec![], false), Unrecorded::None);
        // Consent does not invent work.
        assert_eq!(decide_unrecorded(vec![], true), Unrecorded::None);
    }

    #[test]
    fn an_unrecorded_session_is_reported_not_taken() {
        // The whole point: `runner up` refuses a port held by a runner
        // it has no record of, and `down` used to end exactly that
        // session without asking. Two commands, one situation, opposite
        // answers — and the quiet one was the dangerous one.
        assert_eq!(
            decide_unrecorded(vec![4242], false),
            Unrecorded::Reported(vec![4242])
        );
    }

    #[test]
    fn consent_takes_it_down() {
        // Not "never" — `up`'s refusal points here, and a guard that
        // leaves someone with no way through gets worked around.
        assert_eq!(
            decide_unrecorded(vec![4242, 4243], true),
            Unrecorded::Taken(vec![4242, 4243])
        );
    }

    use super::*;
    use std::fs;
    use std::sync::Mutex;

    const UDID: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";

    /// Serialize the resolver tests that mutate process-global env. Each
    /// uses a test-only var name, but `set_var`/`remove_var` still churn
    /// the shared environ table, so they hold this lock while doing so.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn xcodebuild_argv_targets_explicit_udid() {
        let argv = xcodebuild_argv_for(
            Path::new("/repo/swift-bridge/SmixRunner.xcodeproj"),
            UDID,
            RunnerTarget::Simulator,
        );
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

    /// `capsule up` and `runner up` bring the runner up by binding it
    /// to a bundle, and binding launched — which restarts the app and
    /// drops whatever screen had been navigated to. The runner has
    /// understood `activate` since `LaunchModeResolver` was written and
    /// nothing on this side ever set it, so the mode was unreachable.
    #[test]
    fn attaching_asks_the_runner_to_activate_rather_than_relaunch() {
        let relaunching = runner_env(Some("com.example.app"), false, 22087, false);
        assert!(
            !relaunching
                .iter()
                .any(|(k, _)| k == "TEST_RUNNER_SMIX_RUNNER_LAUNCH_MODE"),
            "the default must stay launch, or every existing flow changes"
        );

        let attaching = runner_env(Some("com.example.app"), false, 22087, true);
        let mode = attaching
            .iter()
            .find(|(k, _)| k == "TEST_RUNNER_SMIX_RUNNER_LAUNCH_MODE")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            mode,
            Some("activate"),
            "the `TEST_RUNNER_` prefix is what makes xcodebuild surface \
             `SMIX_RUNNER_LAUNCH_MODE` inside the runner process"
        );
    }

    #[test]
    fn runner_env_uses_test_runner_prefix() {
        let env = runner_env(Some("com.example.app"), false, 22087, false);
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
        let env_no_bundle = runner_env(None, false, 22090, false);
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
        let env = runner_env(None, false, 22087, false);
        let map: std::collections::HashMap<String, String> = env.into_iter().collect();
        assert_eq!(
            map.get("TEST_RUNNER_SMIX_RUNNER_VERSION")
                .map(String::as_str),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn runner_env_with_record_adds_enabled_var() {
        let env = runner_env(Some("com.example.app"), true, 22087, false);
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
        matches!(
            outcome,
            SyncOutcome::Extracted {
                previous_version: None,
                ..
            }
        )
        .then_some(())
        .expect("expected first-run extract with no previous version");
        assert!(
            dir.path().join(".smix-runner-version").exists(),
            "version file must be written"
        );
        assert!(
            dir.path()
                .join("SmixRunner.xcodeproj/project.pbxproj")
                .exists(),
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
        fs::write(dir.path().join("stale-marker.txt"), b"old contents").expect("seed stale marker");

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
        // Version *and* digest. The version alone was what let a
        // rebuilt tarball compare equal to an old tree between releases.
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".smix-runner-version"))
                .unwrap()
                .trim(),
            smix_runner_sources::version_stamp()
        );
    }

    #[test]
    fn a_bare_version_stamp_is_treated_as_stale() {
        // What every tree written before digests existed looks like.
        // Re-extracting one needlessly costs a second; trusting one
        // costs a device running code nobody can see in the repo.
        assert!(!smix_runner_sources::stamp_is_current(Some(
            smix_runner_sources::SOURCES_VERSION
        )));
        assert!(!smix_runner_sources::stamp_is_current(None));
        assert!(smix_runner_sources::stamp_is_current(Some(
            &smix_runner_sources::version_stamp()
        )));
        // And the digest must actually depend on the bytes, or this
        // whole change is decoration.
        assert!(
            smix_runner_sources::version_stamp()
                .contains(&format!("{:016x}", smix_runner_sources::sources_digest()))
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
    fn switches_from_value_reads_only_present_keys() {
        // Schemaless read: only `autoOcrFallback` is set → the other
        // three stay `None`.
        let yaml = "switches:\n  autoOcrFallback: true\n";
        let value: serde_json::Value = serde_norway::from_str(yaml).unwrap();
        let sw = switches_from_value(&value);
        assert_eq!(sw.auto_ocr_fallback, Some(true));
        assert_eq!(sw.enable_ai_assertions, None);
        assert_eq!(sw.assert_screenshot_no_autorecord, None);
        assert_eq!(sw.launch_fresh_force_reinstall, None);
    }

    #[test]
    fn switches_from_value_empty_when_no_block() {
        let value: serde_json::Value =
            serde_norway::from_str("interactiveProbe:\n  minIdentifierCount: 3\n").unwrap();
        assert_eq!(switches_from_value(&value), SwitchesConfig::default());
    }

    #[test]
    fn resolve_switch_config_wins_over_env() {
        let _g = ENV_LOCK.lock().unwrap();
        let name = "SMIX_TEST_RESOLVE_CONFIG_WINS";
        // SAFETY: ENV_LOCK serializes env churn; the var name is unique
        // to this test.
        unsafe { std::env::set_var(name, "1") };
        // config=Some(false) must beat env=1, source Config, no warn.
        let r = resolve_switch(Some(false), name);
        assert!(!r.value);
        assert_eq!(r.source, SwitchSource::Config);
        unsafe { std::env::remove_var(name) };
    }

    #[test]
    fn resolve_switch_env_used_when_no_config() {
        let _g = ENV_LOCK.lock().unwrap();
        let name = "SMIX_TEST_RESOLVE_ENV_USED";
        // SAFETY: as above.
        unsafe { std::env::set_var(name, "1") };
        let r = resolve_switch(None, name);
        assert!(r.value);
        assert_eq!(r.source, SwitchSource::Env);
        unsafe { std::env::remove_var(name) };
    }

    #[test]
    fn resolve_switch_default_when_neither() {
        let _g = ENV_LOCK.lock().unwrap();
        let name = "SMIX_TEST_RESOLVE_DEFAULT";
        // SAFETY: as above — ensure the var is unset for this thread.
        unsafe { std::env::remove_var(name) };
        let r = resolve_switch(None, name);
        assert!(!r.value);
        assert_eq!(r.source, SwitchSource::Default);
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

    // --- port-scoped sweep -------------------------------------------

    const RUNNER_APP_CMD: &str = "/Users/x/Library/Developer/CoreSimulator/Devices/\
5D087114-ECB3-443C-8DDB-40EEF9CFB90C/data/Containers/Bundle/Application/\
DAD42368-FF61-4237-9205-8C3E041D89A7/SmixRunnerUITests-Runner.app/SmixRunnerUITests-Runner";

    #[test]
    fn udid_read_off_the_runner_app_path() {
        assert_eq!(
            udid_from_device_path(RUNNER_APP_CMD).as_deref(),
            Some("5D087114-ECB3-443C-8DDB-40EEF9CFB90C")
        );
    }

    #[test]
    fn udid_absent_when_the_path_is_not_under_a_device() {
        assert_eq!(
            udid_from_device_path("/usr/bin/some-server --port 22087"),
            None
        );
        assert_eq!(udid_from_device_path(""), None);
    }

    #[test]
    fn xcodebuild_session_matched_by_the_sim_it_drives() {
        let cmd = format!(
            "/usr/bin/xcodebuild test -project /r/SmixRunner.xcodeproj -scheme SmixRunner \
             -destination platform=iOS Simulator,id={UDID}"
        );
        assert!(xcodebuild_drives_udid(&cmd, UDID));
        assert!(!xcodebuild_drives_udid(
            &cmd,
            "FFC57DAE-4B26-4B0C-9FAD-4F5735C0C2B1"
        ));
    }

    #[test]
    fn a_non_xcodebuild_process_is_never_a_runner_session() {
        // Something else mentioning the same sim must not be swept: the
        // whole point of scoping is that we only ever stop our own
        // xcodebuild sessions.
        let cmd = format!("/usr/bin/log stream --predicate device == '{UDID}'");
        assert!(!xcodebuild_drives_udid(&cmd, UDID));
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;
    use std::path::Path;

    const SIM: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";
    const PHONE: &str = "00008120-001410C11A42201E";

    fn argv_for(udid: &str, target: RunnerTarget<'_>) -> Vec<String> {
        xcodebuild_argv_for(Path::new("/repo/SmixRunner.xcodeproj"), udid, target)
    }

    #[test]
    fn a_simulator_build_is_unchanged_and_unsigned() {
        // Every existing caller lands here. If this drifts, a working
        // setup breaks for a feature it never asked for.
        let argv = argv_for(SIM, RunnerTarget::Simulator);
        assert!(
            argv.iter()
                .any(|a| a == &format!("platform=iOS Simulator,id={SIM}"))
        );
        assert!(
            !argv.iter().any(|a| a.starts_with("DEVELOPMENT_TEAM")),
            "a simulator build needs no signing team: {argv:?}"
        );
        assert!(!argv.iter().any(|a| a == "-allowProvisioningUpdates"));
    }

    #[test]
    fn a_device_build_names_the_platform_the_team_and_the_flag() {
        let argv = argv_for(PHONE, RunnerTarget::Physical { team: "KF79DRC524" });
        assert!(
            argv.iter()
                .any(|a| a == &format!("platform=iOS,id={PHONE}"))
        );
        assert!(argv.iter().any(|a| a == "DEVELOPMENT_TEAM=KF79DRC524"));
        assert!(argv.iter().any(|a| a == "-allowProvisioningUpdates"));
    }

    #[test]
    fn a_device_build_never_says_simulator() {
        // The failure this pins names neither cause: xcodebuild goes
        // looking for a simulator with a phone's udid, finds none, and
        // reports a destination error that mentions neither signing nor
        // the device being physical.
        let argv = argv_for(PHONE, RunnerTarget::Physical { team: "KF79DRC524" });
        assert!(
            !argv.iter().any(|a| a.contains("iOS Simulator")),
            "device build carries a simulator destination: {argv:?}"
        );
    }

    #[test]
    fn the_derived_data_path_is_still_per_device() {
        // Two devices building at once would otherwise contend on one
        // DerivedData root, and the second hangs for minutes before
        // failing — the reason this path exists at all.
        let a = argv_for(SIM, RunnerTarget::Simulator);
        let b = argv_for(PHONE, RunnerTarget::Physical { team: "T" });
        let path_of = |v: &[String]| {
            let i = v.iter().position(|x| x == "-derivedDataPath").unwrap();
            v[i + 1].clone()
        };
        assert_ne!(path_of(&a), path_of(&b));
        assert!(path_of(&b).contains(PHONE));
    }
}
