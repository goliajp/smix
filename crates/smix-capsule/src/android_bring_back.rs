//! Carries out what `android_foreground` decides, and reads the device
//! back after every step before taking the next one.
//!
//! The consumer who asked for this had written it into their own runner:
//! take the runner down, collapse the shade, foreground the app, wait
//! eight seconds, ask again. Every step here is theirs except two — the
//! runner is not taken down (the shade covers a working runner, and
//! cycling one is `--force`'s decision), and nothing waits a fixed time:
//! each step ends when the device says it is done, or at the deadline.

use smix_adb::wait::Deadline;
use std::time::{Duration, Instant};

use crate::android_foreground::{Screen, Step, describe, next_step, parse_windows};
use crate::runner_android::{adb, get_body, resumed_package};

/// Why the screen could not be put right, split by what the caller can
/// do about it.
#[derive(Debug)]
pub(crate) enum Unsettled {
    /// Only system UI can be read, after the shade was put away: a lock
    /// screen, or an instrumentation that sees nothing else. Cycling the
    /// runner is a fix for the second.
    Screen(String),
    /// The named app could not be put in front: not installed, no
    /// launcher entry, or it did not come up. Cycling the runner fixes
    /// none of these.
    App(String),
    /// A window of another package is in front of the app. Neither
    /// cycling the runner nor bringing the app forward moves it.
    Covered(String),
}

/// How long the whole settling may take. One shade and one launch fit
/// in a few seconds on the emulators this was measured on; this is the
/// ceiling for a slow device, not a wait.
const SETTLE: Duration = Duration::from_secs(20);
const LOOK_AGAIN: Duration = Duration::from_millis(250);

/// How long a window of another package may hold the focus in front of
/// an unreadable app before it is reported. A dialog that closes by itself
/// is gone well inside this; one waiting for an answer is not, and waiting
/// out the whole settle for it only delays saying so.
const COVERED_FOR: Duration = Duration::from_secs(3);

/// Put the screen right for `app`, or say why it cannot be.
///
/// `relaunch` is for a runner just brought up: it restarts the named app
/// once, which is what iOS does on bring-up. On a runner that was already
/// answering, the app is only brought forward if it is not in front.
pub(crate) fn settle(
    serial: &str,
    port: u16,
    app: Option<&str>,
    relaunch: bool,
) -> Result<(), Unsettled> {
    let deadline = Deadline::after(SETTLE);
    let mut collapsed = false;
    let mut started = false;
    let mut covered_since: Option<Instant> = None;
    loop {
        let look = deadline.look();
        // A runner too old to serve /windows, or a transport hiccup,
        // gives nothing to plan from. That is not a verdict: what came
        // after this call before it existed runs as it always did.
        let Some(screen) = read_screen(serial, port) else {
            return Ok(());
        };
        let step = next_step(&screen, app, collapsed);
        if !matches!(step, Step::Covered { .. }) {
            covered_since = None;
        }
        if let Some(name) = app
            && relaunch
            && !started
            && match step {
                Step::Ready | Step::BringForward { .. } => true,
                Step::CollapseShade | Step::NotYet | Step::Covered { .. } | Step::Refuse(_) => {
                    false
                }
            }
        {
            start_app(serial, port, name, true)?;
            started = true;
            println!(
                "[runner] relaunched {name}{}",
                instead_of(screen.resumed.as_deref(), name)
            );
            await_in_front(serial, name, deadline)?;
            continue;
        }
        match step {
            Step::Ready => return Ok(()),
            Step::CollapseShade => {
                let _ = adb(serial)
                    .args(["shell", "cmd", "statusbar", "collapse"])
                    .output();
                collapsed = true;
                println!(
                    "[runner] the notification shade was over the screen (system UI held \
                     the focus) — put it away"
                );
                // Whether that was enough is the planner's next answer,
                // not this wait's: it looks until an app window can be
                // read or the deadline passes, and either way asks again.
                wait_until(deadline, || {
                    read_screen(serial, port).is_some_and(|s| an_app_can_be_read(&s))
                });
            }
            Step::BringForward { app: name, was } => {
                if started {
                    return Err(Unsettled::App(format!(
                        "{name} was started and {} is in front instead",
                        was.as_deref().unwrap_or("nothing")
                    )));
                }
                start_app(serial, port, &name, false)?;
                started = true;
                println!(
                    "[runner] brought {name} forward{}",
                    instead_of(was.as_deref(), &name)
                );
                await_in_front(serial, &name, deadline)?;
            }
            Step::NotYet => {
                if deadline.passed_by(look) {
                    return Err(Unsettled::Screen(format!(
                        "no application window became readable within {}s (windows: {})",
                        SETTLE.as_secs(),
                        describe(&screen.windows)
                    )));
                }
                std::thread::sleep(LOOK_AGAIN);
            }
            Step::Covered { by } => {
                let since = *covered_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= COVERED_FOR || deadline.passed_by(look) {
                    return Err(Unsettled::Covered(covered(&by, &screen)));
                }
                std::thread::sleep(LOOK_AGAIN);
            }
            Step::Refuse(why) => return Err(Unsettled::Screen(why)),
        }
    }
}

fn instead_of(was: Option<&str>, name: &str) -> String {
    match was {
        Some(w) if w != name => format!(" ({w} was in front)"),
        _ => String::new(),
    }
}

/// What to say about a window of another package in front of the app.
pub(crate) fn covered(by: &str, screen: &Screen) -> String {
    format!(
        "a window of {by} holds the focus and no application window can be read \
         behind it (windows: {}) — a system dialog or an input method's own screen \
         is in front of the app. smix does not dismiss it: a BACK key would answer \
         it for whoever uses the device. Close it on the device and bring the \
         runner up again",
        describe(&screen.windows)
    )
}

fn an_app_can_be_read(screen: &Screen) -> bool {
    matches!(next_step(screen, None, false), Step::Ready)
}

/// The runner's windows and the platform's resumed activity, or `None`
/// when the runner cannot be asked.
fn read_screen(serial: &str, port: u16) -> Option<Screen> {
    let windows = parse_windows(&get_body(port, "/windows").ok()?)?;
    Some(Screen {
        resumed: resumed_package(serial),
        windows,
    })
}

fn wait_until(deadline: Deadline, mut done: impl FnMut() -> bool) -> bool {
    loop {
        let look = deadline.look();
        if done() {
            return true;
        }
        if deadline.passed_by(look) {
            return false;
        }
        std::thread::sleep(LOOK_AGAIN);
    }
}

fn await_in_front(serial: &str, name: &str, deadline: Deadline) -> Result<(), Unsettled> {
    if wait_until(deadline, || {
        resumed_package(serial).as_deref() == Some(name)
    }) {
        return Ok(());
    }
    Err(Unsettled::App(format!(
        "{name} was started and is not the resumed activity {}s later — {} is",
        SETTLE.as_secs(),
        resumed_package(serial).unwrap_or_else(|| "nothing".to_string())
    )))
}

/// Start `name` from its launcher entry, restarting it first when
/// `relaunch`. The launcher intent brings an existing task to the front
/// rather than stacking a new activity on it — the same thing a tap on
/// the app's icon does.
fn start_app(serial: &str, port: u16, name: &str, relaunch: bool) -> Result<(), Unsettled> {
    let installed = adb(serial)
        .args(["shell", "pm", "path", name])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("package:"))
        .unwrap_or(false);
    if !installed {
        return Err(Unsettled::App(format!(
            "{name} is not installed on {serial}. Install it first: smix sim install \
             {serial} <path-to.apk>"
        )));
    }
    let resolved = adb(serial)
        .args([
            "shell",
            "cmd",
            "package",
            "resolve-activity",
            "--brief",
            "-c",
            "android.intent.category.LAUNCHER",
            name,
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let Some(activity) = smix_adb::parse_resolved_activity(&resolved, name) else {
        return Err(Unsettled::App(format!(
            "{name} is installed on {serial} and has no launcher activity to start"
        )));
    };
    if relaunch {
        let _ = adb(serial)
            .args(["shell", "am", "force-stop", name])
            .output();
        // Until the stopped instance's window has left the runner's list,
        // that window is what "in front" would be read from. Measured: a
        // relaunch answered while the old window was still listed, and the
        // runner's next read found only system UI.
        let gone = wait_until(Deadline::after(SETTLE), || {
            read_screen(serial, port).is_some_and(|s| !s.windows.iter().any(|w| w.package == name))
        });
        if !gone {
            return Err(Unsettled::App(format!(
                "{name} was stopped and its window was still listed {}s later",
                SETTLE.as_secs()
            )));
        }
    }
    let component = format!("{name}/{activity}");
    let out = adb(serial)
        .args([
            "shell",
            "am",
            "start",
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.LAUNCHER",
            "-n",
            &component,
        ])
        .output()
        .map_err(|e| Unsettled::App(format!("adb shell am start {component}: {e}")))?;
    let said = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || said.contains("Error") {
        return Err(Unsettled::App(format!(
            "am start {component} was refused: {}",
            said.trim()
        )));
    }
    Ok(())
}
