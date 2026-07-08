//! `smix capsule up/down` subcommands (pure logic layer).
//!
//! The capsule has two modes:
//!   Hard capsule — used when Simulator.app is NOT running. `simctl boot`
//!     brings the sim up headless with no visible window; screenshot /
//!     capture device-level APIs still work.
//!   Soft capsule — used when Simulator.app IS running. Falls back to
//!     the EventRecorder + SDK ledger reconciliation path.
//!
//! `capsule up <DEVICE>` runs a guard: `pgrep -x Simulator` — a zero exit
//! code means Simulator.app is on screen, which is refused by default
//! (returns [`CapsuleGuardRejected`]). The user must pass `--soft` to
//! accept the guarded fallback to soft mode.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapsuleMode {
    Hard,
    Soft,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapsuleState {
    pub mode: CapsuleMode,
    pub udid: String,
    /// RFC3339 timestamp (`chrono::Utc::now().to_rfc3339()`).
    pub started_at: String,
    pub runner_port: u16,
    /// e.g. `http://127.0.0.1:8787` — smix-server entry point. `capsule
    /// down` posts to `{capture_endpoint}/api/capture/stop` to tear it
    /// back down.
    pub capture_endpoint: String,
    /// Was Simulator.app running at `capsule up` time? True implies
    /// mode = Soft, false implies mode = Hard. Retained on `down` for
    /// audit; not re-read.
    pub simulator_app_was_running: bool,
    /// True iff `capsule up --no-capture` was used, meaning we skipped
    /// the smix-server `/api/capture/start` call (and the long-running
    /// `simctl io recordVideo` capture pipeline on the host). This lets
    /// [`down`] know NOT to POST `/api/capture/stop`. Callers that also
    /// invoke `simctl io recordVideo` from inside their scenario use
    /// this to avoid the EBUSY mutex against the host capture pipeline.
    #[serde(default)]
    pub no_capture: bool,
}

#[derive(Debug)]
pub struct CapsuleGuardRejected {
    pub hint: String,
}

pub const GUARD_HINT: &str = "Simulator.app is running — simctl boot will pop a window, \
     which violates the hard-capsule precondition.\n\
     Close it (`pkill -INT Simulator`) and retry, or pass `--soft` to \
     explicitly accept the soft-capsule fallback (window visible, \
     event-ledger reconciliation only; no headless entry point).";

/// Full path to `.smix/capsule/<UDID>.state.json`. Pure function — no I/O.
pub fn state_path(workspace_root: &Path, udid: &str) -> PathBuf {
    workspace_root
        .join(".smix")
        .join("capsule")
        .join(format!("{udid}.state.json"))
}

/// Guard probe: `pgrep -x Simulator` exit 0 => Simulator.app is running.
pub fn simulator_app_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "Simulator"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Decide capsule mode. `soft=true` is the user's explicit escape hatch
/// via `--soft`. `sim_running=true && soft=false` refuses the request
/// and returns a [`CapsuleGuardRejected`] with the hint text.
pub fn decide_mode(sim_running: bool, soft: bool) -> Result<CapsuleMode, CapsuleGuardRejected> {
    match (sim_running, soft) {
        (false, _) => Ok(CapsuleMode::Hard),
        (true, true) => Ok(CapsuleMode::Soft),
        (true, false) => Err(CapsuleGuardRejected {
            hint: GUARD_HINT.to_string(),
        }),
    }
}

/// End-to-end options for `capsule up <DEVICE>`.
pub struct UpOptions<'a> {
    pub root: &'a Path,
    pub udid: &'a str,
    pub runner_port: u16,
    pub capture_endpoint: &'a str,
    pub bundle: Option<&'a str>,
    pub soft: bool,
    /// True to skip the `/api/capture/start` call (skips the /live HLS
    /// capture pipeline). Use when the scenario itself invokes
    /// `simctl io recordVideo` so the two do not fight for the EBUSY
    /// recording mutex.
    pub no_capture: bool,
}

/// End-to-end `capsule up`: guard + boot + capture start + /live URL +
/// runner up (record mode) + state.json write. Each step fails fast to
/// stderr and skips the rest; state.json is written only on full success
/// (atomic write+rename) so a partial failure leaves no residue.
///
/// This is an async fn because embedding a `Builder::new_current_thread()
/// .block_on(...)` inside the `#[tokio::main]` runtime is a nested-runtime
/// error under tokio 1.x. The surface is async and `main` awaits it
/// directly rather than spinning up a new runtime in the cement layer.
pub async fn up(opts: UpOptions<'_>) -> Result<(), String> {
    let sim_running = simulator_app_running();
    let mode = decide_mode(sim_running, opts.soft).map_err(|e| e.hint)?;

    // 2. Boot sim (skipped if already Booted). `boot_and_wait` fuses
    // boot + bootstatus and returns when the sim is fully ready;
    // a NonZeroExit with stderr "current state: Booted" means it was
    // already booted and is treated as success.
    let simctl = smix_simctl::SimctlClient::new();
    match simctl
        .boot_and_wait(opts.udid, std::time::Duration::from_secs(120))
        .await
    {
        Ok(_) => {}
        Err(smix_simctl::SimctlError::NonZeroExit { stderr, .. })
            if stderr.contains("current state: Booted") => {}
        Err(e) => return Err(format!("simctl boot {}: {e}", opts.udid)),
    }

    // 3. Capture start. Skipped when --no-capture (releases the
    // simctl io recordVideo lock for in-scenario recording segments).
    if !opts.no_capture {
        let capture_start_url = format!("{}/api/capture/start", opts.capture_endpoint);
        let body = format!("{{\"udid\":\"{}\"}}", opts.udid);
        post_json(&capture_start_url, &body).map_err(|e| {
            format!(
                "capsule up: capture start failed: {e} (is smix-server running at {}?)",
                opts.capture_endpoint
            )
        })?;
    }

    // 4. print /live URL (or skip-capture banner).
    if opts.no_capture {
        println!(
            "capsule up: mode={mode:?} device={} no-capture (simctl io recordVideo lock free for in-scenario use)",
            opts.udid
        );
    } else {
        println!(
            "capsule up: mode={mode:?} device={} /live={}/live/{}",
            opts.udid, opts.capture_endpoint, opts.udid
        );
    }

    // 5. runner up with record=true. Uses runner-project cascade
    // (repo-side swift-bridge/ or install-shipped ~/.local/share/smix/runner/,
    // whichever exists first). capsule callers don't override.
    crate::runner::up(
        opts.root,
        opts.udid,
        opts.runner_port,
        opts.bundle,
        true,
        None,
    )?;

    // 6. Write state.json (atomic write+rename).
    let path = state_path(opts.root, opts.udid);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let state = CapsuleState {
        mode,
        udid: opts.udid.to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        runner_port: opts.runner_port,
        capture_endpoint: opts.capture_endpoint.to_string(),
        simulator_app_was_running: sim_running,
        no_capture: opts.no_capture,
    };
    let json = serde_json::to_string_pretty(&state).map_err(|e| format!("serialize state: {e}"))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    Ok(())
}

/// Reverse `capsule up`: read state.json → runner down → capture stop
/// → simctl shutdown → delete state.json. A single step failure is
/// logged to stderr and the rest of the teardown continues (best
/// effort); state.json is not removed unless the earlier steps completed.
///
/// Async fn for the same reason as [`up`].
pub async fn down(root: &Path, udid: &str) -> Result<(), String> {
    let path = state_path(root, udid);
    if !path.exists() {
        // Idempotent: no state.json => noop.
        eprintln!("capsule down: {} not active (no state.json)", udid);
        return Ok(());
    }
    let json =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let state: CapsuleState =
        serde_json::from_str(&json).map_err(|e| format!("deserialize {}: {e}", path.display()))?;

    let mut errors: Vec<String> = Vec::new();
    if let Err(e) = crate::runner::down(root, state.runner_port) {
        eprintln!("capsule down: runner down failed: {e}");
        errors.push(format!("runner: {e}"));
    }

    if !state.no_capture {
        let capture_stop_url = format!("{}/api/capture/stop", state.capture_endpoint);
        let body = format!("{{\"udid\":\"{}\"}}", state.udid);
        if let Err(e) = post_json(&capture_stop_url, &body) {
            // 404 / 410 treated as normal (capture is already gone).
            if !e.contains("status=404") && !e.contains("status=410") {
                eprintln!("capsule down: capture stop failed: {e}");
                errors.push(format!("capture: {e}"));
            }
        }
    }

    let simctl = smix_simctl::SimctlClient::new();
    if let Err(e) = simctl.shutdown(&state.udid).await {
        eprintln!("capsule down: simctl shutdown failed: {e}");
        errors.push(format!("shutdown: {e}"));
    }

    if let Err(e) = std::fs::remove_file(&path) {
        eprintln!("capsule down: remove {}: {e}", path.display());
        errors.push(format!("rm state: {e}"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "capsule down completed with errors: {}",
            errors.join("; ")
        ))
    }
}

/// Bare HTTP POST helper (same hand-written TCP style as
/// [`crate::runner::health_ok`] — avoids pulling in `reqwest` / `ureq`).
/// The returned `Err` string embeds `status=<code>` so callers can
/// branch on the HTTP status.
fn post_json(url: &str, body: &str) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let (host, port, path) = parse_url(url)?;
    let mut stream = TcpStream::connect_timeout(
        &(host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| format!("resolve {host}:{port}: {e}"))?
            .next()
            .ok_or_else(|| format!("no socket addr for {host}:{port}"))?,
        Duration::from_secs(5),
    )
    .map_err(|e| format!("connect {host}:{port}: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| format!("set read_timeout: {e}"))?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    let mut resp = String::new();
    stream
        .read_to_string(&mut resp)
        .map_err(|e| format!("read: {e}"))?;
    let status_line = resp.lines().next().unwrap_or("");
    // HTTP/1.1 200 OK
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("malformed status line: {status_line:?}"))?;
    if (200..300).contains(&status_code) {
        Ok(())
    } else {
        Err(format!(
            "status={status_code} body={}",
            resp.lines().last().unwrap_or("")
        ))
    }
}

fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("only http:// supported, got {url}"))?;
    let (host_port, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        (
            h.to_string(),
            p.parse().map_err(|e| format!("port parse: {e}"))?,
        )
    } else {
        (host_port.to_string(), 80)
    };
    Ok((host, port, format!("/{path}")))
}

use std::net::ToSocketAddrs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_path_under_workspace() {
        let p = state_path(Path::new("/tmp/ws"), "ABCD-1234");
        assert_eq!(
            p,
            PathBuf::from("/tmp/ws/.smix/capsule/ABCD-1234.state.json")
        );
    }

    #[test]
    fn mode_default_hard_when_simulator_absent() {
        assert_eq!(decide_mode(false, false).unwrap(), CapsuleMode::Hard);
        assert_eq!(decide_mode(false, true).unwrap(), CapsuleMode::Hard);
    }

    #[test]
    fn mode_requires_soft_flag_when_simulator_present() {
        let err = decide_mode(true, false).unwrap_err();
        assert!(
            err.hint.contains("Simulator.app is running"),
            "guard hint should name the precondition, got {:?}",
            err.hint
        );
        assert!(
            err.hint.contains("--soft"),
            "guard hint should suggest --soft, got {:?}",
            err.hint
        );
    }

    #[test]
    fn mode_soft_when_explicit() {
        assert_eq!(decide_mode(true, true).unwrap(), CapsuleMode::Soft);
    }

    #[test]
    fn state_round_trips_json() {
        let s = CapsuleState {
            mode: CapsuleMode::Hard,
            udid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".to_string(),
            started_at: "2026-06-14T12:34:56+09:00".to_string(),
            runner_port: 22087,
            capture_endpoint: "http://127.0.0.1:8787".to_string(),
            simulator_app_was_running: false,
            no_capture: false,
        };
        let j = serde_json::to_string(&s).unwrap();
        let r: CapsuleState = serde_json::from_str(&j).unwrap();
        assert_eq!(r.mode, CapsuleMode::Hard);
        assert_eq!(r.udid, s.udid);
        assert_eq!(r.started_at, s.started_at);
        assert_eq!(r.runner_port, 22087);
        assert_eq!(r.capture_endpoint, s.capture_endpoint);
        assert!(!r.simulator_app_was_running);
        assert!(!r.no_capture);
    }

    // State round-trip honors --no-capture flag for scenario recording
    // (avoids simctl io recordVideo EBUSY 16 mutex with the live HLS
    // capture pipeline).
    #[test]
    fn state_round_trips_with_no_capture_flag() {
        let s = CapsuleState {
            mode: CapsuleMode::Hard,
            udid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".to_string(),
            started_at: "2026-06-16T12:34:56+09:00".to_string(),
            runner_port: 22087,
            capture_endpoint: "http://127.0.0.1:8787".to_string(),
            simulator_app_was_running: false,
            no_capture: true,
        };
        let j = serde_json::to_string(&s).unwrap();
        let r: CapsuleState = serde_json::from_str(&j).unwrap();
        assert!(r.no_capture);
    }

    // state.json written by older binaries (without the `no_capture`
    // field) deserializes with no_capture=false default. Forward compat.
    #[test]
    fn state_back_compat_missing_no_capture_field_defaults_false() {
        let legacy = r#"{
            "mode": "hard",
            "udid": "X",
            "started_at": "2026-06-14T12:34:56+09:00",
            "runner_port": 22087,
            "capture_endpoint": "http://127.0.0.1:8787",
            "simulator_app_was_running": false
        }"#;
        let r: CapsuleState = serde_json::from_str(legacy).unwrap();
        assert!(!r.no_capture);
    }
}
