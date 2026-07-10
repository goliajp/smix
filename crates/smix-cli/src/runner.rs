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
    env
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
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let Ok(mut s) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(1))
    else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    if s.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 64];
    let Ok(n) = s.read(&mut buf) else {
        return false;
    };
    String::from_utf8_lossy(&buf[..n]).contains(" 200")
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
/// 3. Install-shipped default:
///    - `$XDG_DATA_HOME/smix/runner/SmixRunner.xcodeproj` when set
///    - `~/.local/share/smix/runner/SmixRunner.xcodeproj` (macOS + Linux
///      XDG fallback)
/// 4. `<root>/swift-bridge/SmixRunner.xcodeproj` — smix-dev-repo fallback
///    so `cd smix; cargo run --bin smix -- runner up ...` still works
///    from a fresh checkout.
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
         fix: (a) install via `bash scripts/install-local.sh` to populate ~/.local/share/smix/runner/, \
         or (b) pass `--runner-project <path>` on `smix runner up`, \
         or (c) set $SMIX_RUNNER_PROJECT",
        last.display()
    ))
}

/// Install-shipped runner project location. Follows XDG basedir when
/// `$XDG_DATA_HOME` is set; falls back to `~/.local/share/smix/runner/`
/// on macOS + Linux. Returns `None` when `$HOME` is unset (rare).
fn installed_runner_project() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("smix/runner/SmixRunner.xcodeproj"))
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
    // v1.0.4 §A / D8 — refuse to boot without --bundle unless the
    // caller explicitly opts in via SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE=1.
    // Rationale: the runner's built-in default `com.apple.Preferences`
    // silently latches every `/tree` call to Preferences and every
    // `takeScreenshot` to the wrong app. Feedback §A: this cost
    // insight-side "an afternoon chasing 'empty tree' ghosts". Now
    // explicit-or-error.
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
    println!(
        "runner starting: udid={udid} port={port} pid={pid} (log: {}, timeout {timeout_secs}s)",
        log.display()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            let _ = std::fs::remove_file(state_path(root));
            return Err(format!(
                "xcodebuild exited early ({status}) — log tail:\n{}",
                tail_log(&log, 25)
            ));
        }
        if health_ok(port) {
            println!("runner up: http://localhost:{port}/health = 200");
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
pub fn down(root: &Path, port: u16) -> Result<(), String> {
    let mut acted = false;
    if let Some(st) = read_state(root) {
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

/// v1.0.4 — `smix runner cycle`.
///
/// Reads the current runner state, tears the runner down (SIGINT +
/// wait), and brings it back up on the SAME device + port + bundle. The
/// per-udid derived-data directory (`.smix/runner/derived-data-<udid>/`)
/// is preserved by both [`down`] and [`up`], so the second `xcodebuild
/// test-without-building` boots in ~3 s instead of the ~15 s cold path.
///
/// Motivation: feedback §E and D6 — when the XCTest test-host observes
/// `** TEST INTERRUPTED **`, the safest recovery is to cycle. This
/// verb exposes cycle to consumers explicitly, and is also invoked
/// internally by the runner supervisor (S7).
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
    up(
        root,
        &udid,
        cycle_port,
        bundle.as_deref(),
        false,
        runner_project,
    )
}

/// v1.0.5 §D2 — host-side XCTest supervisor.
///
/// Tails the runner log at `.smix/runner/runner-<UDID>.log` and looks
/// for interrupt patterns (`** TEST INTERRUPTED **`,
/// `SchemeActionResultOperation started unexpectedly`). On match:
/// invokes [`cycle`] to tear the runner down and bring it back up on
/// the same device/port/bundle. Session persistence (§D1) preserves
/// the consumer's `Session-Id` across the cycle.
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

    let mut carry = String::new();
    loop {
        // Sleep between polls; keeps CPU low.
        std::thread::sleep(std::time::Duration::from_millis(500));

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
            if let Some(prev) = last_cycle_at {
                if now.duration_since(prev) < cycle_cooldown {
                    eprintln!(
                        "supervise: interrupt hit within {:?} of last cycle — \
                         skipping (cooldown)",
                        cycle_cooldown
                    );
                    continue;
                }
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
            println!(
                r#"{{"event":"RunnerCycled","reasonMatched":{:?},"atMs":{}}}"#,
                line.trim(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            );
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
        assert_eq!(env_no_bundle.len(), 1);
        assert_eq!(env_no_bundle[0].0, "TEST_RUNNER_SMIX_RUNNER_PORT");
        assert_eq!(env_no_bundle[0].1, "22090");
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
