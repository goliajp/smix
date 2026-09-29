//! `smix capsule up/down` subcommands (pure logic layer).
//!
//! The capsule has two modes:
//!   Hard capsule — used when no simulator window is on screen. `simctl
//!     boot` brings the sim up headless with no visible window;
//!     screenshot / capture device-level APIs still work.
//!   Soft capsule — used when a simulator window IS on screen. Falls
//!     back to the EventRecorder + SDK ledger reconciliation path.
//!
//! Whether a boot would be on screen is decided by [`on_screen_reason`].
//! Simulator.app (Xcode <= 26) pops a window for every boot, so its running
//! is the condition. Device Hub, the simulator UI on Xcode 27, shows only
//! the simulator selected in it — but that one it shows again the moment
//! it boots, so the question there is whether it is showing this UDID
//! (see [`crate::device_hub`]).

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
    /// Was a simulator window on screen at `capsule up` time (see
    /// [`on_screen_reason`])? True implies mode = Soft, false implies
    /// mode = Hard. Retained on `down` for audit; not re-read.
    ///
    /// The alias is the field's name before 10.1: `down` reads the
    /// record `up` wrote, and that `up` may have been an older binary.
    #[serde(alias = "simulator_app_was_running")]
    pub window_was_on_screen: bool,
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

pub const GUARD_HINT: &str = "a hard capsule promises a boot that stays off screen, \
     and this one cannot be promised.\n\
     Quit Simulator.app (Xcode <= 26), or on Xcode 27 select another device in \
     Device Hub or close its window, and retry; or pass `--soft` to explicitly \
     accept the soft-capsule fallback (window visible, event-ledger reconciliation \
     only; no headless entry point).";

/// Where the capture endpoint comes from, said in full.
///
/// The message named the dependency and not how to satisfy it, and
/// nothing in `--help` said capture needed a separate process at all —
/// so the way out was discoverable only by reading `capsule up --help`
/// after already failing.
pub const CAPTURE_HINT: &str = "capture needs the smix-server process, which is separate \
     from the runner.\n\
     Start it with `cargo run -p smix-server` (or the installed \
     `smix-server` binary), or pass `--no-capture` to bring the capsule \
     up without it.";

/// Full path to `.smix/capsule/<UDID>.state.json`. Pure function — no I/O.
pub fn state_path(workspace_root: &Path, udid: &str) -> PathBuf {
    workspace_root
        .join(".smix")
        .join("capsule")
        .join(format!("{udid}.state.json"))
}

/// Guard probe: `pgrep -x Simulator` exit 0 => Simulator.app (Xcode <= 26) is running.
pub fn simulator_app_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "Simulator"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Why a boot of this simulator would be on screen, or `None` when it
/// would not.
///
/// "Cannot tell" is a reason too: a window that could not be read is not
/// an absent one, and a hard capsule is a promise about the screen.
pub fn on_screen_reason(
    simulator_app_running: bool,
    hub: &crate::device_hub::HubShows,
) -> Option<String> {
    use crate::device_hub::HubShows;
    if simulator_app_running {
        return Some(
            "Simulator.app is running (Xcode <= 26), and it opens a window for every boot"
                .to_string(),
        );
    }
    match hub {
        HubShows::This => Some(
            "Device Hub is showing this simulator, and it shows it again as soon as it boots"
                .to_string(),
        ),
        HubShows::CannotTell(why) => Some(format!(
            "{why}; whether this simulator would be on screen cannot be told"
        )),
        HubShows::NotRunning | HubShows::NoWindowOnScreen | HubShows::Other => None,
    }
}

/// Decide capsule mode, and say so when it is not the one asked for.
///
/// A window on screen used to be a refusal. But `expo run:ios` opens
/// the simulator UI (Simulator.app on Xcode <= 26) by design, so on any
/// React Native dev machine the hard capsule was never available — and a condition that is normal
/// for a whole class of users reads as an error only once before it
/// reads as noise.
///
/// So it degrades and says which mode it got. `require_hard` keeps the
/// refusal for CI, where a window means something is wrong rather than
/// that someone is working. `soft` is the caller stating the intent up
/// front, which suppresses the warning — there is nothing to warn
/// about when it is what was asked for.
///
/// Returning the warning rather than printing it keeps this a pure
/// function, which is what lets the table below test the decision
/// instead of the plumbing.
pub fn decide_mode(
    on_screen: Option<&str>,
    soft: bool,
    require_hard: bool,
) -> Result<(CapsuleMode, Option<String>), CapsuleGuardRejected> {
    match (on_screen, soft, require_hard) {
        (None, _, _) => Ok((CapsuleMode::Hard, None)),
        (Some(why), _, true) => Err(CapsuleGuardRejected {
            hint: format!("{why} — {GUARD_HINT}"),
        }),
        (Some(_), true, false) => Ok((CapsuleMode::Soft, None)),
        (Some(why), false, false) => Ok((
            CapsuleMode::Soft,
            Some(format!(
                "{why}, so this is a soft capsule: the window is visible and \
                 reconciliation is event-ledger only. Pass `--require-hard` to make \
                 this a failure instead."
            )),
        )),
    }
}

/// End-to-end options for `capsule up <DEVICE>`.
pub struct UpOptions<'a> {
    /// Refuse rather than degrade when a window is on screen.
    ///
    /// For CI, where a simulator window being open means something is
    /// wrong rather than that someone is working.
    pub require_hard: bool,
    pub root: &'a Path,
    pub udid: &'a str,
    pub runner_port: u16,
    pub capture_endpoint: &'a str,
    pub bundle: Option<&'a str>,
    pub soft: bool,
    /// Foreground the app instead of relaunching it. See
    /// [`smix_capsule::runner::UpOptions::attach_without_relaunch`].
    pub no_launch: bool,
    /// True to skip the `/api/capture/start` call (skips the /live HLS
    /// capture pipeline). Use when the scenario itself invokes
    /// `simctl io recordVideo` so the two do not fight for the EBUSY
    /// recording mutex.
    pub no_capture: bool,
}

/// Refuse a device the capsule cannot bring up, naming what can.
///
/// All three of the capsule's legs are simulator-shaped: the
/// simulator-window guard, `simctl boot`, and the HLS capture that
/// serves `/live`. Handed an emulator it used to run them anyway —
/// `simctl boot emulator-5554` sits there until it times out 120
/// seconds later, and the report ("boot timed out") describes the
/// symptom of asking the wrong tool rather than the mistake.
///
/// There is no silent degradation available here: a capsule that
/// skipped the parts it cannot do would be a `runner up` wearing
/// another command's name. So say which device this is, and which
/// command brings it up.
pub fn capsule_supports(
    kind: smix_simctl::registry::DeviceKind,
    device: &str,
) -> Result<(), String> {
    use smix_simctl::registry::DeviceKind;
    let (what, instead) = match kind {
        DeviceKind::Simulator => return Ok(()),
        DeviceKind::Emulator => (
            "an Android emulator",
            format!("smix runner up {device} --platform android"),
        ),
        DeviceKind::PhysicalAndroid => (
            "a physical Android device",
            format!("smix runner up {device} --platform android"),
        ),
        DeviceKind::PhysicalIos => (
            "a physical iPhone or iPad",
            format!("smix runner up {device} --physical"),
        ),
    };
    Err(format!(
        "capsule up is for iOS Simulators, and {device} is {what}.\n\
         The capsule boots through simctl, guards against a simulator \
         window, and records the /live capture — none of which this device \
         has. Bring it up with:\n\
         \n    {instead}\n"
    ))
}

/// End-to-end `capsule up`: guard + boot + capture start + stream URL +
/// runner up (record mode) + state.json write. Each step fails fast to
/// stderr and skips the rest; state.json is written only on full success
/// (atomic write+rename) so a partial failure leaves no residue.
///
/// This is an async fn because embedding a `Builder::new_current_thread()
/// .block_on(...)` inside the `#[tokio::main]` runtime is a nested-runtime
/// error under tokio 1.x. The surface is async and `main` awaits it
/// directly rather than spinning up a new runtime in the cement layer.
pub async fn up(opts: UpOptions<'_>) -> Result<(), String> {
    let simctl = smix_simctl::SimctlClient::new();
    let devices = simctl.list_devices().await.unwrap_or_default();
    let name = devices
        .iter()
        .find(|d| d.udid.eq_ignore_ascii_case(opts.udid))
        .map_or("", |d| d.name.as_str());
    let name_is_unique = !name.is_empty() && devices.iter().filter(|d| d.name == name).count() == 1;
    let hub = crate::device_hub::probe(opts.udid, name, name_is_unique);
    let reason = on_screen_reason(simulator_app_running(), &hub);
    let on_screen = reason.is_some();
    let (mode, warning) =
        decide_mode(reason.as_deref(), opts.soft, opts.require_hard).map_err(|e| e.hint)?;
    if let Some(w) = warning {
        eprintln!("capsule up: {w}");
    }

    // 2. Boot sim. `boot_and_wait` returns once the sim is fully ready,
    // and answers the same whether it was already running — which is why
    // who turned it on is read before it.
    let claim = crate::boot_record::claim_before_boot(&simctl, opts.udid).await;
    simctl
        .boot_and_wait(opts.udid, std::time::Duration::from_secs(120))
        .await
        .map_err(|e| format!("simctl boot {}: {e}", opts.udid))?;
    crate::boot_record::record_simulator_boot(opts.udid, claim);

    // 3. Capture start. Skipped when --no-capture (releases the
    // simctl io recordVideo lock for in-scenario recording segments).
    if !opts.no_capture {
        let capture_start_url = format!("{}/api/capture/start", opts.capture_endpoint);
        let body = format!("{{\"udid\":\"{}\"}}", opts.udid);
        post_json(&capture_start_url, &body).map_err(|e| {
            format!(
                "capsule up: capture start failed: {e}\n\
                 tried {} — {CAPTURE_HINT}",
                opts.capture_endpoint
            )
        })?;
    }

    // 4. Print where the live stream is (or the skip-capture banner).
    // smix-server serves the HLS playlist under /streams; `/live` is a page
    // of the dashboard, not a path this server has.
    if opts.no_capture {
        println!(
            "capsule up: mode={mode:?} device={} no-capture (simctl io recordVideo lock free for in-scenario use)",
            opts.udid
        );
    } else {
        println!(
            "capsule up: mode={mode:?} device={} stream={}/streams/{}/index.m3u8",
            opts.udid, opts.capture_endpoint, opts.udid
        );
    }

    // 5. runner up with record=true. Uses runner-project cascade
    // (install-shipped ~/.local/share/smix/runner-sources/ios/<sources>/ or
    // repo-side swift-bridge/, whichever exists first). capsule callers don't override.
    smix_capsule::runner::up(
        opts.root,
        opts.udid,
        opts.runner_port,
        opts.bundle,
        None,
        smix_capsule::runner::UpOptions {
            record_enabled: true,
            attach_without_relaunch: opts.no_launch,
            ..Default::default()
        },
    )?;

    // 6. Record the capsule.
    let state = CapsuleState {
        mode,
        udid: opts.udid.to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        runner_port: opts.runner_port,
        capture_endpoint: opts.capture_endpoint.to_string(),
        window_was_on_screen: on_screen,
        no_capture: opts.no_capture,
    };
    write_state(opts.root, opts.udid, &state)?;
    Ok(())
}

/// Open the store for capsule records.
fn open_store(root: &Path) -> Result<smix_store::Store, String> {
    smix_store::Store::open(&root.join(".smix"))
        .map_err(|e| format!("open capsule store under {}: {e}", root.display()))
}

/// Read one device's capsule record.
///
/// `Ok(None)` is "no capsule here"; a record that exists but cannot be
/// read is an error naming the device, so a damaged record is never
/// mistaken for an absent one and torn down as if nothing were running.
fn read_state(root: &Path, udid: &str) -> Result<Option<CapsuleState>, String> {
    let store = open_store(root)?;
    if let Some(state) = store
        .capsules()
        .get_json::<CapsuleState>(udid)
        .map_err(|e| format!("read capsule {udid}: {e}"))?
    {
        return Ok(Some(state));
    }
    // A capsule brought up before the store still has its file.
    let legacy = state_path(root, udid);
    let json = match std::fs::read_to_string(&legacy) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", legacy.display())),
    };
    serde_json::from_str(&json)
        .map(Some)
        .map_err(|e| format!("deserialize {}: {e}", legacy.display()))
}

/// Write one device's capsule record.
///
/// The file this replaces was the one atomic, fully error-checked
/// writer in the tree (tmp + rename, every step mapped). Nothing here
/// may be quieter than that: every failure is returned, and the record
/// is on disk before this returns, because the capsule it describes
/// outlives the process.
fn write_state(root: &Path, udid: &str, state: &CapsuleState) -> Result<(), String> {
    let store = open_store(root)?;
    store
        .capsules()
        .put_json(udid, state)
        .map_err(|e| format!("write capsule {udid}: {e}"))?;
    store
        .sync()
        .map_err(|e| format!("persist capsule {udid}: {e}"))
}

/// Forget one device's capsule record.
fn clear_state(root: &Path, udid: &str) -> Result<(), String> {
    let store = open_store(root)?;
    store
        .capsules()
        .delete(udid)
        .map_err(|e| format!("clear capsule {udid}: {e}"))?;
    store
        .sync()
        .map_err(|e| format!("persist capsule {udid}: {e}"))
}

/// Reverse `capsule up`: read state.json → runner down → capture stop
/// → simctl shutdown → delete state.json. A single step failure is
/// logged to stderr and the rest of the teardown continues (best
/// effort); state.json is not removed unless the earlier steps completed.
///
/// Async fn for the same reason as [`up`].
pub async fn down(root: &Path, udid: &str) -> Result<(), String> {
    // Idempotent: nothing recorded => noop. A record that exists but
    // cannot be read is an error rather than a noop — tearing down as
    // if nothing were running would leave the runner and the capture
    // behind.
    let Some(state) = read_state(root, udid)? else {
        eprintln!("capsule down: {udid} not active");
        return Ok(());
    };

    let mut errors: Vec<String> = Vec::new();
    if let Err(e) = smix_capsule::runner::down(state.runner_port) {
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

    if let Err(e) = clear_state(root, udid) {
        eprintln!("capsule down: {e}");
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
/// [`smix_capsule::runner::health_ok`] — avoids pulling in `reqwest` / `ureq`).
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
        assert_eq!(
            decide_mode(None, false, false).unwrap().0,
            CapsuleMode::Hard
        );
        assert_eq!(decide_mode(None, true, false).unwrap().0, CapsuleMode::Hard);
    }

    #[test]
    fn mode_requires_soft_flag_when_simulator_present() {
        let err =
            decide_mode(Some("Device Hub is showing this simulator"), false, true).unwrap_err();
        // The reason leads, and both generations are named in the way out:
        // Simulator.app on Xcode <= 26, Device Hub on Xcode 27.
        assert!(
            err.hint.starts_with("Device Hub is showing this simulator"),
            "{:?}",
            err.hint
        );
        assert!(
            err.hint.contains("Simulator.app (Xcode <= 26)") && err.hint.contains("Device Hub"),
            "guard hint should name both generations of the simulator UI, got {:?}",
            err.hint
        );
        assert!(
            err.hint.contains("--soft"),
            "guard hint should suggest --soft, got {:?}",
            err.hint
        );
        // `pkill -INT Simulator` kills nothing on Xcode 27; a command that
        // works on one generation is not advice.
        assert!(
            !err.hint.contains("pkill"),
            "guard hint must not prescribe a kill command, got {:?}",
            err.hint
        );
    }

    // Device Hub shows only the simulator selected in it, and shows that
    // one again when it boots (measured 2026-09-29): running, or having a
    // window, is not a reason; showing this UDID is, and so is not being
    // able to read which one it shows.
    #[test]
    fn device_hub_counts_only_when_it_shows_this_simulator() {
        use crate::device_hub::HubShows;
        assert_eq!(on_screen_reason(false, &HubShows::NotRunning), None);
        assert_eq!(on_screen_reason(false, &HubShows::NoWindowOnScreen), None);
        assert_eq!(on_screen_reason(false, &HubShows::Other), None);
        assert!(on_screen_reason(false, &HubShows::This).is_some());
        let unread = on_screen_reason(false, &HubShows::CannotTell("no permission".into()));
        assert!(
            unread
                .as_deref()
                .is_some_and(|r| r.starts_with("no permission")),
            "{unread:?}"
        );
    }

    #[test]
    fn simulator_app_is_a_reason_whatever_device_hub_says() {
        use crate::device_hub::HubShows;
        assert!(on_screen_reason(true, &HubShows::NotRunning).is_some());
        assert!(on_screen_reason(true, &HubShows::Other).is_some());
    }

    #[test]
    fn a_reason_without_require_hard_degrades_and_says_why() {
        let (mode, warning) = decide_mode(Some("cannot tell"), false, false).unwrap();
        assert_eq!(mode, CapsuleMode::Soft);
        assert!(warning.is_some_and(|w| w.starts_with("cannot tell")));
    }

    #[test]
    fn mode_soft_when_explicit() {
        assert_eq!(
            decide_mode(Some("x"), true, false).unwrap().0,
            CapsuleMode::Soft
        );
    }

    #[test]
    fn state_round_trips_json() {
        let s = CapsuleState {
            mode: CapsuleMode::Hard,
            udid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".to_string(),
            started_at: "2026-06-14T12:34:56+09:00".to_string(),
            runner_port: 22087,
            capture_endpoint: "http://127.0.0.1:8787".to_string(),
            window_was_on_screen: false,
            no_capture: false,
        };
        let j = serde_json::to_string(&s).unwrap();
        let r: CapsuleState = serde_json::from_str(&j).unwrap();
        assert_eq!(r.mode, CapsuleMode::Hard);
        assert_eq!(r.udid, s.udid);
        assert_eq!(r.started_at, s.started_at);
        assert_eq!(r.runner_port, 22087);
        assert_eq!(r.capture_endpoint, s.capture_endpoint);
        assert!(!r.window_was_on_screen);
        assert!(!r.no_capture);
    }

    // `capsule down` reads the file `capsule up` wrote, and the `up` may
    // have been an older binary: the field's old name must still load.
    #[test]
    fn state_written_under_the_old_field_name_still_loads() {
        let legacy = r#"{
            "mode": "soft",
            "udid": "X",
            "started_at": "2026-06-14T12:34:56+09:00",
            "runner_port": 22087,
            "capture_endpoint": "http://127.0.0.1:8787",
            "simulator_app_was_running": true
        }"#;
        let r: CapsuleState = serde_json::from_str(legacy).unwrap();
        assert!(r.window_was_on_screen);
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
            window_was_on_screen: false,
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

#[cfg(test)]
mod store_tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("smix-capsule-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn sample(udid: &str, port: u16) -> CapsuleState {
        CapsuleState {
            mode: CapsuleMode::Hard,
            udid: udid.to_string(),
            started_at: "2026-07-20T00:00:00Z".to_string(),
            runner_port: port,
            capture_endpoint: "http://127.0.0.1:9000".to_string(),
            window_was_on_screen: false,
            no_capture: false,
        }
    }

    #[test]
    fn state_round_trips_through_the_store() {
        let root = temp_root("roundtrip");
        write_state(&root, "UDID-1", &sample("UDID-1", 22087)).expect("write");
        let back = read_state(&root, "UDID-1").expect("read").expect("present");
        assert_eq!(back.udid, "UDID-1");
        assert_eq!(back.runner_port, 22087);
    }

    #[test]
    fn the_legacy_state_file_is_not_written() {
        let root = temp_root("no-file");
        write_state(&root, "UDID-1", &sample("UDID-1", 22087)).expect("write");
        assert!(
            !state_path(&root, "UDID-1").exists(),
            "the legacy per-udid state.json is still being written"
        );
    }

    #[test]
    fn two_devices_keep_their_own_capsule() {
        // Capsule state has always been per-udid. That must survive the
        // move: two sims under one workspace are the normal case.
        let root = temp_root("two-devices");
        write_state(&root, "A", &sample("A", 22087)).expect("write a");
        write_state(&root, "B", &sample("B", 22088)).expect("write b");
        assert_eq!(
            read_state(&root, "A")
                .expect("read")
                .expect("present")
                .runner_port,
            22087
        );
        assert_eq!(
            read_state(&root, "B")
                .expect("read")
                .expect("present")
                .runner_port,
            22088
        );
    }

    #[test]
    fn clearing_one_device_leaves_the_other() {
        let root = temp_root("clear");
        write_state(&root, "A", &sample("A", 22087)).expect("write a");
        write_state(&root, "B", &sample("B", 22088)).expect("write b");
        clear_state(&root, "A").expect("clear");
        assert!(read_state(&root, "A").expect("read").is_none());
        assert!(read_state(&root, "B").expect("read").is_some());
    }

    #[test]
    fn a_pre_store_capsule_is_still_found() {
        let root = temp_root("legacy");
        let path = state_path(&root, "LEGACY");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(
            &path,
            serde_json::to_string(&sample("LEGACY", 22087)).expect("json"),
        )
        .expect("write legacy");

        let back = read_state(&root, "LEGACY").expect("read").expect("present");
        assert_eq!(back.udid, "LEGACY");
        assert!(path.exists(), "the legacy file must be left where it is");
    }

    #[test]
    fn a_corrupt_record_is_named_not_treated_as_absent() {
        let root = temp_root("corrupt");
        {
            let store = smix_store::Store::open(&root.join(".smix")).expect("open");
            store.capsules().put("BAD", b"{not a capsule").expect("put");
        }
        let err = read_state(&root, "BAD").expect_err("corrupt must not read as absent");
        assert!(err.contains("BAD"), "the error must name the device: {err}");
    }
}
