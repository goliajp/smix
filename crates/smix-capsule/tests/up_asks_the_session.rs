//! `runner up`, against a runner whose server answers and whose session
//! does not.
//!
//! This is the consumer's report, in a temp directory: seed the record
//! `runner up` reads, put something on the port that says 200 to
//! `/health` and 500 to `/tree`, and run the real decision. Before this
//! checkpoint it printed `runner already up` and returned success — the
//! only command that could recover the runner refusing to, three times
//! in one day.
//!
//! `up_on` rather than the `smix` binary: the CLI boots a simulator and
//! resolves the device against the registry before it reaches this
//! decision, so driving it end to end would need a real device to test a
//! judgement that has nothing to do with one. The flag that reaches this
//! from the command line is checked in smix-cli's own suite.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

use smix_capsule::runner::{RunnerState, RunnerTarget, UpOptions, up_on};
use smix_capsule::runner_state::{Platform, write as write_state};

/// A runner-shaped server: healthy, with a `/tree` we choose, and a
/// `/soft-cycle` that works. It records the request lines it served, so
/// a test can say what recovery actually did rather than that it
/// returned Ok.
fn stub(tree_status: u16, tree_body: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut sock) = conn else { return };
            let mut scratch = [0u8; 2048];
            let n = sock.read(&mut scratch).unwrap_or(0);
            let request = String::from_utf8_lossy(&scratch[..n]).to_string();
            let line = request.lines().next().unwrap_or("").to_string();
            log.lock().expect("stub log").push(line.clone());
            let (status, body) = if line.contains("/health") {
                (
                    200,
                    r#"{"ok":true,"runnerVersion":"4.3.0","wireSchema":{"supports":[1,2]}}"#
                        .to_string(),
                )
            } else if line.contains("/soft-cycle") {
                (200, r#"{"ok":true}"#.to_string())
            } else if line.contains("/tree") {
                (tree_status, tree_body.to_string())
            } else {
                (404, r#"{"ok":false}"#.to_string())
            };
            let _ = write!(
                sock,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, seen)
}

const HEALTHY_TREE: &str = r#"{"ok":true,"tree":{"role":"app","children":[]}}"#;
const DEAD_SESSION: &str = concat!(
    r#"{"ok":false,"error":"snapshot_unavailable","reason":"not-running","#,
    r#""hint":"The app is not running - it was reinstalled out from under "#,
    r#"the runner. `smix runner cycle` to rebind."}"#
);

const UDID: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";
const BUNDLE: &str = "jp.golia.smix.fixture";

fn seed(root: &Path, port: u16) {
    write_state(
        root,
        Platform::Ios,
        &RunnerState {
            pid: std::process::id(),
            udid: UDID.to_string(),
            port,
            log: root.join("runner.log"),
            bundle: Some(BUNDLE.to_string()),
            supervisor_pid: None,
        },
    )
    .expect("seed the runner record");
}

fn run_up(root: &Path, port: u16, force_recover: bool) -> Result<(), String> {
    up_on(
        root,
        UDID,
        port,
        Some(BUNDLE),
        None,
        UpOptions {
            force_recover,
            ..Default::default()
        },
        RunnerTarget::Simulator,
    )
}

#[test]
fn a_working_session_still_reports_already_up() {
    let root = tempfile::tempdir().expect("tempdir");
    let (port, _seen) = stub(200, HEALTHY_TREE);
    seed(root.path(), port);
    assert_eq!(
        run_up(root.path(), port, false),
        Ok(()),
        "the path that was never broken has to stay exactly as it was"
    );
}

#[test]
fn a_dead_session_is_refused_rather_than_reported_as_up() {
    let root = tempfile::tempdir().expect("tempdir");
    let (port, _seen) = stub(500, DEAD_SESSION);
    seed(root.path(), port);
    let Err(message) = run_up(root.path(), port, false) else {
        panic!(
            "`runner up` reported success against a runner whose session is \
             gone — this is the consumer's report, reproduced"
        );
    };
    for wanted in [
        "/health",
        "session",
        "not-running",
        "smix runner cycle",
        "--force",
    ] {
        assert!(
            message.contains(wanted),
            "the refusal has to carry {wanted:?}: {message}"
        );
    }
}

#[test]
fn force_recovers_our_own_wedged_runner_without_ending_it() {
    let root = tempfile::tempdir().expect("tempdir");
    let (port, seen) = stub(500, DEAD_SESSION);
    seed(root.path(), port);
    assert_eq!(
        run_up(root.path(), port, true),
        Ok(()),
        "--force on our own wedged runner recovers it"
    );
    let served = seen.lock().expect("stub log").clone();
    assert!(
        served.iter().any(|line| line.contains("/soft-cycle")),
        "recovery has to go through the in-place cycle rather than a kill; \
         the runner served: {served:?}"
    );
}

#[test]
fn the_attach_retry_does_not_reach_a_runner_that_is_not_ours() {
    let root = tempfile::tempdir().expect("tempdir");
    let (port, seen) = stub(500, DEAD_SESSION);
    // No record at all: the store has never heard of whatever is on this
    // port. C5 refuses that, and the timeout retry added in C6 must not
    // become a way around it — the retry lives past this branch, and a
    // test is what keeps it there.
    let Err(message) = run_up(root.path(), port, false) else {
        panic!("an unrecorded runner on the port is a stop, not a bring-up");
    };
    assert!(
        message.contains("--include-unrecorded"),
        "keep naming the sanctioned way through: {message}"
    );
    assert!(
        !message.contains("attach"),
        "whose runner this is was answered before any retry could apply: {message}"
    );
    let served = seen.lock().expect("stub log").clone();
    assert!(
        !served.iter().any(|line| line.contains("/soft-cycle")),
        "nothing should have been done to it: {served:?}"
    );
}
