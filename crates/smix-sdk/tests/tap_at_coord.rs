//! App::tap_at_coord(nx, ny) escape hatch.
//! Test that the SDK surface emits POST /tap-at-norm-coord with the
//! normalized coords in the JSON body — end-to-end thin pipe from
//! SDK App → SimctlDriver → HttpRunnerClient → wire.

use smix_sdk::{App, FailureCode, HttpRunnerClient, SimctlClient, SimctlDriver};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mk_app_for(server: &MockServer) -> App {
    let runner = HttpRunnerClient::with_base(server.uri());
    let driver = SimctlDriver::new(runner);
    App::new(driver, SimctlClient::new())
}

#[tokio::test]
async fn tap_at_coord_emits_normalized_wire() {
    let server = MockServer::start().await;
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
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    let err = app.tap_at_coord(0.5, 0.5).await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
}
