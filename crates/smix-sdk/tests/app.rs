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
    let runner = HttpRunnerClient::with_base(server.uri());
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
    let _: smix_sdk::Selector = text("Login");
    let _: smix_sdk::Selector = text_regex("^Lo");
    let _: smix_sdk::Selector = id("btn-x");
    let _: smix_sdk::Selector = label("Settings");
    let _: smix_sdk::Selector = role(Role::Button);
    let _: smix_sdk::Selector = role_named(Role::Button, "Submit");
    let _: smix_sdk::Selector = focused();
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
            "exists": true
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
