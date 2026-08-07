//! The flow's own `appId:` must be what the session opens for.
//!
//! `FlowArgs.bundle_id`'s doc said "Yaml header `appId:` overrides per
//! flow" while `run_flow` opened the session and foregrounded BEFORE
//! parsing the yaml, using only the CLI-supplied bundle — which the CLI
//! defaulted to the literal placeholder `com.example.app`. Net effect:
//! the README quickstart form (`smix run flow.yaml --device X`, no
//! `--bundle-id`) could not drive any real app, ever. Found by running
//! it on a simulator, not by any of the 800 tests.

use smix_adapter_maestro::{FlowArgs, FlowPlatform, OutputFormat, run_flow};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn args_for(flow: std::path::PathBuf, port: u16, bundle_id: Option<String>) -> FlowArgs {
    FlowArgs {
        physical_ios: false,
        flow,
        udid: None,
        bundle_id,
        runner_port: port,
        animations: false,
        no_launch: false,
        platform: FlowPlatform::Ios,
        apps_config: None,
        env_vars: vec![],
        debug_output: None,
        verbose: false,
        format: OutputFormat::Human,
        auto_activate: false,
        metro_log_url: None,
        await_signal: None,
        gate_signal: None,
        gate_signal_timeout_ms: 60_000,
        expect_log_clean: false,
        fixture_registry: None,
        force_key_events: false,
        no_fail_annotate: false,
        auto_ocr_fallback: Some(false),
        ai_assertions: Some(false),
        assert_screenshot_no_autorecord: Some(false),
        launch_fresh_force_reinstall: Some(false),
    }
}

async fn mock_runner_expecting(bundle: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    // The proof: the session must open for the yaml's app, not a
    // placeholder. The matcher makes any other bundle a 404 → exit 6.
    Mock::given(method("POST"))
        .and(path("/session/open"))
        .and(body_partial_json(serde_json::json!({ "bundleId": bundle })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "sessionId": "sess-yaml-1",
            "activatedOnce": false,
            "serverTimeMs": 0
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/foreground"))
        .and(body_partial_json(serde_json::json!({ "bundleId": bundle })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/session/close"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .mount(&server)
        .await;
    server
}

fn write_flow(dir: &std::path::Path, app_id: &str) -> std::path::PathBuf {
    let path = dir.join("flow.yaml");
    // assertTrue is host-side expression eval — no wire, no UDID. The
    // flow body is irrelevant here; the session handshake is the subject.
    std::fs::write(
        &path,
        format!("appId: {app_id}\n---\n- assertTrue: ${{1 == 1}}\n"),
    )
    .unwrap();
    path
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_bundle_flag_the_yaml_app_id_opens_the_session() {
    let server = mock_runner_expecting("com.smix.yamlapp").await;
    let dir = std::env::temp_dir().join("smix-bundle-from-yaml-none");
    std::fs::create_dir_all(&dir).unwrap();
    let flow = write_flow(&dir, "com.smix.yamlapp");

    let exit = run_flow(args_for(flow, server.address().port(), None)).await;
    assert_eq!(
        format!("{exit:?}"),
        format!("{:?}", std::process::ExitCode::from(0)),
        "flow should pass with the session opened for the yaml appId"
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_explicit_bundle_flag_still_overrides_the_yaml() {
    let server = mock_runner_expecting("com.smix.flagged").await;
    let dir = std::env::temp_dir().join("smix-bundle-from-yaml-flag");
    std::fs::create_dir_all(&dir).unwrap();
    let flow = write_flow(&dir, "com.smix.yamlapp");

    let exit = run_flow(args_for(
        flow,
        server.address().port(),
        Some("com.smix.flagged".to_string()),
    ))
    .await;
    assert_eq!(
        format!("{exit:?}"),
        format!("{:?}", std::process::ExitCode::from(0)),
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_flag_and_no_app_id_is_a_loud_parse_time_error() {
    // No mocks mounted: the refusal must happen before any wire call.
    let server = MockServer::start().await;
    let dir = std::env::temp_dir().join("smix-bundle-from-yaml-empty");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("flow.yaml");
    std::fs::write(&path, "---\n- assertTrue: ${1 == 1}\n").unwrap();

    let exit = run_flow(args_for(path, server.address().port(), None)).await;
    assert_eq!(
        format!("{exit:?}"),
        format!("{:?}", std::process::ExitCode::from(2)),
        "a flow with no appId and no --bundle-id has no app to drive"
    );
    server.verify().await;
}
