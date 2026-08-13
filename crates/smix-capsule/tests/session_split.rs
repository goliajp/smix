//! "The server is answering" and "the session still works" are two
//! questions, and until now only the first one was ever asked.
//!
//! A consumer hit the gap three times in one day: `/health` 200, `/tree`
//! dead, and `smix runner up` reading the first of those and returning
//! success without doing anything — so the only command that could have
//! recovered the runner was the one refusing to. Reproduced here in
//! shape: a stub that answers `/health` and fails `/tree` is exactly what
//! a runner looks like after `simctl install` overwrites the app it was
//! bound to.
//!
//! The stubs are hand-written TCP because the capsule's HTTP is: there is
//! no client crate on this side to reach for, and a stub built on one
//! would be testing a different transport than the one that ships.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use smix_capsule::runner::{
    AlreadyServing, RunnerState, SessionProbe, decide_already_serving, probe_session,
    probe_session_for,
};

/// The body a real runner sent at the moment this was reproduced —
/// `/health` 200 and `/tree` 500, in the same second, after the app was
/// reinstalled underneath it.
const SNAPSHOT_UNAVAILABLE: &str = concat!(
    r#"{"ok":false,"error":"snapshot_unavailable","reason":"not-running","#,
    r#""hint":"The app is not running - it was terminated, or reinstalled "#,
    r#"out from under the runner. Launch it again (`smix sim launch <device> "#,
    r#"<bundle-id>`, or `smix runner cycle` to rebind), then retry."}"#
);

enum Stub {
    /// Answer every request with this status and body.
    Answer(u16, &'static str),
    /// Accept, then close without writing a byte.
    HangUp,
    /// Accept and hold the socket open, saying nothing.
    Mute,
}

/// Bind an ephemeral port, serve one request the given way, return the port.
fn serve(stub: Stub) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let mut scratch = [0u8; 1024];
        let _ = sock.read(&mut scratch);
        match stub {
            Stub::Answer(status, body) => {
                let _ = write!(
                    sock,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
            Stub::HangUp => drop(sock),
            Stub::Mute => std::thread::sleep(Duration::from_secs(30)),
        }
    });
    port
}

/// Same, and hand back what the probe actually said.
///
/// The stub above only answers. Asking whether the probe named the app is
/// a question about the request, and a verdict-only assertion would pass
/// for an implementation that never sent a header at all.
fn serve_recording(status: u16, body: &'static str) -> (u16, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let heard = Arc::new(Mutex::new(String::new()));
    let write_down = Arc::clone(&heard);
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let mut scratch = [0u8; 2048];
        let n = sock.read(&mut scratch).unwrap_or(0);
        *write_down.lock().expect("request log") =
            String::from_utf8_lossy(&scratch[..n]).to_string();
        let _ = write!(
            sock,
            "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    (port, heard)
}

fn state(udid: &str, bundle: Option<&str>) -> RunnerState {
    RunnerState {
        pid: 4242,
        udid: udid.to_string(),
        port: 22087,
        log: PathBuf::from("/tmp/runner.log"),
        bundle: bundle.map(str::to_string),
        supervisor_pid: None,
    }
}

// ---------------------------------------------------------------- probe

#[test]
fn a_snapshot_that_comes_back_means_usable() {
    let port = serve(Stub::Answer(200, r#"{"ok":true,"tree":{"role":"app"}}"#));
    assert_eq!(probe_session(port), SessionProbe::Usable);
}

#[test]
fn the_runner_saying_it_cannot_snapshot_is_quoted_not_paraphrased() {
    let port = serve(Stub::Answer(500, SNAPSHOT_UNAVAILABLE));
    match probe_session(port) {
        SessionProbe::Gone { reason, hint } => {
            assert_eq!(reason, "not-running");
            assert!(
                hint.contains("reinstalled out from under the runner"),
                "the runner's own diagnosis has to survive the trip: {hint}"
            );
        }
        other => panic!("a 500 snapshot_unavailable is Gone, not {other:?}"),
    }
}

#[test]
fn a_runner_that_stops_answering_is_not_read_as_working() {
    let port = serve(Stub::HangUp);
    match probe_session(port) {
        SessionProbe::Silent { detail } => assert!(!detail.is_empty(), "Silent has to say why"),
        other => panic!("no answer at all is Silent, not {other:?}"),
    }
}

#[test]
fn a_probe_that_gets_no_answer_gives_a_verdict_rather_than_waiting() {
    let port = serve(Stub::Mute);
    let started = Instant::now();
    let verdict = probe_session(port);
    let waited = started.elapsed();
    assert!(
        matches!(verdict, SessionProbe::Silent { .. }),
        "a runner that never answers is Silent, not {verdict:?}"
    );
    // The deadline is the point. A probe without one inherits exactly the
    // failure it exists to detect: the typical way a session dies is that
    // the snapshot never returns, so a probe that waits for it hangs in
    // the one case it was written for.
    assert!(
        waited < Duration::from_secs(20),
        "the probe waited {waited:?} — it has no deadline of its own"
    );
}

// --------------------------------------------------------------- decide

#[test]
fn ours_and_working_is_reported_up() {
    let st = state("UDID-1", Some("com.example.app"));
    let verdict = decide_already_serving(
        Some(&st),
        22087,
        "UDID-1",
        Some("com.example.app"),
        &SessionProbe::Usable,
        false,
    );
    assert_eq!(verdict, AlreadyServing::ReportUp { pid: 4242 });
}

#[test]
fn ours_and_dead_refuses_with_both_facts_and_the_way_out() {
    let st = state("UDID-1", Some("com.example.app"));
    let probe = SessionProbe::Gone {
        reason: "not-running".into(),
        hint: "reinstalled out from under the runner".into(),
    };
    let AlreadyServing::Refuse { message } = decide_already_serving(
        Some(&st),
        22087,
        "UDID-1",
        Some("com.example.app"),
        &probe,
        false,
    ) else {
        panic!("a dead session must not be reported as up");
    };
    assert!(
        message.contains("/health"),
        "say what did answer: {message}"
    );
    assert!(
        message.contains("session"),
        "say what did not answer: {message}"
    );
    assert!(
        message.contains("not-running"),
        "the runner's own reason belongs in the sentence: {message}"
    );
    assert!(
        message.contains("smix runner cycle"),
        "name the command that recovers it: {message}"
    );
    assert!(
        message.contains("--force"),
        "name the flag that recovers it in place: {message}"
    );
}

#[test]
fn ours_and_silent_is_the_same_refusal() {
    let st = state("UDID-1", None);
    let probe = SessionProbe::Silent {
        detail: "connected, then nothing before the deadline".into(),
    };
    let verdict = decide_already_serving(Some(&st), 22087, "UDID-1", None, &probe, false);
    assert!(
        matches!(&verdict, AlreadyServing::Refuse { message } if message.contains("smix runner cycle")),
        "silence is not usability either: {verdict:?}"
    );
}

#[test]
fn ours_and_dead_with_force_recovers() {
    let st = state("UDID-1", Some("com.example.app"));
    let probe = SessionProbe::Gone {
        reason: "not-running".into(),
        hint: "reinstalled out from under the runner".into(),
    };
    let verdict = decide_already_serving(
        Some(&st),
        22087,
        "UDID-1",
        Some("com.example.app"),
        &probe,
        true,
    );
    assert!(
        matches!(&verdict, AlreadyServing::Recover { because } if because.contains("not-running")),
        "--force on our own dead runner recovers it, saying why: {verdict:?}"
    );
}

#[test]
fn force_does_not_unlock_the_ownership_judgement() {
    let theirs = state("SOMEONE-ELSE", Some("com.other.app"));
    let probe = SessionProbe::Gone {
        reason: "not-running".into(),
        hint: "…".into(),
    };
    let AlreadyServing::Refuse { message } = decide_already_serving(
        Some(&theirs),
        22087,
        "UDID-1",
        Some("com.example.app"),
        &probe,
        true,
    ) else {
        panic!("--force must not reach across to somebody else's runner");
    };
    assert!(
        message.contains("SOMEONE-ELSE"),
        "name whose it is: {message}"
    );
    assert!(
        message.contains("smix runner down"),
        "keep today's way out: {message}"
    );
}

#[test]
fn force_does_not_unlock_an_unrecorded_runner_either() {
    let probe = SessionProbe::Gone {
        reason: "not-running".into(),
        hint: "…".into(),
    };
    let AlreadyServing::Refuse { message } =
        decide_already_serving(None, 22087, "UDID-1", Some("com.example.app"), &probe, true)
    else {
        panic!("--force must not kill a runner the store has never heard of");
    };
    assert!(
        message.contains("--include-unrecorded"),
        "keep today's way out: {message}"
    );
}

// ------------------------------------------------- naming the app

#[test]
fn the_probe_names_the_app_it_is_asking_about() {
    let (port, heard) = serve_recording(200, r#"{"ok":true,"tree":{"role":"app"}}"#);
    let _ = probe_session_for(port, Some("com.example.app"));
    let request = heard.lock().expect("request log").clone();
    assert!(
        request.contains("App-Bundle-Id: com.example.app"),
        "the probe asked about whatever the runner happened to be bound to, \
         not about the app it was told to check: {request:?}"
    );
}

#[test]
fn a_named_app_that_cannot_be_snapshotted_is_gone() {
    let (port, _heard) = serve_recording(500, SNAPSHOT_UNAVAILABLE);
    match probe_session_for(port, Some("com.example.app")) {
        SessionProbe::Gone { reason, .. } => assert_eq!(reason, "not-running"),
        other => panic!("a 500 snapshot_unavailable is Gone, not {other:?}"),
    }
}

#[test]
fn not_naming_an_app_sends_no_name_rather_than_an_empty_one() {
    let (port, heard) = serve_recording(200, r#"{"ok":true,"tree":{"role":"app"}}"#);
    let _ = probe_session_for(port, None);
    let request = heard.lock().expect("request log").clone();
    // An implementation that always sends the header, empty when there is
    // no name, would make the first case above true for free — the runner
    // reads an empty one as absent, so nothing would break and nothing
    // would be checked either.
    assert!(
        !request.contains("App-Bundle-Id"),
        "no app was named and the probe named one anyway: {request:?}"
    );
}
