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

use crate::runner::{RunnerState, health_ok};

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

/// `adb forward` argv mapping a host port onto the runner's fixed
/// device port.
fn forward_argv(host_port: u16) -> Vec<String> {
    vec![
        "forward".into(),
        format!("tcp:{host_port}"),
        format!("tcp:{DEFAULT_ANDROID_PORT}"),
    ]
}

/// Where the installed Android runner project lives, mirroring the iOS
/// `~/.local/share/smix/runner/`.
fn installed_android_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("smix/android-runner"))
}

/// Extract the shipped Android runner project and build its
/// instrumentation APK.
///
/// The old behaviour was to fail here, naming a path relative to the
/// caller's working directory — which is the *driven project*, not
/// smix. So the instruction ("cd android-runner") was addressed to a
/// directory that exists only in a clone of smix itself, and everyone
/// who had merely installed smix was told there was no APK. They
/// concluded, reasonably and wrongly, that smix could not drive Android
/// at all: the capability was implemented, gated behind an artifact
/// nothing shipped.
///
/// The sources ship now (`smix-runner-sources`), so the missing APK is
/// something smix can produce rather than something to complain about —
/// exactly what the iOS path does with `xcodebuild`. First run pays a
/// gradle build; later runs find the APK where this left it.
fn ensure_installed_apk() -> Result<PathBuf, String> {
    let dir = installed_android_dir().ok_or_else(|| {
        "no HOME or XDG_DATA_HOME, so there is nowhere to install the \
                        Android runner project"
            .to_string()
    })?;
    let extracted = smix_runner_sources::extract_android_to(&dir).map_err(|e| {
        format!(
            "extracting the Android runner project to {}: {e}",
            dir.display()
        )
    })?;
    if extracted {
        println!(
            "[runner] android sources synced → {} ({})",
            dir.display(),
            smix_runner_sources::SOURCES_VERSION
        );
    }

    let apk = dir.join("app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk");
    if apk.is_file() {
        return Ok(apk);
    }

    println!("[runner] building the instrumentation APK (first run on this machine)");
    let out = Command::new("./gradlew")
        .args([":app:assembleDebugAndroidTest", "--console=plain"])
        .current_dir(&dir)
        .output()
        .map_err(|e| {
            format!(
                "the Android runner needs a build and ./gradlew could not start: {e}\n\
                 Its project is at {} — an Android SDK is required, the same way the \
                 iOS runner requires Xcode.",
                dir.display()
            )
        })?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let all: Vec<&str> = stdout.lines().collect();
        let tail = all[all.len().saturating_sub(15)..].join("\n");
        return Err(format!(
            "building the Android runner failed in {}:\n{}\n{}",
            dir.display(),
            tail,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    apk.is_file().then_some(apk.clone()).ok_or_else(|| {
        format!(
            "the gradle build reported success but produced no APK at {}",
            apk.display()
        )
    })
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

    let apk = match find_test_apk(root) {
        Some(a) => a,
        None => ensure_installed_apk()?,
    };

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
    //
    // The two sides are not the same number. Inside the device the
    // runner listens on a compiled-in 28080 (RunnerTest.PORT); `port`
    // is the host side the caller asked for. Forwarding tcp:port to
    // tcp:port was an identity map that only ever worked because every
    // Android caller in this repo passed 28080 — `--runner-port 22093`
    // forwarded to a device port nothing was listening on, and the wait
    // for /health then timed out with the runner running perfectly.
    let forward_argv = forward_argv(port);
    let fwd = adb(serial)
        .args(&forward_argv)
        .output()
        .map_err(|e| format!("adb forward: {e}"))?;
    if !fwd.status.success() {
        return Err(format!(
            "adb {} failed: {}",
            forward_argv.join(" "),
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
    record_android_lease(root, serial, port, pid);

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
    if let Err(e) = crate::runner_state::write(root, crate::runner_state::Platform::Android, &state)
    {
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

    if let Err(e) = crate::runner_state::clear(root, crate::runner_state::Platform::Android) {
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

/// Record this runner in the device's ledger.
///
/// The host-side process outlives `runner up`, and what it leaves on the
/// device outlives the host-side process. A row is what lets a later
/// command find both and end them in the right order.
fn record_android_lease(root: &Path, serial: &str, port: u16, pid: u32) {
    use smix_lease::store;
    let proc = store::identify(pid).unwrap_or(smix_lease::ProcIdentity {
        pid,
        started_at: String::new(),
        cmd: format!("instrumentation runner on {serial}"),
    });
    if let Err(e) = store::add_resource(
        root,
        serial,
        smix_lease::Resource::AndroidRunner {
            port,
            serial: serial.to_string(),
            proc,
        },
    ) {
        eprintln!("warning: android runner not recorded in the device ledger: {e}");
    }
}

/// Is this package present on the device?
///
/// `pm list packages <name>` prefix-matches, so the answer is checked
/// against the exact line rather than "did anything come back".
fn package_installed(serial: &str, package: &str) -> bool {
    let Ok(out) = adb(serial)
        .args(["shell", "pm", "list", "packages", package])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == format!("package:{package}"))
}

/// Remove the instrumentation package this runner installs.
///
/// `up` installs a package onto the device and `down` only stops it —
/// which left smix able to put something on a phone and unable to take
/// it off. On an emulator that is untidy; on somebody's own device it is
/// smix leaving its things in a house it was let into.
///
/// Stops the instrumentation first: uninstalling a package whose process
/// is running is a harder ask, and the stop is idempotent.
pub fn uninstall(serial: &str, port: u16) -> Result<(), String> {
    let _ = adb(serial)
        .args(["shell", "am", "force-stop", TEST_PACKAGE])
        .output();
    let _ = adb(serial)
        .args(["forward", "--remove", &format!("tcp:{port}")])
        .output();
    // Ask whether it is there before removing it.
    //
    // The tempting shortcut is to run the removal and treat its error as
    // "already gone" — but Android answers a missing package with
    // `DELETE_FAILED_INTERNAL_ERROR`, the same code a device policy that
    // genuinely refuses the removal returns. Reading idempotence out of
    // that string would swallow the real failure. Asking first keeps the
    // two apart: absent is reported as absent, and a refusal stays a
    // refusal.
    if !package_installed(serial, TEST_PACKAGE) {
        println!("runner uninstall: {TEST_PACKAGE} is not installed on {serial}");
        return Ok(());
    }
    let out = adb(serial)
        .args(["uninstall", TEST_PACKAGE])
        .output()
        .map_err(|e| format!("uninstall {TEST_PACKAGE}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() && !stdout.contains("Failure") {
        println!("runner uninstall: {TEST_PACKAGE} removed from {serial}");
        return Ok(());
    }
    Err(format!(
        "adb uninstall {TEST_PACKAGE} failed on {serial}: {}",
        if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        }
    ))
}

/// Host ports this serial forwards to the runner's device port, read
/// out of `adb forward --list`.
///
/// Not derived from the caller's `port`: the host side of the forward
/// is whatever `up` was asked for, and a `down` that has lost its
/// workspace state (a different directory, a deleted one) falls back to
/// the default and would "close" a port nobody opened while the real
/// one keeps forwarding into a dead runner. The device side is fixed,
/// so it is the thing worth matching on.
fn our_forward_ports(list: &str, serial: &str) -> Vec<u16> {
    let device_side = format!("tcp:{DEFAULT_ANDROID_PORT}");
    list.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let (dev, host, remote) = (f.next()?, f.next()?, f.next()?);
            (dev == serial && remote == device_side)
                .then(|| host.strip_prefix("tcp:")?.parse().ok())
                .flatten()
        })
        .collect()
}

fn remove_our_forwards(serial: &str) -> Vec<u16> {
    let Ok(out) = adb(serial).args(["forward", "--list"]).output() else {
        return Vec::new();
    };
    let ports = our_forward_ports(&String::from_utf8_lossy(&out.stdout), serial);
    ports
        .iter()
        .filter(|p| {
            adb(serial)
                .args(["forward", "--remove", &format!("tcp:{p}")])
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .copied()
        .collect()
}

/// Stop the instrumentation, drop the port forward, and clear the rows.
pub fn down(root: &Path, serial: &str, port: u16) -> Result<(), String> {
    // `am force-stop` on the instrumentation package is what actually
    // ends the server; killing the host-side adb client would leave the
    // on-device process running and the port still answering.
    let _ = adb(serial)
        .args(["shell", "am", "force-stop", TEST_PACKAGE])
        .output();
    let closed = remove_our_forwards(serial);
    if let Err(e) = crate::runner_state::clear(root, crate::runner_state::Platform::Android) {
        eprintln!("runner: {e}");
    }
    if let Err(e) = smix_lease::store::drop_resource_kind(
        root,
        serial,
        &smix_lease::Resource::AndroidRunner {
            port: 0,
            serial: String::new(),
            proc: smix_lease::store::identify_self(),
        },
    ) {
        eprintln!("runner down: lease ledger not updated: {e}");
    }
    match closed.as_slice() {
        [] => println!("runner down: device={serial} — no forward to close"),
        ports => println!(
            "runner down: device={serial} host port{} {} closed",
            if ports.len() == 1 { "" } else { "s" },
            ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
    let _ = port;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_maps_the_host_port_onto_the_device_side_28080() {
        assert_eq!(
            forward_argv(22093),
            vec!["forward", "tcp:22093", "tcp:28080"]
        );
    }

    #[test]
    fn forwards_are_matched_by_the_device_side_not_the_caller_s_port() {
        let list = "emulator-5554 tcp:22093 tcp:28080\n\
                    emulator-5554 tcp:9999 tcp:5037\n\
                    emulator-5556 tcp:28080 tcp:28080\n";
        assert_eq!(our_forward_ports(list, "emulator-5554"), vec![22093]);
        assert_eq!(our_forward_ports(list, "emulator-5556"), vec![28080]);
        assert!(our_forward_ports(list, "emulator-9999").is_empty());
    }

    #[test]
    fn the_device_side_port_matches_what_the_kotlin_runner_compiles_in() {
        let kt = include_str!(
            "../../../android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt"
        );
        assert!(
            kt.contains(&format!("const val PORT = {DEFAULT_ANDROID_PORT}")),
            "the runner listens on a port this crate no longer forwards to"
        );
    }
}
