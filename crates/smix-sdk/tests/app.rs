//! Smix-sdk App + selector helper tests via wiremock.

use smix_screen::{A11yNode, Rect};
use smix_sdk::{
    App, FailureCode, HttpRunnerClient, KeyName, Role, SimctlClient, SimctlDriver, SwipeDirection,
    focused, id, label, role, role_named, text, text_regex,
};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mk_app_for(server: &MockServer) -> App {
    let mut runner = HttpRunnerClient::with_base(server.uri());
    // iOS driving requires a live session (v2 break #1); stamp one so
    // these SDK→driver→wire pipe tests exercise the session-bound path.
    runner.set_session_id("test-session");
    let driver = SimctlDriver::new(runner);
    App::new(driver, SimctlClient::new())
}

fn login_tree() -> A11yNode {
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
        children: vec![A11yNode {
            raw_type: "button".into(),
            element_type_raw: 1,
            role: Some(Role::Button),
            identifier: Some("btn-login".into()),
            label: Some("Login".into()),
            title: None,
            placeholder_value: None,
            value: None,
            text: None,
            bounds: Rect {
                x: 50.0,
                y: 100.0,
                w: 200.0,
                h: 40.0,
            },
            enabled: true,
            selected: false,
            has_focus: false,
            visible: true,
            children: vec![],
        }],
    }
}

// ---- selector ergonomic factories ---------------------------------------

#[test]
fn ergonomic_factories_build_correct_selector_variants() {
    // Asserted by the wire JSON each factory encodes to — the previous
    // version was seven `let _:` bindings, i.e. a compile check wearing
    // a test's name.
    let cases: &[(smix_sdk::Selector, serde_json::Value)] = &[
        (text("Login"), serde_json::json!({"text": "Login"})),
        (id("btn-x"), serde_json::json!({"id": "btn-x"})),
        (label("Settings"), serde_json::json!({"label": "Settings"})),
        (role(Role::Button), serde_json::json!({"role": "button"})),
        (
            role_named(Role::Button, "Submit"),
            serde_json::json!({"role": "button", "name": "Submit"}),
        ),
        (focused(), serde_json::json!({"focused": true})),
    ];
    for (sel, wire) in cases {
        assert_eq!(&serde_json::to_value(sel).expect("encodes"), wire);
    }
    let regex_wire = serde_json::to_value(text_regex("^Lo")).expect("encodes");
    assert_eq!(regex_wire["text"]["regex"], "^Lo");
}

// ---- App.tree / find_one / find ----------------------------------------

#[tokio::test]
async fn app_tree_uses_driver() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(login_tree()))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    let tree = app.tree().await.expect("tree");
    assert_eq!(tree.children.len(), 1);
}

#[tokio::test]
async fn app_find_one_resolves() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(login_tree()))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    let node = app.find_one(&text("Login")).await.expect("ok");
    assert!(node.is_some());
    assert_eq!(node.unwrap().identifier.as_deref(), Some("btn-login"));
}

#[tokio::test]
async fn app_find_quick_probe() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/find"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "found": true
        })))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    assert!(app.find(&text("Login")).await.unwrap());
}

// ---- App.tap full pipeline ---------------------------------------------

#[tokio::test]
async fn app_tap_full_pipeline() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(login_tree()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    app.tap(&text("Login")).await.expect("tap ok");
}

// ---- App.wait_for + assert_visible ------------------------------------

#[tokio::test]
async fn app_wait_for_immediate_hit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(login_tree()))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    let node = app
        .wait_for(&text("Login"), Duration::from_millis(500))
        .await
        .expect("wait ok");
    assert_eq!(node.label.as_deref(), Some("Login"));
}

#[tokio::test]
async fn app_assert_visible_passes_on_hit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(login_tree()))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    app.assert_visible(&text("Login")).await.expect("visible");
}

#[tokio::test]
async fn app_assert_text_shortcut() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(login_tree()))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    app.assert_text("Login").await.expect("text visible");
}

#[tokio::test]
async fn app_assert_enabled_distinguishes_disabled() {
    let server = MockServer::start().await;
    // tree where Login button is enabled=false.
    let mut tree = login_tree();
    tree.children[0].enabled = false;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    let err = app.assert_enabled(&text("Login")).await.unwrap_err();
    assert_eq!(err.code, FailureCode::NotEnabled);
}

// ---- App passthroughs --------------------------------------------------

#[tokio::test]
async fn app_press_key_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/press-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    app.press_key(KeyName::Return).await.unwrap();
}

#[tokio::test]
async fn app_swipe_once_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/swipe-once"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    app.swipe_once(SwipeDirection::Up).await.unwrap();
}

#[tokio::test]
async fn app_go_back_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/back"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let app = mk_app_for(&server);
    app.go_back().await.unwrap();
}

// ---- App.launch requires UDID -----------------------------------------

#[tokio::test]
async fn app_launch_requires_udid() {
    let server = MockServer::start().await;
    let app = mk_app_for(&server);
    let err = app.launch("com.example").await.unwrap_err();
    assert_eq!(err.code, FailureCode::DriverError);
    assert!(err.message.contains("UDID"));
}

/// The quickstart's own final step used to die here with a raw
/// `xcrun simctl get_app_container ... exited 2 ... No such file or
/// directory` — a bare subprocess error for the single most common
/// new-user mistake (running a flow whose appId names an app that is
/// not installed). The failure must say what is wrong and what to type
/// next, in the AI-readable shape everything else uses.
#[test]
fn an_uninstalled_app_reads_as_app_not_installed_not_a_subprocess_error() {
    let e = smix_simctl::DeviceControlError::AppNotInstalled {
        bundle_id: "com.example.app".into(),
        udid: "5D087114-ECB3-443C-8DDB-40EEF9CFB90C".into(),
    };
    let f = smix_sdk::simctl_to_failure(e);
    assert_eq!(f.code, smix_sdk::FailureCode::AppNotRunning);
    let prompt = f.to_prompt();
    assert!(
        prompt.contains("com.example.app") && prompt.contains("not installed"),
        "must name the bundle and the condition: {prompt}"
    );
    assert!(
        prompt.contains("smix sim install"),
        "must say what to type next: {prompt}"
    );
}

/// The MCP server used to die at spawn when the runner was not up yet —
/// but MCP clients launch their servers at client startup, long before
/// anyone has typed `smix runner up`, so the documented Claude Code
/// config produced a dead server for the whole session. Lazy
/// construction defers the probe to the first real call, which already
/// reports unreachable with an actionable hint.
#[tokio::test]
async fn a_lazily_connected_app_works_once_the_runner_exists() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/find"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "found": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/session/open"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "sessionId": "sess-lazy", "activatedOnce": false, "serverTimeMs": 0
        })))
        .mount(&server)
        .await;
    // Mirror the MCP server's real order: construct lazily, then the
    // first tool call (launch_app) opens the session, then sense works.
    let mut app = smix_sdk::App::connect_to_runner_lazy(server.address().port());
    app.open_session_in_place("com.example.app", false)
        .await
        .expect("session opens once the runner exists");
    assert!(app.find(&text("Login")).await.unwrap());
}

#[tokio::test]
async fn a_lazily_connected_app_reports_unreachable_on_first_use_not_at_birth() {
    // Port 1 answers nothing. Construction must succeed; the first
    // real call — session open, same as MCP's launch_app — must fail
    // with the runner-unreachable story.
    let mut app = smix_sdk::App::connect_to_runner_lazy(1);
    let err = app
        .open_session_in_place("com.example.app", false)
        .await
        .expect_err("no runner");
    let prompt = err.to_prompt();
    assert!(
        prompt.contains("unreachable") || prompt.contains("not reachable"),
        "first use must tell the runner story: {prompt}"
    );
}
