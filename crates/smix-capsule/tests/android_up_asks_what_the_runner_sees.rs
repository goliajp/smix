//! `runner up --platform android`, against a runner whose server answers
//! and whose view of the device is stale.
//!
//! The iOS half of this decision has been in `up_asks_the_session.rs`
//! since a consumer reported `runner up` calling a wedged runner healthy.
//! The Android half was never written: `runner_android::up` reads
//! `health_ok(port)` and nothing else, so it prints "already healthy" and
//! returns success no matter what that runner can see — and the `--force`
//! the shared CLI help promises ("without this flag it now says no and
//! names the fix; with it, it runs the fix") never reaches it, because
//! `up` does not take the flag at all.
//!
//! Reproduced on emulator-5554 before this was written: the foreground
//! was `dev.smix.fixture`, `/windows` listed only `com.android.systemui`
//! and another app installed hours earlier, and
//! `/tree` carried no package at all. `runner up` — with and without
//! `--force` — said "already healthy" both times.
//!
//! Note which predicate that rules out. `automation_sees_an_app` asks
//! whether any readable application window is attached; the stale
//! consumer window is type 1 and readable, so it answers yes. Seeing *an*
//! app is not seeing *this device*. The question that separates them
//! needs a second, independent source: what the device says is in front.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use smix_capsule::runner_android::runner_view_is_current;

/// A runner-shaped server whose `/windows` we choose.
fn stub(windows_body: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut sock) = conn else { return };
            let mut scratch = [0u8; 2048];
            let n = sock.read(&mut scratch).unwrap_or(0);
            let line = String::from_utf8_lossy(&scratch[..n])
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            log.lock().expect("stub log").push(line.clone());
            let (status, body) = if line.contains("/health") {
                (200, r#"{"ok":true}"#.to_string())
            } else if line.contains("/windows") {
                (200, windows_body.to_string())
            } else {
                (404, r#"{"ok":false}"#.to_string())
            };
            let _ = write!(
                sock,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.flush();
            // The real runner says `Connection: close` and then keeps the
            // socket open. A reader that waits for EOF waits for its own
            // timeout instead, and every one of these predicates then
            // answers "cannot tell" against every real device — which is
            // how the one already in the file has been passing since it
            // was written. Holding the socket here is what makes these
            // tests describe the runner that exists.
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    });
    (port, seen)
}

/// What emulator-5554 actually served while the fixture was in front.
const STALE_VIEW: &str = concat!(
    r#"{"ok":true,"windows":["#,
    r#"{"package":"com.android.systemui","type":3,"rootReadable":true},"#,
    r#"{"package":"com.other.app","type":1,"rootReadable":true}"#,
    r#"]}"#
);

const CURRENT_VIEW: &str = concat!(
    r#"{"ok":true,"windows":["#,
    r#"{"package":"com.android.systemui","type":3,"rootReadable":true},"#,
    r#"{"package":"dev.smix.fixture","type":1,"rootReadable":true}"#,
    r#"]}"#
);

const NOTHING_BUT_SYSTEM: &str = concat!(
    r#"{"ok":true,"windows":["#,
    r#"{"package":"com.android.systemui","type":3,"rootReadable":true}"#,
    r#"]}"#
);

#[test]
fn a_runner_that_sees_the_foreground_is_current() {
    let (port, _seen) = stub(CURRENT_VIEW);
    assert_eq!(
        runner_view_is_current(port, "dev.smix.fixture"),
        Ok(()),
        "the path that was never broken has to stay exactly as it was"
    );
}

#[test]
fn a_runner_seeing_only_some_other_app_is_stale() {
    let (port, _seen) = stub(STALE_VIEW);
    let Err(message) = runner_view_is_current(port, "dev.smix.fixture") else {
        panic!(
            "the runner listed com.other.app while \
             dev.smix.fixture was in front, and this said its view was \
             current — that is the emulator-5554 reproduction, and it is \
             exactly what `automation_sees_an_app` cannot tell apart, \
             because the stale window is a readable application window"
        );
    };
    for wanted in ["dev.smix.fixture", "com.other.app"] {
        assert!(
            message.contains(wanted),
            "the refusal has to name both what is in front and what the \
             runner sees instead, or it cannot be acted on: {message}"
        );
    }
}

#[test]
fn a_runner_seeing_nothing_but_system_windows_is_stale() {
    let (port, _seen) = stub(NOTHING_BUT_SYSTEM);
    assert!(
        runner_view_is_current(port, "dev.smix.fixture").is_err(),
        "this is the shape the consumer reported — a tree carrying only \
         com.android.systemui — and it has to be refused too"
    );
}

const NO_WINDOWS_AT_ALL: &str = r#"{"ok":true,"windows":[]}"#;

const NO_WINDOWS_FIELD: &str = r#"{"ok":true}"#;

#[test]
fn a_runner_that_sees_no_windows_at_all_is_stale() {
    // Observed on emulator-5554 after a forced replacement: /health 200,
    // `dumpsys accessibility` reporting `Bound services:{}`, and /windows
    // an empty list. The HTTP face is alive and the sensing face is dead,
    // which is the worst shape of this bug — an empty list is not a
    // missing answer, it is the runner saying it sees nothing.
    let (port, _seen) = stub(NO_WINDOWS_AT_ALL);
    assert!(
        runner_view_is_current(port, "dev.smix.fixture").is_err(),
        "an empty window list is the runner answering, and the answer is \
         that it is blind"
    );
}

#[test]
fn a_runner_too_old_for_the_route_is_not_called_stale() {
    // No `windows` key at all: the route is not served. That is not an
    // answer about the device, so it cannot be read as a bad one.
    let (port, _seen) = stub(NO_WINDOWS_FIELD);
    assert_eq!(
        runner_view_is_current(port, "dev.smix.fixture"),
        Ok(()),
        "a runner that does not serve the route has not been shown to be stale"
    );
}

#[test]
fn an_unreachable_runner_is_not_called_stale() {
    // Nothing is listening here. An unknown answer is not a failing one:
    // this predicate exists to catch a runner that is demonstrably behind,
    // not to invent a verdict when it cannot ask.
    assert_eq!(
        runner_view_is_current(1, "dev.smix.fixture"),
        Ok(()),
        "a runner that cannot be asked has not been shown to be stale"
    );
}

/// The device half of the comparison, parsed from what `dumpsys activity
/// activities` prints. Kept separate from the adb call so the shapes it
/// has to survive can be stated here rather than discovered on a device.
mod foreground {
    use smix_capsule::runner_android::parse_resumed_package;

    #[test]
    fn reads_the_package_out_of_a_resumed_activity_line() {
        let dump = "  ResumedActivity: ActivityRecord{d752f8a u0 \
                    dev.smix.fixture/.MainActivity} t1379}";
        assert_eq!(
            parse_resumed_package(dump),
            Some("dev.smix.fixture".to_string())
        );
    }

    #[test]
    fn prefers_the_top_resumed_activity_when_both_are_present() {
        // The two lines name different packages, and the fallback's line
        // comes first in the text — so a reader that ignored the
        // preference, or matched "ResumedActivity" loosely enough to hit
        // the top line's tail, would answer launcher here.
        let dump = "\
  ResumedActivity: ActivityRecord{aaa u0 com.android.launcher3/.Launcher} t734}
    topResumedActivity=ActivityRecord{1b9c118 u0 dev.smix.fixture/.MainActivity} t1590}";
        assert_eq!(
            parse_resumed_package(dump),
            Some("dev.smix.fixture".to_string()),
            "topResumedActivity is what is in front when both report"
        );
    }

    #[test]
    fn falls_back_to_the_plain_line_when_there_is_no_top_one() {
        let dump = "  ResumedActivity: ActivityRecord{aaa u0 \
                    com.android.launcher3/.Launcher} t734}";
        assert_eq!(
            parse_resumed_package(dump),
            Some("com.android.launcher3".to_string())
        );
    }

    #[test]
    fn says_nothing_rather_than_guessing_when_there_is_no_such_line() {
        assert_eq!(
            parse_resumed_package("Activity Resolver Table:\n  none"),
            None
        );
    }

    #[test]
    fn says_nothing_on_a_line_it_cannot_split() {
        // A malformed record must not yield half a package name — an
        // empty foreground is what makes the comparison stand down.
        assert_eq!(
            parse_resumed_package("ResumedActivity: ActivityRecord{}"),
            None
        );
        // No slash anywhere: a reader that fell back to the whole field
        // would answer "u0" here, which is not a package.
        assert_eq!(
            parse_resumed_package("ResumedActivity: ActivityRecord{aaa u0 noslash} t1}"),
            None
        );
    }
}
