//! Wiremock-based wire-level integration tests for HttpRunnerClient.
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
            "ok": true, "found": true
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

/// A burst names its count and cadence; an ordinary tap does not.
///
/// The test above passes unchanged, which is the point: adding bursts
/// left the bytes an ordinary tap puts on the wire exactly as they
/// were, so a runner that has never heard of one is unaffected.
#[tokio::test]
async fn tap_at_norm_coord_burst_posts_its_cadence() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(body_json(serde_json::json!({
            "nx": 0.5, "ny": 0.25, "times": 10, "intervalMs": 80
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    client
        .tap_at_norm_coord_burst(0.5, 0.25, 10, Some(80), None)
        .await
        .unwrap();
}

// ---- tap (selector + mode) ---------------------------------------------

#[tokio::test]
async fn tap_posts_selector_and_mode_resolve_camel_case() {
    let server = MockServer::start().await;
    // The request body is asserted exactly (mode serializes camelCase),
    // and the response is the shape TapRoute.success emits — the
    // previous version of this test carried "camel_case" in its name
    // while asserting neither, and mocked a hand-flattened all-null
    // body no runner ever sent.
    Mock::given(method("POST"))
        .and(path("/tap"))
        .and(body_json(serde_json::json!({
            "selector": {"text": "OK"},
            "mode": "resolve"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"matchedLabel":"OK","frame":{"x":20.00,"y":118.50,"w":353.00,"h":44.00},"appFrame":{"x":0.00,"y":0.00,"w":393.00,"h":852.00},"stages":{"resolveMs":12.5,"tapCallMs":0.0,"totalMs":12.5}}"#,
        ))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let result = client
        .tap(&text_sel("OK"), TapMode::Resolve, None)
        .await
        .expect("tap ok");
    // The whole point of mode=resolve is the frame coming back — assert
    // it survives the parse instead of being defaulted away.
    let frame = result.frame.expect("frame populated");
    assert_eq!((frame.x, frame.y), (20.0, 118.5));
    assert!(result.app_frame.is_some(), "appFrame lost");
    let stages = result.stages.expect("stages populated");
    assert!(stages.resolve_ms > 0.0, "resolveMs defaulted to zero");
    assert_eq!(result.matched_label.as_deref(), Some("OK"));
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
    // The field is `raw_code` (serde camelCase → `rawCode`), matching
    // the schema the Swift `EventRecorder` actually emits.
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

// -------------------- session lifecycle --------------------------------

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
            "ok": true, "found": true,
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

/// Matches only requests that do NOT carry the named header. wiremock
/// has no absence matcher, and without one the clear-session test below
/// passed no matter what the client sent.
struct NoHeader(&'static str);

impl wiremock::Match for NoHeader {
    fn matches(&self, request: &wiremock::Request) -> bool {
        !request.headers.contains_key(self.0)
    }
}

#[tokio::test]
async fn client_clear_session_id_stops_sending_header() {
    let server = MockServer::start().await;
    // The mock matches ONLY a request without Session-Id, and .expect(1)
    // makes the match mandatory — a client that keeps sending the header
    // matches nothing and fails verification on drop. The old version
    // discarded the result with `let _ =`, so it passed even when
    // clear_session_id was a no-op.
    Mock::given(method("POST"))
        .and(path("/find"))
        .and(header("app-bundle-id", "com.example.app"))
        .and(NoHeader("session-id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "found": false,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut client =
        HttpRunnerClient::with_base(server.uri()).with_target_bundle_id("com.example.app");
    client.set_session_id("sess-xyz");
    client.clear_session_id();
    let found = client
        .find(&text_sel("X"), None)
        .await
        .expect("request without session-id reaches the mock");
    assert!(!found);
    server.verify().await;
}

/// `exists` is a legacy alias no current runner emits (iOS sends
/// `found`; the Android runner serves no /find at all). The client
/// still accepts it; this is the one test that says so, so the alias's
/// only coverage is labeled as what it is instead of impersonating the
/// production field in every mock.
#[tokio::test]
async fn find_accepts_legacy_exists_alias() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/find"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "exists": true
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    assert!(client.find(&text_sel("Old"), None).await.unwrap());
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

// ---- sim-health feed --------------------------------------------------

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

/// A 200 body carrying `ok:false` is a refusal, not a success — eleven
/// act routes discarded the body entirely, so a tap whose terminal
/// synthesis failed (or whose handler crashed into the guarded
/// fallback) reported Ok end-to-end.
#[tokio::test]
async fn a_200_ok_false_body_is_an_error_not_a_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": false
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let err = client
        .tap_at_norm_coord(0.5, 0.5)
        .await
        .expect_err("ok:false must surface");
    assert!(
        format!("{err}").contains("ok:false"),
        "refusal must say what happened: {err}"
    );
}

// The webview bridge port stopped being a literal.
//
// `webview_eval` reaches an in-app bridge on the simulator's shared
// loopback — that host is the design, recorded when the Android half
// moved to the runner's proxy. The port was not: 28080 was written into
// the URL with nothing anywhere letting a user change it, so a runner
// on any other port could not be reached at all.
//
// Parsing takes an Option<&str> rather than reading the environment, so
// these run without mutating process state. A suite that sets env vars
// to test them is a suite that fails when run in parallel with itself.

#[test]
fn webview_bridge_port_defaults_to_28080() {
    assert_eq!(
        smix_runner_client::webview_bridge_port_from(None),
        smix_runner_client::DEFAULT_WEBVIEW_BRIDGE_PORT
    );
}

#[test]
fn webview_bridge_port_reads_override() {
    assert_eq!(
        smix_runner_client::webview_bridge_port_from(Some("29999")),
        29999
    );
}

/// Unparseable falls back rather than failing the call: the same shape
/// `runner_port_from_env` already uses for SMIX_RUNNER_PORT. Pinned here
/// so it reads as the convention it is, not as an accident.
#[test]
fn webview_bridge_port_ignores_unparseable() {
    assert_eq!(
        smix_runner_client::webview_bridge_port_from(Some("nonsense")),
        smix_runner_client::DEFAULT_WEBVIEW_BRIDGE_PORT
    );
}

#[test]
fn webview_bridge_url_uses_given_port() {
    assert_eq!(
        smix_runner_client::webview_bridge_url(29999),
        "http://127.0.0.1:29999/eval"
    );
}
