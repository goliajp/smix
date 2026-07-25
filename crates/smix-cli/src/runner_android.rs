//! `smix runner up/down` for the Android emulator.
//!
//! The iOS path drives xcodebuild; this one drives adb. Same contract,
//! same state file, same "block until /health answers" promise — the
//! difference is only what gets spawned.
//!
//! Bringing the Kotlin runner up by hand takes three steps that were
//! documented nowhere: install the instrumentation APK, forward the
//! port, and `am instrument` the server entry point. Anyone who wanted
//! `smix run --platform android` had to reverse-engineer them from the
//! runner's source.
//!
//! **Every adb invocation here names its device with `-s`.** An adb
//! command without it targets whatever single device is attached — and
//! when a developer's own phone is plugged in alongside the emulator,
//! "whatever" is a coin flip. `gradlew install*` has no such guard at
//! all, which is why this installs via `adb -s` rather than a gradle
//! task.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use smix_capsule::runner::{RunnerState, health_ok};

/// The Kotlin runner's HTTP port, by convention. iOS uses 22087.
pub const DEFAULT_ANDROID_PORT: u16 = 28080;

/// Instrumentation coordinates of the runner's server entry point.
/// `am instrument` needs all three, and a typo in any of them produces
/// `OK (0 tests)` — a silent no-op that looks like success.
const TEST_PACKAGE: &str = "dev.smix.runner.test";
const TEST_RUNNER: &str = "androidx.test.runner.AndroidJUnitRunner";
const SERVER_ENTRY: &str = "dev.smix.runner.RunnerTest#runServerForever";

fn adb(serial: &str) -> Command {
    let mut c = Command::new("adb");
    c.args(["-s", serial]);
    c
}

/// Locate the instrumentation APK. Built by
/// `./gradlew :app:assembleDebugAndroidTest` in `android-runner/`.
fn find_test_apk(root: &Path) -> Option<PathBuf> {
    let candidate = root
        .join("android-runner/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk");
    candidate.is_file().then_some(candidate)
}

/// Is this serial actually attached and ready?
fn device_present(serial: &str) -> bool {
    let Ok(out) = Command::new("adb").args(["devices"]).output() else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with(serial) && l.split_whitespace().nth(1) == Some("device"))
}

/// Bring the Kotlin runner up on `serial` and block until `/health`
/// answers or `timeout_secs` elapses.
pub fn up(root: &Path, serial: &str, port: u16, timeout_secs: u64) -> Result<(), String> {
    if !device_present(serial) {
        return Err(format!(
            "adb has no ready device {serial:?}. `adb devices` lists what is \
             attached; start the emulator first (`emulator -avd <name>`), or \
             pass the serial of a running one."
        ));
    }

    // Already up on this port? Say so rather than stacking a second
    // instrumentation onto the same forwarded port.
    if health_ok(port) {
        println!("runner up: already healthy on http://localhost:{port}");
        return Ok(());
    }

    let apk = find_test_apk(root).ok_or_else(|| {
        "no instrumentation APK found at android-runner/app/build/outputs/apk/\
         androidTest/debug/app-debug-androidTest.apk — build it with \
         `cd android-runner && ./gradlew :app:assembleDebugAndroidTest`"
            .to_string()
    })?;

    println!("[runner] android device: {serial}");
    let install = adb(serial)
        .args(["install", "-r", "-t"])
        .arg(&apk)
        .output()
        .map_err(|e| format!("adb install: {e}"))?;
    if !install.status.success() {
        return Err(format!(
            "adb install failed: {}",
            String::from_utf8_lossy(&install.stderr).trim()
        ));
    }

    // Host:device port forward. Re-running is harmless; adb replaces.
    let fwd = adb(serial)
        .args(["forward", &format!("tcp:{port}"), &format!("tcp:{port}")])
        .output()
        .map_err(|e| format!("adb forward: {e}"))?;
    if !fwd.status.success() {
        return Err(format!(
            "adb forward tcp:{port} failed: {}",
            String::from_utf8_lossy(&fwd.stderr).trim()
        ));
    }

    let log_dir = root.join(".smix/runner");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("create {log_dir:?}: {e}"))?;
    let log = log_dir.join(format!("runner-{serial}.log"));
    let log_file = std::fs::File::create(&log).map_err(|e| format!("create {log:?}: {e}"))?;

    // `am instrument -w` blocks for the life of the server, so it is
    // spawned rather than awaited. Its stdout is the JUnit stream; the
    // useful signal is /health, not this log.
    let child = adb(serial)
        .args([
            "shell",
            "am",
            "instrument",
            "-w",
            "-e",
            "class",
            SERVER_ENTRY,
            &format!("{TEST_PACKAGE}/{TEST_RUNNER}"),
        ])
        .stdout(Stdio::from(
            log_file.try_clone().map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(log_file))
        .spawn()
        .map_err(|e| format!("adb shell am instrument: {e}"))?;
    let pid = child.id();

    let state = RunnerState {
        pid,
        udid: serial.to_string(),
        port,
        log: log.clone(),
        bundle: None,
        supervisor_pid: None,
    };
    // Not discarded, and not the iOS slot. Both halves were wrong
    // before: this wrote the same file `runner.rs` wrote, so an Android
    // runner replaced the iOS record — and `let _ =` meant a failed
    // write said nothing at all.
    if let Err(e) = smix_capsule::runner_state::write(
        root,
        smix_capsule::runner_state::Platform::Android,
        &state,
    ) {
        eprintln!("runner: {e}");
    }

    println!(
        "runner starting: device={serial} port={port} pid={pid} — log: {}",
        log.display()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if health_ok(port) {
            println!("runner up: http://localhost:{port}/health = 200");
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    if let Err(e) =
        smix_capsule::runner_state::clear(root, smix_capsule::runner_state::Platform::Android)
    {
        eprintln!("runner: {e}");
    }
    Err(format!(
        "runner did not become healthy within {timeout_secs}s. Log tail:\n{}",
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .rev()
            .take(15)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

/// Stop the instrumentation and drop the port forward.
pub fn down(root: &Path, serial: &str, port: u16) -> Result<(), String> {
    // `am force-stop` on the instrumentation package is what actually
    // ends the server; killing the host-side adb client would leave the
    // on-device process running and the port still answering.
    let _ = adb(serial)
        .args(["shell", "am", "force-stop", TEST_PACKAGE])
        .output();
    let _ = adb(serial)
        .args(["forward", "--remove", &format!("tcp:{port}")])
        .output();
    if let Err(e) =
        smix_capsule::runner_state::clear(root, smix_capsule::runner_state::Platform::Android)
    {
        eprintln!("runner: {e}");
    }
    println!("runner down: device={serial} port {port} closed");
    Ok(())
}
