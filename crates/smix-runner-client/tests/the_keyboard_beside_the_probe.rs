//! A tree read through the probe keeps the windows that are not the app's.
//!
//! With the probe present `get_tree` used to hand back the probe's tree and
//! nothing else, so the keyboard — a window of the input method, never of
//! the app — was not in any tree a flow or a CLI verb could see.
//! `role:keyboard` timed out on every app that carried the probe while the
//! keyboard was on screen: a flow on 10.1.0, and from v10.2 every CLI verb
//! too, since they read the probe as well.

use smix_runner_client::{HttpRunnerClient, TreeSource};
use smix_screen::{A11yNode, Role};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn window(package: &str, kind: &str, focused: bool, role: Option<&str>) -> serde_json::Value {
    let mut w = serde_json::json!({
        "rawType": "android.widget.FrameLayout",
        "window": { "package": package, "kind": kind, "focused": focused },
        "bounds": { "x": 0.0, "y": 0.0, "w": 1080.0, "h": 2340.0 },
        "enabled": true, "selected": false, "hasFocus": false, "visible": true,
        "children": [],
    });
    if let Some(r) = role {
        w["role"] = serde_json::json!(r);
    }
    w
}

async fn screen_with_a_keyboard(probe: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/probe"))
        .respond_with(ResponseTemplate::new(200).set_body_json(probe))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/probe/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "screen": [1080, 2340],
            "roots": [{
                "id": 1, "testTag": "compose_input", "focused": true,
                "bounds": [40, 200, 840, 350], "children": [],
            }],
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rawType": "android.view.WindowRoot",
            "bounds": { "x": 0.0, "y": 0.0, "w": 1080.0, "h": 2340.0 },
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "children": [
                window("com.android.systemui", "system", false, None),
                // The accessibility runner names this role itself, from
                // the window's type (`RunnerWire.roleForWindowType`).
                window("com.android.inputmethod.latin", "inputMethod", false, Some("keyboard")),
                window("dev.smix.fixture", "application", true, None),
            ],
        })))
        .mount(&server)
        .await;
    server
}

fn keyboards(n: &A11yNode) -> usize {
    usize::from(n.role == Some(Role::Keyboard)) + n.children.iter().map(keyboards).sum::<usize>()
}

fn has_id(n: &A11yNode, id: &str) -> bool {
    n.identifier.as_deref() == Some(id) || n.children.iter().any(|c| has_id(c, id))
}

#[tokio::test]
async fn a_flow_sees_the_keyboard_on_an_app_with_the_probe() {
    let server = screen_with_a_keyboard(serde_json::json!({
        "present": true, "version": "1", "roots": 1, "quietMs": 40,
        "app": "dev.smix.fixture",
    }))
    .await;
    let client =
        HttpRunnerClient::with_base(server.uri()).with_target_bundle_id("dev.smix.fixture");
    let tree = client.get_tree(None).await.expect("tree");
    assert_eq!(
        tree.source,
        TreeSource::Semantics,
        "the app is still read through the probe"
    );
    assert!(
        has_id(&tree.root, "compose_input"),
        "the probe's view of the app is gone"
    );
    assert_eq!(keyboards(&tree.root), 1, "the keyboard is not in the tree");
}

#[tokio::test]
async fn a_cli_verb_sees_the_keyboard_on_an_app_with_the_probe() {
    // No app named: the runner picks the app holding the focus and says
    // which one it picked, so the host knows which windows are its.
    let server = screen_with_a_keyboard(serde_json::json!({
        "present": true, "version": "1", "roots": 1, "quietMs": 40,
        "app": "dev.smix.fixture",
    }))
    .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let tree = client.get_tree(None).await.expect("tree");
    assert_eq!(keyboards(&tree.root), 1, "the keyboard is not in the tree");
    let apps: Vec<_> = tree
        .root
        .children
        .iter()
        .filter_map(|c| c.window.as_ref())
        .filter(|w| w.package.as_deref() == Some("dev.smix.fixture"))
        .collect();
    assert_eq!(
        apps.len(),
        1,
        "the app's window must appear once, as the probe's"
    );
}
