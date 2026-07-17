//! v2 break #1: the iOS driving boundary requires a live runner session.
//!
//! An iOS `App` with no session id must fail every sense/act verb with a
//! named error (not a silent legacy per-request drive). Android has no
//! `/session/*` route and no session concept, so the same guard must NOT
//! fire on an Android `App` — it drives sessionless as before.

use smix_screen::{A11yNode, Rect};
use smix_sdk::{
    AndroidDeviceControl, AndroidDriver, App, FailureCode, HttpRunnerClient, SimctlClient,
    SimctlDriver, text,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn a_tree() -> A11yNode {
    A11yNode {
        raw_type: "application".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 390.0,
            h: 844.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

/// iOS App, no session set → `tap` short-circuits at the guard with a
/// named DriverError mentioning the session, before any wire call.
#[tokio::test]
async fn ios_no_session_tap_is_named_error() {
    // Base URL is never contacted: the guard fires before the driver
    // reaches the runner.
    let runner = HttpRunnerClient::with_base("http://127.0.0.1:1");
    let driver = SimctlDriver::new(runner);
    let app = App::new(driver, SimctlClient::new());

    let err = app.tap(&text("Login")).await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    assert!(
        err.message.contains("session"),
        "expected a session-guard message, got: {}",
        err.message
    );
}

/// iOS App, no session → the guard fires on a sense verb too (`tree`),
/// proving it is at the shared driving chokepoint, not one verb.
#[tokio::test]
async fn ios_no_session_tree_is_named_error() {
    let runner = HttpRunnerClient::with_base("http://127.0.0.1:1");
    let driver = SimctlDriver::new(runner);
    let app = App::new(driver, SimctlClient::new());

    let err = app.tree().await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    assert!(err.message.contains("session"), "got: {}", err.message);
}

/// iOS App, session stamped → the guard passes and the verb reaches the
/// wire (mock 200), confirming the guard gates on session presence only.
#[tokio::test]
async fn ios_with_session_drives() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_tree()))
        .mount(&server)
        .await;
    let mut runner = HttpRunnerClient::with_base(server.uri());
    runner.set_session_id("sess-1");
    let driver = SimctlDriver::new(runner);
    let app = App::new(driver, SimctlClient::new());

    app.tree().await.expect("iOS with session should drive");
}

/// Android App, no session → the guard must NOT fire (Android has no
/// session concept). The verb reaches the Kotlin-runner wire (same
/// `/tree` route) and succeeds against the mock.
#[tokio::test]
async fn android_no_session_drives() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_tree()))
        .mount(&server)
        .await;
    // No session id set — sessionless is Android's only path.
    let runner = HttpRunnerClient::with_base(server.uri());
    let app = App::new_with(
        Box::new(AndroidDriver::new(runner)),
        Box::new(AndroidDeviceControl::new()),
    );

    app.tree()
        .await
        .expect("Android drives sessionless — guard must not fire");
}
