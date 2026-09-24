//! iOS double-tap and long-press go where a tap goes, and are judged the
//! way a tap is.
//!
//! They used to go to `/double-tap` and `/long-press`, which resolved the
//! selector inside the runner with an XCUI query and answered `ok` and
//! nothing about where the touch went — the double-tap handler's own
//! note says it does not fire on a React Native modal. A tap has for
//! some time been host-resolved, sent to `/tap-at-norm-coord`, and
//! judged against what the runner found under the point. Now all three
//! are one route, one resolver and one judge.
//!
//! And the coordinate forms (an OCR box, an anchor plus a shift) posted
//! to `/double-tap-at-norm-coord` and `/long-press-at-norm-coord`, which
//! the iOS runner has never served.

use smix_driver::{HttpRunnerClient, PressTiming, SimctlDriver};
use smix_error::FailureCode;
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn node(label: Option<&str>, bounds: Rect, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        visible_bounds: None,
        hittable: None,
        window: None,
        unreadable_windows: None,
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

fn tree() -> A11yNode {
    let r = |x, y, w, h| Rect { x, y, w, h };
    let mut root = node(
        None,
        r(0.0, 0.0, 390.0, 844.0),
        vec![node(Some("Like"), r(50.0, 100.0, 200.0, 40.0), vec![])],
    );
    root.raw_type = "application".into();
    root
}

fn like() -> Selector {
    Selector::Text {
        text: Pattern::text("Like"),
        modifiers: Modifiers::default(),
    }
}

fn chain_of(label: &str) -> serde_json::Value {
    serde_json::json!([{
        "identifier": "",
        "label": label,
        "frame": {"x": 50.0, "y": 100.0, "w": 200.0, "h": 40.0}
    }])
}

/// The body's `times` / `holdMs`, with the wire's defaults for absence.
struct Body {
    times: u64,
    hold_ms: Option<u64>,
}

impl wiremock::Match for Body {
    fn matches(&self, request: &Request) -> bool {
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
            return false;
        };
        v["times"].as_u64().unwrap_or(1) == self.times && v["holdMs"].as_u64() == self.hold_ms
    }
}

async fn server_with_tree() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree()))
        .mount(&server)
        .await;
    server
}

/// Every gesture went to the tap route. Asked of what the server
/// received rather than by mocking the retired routes to expect nothing:
/// a list of old names only catches the old names. Reads (the tree, the
/// probe) are GETs and are not gestures.
async fn only_tap_routes_were_used(server: &MockServer) {
    let received = server.received_requests().await.expect("recording is on");
    let posts: Vec<String> = received
        .iter()
        .filter(|r| r.method == wiremock::http::Method::POST)
        .map(|r| r.url.path().to_string())
        .filter(|p| p != "/coordinate-space")
        .collect();
    assert!(!posts.is_empty(), "the driver sent no gesture at all");
    for p in posts {
        assert_eq!(p, "/tap-at-norm-coord", "a gesture went somewhere else");
    }
}

fn driver(server: &MockServer) -> SimctlDriver {
    SimctlDriver::new(HttpRunnerClient::with_base(server.uri()))
}

#[tokio::test]
async fn a_double_tap_is_two_touches_on_the_tap_route_and_is_judged() {
    let server = server_with_tree().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(Body {
            times: 2,
            hold_ms: None,
        })
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"ok": true, "chain": chain_of("Like")})),
        )
        .expect(1)
        .mount(&server)
        .await;
    driver(&server)
        .double_tap(&like(), None)
        .await
        .expect("a double tap that landed on its element");
    server.verify().await;
    only_tap_routes_were_used(&server).await;
}

#[tokio::test]
async fn a_double_tap_delivered_to_something_else_is_a_miss() {
    let server = server_with_tree().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"ok": true, "chain": chain_of("Dialog")})),
        )
        .mount(&server)
        .await;
    let err = driver(&server)
        .double_tap(&like(), None)
        .await
        .expect_err("the touch went to the dialog");
    assert_eq!(err.code, FailureCode::TapMissed, "{}", err.to_prompt());
    server.verify().await;
    only_tap_routes_were_used(&server).await;
}

#[tokio::test]
async fn a_long_press_holds_for_its_duration_and_carries_the_runners_bounds() {
    let server = server_with_tree().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(Body {
            times: 1,
            hold_ms: Some(800),
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "chain": chain_of("Like"),
            "latestDownOffsetMs": 310,
            "earliestUpOffsetMs": 1105,
            "handlerWallMs": 1140
        })))
        .expect(1)
        .mount(&server)
        .await;
    let timing: PressTiming = driver(&server)
        .long_press(&like(), Duration::from_millis(800), None)
        .await
        .expect("a long press that landed on its element");
    assert_eq!(
        (
            timing.latest_down_offset_ms,
            timing.earliest_up_offset_ms,
            timing.handler_wall_ms
        ),
        (310, 1105, 1140)
    );
    server.verify().await;
    only_tap_routes_were_used(&server).await;
}

#[tokio::test]
async fn a_long_press_delivered_to_something_else_is_a_miss() {
    let server = server_with_tree().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "chain": chain_of("Dialog"),
            "latestDownOffsetMs": 310,
            "earliestUpOffsetMs": 1105,
            "handlerWallMs": 1140
        })))
        .mount(&server)
        .await;
    let err = driver(&server)
        .long_press(&like(), Duration::from_millis(800), None)
        .await
        .expect_err("the touch went to the dialog");
    assert_eq!(err.code, FailureCode::TapMissed, "{}", err.to_prompt());
    server.verify().await;
    only_tap_routes_were_used(&server).await;
}

/// A runner that does not say when the touch was down gets the answer
/// Android gets: the press cannot be placed — not a press at time zero.
#[tokio::test]
async fn a_long_press_without_bounds_cannot_be_placed() {
    let server = server_with_tree().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"ok": true, "chain": chain_of("Like")})),
        )
        .mount(&server)
        .await;
    let timing = driver(&server)
        .long_press(&like(), Duration::from_millis(800), None)
        .await
        .expect("landed");
    assert_eq!(timing, PressTiming::unplaceable());
}

#[tokio::test]
async fn a_coordinate_double_tap_and_long_press_use_the_route_the_runner_serves() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(Body {
            times: 2,
            hold_ms: None,
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .and(Body {
            times: 1,
            hold_ms: Some(700),
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    let d = driver(&server);
    d.double_tap_at_norm_coord(0.4, 0.6)
        .await
        .expect("coordinate double tap");
    d.long_press_at_norm_coord(0.4, 0.6, 700)
        .await
        .expect("coordinate long press");
    server.verify().await;
    only_tap_routes_were_used(&server).await;
}
