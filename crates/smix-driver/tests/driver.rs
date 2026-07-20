//! Wiremock-based driver integration tests.

use smix_driver::{HttpRunnerClient, SimctlDriver};
use smix_error::FailureCode;
use smix_input::{KeyName, SwipeDirection};
use smix_screen::Role;
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use std::time::Duration;
use wiremock::matchers::{body_json, body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mk_node(label: Option<&str>, bounds: Rect, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: label.map(String::from),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds,
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn tree_with_login() -> A11yNode {
    let mut root = mk_node(
        None,
        rect(0.0, 0.0, 390.0, 844.0),
        vec![mk_node(
            Some("Login"),
            rect(50.0, 100.0, 200.0, 40.0),
            vec![],
        )],
    );
    root.raw_type = "application".into();
    root
}

fn text_sel(t: &str) -> Selector {
    Selector::Text {
        text: Pattern::text(t),
        modifiers: Modifiers::default(),
    }
}

fn driver_for(server: &MockServer) -> SimctlDriver {
    SimctlDriver::new(HttpRunnerClient::with_base(server.uri()))
}

#[tokio::test]
async fn tree_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let t = d.tree(None).await.expect("tree");
    assert_eq!(t.children.len(), 1);
    assert_eq!(t.children[0].label.as_deref(), Some("Login"));
}

#[tokio::test]
async fn describe_returns_visible_summaries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let desc = d.describe().await.expect("describe");
    // Only Login. The fixture's root is an `application` with no label and no
    // id, and `application` has no curated role — so it carries no identity a
    // reader could use, and describe now leaves those out rather than spending
    // the list on layout. (A real application node does have a label: on a
    // stock Settings screen it comes through as "Settings".)
    assert_eq!(desc.elements.len(), 1, "got {:?}", desc.elements);
    assert_eq!(desc.elements[0].name.as_deref(), Some("Login"));
}

#[tokio::test]
async fn find_one_resolves_via_tree() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let node = d.find_one(&text_sel("Login"), None).await.expect("ok");
    assert!(node.is_some());
}

#[tokio::test]
async fn find_one_returns_none_when_missing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let node = d
        .find_one(&text_sel("DoesNotExist"), None)
        .await
        .expect("ok");
    assert!(node.is_none());
}

#[tokio::test]
async fn find_quick_probe_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/find"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "found": true
        })))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    assert!(d.find(&text_sel("X"), None).await.unwrap());
}

#[tokio::test]
async fn tap_resolves_then_tap_at_norm_coord() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    // Login bounds (50,100,200,40), root (0,0,390,844):
    // centroid (150, 120) → normalized (150/390, 120/844). The matcher
    // checks the posted nx/ny against those values — the old
    // `body_partial_json(json!({}))` matched every object, so a driver
    // that tapped the wrong coordinate (or none) still passed.
    struct NormCoordNear {
        nx: f64,
        ny: f64,
    }
    impl wiremock::Match for NormCoordNear {
        fn matches(&self, request: &wiremock::Request) -> bool {
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
                return false;
            };
            let (Some(nx), Some(ny)) = (v["nx"].as_f64(), v["ny"].as_f64()) else {
                return false;
            };
            (nx - self.nx).abs() < 1e-6 && (ny - self.ny).abs() < 1e-6
        }
    }
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(NormCoordNear {
            nx: 150.0 / 390.0,
            ny: 120.0 / 844.0,
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.tap(&text_sel("Login"), None).await.expect("tap ok");
    server.verify().await;
}

#[tokio::test]
async fn tap_miss_returns_element_not_found_after_implicit_wait() {
    let server = MockServer::start().await;
    // Empty tree — selector "DoesNotExist" never resolves.
    let empty_tree = mk_node(None, rect(0.0, 0.0, 390.0, 844.0), vec![]);
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_tree))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    // Override implicit wait via clock: 5s default. Since timer is real,
    // we shorten by calling tap with a selector that fast-fails (root
    // tree empty → all candidates fail). The poll loop sleeps 250ms but
    // returns ELEMENT_NOT_FOUND after the 5s budget; we accept that wall
    // time here (test takes ~5s).
    let err = d.tap(&text_sel("DoesNotExist"), None).await.unwrap_err();
    assert_eq!(err.code, FailureCode::ElementNotFound);
    assert!(err.message.contains("element not found"));
}

#[tokio::test]
async fn fill_chunks_one_char_per_post() {
    // Driver.fill splits multi-char text into 1-char
    // chunks, posting each to runner /fill separately so the RN
    // TextInput onChangeText debounce on the main thread can flush
    // between keystrokes. Verify the mock receives exactly N posts for
    // an N-character fill — body_json per char would over-constrain
    // (we'd have to enumerate all 7); body_partial_json on the
    // selector field is sufficient because every chunk posts the
    // same selector.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/fill"))
        .and(body_partial_json(serde_json::json!({
            "selector": {"text": "Email"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .expect(7) // chars in "u@x.com"
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.fill(&text_sel("Email"), "u@x.com", None).await.unwrap();
}

#[tokio::test]
async fn fill_with_id_selector_focus_taps_then_fills_focused() {
    // Runner /fill only accepts text selectors (or the
    // _focused_ magic). For id/label/role/anchor selectors, driver.fill
    // host-resolves + taps the target first (to give it keyboard focus),
    // then issues chunked /fill posts with `selector: {focused: true}`.
    // This test verifies the dispatch end-to-end via wiremock route
    // coverage: a single /tap-at-norm-coord (focus tap), then N chunked
    // /fill posts each carrying the focused-magic selector.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_id_and_role()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fill"))
        .and(body_partial_json(serde_json::json!({
            "selector": {"focused": true}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .expect(5) // "Hello" — 5 chars
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.fill(&id_sel("login-btn"), "Hello", None).await.unwrap();
}

#[tokio::test]
async fn fill_single_char_takes_fast_path() {
    // 1-char fills bypass the chunking loop — a single POST with the
    // whole text. Verifies the fast-path branch stays intact.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/fill"))
        .and(body_json(serde_json::json!({
            "selector": {"text": "Email"},
            "text": "x"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.fill(&text_sel("Email"), "x", None).await.unwrap();
}

#[tokio::test]
async fn clear_with_text_selector_takes_fast_path() {
    // Pure Text selector hits /clear directly (mirroring fill
    // fast path). No /tree, no /tap.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/clear"))
        .and(body_partial_json(serde_json::json!({
            "selector": {"text": "Email"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.clear(&text_sel("Email"), None).await.unwrap();
}

#[tokio::test]
async fn clear_with_id_selector_focus_taps_then_clears_focused() {
    // Runner /clear only accepts text selectors (or the
    // `_focused_` magic); id-selector clear returns missingText. Mirrors
    // the fill dispatch: host-resolve + tap the id target for
    // focus, then issue /clear with `selector: {focused: true}`.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_id_and_role()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/clear"))
        .and(body_partial_json(serde_json::json!({
            "selector": {"focused": true}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.clear(&id_sel("login-btn"), None).await.unwrap();
}

#[tokio::test]
async fn press_key_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/press-key"))
        .and(body_json(serde_json::json!({"key": "return"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tree": null, "stages": null
        })))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.press_key(KeyName::Return).await.unwrap();
}

#[tokio::test]
async fn swipe_once_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/swipe-once"))
        .and(body_json(serde_json::json!({"direction": "up"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.swipe_once(SwipeDirection::Up).await.unwrap();
}

#[tokio::test]
async fn back_and_hide_keyboard_passthroughs() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/back"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/hide-keyboard"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.back().await.unwrap();
    d.hide_keyboard().await.unwrap();
}

#[tokio::test]
async fn wait_for_resolves_immediately_on_hit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let node = d
        .wait_for(&text_sel("Login"), Duration::from_millis(500), None)
        .await
        .expect("wait_for");
    assert_eq!(node.label.as_deref(), Some("Login"));
}

#[tokio::test]
async fn wait_for_times_out_when_never_appears() {
    let server = MockServer::start().await;
    let empty_tree = mk_node(None, rect(0.0, 0.0, 390.0, 844.0), vec![]);
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_tree))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let err = d
        .wait_for(&text_sel("Never"), Duration::from_millis(300), None)
        .await
        .unwrap_err();
    assert_eq!(err.code, FailureCode::Timeout);
}

#[tokio::test]
async fn dispose_idempotent_noop() {
    let server = MockServer::start().await;
    let d = driver_for(&server);
    d.dispose().await.unwrap();
    d.dispose().await.unwrap();
}

// ---- find() selector-type dispatch ---------------------------------------

fn id_sel(s: &str) -> Selector {
    Selector::Id {
        id: s.to_string(),
        modifiers: Modifiers::default(),
    }
}

fn label_sel(s: &str) -> Selector {
    Selector::Label {
        label: s.to_string(),
        modifiers: Modifiers::default(),
    }
}

fn role_sel(name: &str) -> Selector {
    Selector::Role {
        role: Role::Button,
        name: Some(Pattern::text(name)),
        modifiers: Modifiers::default(),
    }
}

fn tree_with_id_and_role() -> A11yNode {
    let mut button = mk_node(Some("Login"), rect(50.0, 100.0, 200.0, 40.0), vec![]);
    button.raw_type = "button".into();
    button.identifier = Some("login-btn".into());
    // DO NOT set `role` manually here. The Swift /tree route
    // never emits `role` on the wire, only `rawType`; runner-client's
    // `get_tree` runs `derive_roles_recursive` post-deser to lift
    // `raw_type = "button"` into `role = Some(Role::Button)`. This test
    // proves that wire-level round-trip works end-to-end.
    let mut root = mk_node(None, rect(0.0, 0.0, 390.0, 844.0), vec![button]);
    root.raw_type = "application".into();
    root
}

#[tokio::test]
async fn find_with_id_selector_falls_back_to_host_resolve() {
    let server = MockServer::start().await;
    // No /find mock — driver must NOT call /find for id selectors.
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_id_and_role()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    assert!(d.find(&id_sel("login-btn"), None).await.unwrap());
    assert!(!d.find(&id_sel("missing-btn"), None).await.unwrap());
}

#[tokio::test]
async fn find_with_label_selector_falls_back_to_host_resolve() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    assert!(d.find(&label_sel("Login"), None).await.unwrap());
    assert!(!d.find(&label_sel("Logout"), None).await.unwrap());
}

#[tokio::test]
async fn find_with_role_selector_falls_back_to_host_resolve() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_id_and_role()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    assert!(d.find(&role_sel("Login"), None).await.unwrap());
}

#[tokio::test]
async fn find_with_text_plus_index_modifier_falls_back_to_host_resolve() {
    // text + first: Some(true) → host-resolve, not /find route.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let sel = Selector::Text {
        text: Pattern::text("Login"),
        modifiers: Modifiers {
            first: Some(true),
            ..Default::default()
        },
    };
    assert!(d.find(&sel, None).await.unwrap());
}

#[tokio::test]
async fn find_with_text_plus_ancestor_modifier_falls_back_to_host_resolve() {
    // Text + Modifiers::ancestor MUST fall back to host-resolve
    // (via /tree GET) because runner /find route is Apple element query
    // which has no parent-chain semantic. If dispatch wrongly hits /find,
    // wiremock 404 makes this fail. Only the /tree mock is registered.
    let server = MockServer::start().await;
    let mut tab_bar = mk_node(
        None,
        rect(0.0, 760.0, 390.0, 80.0),
        vec![mk_node(Some("Same"), rect(50.0, 780.0, 80.0, 40.0), vec![])],
    );
    tab_bar.raw_type = "tabBar".into();
    tab_bar.role = Some(Role::TabBar);
    let mut root = mk_node(None, rect(0.0, 0.0, 390.0, 844.0), vec![tab_bar]);
    root.raw_type = "application".into();
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(root))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let sel = Selector::Text {
        text: Pattern::text("Same"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(Selector::Role {
                role: Role::TabBar,
                name: None,
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
    };
    assert!(
        d.find(&sel, None)
            .await
            .expect("ancestor selector should dispatch to host-resolve via /tree")
    );
}

#[tokio::test]
async fn tap_at_norm_coord_passthrough() {
    // SimctlDriver::tap_at_norm_coord(nx, ny) thin
    // wrapper on runner. wiremock only mounts POST /tap-at-norm-coord;
    // any other route (e.g. /tree) is wiremock 404 → test fail. Body
    // must contain {nx, ny} per swift-bridge TapAtNormCoordRoute wire
    // shape.
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
    let d = driver_for(&server);
    d.tap_at_norm_coord(0.5, 0.95)
        .await
        .expect("tap_at_norm_coord should pass through to runner");
}

// ---- find / find_one / find_all transient /tree (or /find) transport
// retry. Mirrors the wait_for transient-retry pattern — a sim still
// launching, or the runner switching attached app context, can return a
// transient `/tree 500 snapshot_unavailable` (or `/find 500`). These
// must be polled within the standard 5s implicit-wait budget rather
// than surfaced as a `DriverError`.

#[tokio::test]
async fn find_host_resolve_retries_transient_tree_500() {
    let server = MockServer::start().await;
    // First call returns 500 snapshot_unavailable (transient sim state),
    // subsequent calls return the populated tree. Driver must retry
    // within the 5s budget and resolve to Ok(true).
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({"ok": false, "error": "snapshot_unavailable"})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_id_and_role()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let present = d
        .find(&id_sel("login-btn"), None)
        .await
        .expect("find host-resolve branch must retry transient /tree 500");
    assert!(present, "Login id node is present after retry");
}

#[tokio::test]
async fn find_runner_route_retries_transient_find_500() {
    let server = MockServer::start().await;
    // First /find returns 500 (transient), second returns present=true.
    Mock::given(method("POST"))
        .and(path("/find"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({"ok": false, "error": "transient"})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/find"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"found": true, "ok": true})),
        )
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let present = d
        .find(&text_sel("Login"), None)
        .await
        .expect("find runner-route branch must retry transient /find 500");
    assert!(present, "/find returns exists=true after retry");
}

#[tokio::test]
async fn tap_retries_transient_tree_500() {
    let server = MockServer::start().await;
    // First /tree call returns transient 500, second returns the populated
    // tree containing the "Login" text node. tap() must retry within budget
    // and proceed to /tap-at-norm-coord (parity with wait_for).
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({"ok": false, "error": "snapshot_unavailable"})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_login()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    d.tap(&text_sel("Login"), None)
        .await
        .expect("tap must retry transient /tree 500 and proceed");
}

#[tokio::test]
async fn find_one_and_find_all_retry_transient_tree_500() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({"ok": false, "error": "snapshot_unavailable"})),
        )
        .up_to_n_times(2) // 2 transient errors: 1 for find_one, 1 for find_all
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_id_and_role()))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let one = d
        .find_one(&id_sel("login-btn"), None)
        .await
        .expect("find_one must retry transient /tree 500");
    assert!(one.is_some(), "find_one should resolve after retry");
    let all = d
        .find_all(&id_sel("login-btn"), None)
        .await
        .expect("find_all must retry transient /tree 500");
    assert_eq!(all.len(), 1, "find_all should match 1 node after retry");
}

// -------------------- /system-popup-action passthru ----------------------
//
// G9 act side — Ok(true) on 200 + `{"ok":true}` envelope; Ok(false) on
// 404 `not_found` from the runner (popup or button id stale). camelCase
// request body (`popupId` / `buttonId`) is byte-identical to the swift
// SystemPopupActionRoute.Request decoder.

#[tokio::test]
async fn system_popup_action_returns_true_on_runner_ok() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/system-popup-action"))
        .and(body_partial_json(
            serde_json::json!({"popupId": "p-1", "buttonId": "b-open"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let ok = d
        .system_popup_action("p-1", "b-open")
        .await
        .expect("system_popup_action ok path");
    assert!(ok);
}

#[tokio::test]
async fn system_popup_action_returns_false_on_runner_404_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/system-popup-action"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "ok": false,
            "error": "not_found",
            "popupId": "p-x",
            "buttonId": "b-x"
        })))
        .mount(&server)
        .await;
    let d = driver_for(&server);
    let ok = d
        .system_popup_action("p-x", "b-x")
        .await
        .expect("system_popup_action 404 surfaces as Ok(false)");
    assert!(!ok);
}

/// `dispatch:` on Android must say what the guide says it says.
///
/// 04-actions promises "an explicit unsupported message (native
/// synthesize is already the Android default — the override is never
/// needed there)". The message was "not implemented by the Kotlin
/// runner", which invites a user to wait for a feature that is not
/// coming and is not needed.
#[tokio::test]
async fn android_dispatch_override_explains_itself() {
    use smix_driver::AndroidDriver;
    use smix_runner_client::{HttpRunnerClient, TapMode};
    use smix_selector::Selector;

    let driver = AndroidDriver::new(HttpRunnerClient::new(1));
    let err = smix_driver::Driver::tap_with_mode(
        &driver,
        &Selector::Id {
            id: "x".into(),
            modifiers: Default::default(),
        },
        TapMode::Resolve,
        None,
    )
    .await
    .expect_err("dispatch has no meaning on Android");

    let text = format!("{} {}", err.message, err.hint.clone().unwrap_or_default());
    assert!(
        text.contains("iOS") && text.contains("dispatch"),
        "the error must name what the key is and where it belongs: {text}"
    );
    assert!(
        !text.contains("not implemented"),
        "reads as a missing feature rather than an inapplicable one: {text}"
    );
    assert!(
        text.contains("remove"),
        "an AI-readable failure says what to do next: {text}"
    );
}

/// `--force-key-events` must actually change what Android does.
///
/// The flag set a header for iOS and nothing at all for Android:
/// `set_force_key_events` had an empty default on the trait and
/// AndroidDriver never overrode it. So on Android the flag was inert
/// twice over — no header to send, and no behaviour to change.
///
/// What it means here is skipping host-side focus resolution, since
/// `/input-text` already types into the focused field. Resolving is
/// precisely what fails for the callers who reach for this mode.
#[tokio::test]
async fn android_key_events_mode_skips_focus_resolution() {
    use smix_driver::{AndroidDriver, Driver};
    use smix_runner_client::HttpRunnerClient;
    use smix_selector::Selector;

    // Port 1 answers nothing. With resolution ON, fill fails while
    // resolving the selector; with it OFF, it fails at input_text.
    // The two errors name different stages, which is the observable
    // difference between the modes without a device.
    let selector = Selector::Id {
        id: "hidden-input".into(),
        modifiers: Default::default(),
    };

    let resolving = AndroidDriver::new(HttpRunnerClient::new(1));
    let resolved_err = resolving
        .fill(&selector, "hello", None)
        .await
        .expect_err("no runner");

    let mut key_events = AndroidDriver::new(HttpRunnerClient::new(1));
    key_events.set_force_key_events(true);
    let key_err = key_events
        .fill(&selector, "hello", None)
        .await
        .expect_err("no runner");

    assert!(
        key_err.message.contains("key-events"),
        "key-events mode must take its own path: {}",
        key_err.message
    );
    assert_ne!(
        resolved_err.message, key_err.message,
        "the flag changed nothing — both modes failed at the same stage"
    );
}

/// The package of the app under test has to reach the Android runner.
///
/// `set_target_bundle_id` was an empty default on the trait that only
/// iOS overrode, so on Android the package never left the host. The
/// runner needs it: Compose emits `<pkg>:id/<tag>` on some layouts, and
/// without the package it cannot spell that. It had been compensating
/// with a hardcoded `com.example.app` — the README's placeholder — so
/// the qualified lookup matched nobody's app and every real one fell
/// through to the manual walk.
#[tokio::test]
async fn android_sends_the_target_package_to_the_runner() {
    use smix_driver::{AndroidDriver, Driver};
    use smix_runner_client::HttpRunnerClient;
    use std::io::Read;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    // One request is enough; the driver's error afterwards is irrelevant.
    let recorder = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("accept");
        let mut buf = [0u8; 4096];
        let n = sock.read(&mut buf).unwrap_or(0);
        String::from_utf8_lossy(&buf[..n]).to_string()
    });

    let mut driver = AndroidDriver::new(HttpRunnerClient::new(port));
    driver.set_target_bundle_id("jp.golia.app");
    let _ = driver.tap_by_id("submit").await;

    let request = recorder.join().expect("recorder");
    let header = request
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("app-bundle-id:"))
        .unwrap_or_else(|| {
            panic!("no App-Bundle-Id header reached the runner:\n{request}")
        });
    assert!(
        header.contains("jp.golia.app"),
        "the header carried the wrong package: {header}"
    );
}
