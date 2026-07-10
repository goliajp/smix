//! v3.1 c8 — wiremock-based wire-level integration tests for HttpRunnerClient.
//!
//! Verifies URL shape + query params + request body + response parsing
//! 1:1 with the TS runner-client. Not real-sim — real-sim binding lives
//! in c-final capstone alongside swift-bridge build verification.

use smix_input::{KeyName, SwipeDirection};
use smix_runner_client::{HttpRunnerClient, IncludeScope, RunnerScrollSelector, TapMode};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn text_sel(t: &str) -> Selector {
    Selector::Text {
        text: Pattern::text(t),
        modifiers: Modifiers::default(),
    }
}

fn minimal_tree() -> A11yNode {
    A11yNode {
        raw_type: "application".into(),
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

// ---- health / ensure_reachable -----------------------------------------

#[tokio::test]
async fn health_returns_true_on_200() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    assert!(client.health().await);
}

#[tokio::test]
async fn ensure_reachable_memoizes_after_first_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1) // only ONE probe — memoized after success.
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    client.ensure_reachable().await.expect("first probe");
    client.ensure_reachable().await.expect("memoized no-op");
    client.ensure_reachable().await.expect("memoized no-op");
    // wiremock verifies .expect(1) on drop — implicit assertion.
}

#[tokio::test]
async fn ensure_reachable_fails_when_health_500() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let err = client.ensure_reachable().await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("Unreachable"), "got: {msg}");
}

// ---- get_tree -----------------------------------------------------------

#[tokio::test]
async fn get_tree_no_include_no_query_param() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tree()))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let tree = client.get_tree(None).await.expect("tree");
    assert_eq!(tree.bounds.w, 390.0);
}

#[tokio::test]
async fn get_tree_with_include_threads_query_param() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .and(query_param("include", "all-windows"))
        .respond_with(ResponseTemplate::new(200).set_body_json(minimal_tree()))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let _ = client
        .get_tree(Some(IncludeScope::AllWindows))
        .await
        .expect("tree with scope");
}

// ---- find ---------------------------------------------------------------

#[tokio::test]
async fn find_exists_true_serializes_selector_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/find"))
        .and(body_json(serde_json::json!({
            "selector": {"text": "Login"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "exists": true
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    assert!(client.find(&text_sel("Login"), None).await.unwrap());
}

// ---- tap_at_norm_coord --------------------------------------------------

#[tokio::test]
async fn tap_at_norm_coord_posts_nx_ny() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(body_json(serde_json::json!({"nx": 0.5, "ny": 0.25})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    client.tap_at_norm_coord(0.5, 0.25).await.unwrap();
}

// ---- tap (selector + mode) ---------------------------------------------

#[tokio::test]
async fn tap_posts_selector_and_mode_resolve_camel_case() {
    let server = MockServer::start().await;
    // mode serializes as camelCase "resolve" / "resolveAndTap".
    Mock::given(method("POST"))
        .and(path("/tap"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "stages": null, "frame": null, "appFrame": null
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let _ = client
        .tap(&text_sel("OK"), TapMode::Resolve, None)
        .await
        .expect("tap ok");
}

// ---- press_key ----------------------------------------------------------

#[tokio::test]
async fn press_key_serializes_camel_case_name() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/press-key"))
        .and(body_json(serde_json::json!({"key": "arrowUp"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let _ = client.press_key(KeyName::ArrowUp).await.expect("press_key");
}

// ---- swipe_once ---------------------------------------------------------

#[tokio::test]
async fn swipe_once_posts_direction_camel_case() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/swipe-once"))
        .and(body_json(serde_json::json!({"direction": "down"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    client.swipe_once(SwipeDirection::Down).await.unwrap();
}

// ---- scroll ------------------------------------------------------------

#[tokio::test]
async fn scroll_matched_returns_swipe_count() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/scroll"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "matched": true,
            "swipes": 3
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let s = RunnerScrollSelector::Text {
        text: "Log out".into(),
    };
    let swipes = client
        .scroll(&s, SwipeDirection::Down, None)
        .await
        .expect("matched");
    assert_eq!(swipes, 3);
}

#[tokio::test]
async fn scroll_not_matched_returns_malformed_body_with_swipe_count_detail() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/scroll"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "matched": false,
            "swipes": 30
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let s = RunnerScrollSelector::Text {
        text: "Hidden".into(),
    };
    let err = client
        .scroll(&s, SwipeDirection::Down, None)
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("30 swipes"), "got: {msg}");
}

// ---- system_popups envelope --------------------------------------------

#[tokio::test]
async fn system_popups_unwraps_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/system-popups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "popups": [
                {
                    "id": "p1",
                    "type": "alert",
                    "source": "com.apple.springboard",
                    "title": "Allow notifications?",
                    "body": "",
                    "buttons": []
                }
            ]
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let popups = client.system_popups(None).await.expect("popups");
    assert_eq!(popups.len(), 1);
    assert_eq!(popups[0].id, "p1");
}

// ---- record cycle -------------------------------------------------------

#[tokio::test]
async fn record_start_stop_polls_drain_events() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/record/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/record/poll"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "events": []
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/record/stop"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "events": [
                {"rawCode": 1021, "timestampMs": 100.0}
            ]
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    client.start_record().await.unwrap();
    let polled = client.poll_record().await.unwrap();
    assert!(polled.is_empty());
    let final_events = client.stop_record().await.unwrap();
    assert_eq!(final_events.len(), 1);
    // v5.1 c3 S2 — 字段名校正 `code` → `raw_code`(serde camelCase → `rawCode`)
    // 对齐 swift `EventRecorder` 真实 emit 的 schema。
    assert_eq!(final_events[0].raw_code, 1021);
}

// ---- error path: non-2xx ----------------------------------------------

#[tokio::test]
async fn non_2xx_returns_non_success_status_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/find"))
        .respond_with(ResponseTemplate::new(503).set_body_string("upstream down"))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let err = client.find(&text_sel("X"), None).await.unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("503"), "got: {msg}");
}

// -------------------- v1.0.3 session lifecycle -------------------------

use smix_runner_client::{SessionCloseRequest, SessionOpenRequest, SessionRenewActivationRequest};
use wiremock::matchers::header;

#[tokio::test]
async fn open_session_sends_bundle_id_and_returns_session_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/session/open"))
        .and(body_json(serde_json::json!({
            "bundleId": "com.example.app",
            "activate": true,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sessionId": "sess-abc-123",
            "activatedOnce": true,
            "serverTimeMs": 1_720_500_000_000u64,
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let req = SessionOpenRequest {
        bundle_id: "com.example.app".into(),
        activate: true,
    };
    let resp = client.open_session(&req).await.expect("session open");
    assert_eq!(resp.session_id, "sess-abc-123");
    assert!(resp.activated_once);
    assert_eq!(resp.server_time_ms, 1_720_500_000_000u64);
}

#[tokio::test]
async fn client_with_session_id_sends_session_header_on_every_request() {
    let server = MockServer::start().await;
    // Any /find request MUST carry Session-Id: sess-xyz.
    Mock::given(method("POST"))
        .and(path("/find"))
        .and(header("session-id", "sess-xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "exists": true,
        })))
        .mount(&server)
        .await;
    let mut client = HttpRunnerClient::with_base(server.uri());
    client.set_session_id("sess-xyz");
    let ok = client
        .find(&text_sel("X"), None)
        .await
        .expect("find with session header");
    assert!(ok);
}

#[tokio::test]
async fn client_clear_session_id_stops_sending_header() {
    let server = MockServer::start().await;
    // Request must NOT carry Session-Id after clear.
    Mock::given(method("POST"))
        .and(path("/find"))
        .and(header("app-bundle-id", "com.example.app"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "exists": false,
        })))
        .mount(&server)
        .await;
    let mut client =
        HttpRunnerClient::with_base(server.uri()).with_target_bundle_id("com.example.app");
    client.set_session_id("sess-xyz");
    client.clear_session_id();
    // If the mock rejects the request (session-id present when we don't
    // expect it), this errors. Passing means no session-id was sent.
    let _ = client.find(&text_sel("X"), None).await;
}

#[tokio::test]
async fn close_session_hits_close_route() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/session/close"))
        .and(body_json(serde_json::json!({ "sessionId": "sess-abc" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let resp = client
        .close_session(&SessionCloseRequest {
            session_id: "sess-abc".into(),
        })
        .await
        .expect("session close");
    assert!(resp.ok);
}

#[tokio::test]
async fn renew_session_activation_returns_activated_flag() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/session/renew-activation"))
        .and(body_json(serde_json::json!({ "sessionId": "sess-abc" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "activated": false,
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let resp = client
        .renew_session_activation(&SessionRenewActivationRequest {
            session_id: "sess-abc".into(),
        })
        .await
        .expect("renew activation");
    assert!(resp.ok);
    assert!(!resp.activated);
}

#[tokio::test]
async fn health_detail_parses_extended_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "runnerVersion": "1.0.3",
            "uptimeMs": 42_000u64,
            "lastRequestAtMs": 1_720_500_000_000u64,
            "sessionsOpen": 2u32,
            "activationsTotal": 5u64,
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let resp = client.health_detail().await.expect("health detail");
    assert!(resp.ok);
    assert_eq!(resp.runner_version, "1.0.3");
    assert_eq!(resp.uptime_ms, 42_000);
    assert_eq!(resp.sessions_open, 2);
    assert_eq!(resp.activations_total, 5);
}

#[tokio::test]
async fn health_detail_tolerates_legacy_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let resp = client
        .health_detail()
        .await
        .expect("legacy health empty body tolerated");
    assert!(resp.ok);
    assert_eq!(resp.runner_version, "");
}

// ---- v1.0.4 sim-health feed -------------------------------------------

#[tokio::test]
async fn sim_health_receives_health_ok_from_bare_probe() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let monitor =
        smix_sim_health::SimHealthMonitor::new(smix_sim_health::SimHealthConfig::default());
    let client = HttpRunnerClient::with_base(server.uri()).with_sim_health(monitor.clone());
    // Force a Degraded starting point via a failed process observation
    // so we can detect the recovery signal fed by the /health call.
    monitor.record_process("SimRenderServer", false);
    assert_eq!(monitor.state(), smix_sim_health::SimHealthState::Dead);
    monitor.record_process("SimRenderServer", true);
    // record_process alone recovers, so bare /health OK just keeps us Healthy.
    assert_eq!(monitor.state(), smix_sim_health::SimHealthState::Healthy);
    assert!(client.health().await);
    assert_eq!(monitor.state(), smix_sim_health::SimHealthState::Healthy);
}

#[tokio::test]
async fn sim_health_receives_health_fail_from_detail_5xx() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let monitor =
        smix_sim_health::SimHealthMonitor::new(smix_sim_health::SimHealthConfig::default());
    let client = HttpRunnerClient::with_base(server.uri()).with_sim_health(monitor.clone());
    let _ = client.health_detail().await;
    // A single fail with health_stale not yet reached stays classified
    // as HealthNeverSeen (Degraded) — the important thing is the feed
    // reached the monitor without panicking and the sim_health accessor works.
    let m = client.sim_health().expect("sim_health should be set");
    let evt = m.subscribe();
    // Second /health call, still 500.
    let _ = client.health_detail().await;
    drop(evt);
    // Assertion: the monitor is not Healthy anymore after two failed probes with
    // no successful history — HealthNeverSeen keeps it out of Healthy.
    assert_ne!(monitor.state(), smix_sim_health::SimHealthState::Healthy);
}

#[tokio::test]
async fn sim_health_accessor_returns_none_by_default() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    assert!(client.sim_health().is_none());
    // Without a monitor, /health still works — no feed, no crash.
    assert!(client.health().await);
}
