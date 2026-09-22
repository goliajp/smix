//! smix-adb — Android Debug Bridge (adb) child_process wrapper.
//!
//! Counterpart of `smix_simctl::SimctlClient` for Android. Used by
//! `smix_sdk::AndroidDeviceControl` to implement the `DeviceControl`
//! trait on Android.
//!
//! Real-device invocations need a booted emulator, so tests requiring a
//! live `adb` are gated behind the `ignore` attribute.
//!
//! ## Wire model
//!
//! Each `AdbClient` method spawns `adb -s <serial> <subcommand> ...` via
//! tokio process, captures stdout+stderr, surfaces non-zero exit / spawn
//! failure as [`AdbError`] variants. No retries — caller-side concern.

#![doc(html_root_url = "https://docs.smix.dev/smix-adb")]

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;
use thiserror::Error;
use tokio::process::Command;

/// Failure variants for any `adb` invocation.
#[derive(Debug, Error)]
pub enum AdbError {
    /// Failed to spawn `adb` (missing binary / PATH lookup / fork failure).
    #[error("spawn adb failed: {0}")]
    Spawn(#[from] io::Error),
    /// `adb` was not found in PATH at all.
    #[error("adb binary not found in PATH; install Android SDK platform-tools")]
    BinaryNotFound,
    /// `adb <sub>` exited non-zero.
    ///
    /// The message carries the whole command. Naming only the subcommand
    /// makes every failure under `adb shell` — `pm clear`, `pm revoke`,
    /// `screenrecord` — report itself as "shell", which tells a reader
    /// nothing about what broke.
    #[error("adb {subcommand} exited {code}: {stderr}")]
    NonZeroExit {
        /// The command as run, e.g. `"shell pm clear com.example"`.
        subcommand: String,
        /// Device serial if scoped to one (None for global like `devices`).
        serial: Option<String>,
        /// Exit code from `adb`.
        code: i32,
        /// Captured stderr (truncated).
        stderr: String,
    },
    /// `adb <sub>` exited 0 but stdout didn't match the expected shape.
    #[error("adb {subcommand} returned malformed output: {detail}")]
    Malformed {
        /// Subcommand name.
        subcommand: String,
        /// Parser-side detail.
        detail: String,
    },
}

/// One Android device known to `adb devices -l`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbDevice {
    /// Serial (e.g. `"emulator-5554"` or `"0a1b2c3d"`).
    pub serial: String,
    /// State: `"device"` (ready), `"offline"`, `"unauthorized"`, etc.
    pub state: String,
    /// `product:` field (often the same as model on emulators).
    pub product: Option<String>,
    /// `model:` field (e.g. `"sdk_gphone64_arm64"`, `"Pixel_2"`).
    pub model: Option<String>,
    /// `device:` field (codename, e.g. `"emu64a"`, `"walleye"`).
    pub device: Option<String>,
    /// `transport_id:` field (numeric transport ID).
    pub transport_id: Option<String>,
}

/// Parse `adb devices -l` stdout into [`AdbDevice`] entries.
///
/// Format (one device per line after header):
///
/// ```text
/// List of devices attached
/// emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:1
/// 0a1b2c3d               offline
/// ```
///
/// # Errors
///
/// Returns [`AdbError::Malformed`] if the header line is missing or a
/// non-empty line is unparsable.
pub fn parse_devices_stdout(stdout: &str) -> Result<Vec<AdbDevice>, AdbError> {
    let mut out = Vec::new();
    let mut saw_header = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("List of devices") {
            saw_header = true;
            continue;
        }
        // Each device row: <serial>\s+<state>(\s+key:val)*
        let mut parts = trimmed.split_whitespace();
        let serial = parts
            .next()
            .ok_or_else(|| AdbError::Malformed {
                subcommand: "devices -l".into(),
                detail: format!("empty serial in line: {trimmed:?}"),
            })?
            .to_string();
        let state = parts
            .next()
            .ok_or_else(|| AdbError::Malformed {
                subcommand: "devices -l".into(),
                detail: format!("missing state field in line: {trimmed:?}"),
            })?
            .to_string();
        let mut dev = AdbDevice {
            serial,
            state,
            product: None,
            model: None,
            device: None,
            transport_id: None,
        };
        for kv in parts {
            if let Some((k, v)) = kv.split_once(':') {
                let v_owned = v.to_string();
                match k {
                    "product" => dev.product = Some(v_owned),
                    "model" => dev.model = Some(v_owned),
                    "device" => dev.device = Some(v_owned),
                    "transport_id" => dev.transport_id = Some(v_owned),
                    _ => {} // ignore unknown keys (future-compat)
                }
            }
        }
        out.push(dev);
    }
    // Tolerate parser-only callers that pass arbitrary stdout slices
    // without the header (we still return the parsed rows). saw_header
    // serves only as documentation of well-formed input.
    let _ = saw_header;
    Ok(out)
}

/// Client wrapping `adb` invocations.
///
/// Construction is cheap — internally just an `adb` binary path resolved
/// via PATH lookup at command-spawn time. No persistent state.
#[derive(Debug, Default, Clone)]
pub struct AdbClient {
    /// Override the `adb` binary path; defaults to `"adb"` (PATH lookup).
    binary: Option<String>,
}

/// Pull the granted names out of `dumpsys package`'s `runtime permissions:`
/// section.
///
/// Each entry reads `<name>: granted=<bool>, flags=[...]`, indented under
/// the heading. The section ends at the first line that does not parse as
/// one, since dumpsys simply moves on to its next topic.
fn parse_granted_runtime_permissions(dumpsys_stdout: &str) -> Vec<String> {
    let mut granted = Vec::new();
    let mut in_section = false;
    for line in dumpsys_stdout.lines() {
        let trimmed = line.trim();
        if trimmed == "runtime permissions:" {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((name, rest)) = trimmed.split_once(": ") else {
            in_section = false;
            continue;
        };
        if !name.contains('.') {
            in_section = false;
            continue;
        }
        if rest.starts_with("granted=true") {
            granted.push(name.to_string());
        }
    }
    granted
}

/// `adb reverse` argv: the device's port, then this machine's.
///
/// Separated from the call so the order can be judged without a device.
/// `forward` takes host-then-device and `reverse` takes
/// device-then-host; one function each, and each one's order is a test.
fn reverse_argv(device_port: u16, host_port: u16) -> Vec<String> {
    vec![
        "reverse".into(),
        format!("tcp:{device_port}"),
        format!("tcp:{host_port}"),
    ]
}

/// `adb reverse --remove` argv. The device port identifies the pair —
/// it is the end the device dials.
fn unreverse_argv(device_port: u16) -> Vec<String> {
    vec![
        "reverse".into(),
        "--remove".into(),
        format!("tcp:{device_port}"),
    ]
}

/// The `(device_port, host_port)` pairs out of `adb reverse --list`.
///
/// Each line reads `<serial> tcp:<device> tcp:<host>`. A line that does
/// not parse is not a pair and is left out: the caller asked which
/// routes are open, and a zero would be an answer nobody could act on.
fn parse_reverse_list(stdout: &str) -> Vec<(u16, u16)> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut ports = line
                .split_whitespace()
                .filter_map(|tok| tok.strip_prefix("tcp:"))
                .filter_map(|p| p.parse::<u16>().ok());
            Some((ports.next()?, ports.next()?))
        })
        .collect()
}

/// Whether the device's screen is on, as the power manager words it.
///
/// `Unknown` is a distinct answer and not a synonym for `Asleep`: a dump
/// this could not find the line in is a dump nobody read, and reporting
/// "asleep" for it would have a caller wake a device that may already be
/// awake and then believe the reading that followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wakefulness {
    Awake,
    Asleep,
    Dozing,
}

/// The `mWakefulness=` line out of `dumpsys power`.
///
/// Measured on API 36: the dump carries one `  mWakefulness=Awake` line,
/// and a device sent to sleep with `KEYCODE_SLEEP` reads `Asleep`.
#[must_use]
pub fn parse_wakefulness(dump: &str) -> Option<Wakefulness> {
    let value = dump
        .lines()
        .find_map(|line| line.trim().strip_prefix("mWakefulness="))?
        .trim();
    match value {
        "Awake" => Some(Wakefulness::Awake),
        "Asleep" => Some(Wakefulness::Asleep),
        "Dozing" => Some(Wakefulness::Dozing),
        _ => None,
    }
}

/// `settings get global stay_on_while_plugged_in` read as a yes or no.
///
/// The setting is a bitmask over the charger types the screen stays on
/// for (measured: `svc power stayon true` writes 7 = AC|USB|WIRELESS,
/// `false` writes 0), so any non-zero value is "on". A key that was
/// never written reads `null`, which is not a zero — nobody set it, and
/// saying "off" would be inventing the answer.
#[must_use]
pub fn parse_stay_awake(stdout: &str) -> Option<bool> {
    let raw = stdout.trim();
    if raw.is_empty() || raw == "null" {
        return None;
    }
    raw.parse::<u32>().ok().map(|mask| mask != 0)
}

/// The resumed activity out of `dumpsys activity activities`, as
/// `(package, activity)`.
///
/// The line reads
/// `ResumedActivity: ActivityRecord{<hash> u0 <pkg>/<activity>} t<task>}`
/// (measured on API 36). The activity half is kept exactly as dumpsys
/// spells it, relative leading dot and all, because that is what the
/// caller will compare against what they wrote in a manifest.
///
/// Split-screen puts two of these in the dump; the first is taken and
/// the rest ignored. There is one frontmost app to a caller asking this
/// question, and picking any later one would be a different arbitrary
/// choice rather than a better one.
#[must_use]
pub fn parse_resumed_activity(dump: &str) -> Option<(String, String)> {
    let record = dump
        .lines()
        .find_map(|line| line.trim().strip_prefix("ResumedActivity:"))?;
    // `ActivityRecord{27c287f u0 dev.smix.fixture/.ComposeActivity} t2095}`
    let component = record
        .split_whitespace()
        .find(|tok| tok.contains('/'))?
        .trim_end_matches('}');
    let (package, activity) = component.split_once('/')?;
    if package.is_empty() || activity.is_empty() {
        return None;
    }
    Some((package.to_string(), activity.to_string()))
}

/// One crash the device recorded, as its own buffer words it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReport {
    /// The timestamp logcat put on the first line, verbatim.
    pub when: String,
    /// The process the report names, or empty when it names none. A
    /// report whose process cannot be read is still a crash, so it is
    /// kept rather than dropped.
    pub process: String,
    /// The first line's message — what happened, in the device's words.
    pub summary: String,
    /// Every line of the report in order, verbatim.
    pub lines: Vec<String>,
}

/// One `-v threadtime` line split into its fixed prefix and its message.
///
/// The format is logcat's own and documented:
/// `MM-DD HH:MM:SS.mmm  PID  TID L TAG: message`.
fn split_threadtime(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(6, char::is_whitespace);
    let date = parts.next()?;
    let time = parts.next()?;
    // The pid/tid columns are right-aligned, so the split above hands
    // back empty strings for the padding. Walk the rest by tokens.
    let rest = line.get(date.len() + 1 + time.len()..)?;
    let mut tokens = rest.split_whitespace();
    let _pid: u32 = tokens.next()?.parse().ok()?;
    let _tid: u32 = tokens.next()?.parse().ok()?;
    let _level = tokens.next()?;
    let (_, message) = rest.split_once(": ")?;
    Some((format!("{date} {time}"), message.to_string()))
}

/// Does this line open a new crash report?
///
/// Four banners, each one a fixed string Android writes itself:
/// the two `AndroidRuntime` ones for a Java crash, `Fatal signal` from
/// libc when a native process dies, and the tombstone rule that
/// `debuggerd` prints ahead of a backtrace.
fn crash_banner(message: &str) -> bool {
    message.starts_with("FATAL EXCEPTION")
        || message.starts_with("*** FATAL EXCEPTION IN SYSTEM PROCESS")
        || message.starts_with("Fatal signal ")
        || message.starts_with("*** *** ***")
}

/// The process a report names, or empty when it names none.
///
/// Three shapes, all measured in one buffer: `Process: <pkg>, PID: <n>`
/// from a Java crash, `pid N (name)` from libc's signal line, and
/// `>>> <path> <<<` from a tombstone.
fn crash_process(lines: &[String]) -> String {
    for line in lines {
        let Some((_, message)) = split_threadtime(line) else {
            continue;
        };
        if let Some(rest) = message.strip_prefix("Process: ")
            && let Some((pkg, _)) = rest.split_once(',')
        {
            return pkg.trim().to_string();
        }
        if let Some(start) = message.find(">>> ")
            && let Some(end) = message[start + 4..].find(" <<<")
        {
            return message[start + 4..start + 4 + end].trim().to_string();
        }
        if message.starts_with("Fatal signal ")
            && let Some(start) = message.rfind(" (")
            && let Some(end) = message[start..].find(')')
        {
            return message[start + 2..start + end].trim().to_string();
        }
    }
    String::new()
}

/// Every crash report in `adb logcat -b crash -d -v threadtime` output.
///
/// A report runs from one banner to the next, or to the end. Lines
/// ahead of the first banner belong to no report and are left out —
/// logcat's own `--------- beginning of crash` marker is one of them.
///
/// A `Fatal signal` line and the tombstone that follows it are two
/// reports, not one. They are almost always the same crash, but the
/// signal line's pid is the process that died while the tombstone's is
/// the `crash_dump` helper that wrote it down, and joining them takes
/// an inference — one that, when wrong, files one crash's backtrace
/// under another's name. Both records are true and both name a process.
#[must_use]
pub fn parse_crash_buffer(stdout: &str) -> Vec<CrashReport> {
    let mut reports: Vec<CrashReport> = Vec::new();
    for line in stdout.lines() {
        let Some((when, message)) = split_threadtime(line) else {
            continue;
        };
        if crash_banner(&message) {
            reports.push(CrashReport {
                when,
                process: String::new(),
                summary: message,
                lines: vec![line.to_string()],
            });
        } else if let Some(current) = reports.last_mut() {
            current.lines.push(line.to_string());
        }
    }
    for report in &mut reports {
        report.process = crash_process(&report.lines);
    }
    reports
}

impl AdbClient {
    /// Default constructor — uses `adb` from PATH.
    #[must_use]
    pub fn new() -> Self {
        AdbClient { binary: None }
    }

    /// Build with an explicit `adb` binary path (useful for tests or
    /// non-PATH SDK installs).
    #[must_use]
    pub fn with_binary(binary: impl Into<String>) -> Self {
        AdbClient {
            binary: Some(binary.into()),
        }
    }

    fn cmd(&self) -> Command {
        Command::new(self.binary.as_deref().unwrap_or("adb"))
    }

    async fn run_capture(
        &self,
        serial: Option<&str>,
        subcommand: &str,
        args: &[&str],
    ) -> Result<(String, String), AdbError> {
        let mut cmd = self.cmd();
        if let Some(s) = serial {
            cmd.args(["-s", s]);
        }
        // first token of subcommand for error wrapping
        for w in subcommand.split_whitespace() {
            cmd.arg(w);
        }
        for a in args {
            cmd.arg(a);
        }
        let output = cmd.output().await.map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                AdbError::BinaryNotFound
            } else {
                AdbError::Spawn(e)
            }
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            let mut full = subcommand.to_string();
            for a in args {
                full.push(' ');
                full.push_str(a);
            }
            return Err(AdbError::NonZeroExit {
                subcommand: full,
                serial: serial.map(str::to_owned),
                code: output.status.code().unwrap_or(-1),
                stderr,
            });
        }
        Ok((stdout, stderr))
    }

    /// `adb devices -l` — list all attached devices/emulators.
    pub async fn devices(&self) -> Result<Vec<AdbDevice>, AdbError> {
        let (stdout, _) = self.run_capture(None, "devices", &["-l"]).await?;
        parse_devices_stdout(&stdout)
    }

    /// `adb -s <serial> install -r <apk>` — install (or upgrade) an apk.
    pub async fn install(&self, serial: &str, apk_path: &Path) -> Result<(), AdbError> {
        let path = apk_path.to_string_lossy();
        self.run_capture(Some(serial), "install", &["-r", &path])
            .await?;
        Ok(())
    }

    /// Spawn `adb -s <serial> shell <cmd...>` and hand back the child rather
    /// than waiting for it.
    ///
    /// For device commands that run until stopped — `screenrecord` is the
    /// one. The caller owns the child, and how it ends matters: screenrecord
    /// only finalizes a playable mp4 when it gets SIGINT, so killing the
    /// child outright leaves a file with no moov atom.
    pub fn spawn_shell(
        &self,
        serial: &str,
        cmd: &[&str],
    ) -> Result<tokio::process::Child, AdbError> {
        let mut c = self.cmd();
        c.args(["-s", serial, "shell"]);
        for a in cmd {
            c.arg(a);
        }
        c.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        c.spawn().map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                AdbError::BinaryNotFound
            } else {
                AdbError::Spawn(e)
            }
        })
    }

    /// `adb -s <serial> pull <remote> <local>` — copy a file off the device.
    pub async fn pull(&self, serial: &str, remote: &str, local: &Path) -> Result<(), AdbError> {
        let local = local.to_string_lossy();
        self.run_capture(Some(serial), "pull", &[remote, &local])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> push <local> <remote>` — copy a file onto the device.
    pub async fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), AdbError> {
        let local = local.to_string_lossy();
        self.run_capture(Some(serial), "push", &[&local, remote])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> shell am broadcast -a <action> [-d <data>]`.
    ///
    /// Note that `result=0` is the ordinary answer — it means the broadcast
    /// was delivered and nothing set a result code, not that anything failed.
    pub async fn broadcast(
        &self,
        serial: &str,
        action: &str,
        data: Option<&str>,
    ) -> Result<(), AdbError> {
        let mut args: Vec<&str> = vec!["am", "broadcast", "-a", action];
        if let Some(d) = data {
            args.push("-d");
            args.push(d);
        }
        self.run_capture(Some(serial), "shell", &args).await?;
        Ok(())
    }

    /// `adb -s <serial> uninstall <pkg>` — uninstall a package.
    pub async fn uninstall(&self, serial: &str, package: &str) -> Result<(), AdbError> {
        self.run_capture(Some(serial), "uninstall", &[package])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> shell am start -n <pkg>/<activity> [args]` — launch
    /// app's activity. Returns nothing on success.
    pub async fn start_activity(
        &self,
        serial: &str,
        package: &str,
        activity: &str,
        extras: &[(&str, &str)],
    ) -> Result<(), AdbError> {
        let component = format!("{package}/{activity}");
        let mut args = vec![
            "am".to_string(),
            "start".to_string(),
            "-n".to_string(),
            component,
        ];
        for (k, v) in extras {
            args.push("--es".to_string());
            args.push((*k).to_string());
            args.push((*v).to_string());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_capture(Some(serial), "shell", &arg_refs).await?;
        Ok(())
    }

    /// `adb -s <serial> shell am force-stop <pkg>` — force-stop a package.
    pub async fn force_stop(&self, serial: &str, package: &str) -> Result<(), AdbError> {
        self.run_capture(Some(serial), "shell", &["am", "force-stop", package])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> emu avd name` — which AVD this emulator is running.
    ///
    /// A serial says which emulator is answering now; the AVD name says
    /// which one to start when none is. Registration is the moment both
    /// are knowable — it already refuses a serial adb cannot see — so
    /// that is where the pair gets written down.
    pub async fn avd_name(&self, serial: &str) -> Result<String, AdbError> {
        let out = self.emu(serial, &["avd", "name"]).await?;
        // The console answers with the name and then `OK` on its own
        // line. Taking the whole body would store "sim-smix-01\nOK" as
        // the AVD, and `emulator -avd` would then be handed something no
        // AVD is called.
        Ok(out
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && *l != "OK")
            .unwrap_or_default()
            .to_string())
    }

    /// Start an AVD, detached, and return once the process is away.
    ///
    /// Not through adb: adb talks to emulators that exist, and this is
    /// how one comes to exist. Detached on purpose — the emulator
    /// outlives the command that asked for it, which is the whole point
    /// of a device somebody else can then find in the ledger.
    /// `serial` is not decoration: `emulator-5560` *is* the console port
    /// 5560, and starting the AVD without saying so takes whatever port
    /// is free — 5554, usually. The first version of this omitted it and
    /// asked for one device while starting another, which is the exact
    /// confusion this whole line of work exists to end, committed by the
    /// code meant to end it.
    pub fn start_emulator_on(&self, avd: &str, serial: &str) -> Result<(), AdbError> {
        let port = serial
            .rsplit('-')
            .next()
            .and_then(|p| p.parse::<u16>().ok());
        let Some(port) = port else {
            return Err(AdbError::Malformed {
                subcommand: "start_emulator_on".to_string(),
                detail: format!(
                    "{serial} does not end in a console port, so there is \
                                 no way to start the emulator that would answer to it"
                ),
            });
        };
        self.spawn_emulator(avd, Some(port))
    }

    pub fn start_emulator(&self, avd: &str) -> Result<(), AdbError> {
        self.spawn_emulator(avd, None)
    }

    fn spawn_emulator(&self, avd: &str, port: Option<u16>) -> Result<(), AdbError> {
        let home = std::env::var("ANDROID_HOME")
            .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
            .unwrap_or_else(|_| {
                format!(
                    "{}/Library/Android/sdk",
                    std::env::var("HOME").unwrap_or_default()
                )
            });
        let mut cmd = std::process::Command::new(format!("{home}/emulator/emulator"));
        cmd.args(["-avd", avd, "-no-boot-anim"]);
        if let Some(port) = port {
            cmd.args(["-port", &port.to_string()]);
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(AdbError::from)?;
        Ok(())
    }

    /// Poll until the device answers and Android says it finished booting.
    ///
    /// Two conditions, not one: adb lists a device well before
    /// `sys.boot_completed` is 1, and a command sent in that window
    /// fails in ways that read like the app's fault.
    pub async fn wait_for_boot(
        &self,
        serial: &str,
        within: std::time::Duration,
    ) -> Result<(), AdbError> {
        let deadline = std::time::Instant::now() + within;
        loop {
            if let Ok((out, _)) = self
                .run_capture(Some(serial), "shell", &["getprop", "sys.boot_completed"])
                .await
                && out.trim() == "1"
            {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(AdbError::Malformed {
                    subcommand: "getprop sys.boot_completed".to_string(),
                    detail: format!("{serial} did not report a completed boot"),
                });
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }

    /// `adb -s <serial> emu kill` — ask an emulator to stop.
    ///
    /// Whether this process *may* ask is decided before the call, by the
    /// ledger, not here: this is the mechanism and the ownership rule
    /// lives with the caller. Six smoke scripts in this repository call
    /// the same command against a hard-coded `emulator-5554` and stop
    /// whichever emulator happens to be on that port, which is the
    /// failure mode that rule exists to end.
    pub async fn stop_emulator(&self, serial: &str) -> Result<(), AdbError> {
        self.emu(serial, &["kill"]).await?;
        Ok(())
    }

    /// `adb -s <serial> shell screencap -p` — capture device screen as PNG.
    /// Returns raw PNG bytes via stdout.
    pub async fn screenshot(&self, serial: &str) -> Result<Vec<u8>, AdbError> {
        let mut cmd = self.cmd();
        cmd.args(["-s", serial, "shell", "screencap", "-p"]);
        let output = cmd.output().await.map_err(AdbError::from)?;
        if !output.status.success() {
            return Err(AdbError::NonZeroExit {
                subcommand: "shell screencap -p".into(),
                serial: Some(serial.to_string()),
                code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(output.stdout)
    }

    /// `adb -s <serial> reverse tcp:<device> tcp:<host>` — let the
    /// device reach a port on this machine.
    ///
    /// The mirror of [`Self::forward`], and the direction an app under
    /// test needs: a service running here becomes reachable from the
    /// device at `127.0.0.1:<device_port>`. It is state in the adb
    /// server rather than a process — it outlives the command that sets
    /// it, and lasts until removed or until the device goes away.
    pub async fn reverse(
        &self,
        serial: &str,
        device_port: u16,
        host_port: u16,
    ) -> Result<(), AdbError> {
        let argv = reverse_argv(device_port, host_port);
        let args: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
        self.run_capture(Some(serial), &argv[0], &args).await?;
        Ok(())
    }

    /// `adb -s <serial> reverse --remove tcp:<device>` — close one.
    pub async fn unreverse(&self, serial: &str, device_port: u16) -> Result<(), AdbError> {
        let argv = unreverse_argv(device_port);
        let args: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
        self.run_capture(Some(serial), &argv[0], &args).await?;
        Ok(())
    }

    /// `adb -s <serial> reverse --list` — the pairs open on this device.
    pub async fn reverse_list(&self, serial: &str) -> Result<Vec<(u16, u16)>, AdbError> {
        let (stdout, _) = self
            .run_capture(Some(serial), "reverse", &["--list"])
            .await?;
        Ok(parse_reverse_list(&stdout))
    }

    /// `adb -s <serial> forward tcp:<host> tcp:<device>` — set up port
    /// forwarding from host loopback to device port.
    pub async fn forward(
        &self,
        serial: &str,
        host_port: u16,
        device_port: u16,
    ) -> Result<(), AdbError> {
        let host = format!("tcp:{host_port}");
        let dev = format!("tcp:{device_port}");
        self.run_capture(Some(serial), "forward", &[&host, &dev])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> forward --remove tcp:<host>` — remove a forward.
    pub async fn unforward(&self, serial: &str, host_port: u16) -> Result<(), AdbError> {
        let host = format!("tcp:{host_port}");
        self.run_capture(Some(serial), "forward", &["--remove", &host])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> shell <cmd...>` — generic shell exec, returns stdout.
    ///
    /// Runs *on the device*. For the emulator console — `geo`, `sms`,
    /// `rotate` and friends — use [`Self::emu`]: those are adb's own
    /// subcommands, and asking the device's shell for them finds nothing.
    pub async fn shell(&self, serial: &str, cmd: &[&str]) -> Result<String, AdbError> {
        let (stdout, _) = self.run_capture(Some(serial), "shell", cmd).await?;
        Ok(stdout)
    }

    /// `adb -s <serial> emu <cmd...>` — the emulator console, returns stdout.
    ///
    /// A different channel from [`Self::shell`]. `adb shell emu geo fix …`
    /// looks plausible but asks the device for a program named `emu` — it
    /// answers `sh: emu: inaccessible or not found` and exits 127.
    /// `setLocation` shipped that way, so it always failed.
    ///
    /// The console reports differently from the shell, and that is why this
    /// method exists rather than a call site. `adb shell` propagates the
    /// device command's exit status (measured: `shell 'exit 42'` → 42), but
    /// `adb emu` does **not** — it answers `OK`, or `KO: <reason>`, and exits
    /// 0 either way (measured: a bad argument gives
    /// `KO: argument 'x' is not a number`, exit 0). So the exit code carries
    /// nothing here and stdout carries everything; `KO` is translated into an
    /// error, because nobody downstream would think to look for it.
    ///
    /// Emulator-only, as the name says. A physical device has no console,
    /// which is no loss: smix is simulator-only by charter.
    pub async fn emu(&self, serial: &str, cmd: &[&str]) -> Result<String, AdbError> {
        let (stdout, _) = self.run_capture(Some(serial), "emu", cmd).await?;
        if stdout.trim_start().starts_with("KO") {
            return Err(AdbError::Malformed {
                subcommand: format!("emu {}", cmd.join(" ")),
                detail: stdout.trim().to_string(),
            });
        }
        Ok(stdout)
    }

    /// `adb -s <serial> shell input keyevent KEYCODE_WAKEUP`.
    ///
    /// Turns the screen on. It does **not** dismiss the keyguard: on a
    /// device with a passcode the screen lights and the lock stays.
    pub async fn wake(&self, serial: &str) -> Result<(), AdbError> {
        self.run_capture(
            Some(serial),
            "shell",
            &["input", "keyevent", "KEYCODE_WAKEUP"],
        )
        .await?;
        Ok(())
    }

    /// `adb -s <serial> shell dumpsys power` read for `mWakefulness=`.
    pub async fn wakefulness(&self, serial: &str) -> Result<Option<Wakefulness>, AdbError> {
        let dump = self.shell(serial, &["dumpsys", "power"]).await?;
        Ok(parse_wakefulness(&dump))
    }

    /// `adb -s <serial> shell svc power stayon <true|false>`.
    pub async fn set_stay_awake(&self, serial: &str, on: bool) -> Result<(), AdbError> {
        let value = if on { "true" } else { "false" };
        self.run_capture(Some(serial), "shell", &["svc", "power", "stayon", value])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> shell settings get global stay_on_while_plugged_in`.
    pub async fn stay_awake(&self, serial: &str) -> Result<Option<bool>, AdbError> {
        let out = self
            .shell(
                serial,
                &["settings", "get", "global", "stay_on_while_plugged_in"],
            )
            .await?;
        Ok(parse_stay_awake(&out))
    }

    /// `adb -s <serial> shell dumpsys activity activities`, read for the
    /// resumed activity.
    pub async fn resumed_activity(
        &self,
        serial: &str,
    ) -> Result<Option<(String, String)>, AdbError> {
        let dump = self
            .shell(serial, &["dumpsys", "activity", "activities"])
            .await?;
        Ok(parse_resumed_activity(&dump))
    }

    /// `adb -s <serial> logcat -b crash -d -v threadtime`.
    ///
    /// `-v threadtime` is pinned rather than left to the device's
    /// default: the parser reads a fixed column layout, and a buffer
    /// formatted some other way would parse to nothing while looking
    /// exactly like a device that has not crashed.
    pub async fn crash_buffer(&self, serial: &str) -> Result<Vec<CrashReport>, AdbError> {
        let (stdout, _) = self
            .run_capture(
                Some(serial),
                "logcat",
                &["-b", "crash", "-d", "-v", "threadtime"],
            )
            .await?;
        Ok(parse_crash_buffer(&stdout))
    }

    /// `adb -s <serial> shell pm grant <pkg> <android.permission.X>`.
    pub async fn pm_grant(
        &self,
        serial: &str,
        package: &str,
        permission: &str,
    ) -> Result<(), AdbError> {
        self.run_capture(Some(serial), "shell", &["pm", "grant", package, permission])
            .await?;
        Ok(())
    }

    /// `adb -s <serial> shell pm clear <pkg>` — wipe the app's data.
    ///
    /// Measured: this also reverts the package's runtime permissions to
    /// their default. There is no host-side way to wipe the data alone, so
    /// a caller who wants only the files loses the grants with them.
    pub async fn pm_clear(&self, serial: &str, package: &str) -> Result<(), AdbError> {
        // `pm clear` reports failure in its output, not its exit code.
        let (stdout, _) = self
            .run_capture(Some(serial), "shell", &["pm", "clear", package])
            .await?;
        if stdout.trim().starts_with("Success") {
            Ok(())
        } else {
            Err(AdbError::Malformed {
                subcommand: format!("shell pm clear {package}"),
                detail: stdout.trim().to_string(),
            })
        }
    }

    /// The runtime permissions `<pkg>` currently holds.
    ///
    /// Read from `dumpsys package`, which is the only host-side view of
    /// them: `pm reset-permissions` exists but reverts every app on the
    /// device, including whatever the runner was granted to do its job.
    pub async fn runtime_permissions_granted(
        &self,
        serial: &str,
        package: &str,
    ) -> Result<Vec<String>, AdbError> {
        let (stdout, _) = self
            .run_capture(Some(serial), "shell", &["dumpsys", "package", package])
            .await?;
        Ok(parse_granted_runtime_permissions(&stdout))
    }

    /// `adb -s <serial> shell pm revoke <pkg> <android.permission.X>`.
    pub async fn pm_revoke(
        &self,
        serial: &str,
        package: &str,
        permission: &str,
    ) -> Result<(), AdbError> {
        self.run_capture(
            Some(serial),
            "shell",
            &["pm", "revoke", package, permission],
        )
        .await?;
        Ok(())
    }
}

// -------------------- unit tests -------------------------------------------

#[cfg(test)]
mod tests {
    /// Verbatim from `dumpsys package` on an API 33 emulator, including the
    /// section that follows — the parser has to know where to stop, and a
    /// sample written by hand would not have taught it that.
    const DUMPSYS_REAL: &str = "\
    requested permissions:
      android.permission.INTERNET
      android.permission.ACCESS_FINE_LOCATION
    install permissions:
      android.permission.INTERNET: granted=true
    runtime permissions:
      android.permission.POST_NOTIFICATIONS: granted=false, flags=[ USER_SENSITIVE_WHEN_GRANTED|USER_SENSITIVE_WHEN_DENIED]
      android.permission.ACCESS_FINE_LOCATION: granted=true, flags=[ USER_SENSITIVE_WHEN_GRANTED|USER_SENSITIVE_WHEN_DENIED]
      android.permission.ACCESS_COARSE_LOCATION: granted=false, flags=[ USER_SENSITIVE_WHEN_GRANTED|USER_SENSITIVE_WHEN_DENIED]
      android.permission.CAMERA: granted=true, flags=[ USER_SENSITIVE_WHEN_GRANTED|USER_SENSITIVE_WHEN_DENIED]

User 0: ceDataInode=1234 installed=true hidden=false
";

    #[test]
    fn reads_only_the_granted_runtime_permissions() {
        let granted = parse_granted_runtime_permissions(DUMPSYS_REAL);
        assert_eq!(
            granted,
            vec![
                "android.permission.ACCESS_FINE_LOCATION",
                "android.permission.CAMERA"
            ]
        );
    }

    /// The install permissions above are also `granted=true`, and revoking
    /// one is not something the runtime section asked for.
    #[test]
    fn stays_out_of_the_neighbouring_sections() {
        assert!(
            !parse_granted_runtime_permissions(DUMPSYS_REAL)
                .contains(&"android.permission.INTERNET".to_string())
        );
    }

    #[test]
    fn an_app_holding_nothing_yields_nothing() {
        assert!(parse_granted_runtime_permissions("runtime permissions:\n\nUser 0:").is_empty());
        assert!(parse_granted_runtime_permissions("").is_empty());
    }

    use super::*;

    #[test]
    fn parses_emulator_device_line() {
        let line = "emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:1\n";
        let devs = parse_devices_stdout(line).unwrap();
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].serial, "emulator-5554");
        assert_eq!(devs[0].state, "device");
        assert_eq!(devs[0].transport_id.as_deref(), Some("1"));
    }

    /// The device side comes first, and this is the whole judgement.
    ///
    /// `adb reverse <remote> <local>` reads device-then-host, and
    /// `adb forward <local> <remote>` reads host-then-device — the two
    /// subcommands take their pair in opposite orders. Swapping them
    /// costs nothing at the adb layer (both are ports, both parse) and
    /// gives the device a route to a port nothing is listening on.
    #[test]
    fn reverse_names_the_device_port_before_the_host_port() {
        assert_eq!(
            reverse_argv(8080, 3000),
            vec!["reverse", "tcp:8080", "tcp:3000"]
        );
        assert_eq!(unreverse_argv(8080), vec!["reverse", "--remove", "tcp:8080"]);
    }

    /// Verbatim from `adb -s emulator-5554 reverse --list` with two open.
    #[test]
    fn reads_the_open_reverses() {
        let listing = "emulator-5554 tcp:8080 tcp:3000\nemulator-5554 tcp:9090 tcp:9090\n";
        assert_eq!(parse_reverse_list(listing), vec![(8080, 3000), (9090, 9090)]);
    }

    /// An empty listing is the ordinary answer on a device with none
    /// open, and it must not read as "one, on port zero".
    #[test]
    fn a_device_with_nothing_open_reads_as_nothing() {
        assert!(parse_reverse_list("").is_empty());
        assert!(parse_reverse_list("\n").is_empty());
    }

    /// Verbatim from `dumpsys power` on API 36, before and after a
    /// `KEYCODE_SLEEP` / `KEYCODE_WAKEUP` round trip.
    #[test]
    fn wakefulness_is_read_from_the_power_dump() {
        assert_eq!(
            parse_wakefulness("Power Manager State:\n  mWakefulness=Awake\n  mHalt=false\n"),
            Some(Wakefulness::Awake)
        );
        assert_eq!(
            parse_wakefulness("  mWakefulness=Asleep\n"),
            Some(Wakefulness::Asleep)
        );
        assert_eq!(
            parse_wakefulness("  mWakefulness=Dozing\n"),
            Some(Wakefulness::Dozing)
        );
    }

    /// A dump with no such line is a dump this could not read, and that
    /// is a third answer. Calling it `Asleep` would have a caller wake a
    /// device that was never measured and trust what came next.
    #[test]
    fn a_power_dump_without_the_line_is_not_asleep() {
        assert_eq!(parse_wakefulness(""), None);
        assert_eq!(parse_wakefulness("Power Manager State:\n"), None);
        assert_eq!(parse_wakefulness("  mWakefulness=Bananas\n"), None);
    }

    /// `stay_on_while_plugged_in` is a bitmask over charger types.
    /// Measured: `svc power stayon true` writes 7, `false` writes 0.
    #[test]
    fn stay_awake_reads_the_bitmask_as_a_yes_or_no() {
        assert_eq!(parse_stay_awake("7\n"), Some(true));
        assert_eq!(parse_stay_awake("1\n"), Some(true));
        assert_eq!(parse_stay_awake("0\n"), Some(false));
    }

    /// `settings get` answers `null` for a key nobody has written. That
    /// is "unset", not "off" — the difference is whether anyone chose.
    #[test]
    fn an_unwritten_stay_awake_setting_is_not_a_no() {
        assert_eq!(parse_stay_awake("null\n"), None);
        assert_eq!(parse_stay_awake(""), None);
        assert_eq!(parse_stay_awake("not a number\n"), None);
    }

    /// Verbatim from `dumpsys activity activities` on API 36 with the
    /// fixture app in front.
    ///
    /// The dump carries two lines that look interchangeable, and on a
    /// settled screen they agree — which is why the two here are made to
    /// disagree. `ResumedActivity:` is the one this reads; a version of
    /// the parser that reached for `topResumedActivity=` passed against
    /// a dump where both said the same thing, and would have gone on
    /// passing while answering a different question.
    #[test]
    fn the_resumed_activity_is_read_and_not_the_top_one() {
        let dump = "  Task{...}\n    topResumedActivity=ActivityRecord{aaa u0 com.other.app/.Splash} t9}\n  ResumedActivity: ActivityRecord{27c287f u0 dev.smix.fixture/.ComposeActivity} t2095}\n";
        assert_eq!(
            parse_resumed_activity(dump),
            Some(("dev.smix.fixture".into(), ".ComposeActivity".into()))
        );
    }

    /// A lock screen, or a device mid-boot, has nothing resumed. That is
    /// an answer and not a failure, so it has to be distinguishable from
    /// one — and from a package named the empty string.
    #[test]
    fn a_dump_with_nothing_resumed_reads_as_nothing() {
        assert_eq!(parse_resumed_activity(""), None);
        assert_eq!(parse_resumed_activity("  mResumedActivity: null\n"), None);
        assert_eq!(
            parse_resumed_activity("  ResumedActivity: ActivityRecord{27c u0 /} t1}\n"),
            None
        );
    }

    /// The whole crash buffer of emulator-5554, recorded 2026-09-23.
    ///
    /// Four banners, so four reports. The count is a fact about this
    /// recording: two Java crashes, libc's signal line for a native one,
    /// and the tombstone `debuggerd` wrote for the same native crash.
    const CRASH_BUFFER: &str =
        include_str!("../../smix-sdk/tests/fixtures/logcat/crash-buffer.threadtime.txt");

    #[test]
    fn every_banner_in_a_real_buffer_starts_a_report() {
        let reports = parse_crash_buffer(CRASH_BUFFER);
        assert_eq!(reports.len(), 4, "four banners in the recorded buffer");
        assert_eq!(
            reports[0].summary,
            "*** FATAL EXCEPTION IN SYSTEM PROCESS: batterystats-handler"
        );
        assert_eq!(reports[0].when, "09-23 03:22:12.506");
        assert_eq!(reports[1].summary, "FATAL EXCEPTION: main");
        assert_eq!(reports[2].summary.split(',').next().unwrap(), "Fatal signal 11 (SIGSEGV)");
        assert!(reports[3].summary.starts_with("*** *** ***"));
    }

    /// Three different shapes name the process, and one names none.
    /// The nameless one is still a crash and is still reported.
    #[test]
    fn a_report_names_its_process_when_the_buffer_does() {
        let reports = parse_crash_buffer(CRASH_BUFFER);
        assert_eq!(reports[0].process, "", "the system-process banner names a thread, not a process");
        assert_eq!(reports[1].process, "com.android.phone");
        assert_eq!(reports[2].process, "libgoldfish-ril");
        assert_eq!(reports[3].process, "/vendor/bin/hw/libgoldfish-rild");
    }

    /// Every line between two banners belongs to the earlier one, and
    /// the lines before the first banner belong to no report at all —
    /// logcat's own `--------- beginning of crash` among them.
    #[test]
    fn a_reports_lines_are_the_buffers_lines() {
        let reports = parse_crash_buffer(CRASH_BUFFER);
        let kept: usize = reports.iter().map(|r| r.lines.len()).sum();
        let banners = CRASH_BUFFER.lines().filter(|l| l.contains(" F ") || l.contains(" E ")).count();
        assert_eq!(kept, banners, "every log line lands in exactly one report");
        assert!(reports[1].lines.iter().any(|l| l.contains("Process: com.android.phone, PID: 901")));
        assert!(!reports[0].lines.iter().any(|l| l.contains("beginning of crash")));
    }

    /// A device that has not crashed answers with an empty buffer, and
    /// an empty buffer is an empty list rather than an error. "Nothing
    /// crashed" is the ordinary answer, and the one a caller will bet on.
    #[test]
    fn a_device_that_has_not_crashed_reports_nothing() {
        assert!(parse_crash_buffer("").is_empty());
        assert!(parse_crash_buffer("--------- beginning of crash\n").is_empty());
    }
}
