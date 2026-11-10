//! App::tap_at_coord(nx, ny) escape hatch.
//! Test that the SDK surface emits POST /tap-at-norm-coord with the
//! normalized coords in the JSON body — end-to-end thin pipe from
//! SDK App → SimctlDriver → HttpRunnerClient → wire.

use smix_sdk::{App, FailureCode, HttpRunnerClient, SimctlClient, SimctlDriver};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The wait before a point touch reads the tree and screenshots: a framed
/// app on a screen that does not move.
async fn a_still_screen(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rawType": "application", "enabled": true, "selected": false,
            "hasFocus": false, "visible": true,
            "bounds": {"x": 0.0, "y": 0.0, "w": 390.0, "h": 844.0}, "children": []
        })))
        .mount(server)
        .await;
    let mut png = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png, 8, 8);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&[90; 64])
            .unwrap();
    }
    Mock::given(method("GET"))
        .and(path("/screenshot"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png))
        .mount(server)
        .await;
}

fn mk_app_for(server: &MockServer) -> App {
    let mut runner = HttpRunnerClient::with_base(server.uri());
    // iOS driving requires a live session (v2 break #1).
    runner.set_session_id("test-session");
    let driver = SimctlDriver::new(runner);
    App::new(driver, SimctlClient::new())
}

#[tokio::test]
async fn tap_at_coord_emits_normalized_wire() {
    let server = MockServer::start().await;
    a_still_screen(&server).await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(body_partial_json(
            serde_json::json!({ "nx": 0.5, "ny": 0.95 }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .expect(1)
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    app.tap_at_coord(0.5, 0.95)
        .await
        .expect("tap_at_coord should round-trip wire OK");
}

#[tokio::test]
async fn tap_at_coord_maps_runner_error_to_expectation_failure() {
    // Runner returns 500 — SDK MUST surface as ExpectationFailure
    // (DriverError code), not bare RunnerTransportError, keeping the
    // SDK error mapping invariant uniform with tap/fill/clear/etc.
    let server = MockServer::start().await;
    a_still_screen(&server).await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    let err = app.tap_at_coord(0.5, 0.5).await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    // the failure is the touch's, not the wait's
    let received = server.received_requests().await.expect("recording is on");
    assert!(
        received
            .iter()
            .any(|r| r.url.path() == "/tap-at-norm-coord")
    );
}
