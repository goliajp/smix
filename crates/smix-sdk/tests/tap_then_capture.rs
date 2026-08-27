//! Tap, then one frame, in that order and only once.
//!
//! The consumer this came from was verifying a control bar that appears
//! on tap and hides itself three seconds later. Every way of getting
//! evidence they tried cost them more than three seconds, so they
//! changed the app to hold the bar for a minute and made a diagnostic
//! build to photograph it.
//!
//! Measured on this machine, the wire is not where those seconds went:
//! a tap is 336 ms and a frame from the runner is 88 ms. What a combined
//! call saves is the turn between two tool calls — and, on a simulator,
//! the 237 ms difference between asking the runner for the frame and
//! asking device tooling for it.
//!
//! So the assertions here are about order, count, and provenance rather
//! than about speed: a frame taken before the tap, or a second frame
//! taken after the first, is evidence of the wrong moment.

use smix_driver::SimctlDriver;
use smix_runner_client::HttpRunnerClient;
use smix_screen::{A11yNode, Rect};
use smix_sdk::App;
use smix_selector::{Modifiers, Selector};
use smix_simctl::SimctlClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FRAME: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR-and-then-some";

fn node(
    raw_type: &str,
    identifier: Option<&str>,
    bounds: Rect,
    children: Vec<A11yNode>,
) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: raw_type.into(),
        element_type_raw: 1,
        role: None,
        identifier: identifier.map(String::from),
        label: None,
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

fn tree_with_submit() -> A11yNode {
    node(
        "application",
        None,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 390.0,
            h: 844.0,
        },
        vec![node(
            "button",
            Some("fixture-submit"),
            Rect {
                x: 10.0,
                y: 20.0,
                w: 100.0,
                h: 40.0,
            },
            vec![],
        )],
    )
}

fn submit() -> Selector {
    Selector::Id {
        id: "fixture-submit".to_string(),
        modifiers: Modifiers::default(),
    }
}

/// A runner that answers the routes this needs, and remembers the order
/// it was asked in.
async fn runner_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree_with_submit()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/session/open"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sessionId": "S1", "activatedOnce": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/screenshot"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(FRAME)
                .insert_header("Content-Type", "image/png"),
        )
        .mount(&server)
        .await;
    server
}

async fn driven(server: &MockServer) -> App {
    let runner = HttpRunnerClient::with_base(server.uri());
    let mut app = App::new(SimctlDriver::new(runner), SimctlClient::new());
    app.open_session_in_place("com.example.app", true)
        .await
        .expect("the stub answers /session/open");
    app
}

/// The paths the runner was asked for, in order.
async fn asked(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .expect("wiremock records requests")
        .iter()
        .map(|r| r.url.path().to_string())
        .collect()
}

#[tokio::test]
async fn the_frame_is_taken_after_the_tap() {
    let server = runner_server().await;
    let app = driven(&server).await;
    app.tap_then_capture(&submit())
        .await
        .expect("the stub answers every route this needs");
    let served = asked(&server).await;
    let tapped = served
        .iter()
        .position(|p| p == "/tap-at-norm-coord")
        .unwrap_or_else(|| panic!("nothing tapped: {served:?}"));
    let shot = served
        .iter()
        .position(|p| p == "/screenshot")
        .unwrap_or_else(|| panic!("no frame taken: {served:?}"));
    assert!(
        shot > tapped,
        "the frame was taken before the tap, so it is a picture of the \
         screen this call was supposed to change: {served:?}"
    );
}

#[tokio::test]
async fn exactly_one_frame_is_taken() {
    let server = runner_server().await;
    let app = driven(&server).await;
    app.tap_then_capture(&submit())
        .await
        .expect("the stub answers every route this needs");
    let served = asked(&server).await;
    assert_eq!(
        served.iter().filter(|p| *p == "/screenshot").count(),
        1,
        "a retry loop turns one piece of evidence into a series, and on a \
         UI that hides itself the second frame has nothing in it: {served:?}"
    );
}

#[tokio::test]
async fn the_bytes_are_the_frame_and_the_route_is_named() {
    let server = runner_server().await;
    let app = driven(&server).await;
    let (_outcome, captured) = app
        .tap_then_capture(&submit())
        .await
        .expect("the stub answers every route this needs");
    assert_eq!(captured.png, FRAME, "the frame has to survive the trip");
    assert_eq!(
        captured.via, "runner",
        "on iOS the frame comes from the process that tapped, and the \
         call says so rather than leaving it to be assumed"
    );
}

#[tokio::test]
async fn the_gap_is_the_gap_and_not_the_whole_call() {
    let server = runner_server().await;
    let app = driven(&server).await;
    let started = std::time::Instant::now();
    let (_outcome, captured) = app
        .tap_then_capture(&submit())
        .await
        .expect("the stub answers every route this needs");
    let whole = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    assert!(
        captured.gap_ms <= whole,
        "gap_ms ({}) is not smaller than the whole call ({whole} ms), so it \
         is measuring something else — the number exists to say how late \
         the frame is, not how long the call took",
        captured.gap_ms
    );
}
