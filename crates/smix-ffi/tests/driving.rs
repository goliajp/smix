//! The driving surface, checked against real HTTP.
//!
//! Every one of the three published SDKs tested its HTTP layer against an
//! injected mock, which proved the SDK agreed with itself and let a wire no
//! runner serves ship to three registries. So these go through wiremock: real
//! bytes, real sockets, and assertions about what the runner receives.
//!
//! Two of the three exist to catch things a signature cannot show.

use std::sync::Arc;
use std::time::Duration;

use smix_ffi::driving::{DriveError, SmixDriver};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn port_of(server: &MockServer) -> u16 {
    server.address().port()
}

/// The one that catches a UDL-declared async fn.
///
/// uniffi's UDL scaffolding never wraps the future in a tokio Compat — the
/// word `async_runtime` does not appear in uniffi_bindgen at all; only the
/// proc-macro export does it. The client is reqwest, so declared through UDL
/// this would panic when polled with no reactor, at runtime, with nothing to
/// catch it at compile time. A round trip that returns is the proof there is
/// a reactor under it.
#[tokio::test]
async fn a_call_reaches_the_runner_and_comes_back() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rawType": "other",
            "identifier": "root",
            "bounds": {"x": 0.0, "y": 0.0, "w": 390.0, "h": 844.0},
            "enabled": true,
            "selected": false,
            "hasFocus": false,
            "visible": true,
            "children": [],
        })))
        .mount(&server)
        .await;

    let driver = SmixDriver::new(port_of(&server));
    let tree = driver.tree(None).await.expect("the round trip returns");
    assert!(tree.contains("root"), "got: {tree}");
}

/// The one that catches a cancel that does not cancel.
///
/// uniffi 0.29.5 generates rust_future_cancel and neither the Swift nor the
/// Kotlin backend ever calls it, so Task.cancel() does nothing to a call in
/// flight. Cancellation is therefore explicit, and this holds it to meaning
/// what it says: the call ends because it was cancelled, while the server is
/// still holding the response.
#[tokio::test]
async fn cancel_ends_the_call_before_the_server_answers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;

    let driver = SmixDriver::new(port_of(&server));
    let token = driver.cancel_token();
    let handle = {
        let driver = Arc::clone(&driver);
        let token = Arc::clone(&token);
        tokio::spawn(async move { driver.tree(Some(token)).await })
    };

    tokio::time::sleep(Duration::from_millis(100)).await;
    token.cancel();

    let started = std::time::Instant::now();
    let result = handle.await.expect("the task finishes");
    assert!(
        matches!(result, Err(DriveError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it ended, but by waiting rather than by being cancelled"
    );
}

/// The one that makes a missing session unwriteable rather than a 400.
///
/// /session/launch-app requires a session id, and the SDKs that could not
/// talk to the runner had no session concept at all. The handle carries it,
/// so there is no call to make without one — and the runner sees it.
#[tokio::test]
async fn launching_goes_through_a_session_and_the_runner_sees_it() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/session/open"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"sessionId":"s-42","ok":true}"#),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/session/launch-app"))
        .and(wiremock::matchers::body_string_contains(
            r#""sessionId":"s-42""#,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
        .mount(&server)
        .await;

    let driver = SmixDriver::new(port_of(&server));
    let session = driver
        .open_session("com.example.app".to_string(), None)
        .await
        .expect("session opens");
    assert_eq!(session.id(), "s-42");
    session
        .launch_app(None)
        .await
        .expect("launch carries the session the runner asked for");
}

/// tap_by_id, the replacement for the old absolute-pixel tap: the SDK
/// resolves a selector to an id and taps that, with no coordinate leaving
/// the process.
#[tokio::test]
async fn tap_by_id_hits_the_runner() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tap-by-id"))
        .and(wiremock::matchers::body_string_contains(
            r#""id":"btn-login""#,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
        .mount(&server)
        .await;

    let session = open_session(&server).await;
    let hit = session
        .tap_by_id("btn-login".to_string(), None)
        .await
        .expect("tap reaches the runner");
    assert!(hit, "the runner said it hit");
}

/// The acting methods that carry no structured payload beyond what the
/// method name implies, each proven to reach its route with the session.
#[tokio::test]
async fn the_acting_methods_reach_their_routes() {
    let server = MockServer::start().await;
    for route in [
        "/tap-at-norm-coord",
        "/input-text",
        "/press-key",
        "/swipe-once",
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
            .mount(&server)
            .await;
    }

    let session = open_session(&server).await;
    session
        .tap_at_norm_coord(0.5, 0.5, None)
        .await
        .expect("tap-at-coord");
    session
        .input_text("hello".to_string(), None)
        .await
        .expect("input-text");
    session
        .press_key("return".to_string(), None)
        .await
        .expect("press-key");
    session
        .swipe_once("up".to_string(), None)
        .await
        .expect("swipe-once");
}

/// A key name the runner does not know is rejected here, before any HTTP —
/// the string boundary does not become a way to send nonsense downstream.
///
/// "Before any HTTP" is proven, not narrated: no `/press-key` mock is
/// mounted and the mock server verifies zero unexpected requests on
/// drop, so a version of `press_key` that POSTs first fails this test.
/// The error text is asserted too — a 404-shaped Transport error here
/// used to be indistinguishable from the local refusal.
#[tokio::test]
async fn an_unknown_key_is_refused_locally() {
    let server = MockServer::start().await;
    let session = open_session(&server).await;
    let err = session
        .press_key("banana".to_string(), None)
        .await
        .expect_err("no such key");
    let DriveError::Transport { detail } = &err else {
        panic!("expected Transport, got {err:?}");
    };
    assert!(
        detail.contains("banana"),
        "refusal must name the bad key so the caller can fix it: {detail}"
    );
    assert!(
        !detail.contains("404") && !detail.contains("status"),
        "refusal happened over HTTP, not locally: {detail}"
    );
    server.verify().await;
}

/// Shared helper: open a session against a server that answers /session/open.
async fn open_session(server: &MockServer) -> Arc<smix_ffi::driving::SmixSession> {
    Mock::given(method("POST"))
        .and(path("/session/open"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"sessionId":"s-1","ok":true}"#),
        )
        .mount(server)
        .await;
    SmixDriver::new(server.address().port())
        .open_session("com.example.app".to_string(), None)
        .await
        .expect("session opens")
}
