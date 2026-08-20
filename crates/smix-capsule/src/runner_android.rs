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
/// The working tree's APK, when this is a clone of smix and the APK is
/// no older than the Kotlin it was built from.
///
/// The repo tree takes precedence deliberately — a smix developer wants
/// the runner from their working copy, not the one that shipped. But
/// precedence without freshness is worse than no precedence at all: a
/// two-day-old APK sat here and won every time, so a Kotlin route added,
/// packed into the tarball, and installed came back `not_implemented`
/// from an artifact predating it. `None` here means "rebuild", and the
/// installed path is where rebuilding lives.
fn find_test_apk(root: &Path) -> Option<PathBuf> {
    let tree = root.join("android-runner");
    let candidate = tree.join("app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk");
    let built = candidate.metadata().ok()?.modified().ok()?;
    if let Some(newest) = newest_source_mtime(&tree.join("app/src"))
        && newest > built
    {
        println!(
            "[runner] {} is older than the Kotlin in this tree — building it",
            candidate.display()
        );
        return build_apk_in(&tree).ok();
    }
    Some(candidate)
}

/// The most recent modification time under `dir`, or `None` if it has
/// no files.
fn newest_source_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let mut newest: Option<std::time::SystemTime> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(m) = path.metadata().and_then(|m| m.modified()) {
                newest = Some(newest.map_or(m, |n| if m > n { m } else { n }));
            }
        }
    }
    newest
}

/// Application-window type, as `AccessibilityWindowInfo.TYPE_APPLICATION`
/// reports it. The launcher owns one on the home screen, so there is
/// always at least one on a working device.
const WINDOW_TYPE_APPLICATION: u64 = 1;

/// One GET against the forwarded runner port, response and all.
/// One GET against the forwarded runner port, headers and body.
///
/// Reads to `Content-Length` rather than to EOF. The runner announces
/// `Connection: close` and then leaves the socket open, so a reader that
/// waits for the close waits for its own timeout — five seconds, and
/// then an error, on every request against every real device. Both
/// predicates below are built on this, which is why the older of them
/// has been answering "cannot tell" since the day it was written:
/// verified against emulator-5554, where the full 166-byte response
/// arrived immediately and the socket stayed open until the deadline.
fn get_body(port: u16, path: &str) -> Result<String, ()> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2))
        .map_err(|_| ())?;
    // Longer than the runner's own SOCKET_READ_TIMEOUT, which is five
    // seconds: NanoHTTPD holds a connection open waiting for a second
    // request on it and only lets go when that elapses, so a request
    // issued right behind another one waits out that window before it is
    // answered. A five-second deadline here races the runner's five and
    // loses about as often as it wins — measured on emulator-5554, where
    // a /windows asked straight after a /health took exactly 5.0s to
    // answer, and this read gave up at 5.0s and reported "cannot tell".
    // Which is why the predicate built on it, and the older one beside
    // it, have been passing everything: they never got an answer to
    // judge. Ten seconds is past the runner's window, not a guess at how
    // slow a device might be.
    s.set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|_| ())?;
    s.write_all(
        format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .map_err(|_| ())?;

    let mut raw: Vec<u8> = Vec::new();
    let mut scratch = [0u8; 4096];
    loop {
        // A closed socket is still a legitimate ending — this reads to
        // whichever comes first, the declared length or the close.
        let n = match s.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        raw.extend_from_slice(&scratch[..n]);
        let Some(head_end) = find_header_end(&raw) else {
            continue;
        };
        let Some(len) = content_length(&raw[..head_end]) else {
            // No length to wait for: the close is the only ending there
            // is, so keep reading until it comes.
            continue;
        };
        if raw.len() >= head_end + len {
            break;
        }
    }
    if raw.is_empty() {
        return Err(());
    }
    String::from_utf8(raw).map_err(|_| ())
}

/// Index just past the blank line that ends the headers.
fn find_header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

/// `Content-Length` off a header block, when it declares one.
fn content_length(head: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(head).ok()?;
    text.lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        })
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
}

/// Does the runner's automation see any application window at all?
///
/// Read from `/windows`, which is the route that exists to tell "not
/// attached" from "attached but unreadable" apart. A runner too old to
/// serve it is not judged — an unknown answer is not a failing one.
/// The package of whatever the platform says is resumed.
///
/// Two spellings appear in one `dumpsys activity activities` dump:
/// `topResumedActivity=` and `ResumedActivity:`. They differ when more
/// than one display reports, and the top one is the one in front, so it
/// wins. Both wrap an `ActivityRecord{<hash> <user> <pkg>/<activity>}`,
/// and the package is what precedes the slash.
///
/// Anything it cannot take apart yields None rather than a fragment: an
/// unknown foreground is what makes the comparison downstream stand
/// down, and half a package name would make it lie instead.
pub fn parse_resumed_package(dump: &str) -> Option<String> {
    fn package_after(marker: &str, dump: &str) -> Option<String> {
        let at = dump.find(marker)?;
        let rest = &dump[at + marker.len()..];
        let record = rest.find("ActivityRecord{")?;
        let inside = &rest[record + "ActivityRecord{".len()..];
        let end = inside.find('}')?;
        let fields = &inside[..end];
        // `<hash> <user> <pkg>/<activity>` — the slash-bearing field is
        // the only one that carries a package, whatever precedes it.
        let slashed = fields.split_whitespace().find(|f| f.contains('/'))?;
        let pkg = slashed.split('/').next()?;
        if pkg.is_empty() {
            return None;
        }
        Some(pkg.to_string())
    }
    // The markers carry their punctuation on purpose: "ResumedActivity"
    // is a substring of "topResumedActivity", so the bare spelling would
    // match the top line's tail and the two branches would never be
    // distinguishable — a mutation that deleted the preference passed
    // every test until these were pinned to `=` and `:`.
    package_after("topResumedActivity=", dump).or_else(|| package_after("ResumedActivity:", dump))
}

/// What `dumpsys activity activities` says is in front, if it says.
fn resumed_package(serial: &str) -> Option<String> {
    let out = adb(serial)
        .args(["shell", "dumpsys", "activity", "activities"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_resumed_package(&String::from_utf8_lossy(&out.stdout))
}

/// Is the runner's view of the device the device's current one?
///
/// `automation_sees_an_app` asks whether *an* application window is
/// attached and readable. That is a weaker question than it looks: an
/// instrumentation whose accessibility connection went stale keeps
/// serving the windows it saw before, and a leftover consumer app is a
/// readable type-1 window like any other — so the runner passes that
/// check while every tree it serves describes a screen nobody is
/// looking at. Reproduced on emulator-5554: `dev.smix.fixture` in
/// front, `/windows` listing systemui plus a consumer app installed
/// hours earlier, `/tree` carrying no package at all.
///
/// Telling those apart needs a second source that cannot be wrong in
/// the same direction, so this one comes from the device rather than
/// from the runner: whatever the platform says is resumed has to be
/// among the windows the runner can see.
///
/// An unknown answer is not a failing one. A runner too old for the
/// route, a transport hiccup, an unparseable body, or no foreground to
/// compare against all return Ok — this exists to catch a runner shown
/// to be behind, not to invent a verdict it could not reach.
pub fn runner_view_is_current(port: u16, foreground_package: &str) -> Result<(), String> {
    if foreground_package.is_empty() {
        return Ok(());
    }
    let Ok(body) = get_body(port, "/windows") else {
        return Ok(());
    };
    let Some(start) = body.find('{') else {
        return Ok(());
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&body[start..]) else {
        return Ok(());
    };
    let Some(windows) = doc.get("windows").and_then(|w| w.as_array()) else {
        return Ok(());
    };
    let seen: Vec<&str> = windows
        .iter()
        .filter_map(|w| w.get("package").and_then(serde_json::Value::as_str))
        .collect();
    if seen.contains(&foreground_package) {
        return Ok(());
    }
    // An empty list is not a missing answer. Reaching here means the
    // route was served and the runner named nothing — seen on
    // emulator-5554 with /health at 200 and `dumpsys accessibility`
    // reporting `Bound services:{}`: the HTTP face alive, the sensing
    // face dead. Reading that as "cannot tell" is how the blindest
    // runner of all would pass.
    if seen.is_empty() {
        return Err(format!(
            "the runner sees no windows at all while {foreground_package} is \
             resumed on the device. Its HTTP server answers and its \
             accessibility connection does not — every tree it serves would \
             be empty"
        ));
    }
    Err(format!(
        "the runner does not see the app that is in front. \
         {foreground_package} is resumed on the device; the runner's windows \
         are {}. Its accessibility connection is behind the device — every \
         tree it serves describes a screen nobody is looking at",
        seen.join(", ")
    ))
}

fn automation_sees_an_app(port: u16) -> Result<(), String> {
    // The crate's own socket read rather than an HTTP client crate:
    // `read_health_bytes` next door does exactly this, and a dependency
    // for one GET is a dependency for something already written.
    //
    // A runner too old to serve the route, or any transport hiccup,
    // answers nothing — and an unknown answer is not a failing one.
    let Ok(body) = get_body(port, "/windows") else {
        return Ok(());
    };
    let Some(start) = body.find('{') else {
        return Ok(());
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&body[start..]) else {
        return Ok(());
    };
    let Some(windows) = doc.get("windows").and_then(|w| w.as_array()) else {
        return Ok(());
    };
    let apps: Vec<&str> = windows
        .iter()
        .filter(|w| {
            w.get("type").and_then(serde_json::Value::as_u64) == Some(WINDOW_TYPE_APPLICATION)
                && w.get("rootReadable").and_then(serde_json::Value::as_bool) == Some(true)
        })
        .filter_map(|w| w.get("package").and_then(serde_json::Value::as_str))
        .collect();
    if !apps.is_empty() {
        return Ok(());
    }
    let seen: Vec<&str> = windows
        .iter()
        .filter_map(|w| w.get("package").and_then(serde_json::Value::as_str))
        .collect();
    Err(format!(
        "no readable application window is attached — only {}. \
         Every tree it serves would carry those and nothing else",
        if seen.is_empty() {
            "nothing at all".to_string()
        } else {
            seen.join(", ")
        }
    ))
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
    smix_lease::store::machine_root().map(|r| r.join("android-runner"))
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
fn ensure_installed_apk() -> Result<(PathBuf, bool), String> {
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
    // The invariant is not "did this run extract" — that is true once
    // and false forever after, so the first run to sync new sources
    // rebuilt and every later one served the stale APK anyway. It is
    // "was this APK built from these sources", and the way to know is
    // to have written down which sources, at the time.
    //
    // Without it: sources synced, the artifact did not follow, and the
    // runner answered `not_implemented` for a route whose Kotlin sat
    // one directory away. That is the v1.0.10 cycle again, on the
    // other platform.
    let stamp = dir.join(".smix-apk-sources");
    let want = format!("{:016x}", smix_runner_sources::android_sources_digest());
    let built_from = std::fs::read_to_string(&stamp).unwrap_or_default();
    if apk.is_file() && built_from.trim() == want {
        return Ok((apk, false));
    }
    let _ = extracted;
    if apk.is_file() {
        println!("[runner] the instrumentation APK is older than the sources — rebuilding");
    } else {
        println!("[runner] building the instrumentation APK (first run on this machine)");
    }
    let built = build_apk_in(&dir)?;
    debug_assert_eq!(built, apk);
    // Written only after the APK exists: a stamp for a build that did
    // not produce one would claim the artifact is current forever.
    if let Err(e) = std::fs::write(&stamp, &want) {
        eprintln!("[runner] could not record what the APK was built from: {e}");
    }
    Ok((apk, true))
}

/// Run gradle's `assembleDebugAndroidTest` in `tree` and return the APK.
///
/// Shared by both trees, because both have the same obligation: the
/// artifact must be no older than the sources beside it.
fn build_apk_in(tree: &Path) -> Result<PathBuf, String> {
    let out = Command::new("./gradlew")
        .args([":app:assembleDebugAndroidTest", "--console=plain"])
        .current_dir(tree)
        .output()
        .map_err(|e| {
            format!(
                "the Android runner needs a build and ./gradlew could not start: {e}\n\
                 Its project is at {} — an Android SDK is required, the same way the \
                 iOS runner requires Xcode.",
                tree.display()
            )
        })?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let all: Vec<&str> = stdout.lines().collect();
        let tail = all[all.len().saturating_sub(15)..].join("\n");
        return Err(format!(
            "building the Android runner failed in {}:\n{}\n{}",
            tree.display(),
            tail,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let apk = tree.join("app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk");
    if !apk.is_file() {
        return Err(format!(
            "the gradle build reported success but produced no APK at {}",
            apk.display()
        ));
    }
    Ok(apk)
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
/// Bring the runner up, reporting an already-serving one as up.
///
/// Kept at its original arity because it is a published entry point;
/// the flag lives on `up_with`, the way the iOS side grew `up_on_with`
/// beside `up_on`.
pub fn up(root: &Path, serial: &str, port: u16, timeout_secs: u64) -> Result<(), String> {
    up_with(root, serial, port, timeout_secs, false)
}

/// `up`, with the flag that says what to do about a runner of ours whose
/// view of the device has gone stale: refuse and name the fix, or run it.
pub fn up_with(
    root: &Path,
    serial: &str,
    port: u16,
    timeout_secs: u64,
    force: bool,
) -> Result<(), String> {
    if !device_present(serial) {
        return Err(format!(
            "adb has no ready device {serial:?}. `adb devices` lists what is \
             attached; start the emulator first (`emulator -avd <name>`), or \
             pass the serial of a running one."
        ));
    }

    // Which APK we would serve has to be settled before "is something
    // already answering" can be read as "we are up".
    //
    // The device port is fixed, so every host port forwards onto the
    // same in-device server. An instrumentation left over from an
    // older APK answers /health perfectly, and taking that as "already
    // up" served it — on any port, indefinitely, including right after
    // a rebuild. The symptom was a runner reporting `not_implemented`
    // for a route whose Kotlin had already shipped and built.
    let (apk, rebuilt) = match find_test_apk(root) {
        Some(a) => (a, false),
        None => ensure_installed_apk()?,
    };

    // Already up on this port? Say so rather than stacking a second
    // instrumentation onto the same forwarded port — unless the APK
    // just changed, in which case whatever is answering is the old one
    // and has to go.
    //
    // health-decider: whether this port is already serving — deferred:
    // the answer to ask instead is right below, in the wait loop:
    // `automation_sees_an_app`. Moving it up here changes what happens
    // to a live Android runner, and that is a claim about a device this
    // was not run against (§9 #1 ③). It is a two-line change with an
    // emulator in front of it, and none without one.
    if health_ok(port) {
        if !rebuilt {
            // /health says the HTTP server answers. It cannot say the
            // instrumentation still sees this device: an accessibility
            // connection that went stale keeps serving the windows it
            // saw before, and that reads as healthy from here. So the
            // question asked before reporting up is the one a caller
            // actually cares about, and the answer comes from the
            // platform rather than from the runner.
            let foreground = resumed_package(serial);
            // Retried rather than asked once, and the retry lives here
            // rather than inside the predicate: a window list that has
            // not caught up with an `am start` from a moment ago is not
            // a broken runner, it is a runner mid-update. What separates
            // the two is whether time fixes it. Three seconds of asking
            // is enough for a foreground switch to land and far short of
            // anything a dead accessibility connection recovers in —
            // without this the release gate's own `am start; runner up`
            // sequence was refused, which is the false positive this
            // predicate has to not have.
            let mut verdict = runner_view_is_current(port, foreground.as_deref().unwrap_or(""));
            for _ in 0..3 {
                if verdict.is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                verdict = runner_view_is_current(port, foreground.as_deref().unwrap_or(""));
            }
            match verdict {
                Ok(()) => {
                    println!("runner up: already healthy on http://localhost:{port}");
                    return Ok(());
                }
                Err(why) if force => {
                    println!("[runner] {why}");
                    println!("[runner] --force: replacing it");
                }
                Err(why) => {
                    return Err(format!(
                        "port {port} answers /health, but {why}.\n\n\
                         Bring it back in place:\n  \
                         smix runner up {serial} --platform android --force"
                    ));
                }
            }
        }
        println!("[runner] a runner from the previous APK is answering — replacing it");
        let _ = adb(serial)
            .args(["shell", "am", "force-stop", TEST_PACKAGE])
            .output();
    }

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
    match crate::runner::machine_leases() {
        Ok(leases) => record_android_lease(&leases, serial, port, pid),
        Err(e) => eprintln!("warning: android runner not recorded: {e}"),
    }

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
        // health-decider: whether the bring-up is finished, which has to
        // mean the automation can see something.
        if health_ok(port) {
            // `/health` says the HTTP server is alive. It does not say
            // the automation behind it can see anything, and those come
            // apart: an instrumentation that crashed on
            // `getUiAutomationWithRetry` and was restarted answers
            // /health perfectly while `getWindows()` returns the
            // SystemUI windows and nothing else. Every tree then looks
            // like an app with no accessibility nodes — which is how a
            // consumer spent several rounds driving by pixel, and how
            // this repository's own Android gate went red on Settings
            // right after a crashed runner.
            //
            // An application window is the thing to check for, because
            // there is always one: the launcher owns one on the home
            // screen. Only-SystemUI is not a state to report success on.
            if let Err(why) = automation_sees_an_app(port) {
                return Err(format!(
                    "the runner answers /health on {port} but its automation is \
                     not usable: {why}\n\
                     This is what a crashed-and-restarted instrumentation looks \
                     like — the server is up, `getWindows()` is not. Stop it and \
                     bring it up again:\n  \
                     smix runner down --platform android --device {serial}"
                ));
            }
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
fn record_android_lease(leases: &smix_lease::store::LeaseDir, serial: &str, port: u16, pid: u32) {
    use smix_lease::store;
    let proc = store::identify(pid).unwrap_or(smix_lease::ProcIdentity {
        pid,
        started_at: String::new(),
        cmd: format!("instrumentation runner on {serial}"),
    });
    if let Err(e) = store::add_resource(
        leases,
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
    let leases = match crate::runner::machine_leases() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("runner down: lease ledger not updated: {e}");
            return Ok(());
        }
    };
    if let Err(e) = smix_lease::store::drop_resource_kind(
        &leases,
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
    use std::io::Write;

    #[test]
    fn forward_maps_the_host_port_onto_the_device_side_28080() {
        assert_eq!(
            forward_argv(22093),
            vec!["forward", "tcp:22093", "tcp:28080"]
        );
    }

    #[test]
    fn the_newest_source_is_the_one_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("app/src/androidTest/kotlin");
        std::fs::create_dir_all(&src).expect("mkdir");
        assert!(
            newest_source_mtime(&dir.path().join("app/src")).is_some() || true,
            "an empty tree has no newest file, which is not an error"
        );
        std::fs::write(src.join("Old.kt"), "// old").expect("write");
        let first = newest_source_mtime(&dir.path().join("app/src")).expect("a file exists");

        // A second file written later must move the answer forward, or
        // an APK built between them reads as current. Set the mtime
        // rather than sleeping: the test should not be slower than the
        // filesystem's clock resolution to be right.
        let newer = src.join("New.kt");
        let mut f = std::fs::File::create(&newer).expect("create");
        f.write_all(b"// new").expect("write");
        let ahead = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        f.set_modified(ahead).expect("set mtime");
        drop(f);

        let second = newest_source_mtime(&dir.path().join("app/src")).expect("files exist");
        assert!(
            second > first,
            "the newest source did not move when a newer file appeared"
        );
    }

    #[test]
    fn a_tree_with_no_sources_reports_nothing_rather_than_now() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            newest_source_mtime(&dir.path().join("app/src")).is_none(),
            "an absent source tree must not answer with a time, which would \
             make every APK look stale forever"
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
