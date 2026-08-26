//! Nothing answering the port is one problem, not one per step.
//!
//! A consumer's Android runner was killed mid-suite by the system under
//! memory pressure. Every step after that reported
//!
//!     [DRIVER_ERROR]: step 5 (tapOn): AndroidDriver::tree: runner /tree
//!     fetch failed: error sending request for url (http://…:22089/tree)
//!
//! seven times, which reads as seven steps each going wrong. reqwest's
//! sentence is about a socket; the reader needs the sentence about the
//! runner, and the difference is that one of them names a next step.
//!
//! Narrow on purpose, and that is what the second case holds: a runner
//! that IS there and slow must not be reported as gone, because
//! "restart it" is the wrong instruction for a timeout.

use smix_driver::{RunnerTransportError, transport_to_failure};

/// A connection refused: nothing is listening.
async fn connect_refused() -> reqwest::Error {
    // Port 1 on loopback: nothing binds it, so the connect fails rather
    // than the request timing out.
    reqwest::Client::new()
        .get("http://127.0.0.1:1/tree")
        .send()
        .await
        .expect_err("nothing listens on port 1")
}

/// A read timeout against a socket that accepts and never answers.
async fn read_timeout() -> reqwest::Error {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        // Accept and hold: the client connects, then waits for a body
        // that never comes.
        let _held = listener.accept();
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(300))
        .build()
        .expect("client")
        .get(format!("http://127.0.0.1:{port}/tree"))
        .send()
        .await
        .expect_err("the server never answers")
}

#[tokio::test]
async fn a_refused_connection_names_the_runner_not_the_socket() {
    let failure = transport_to_failure(RunnerTransportError::FetchFailed {
        endpoint: "/tree".into(),
        source: connect_refused().await,
    });
    let said = failure.to_prompt();
    assert!(
        said.contains("nothing is listening"),
        "the reader needs the runner's story, not reqwest's — {said}"
    );
    assert!(
        said.contains("smix runner up"),
        "and a next step, or it is a diagnosis with nothing to do — {said}"
    );
}

#[tokio::test]
async fn a_runner_that_is_merely_slow_is_not_called_gone() {
    let failure = transport_to_failure(RunnerTransportError::FetchFailed {
        endpoint: "/tree".into(),
        source: read_timeout().await,
    });
    let said = failure.to_prompt();
    assert!(
        !said.contains("nothing is listening"),
        "a runner that accepted the connection is there; telling anyone to \
         restart it sends them the wrong way — {said}"
    );
}

#[test]
fn the_two_keyboard_refusals_point_opposite_ways() {
    // "It would not close" and "we never established whether it is up"
    // are not the same finding, and a reader acts on them differently —
    // one is about the screen, the other about the runner. They reached
    // a consumer as the same sentence.
    let did_not_close = transport_to_failure(RunnerTransportError::RefusedNaming {
        endpoint: "/hide-keyboard".into(),
        kind: "keyboard_did_not_close".into(),
        saw: "tried key:Return, tap-above, swipe-down; focus: input-password".into(),
    });
    let unknown = transport_to_failure(RunnerTransportError::RefusedNaming {
        endpoint: "/hide-keyboard".into(),
        kind: "keyboard_state_unknown".into(),
        saw: "XCUITest raised while dismissing".into(),
    });

    let a = did_not_close.to_prompt();
    let b = unknown.to_prompt();
    assert!(
        a.contains("input-password"),
        "what it saw has to survive — {a}"
    );
    assert!(
        a.contains("Tapping the next control"),
        "the screen case needs the screen's next step — {a}"
    );
    assert!(
        b.contains("not evidence it is still up"),
        "an exception must not read as the keyboard being up — {b}"
    );
    assert_ne!(a, b, "two findings that read identically are one finding");
}
