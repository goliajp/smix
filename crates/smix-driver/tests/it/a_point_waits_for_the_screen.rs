//! A touch aimed at a point waits for the screen to stop moving, and says
//! what the screen was doing when it went.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use smix_driver::{ScreenAtTouch, SimctlDriver};
use smix_runner_client::HttpRunnerClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// A 40×80 grayscale PNG of one level.
pub fn frame(level: u8) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, 40, 80);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[level; 40 * 80]).unwrap();
    }
    out
}

fn app_tree(w: f64) -> serde_json::Value {
    serde_json::json!({
        "rawType": "application", "enabled": true, "selected": false,
        "hasFocus": false, "visible": true,
        "bounds": {"x": 0.0, "y": 0.0, "w": w, "h": w * 2.0}, "children": []
    })
}

/// Serve a framed app and a screen that does not move, for a test about
/// something other than the wait.
pub async fn mount_a_still_screen(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(app_tree(400.0)))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/screenshot"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(frame(128)))
        .mount(server)
        .await;
}

/// Frames that change for the first `moving` shots, then hold.
struct Arriving {
    shots: Arc<AtomicUsize>,
    moving: usize,
}

impl Respond for Arriving {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let n = self.shots.fetch_add(1, Ordering::SeqCst);
        let level = if n < self.moving { (n * 40) as u8 } else { 250 };
        ResponseTemplate::new(200).set_body_bytes(frame(level))
    }
}

async fn ok_tap(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(server)
        .await;
}

fn driver(server: &MockServer) -> SimctlDriver {
    SimctlDriver::new(HttpRunnerClient::with_base(server.uri()))
}

/// When the touch went, among everything the server received.
async fn shots_before_the_touch(server: &MockServer) -> usize {
    let received = server.received_requests().await.expect("recording is on");
    let touch = received
        .iter()
        .position(|r| r.url.path() == "/tap-at-norm-coord")
        .expect("a touch was sent");
    received[..touch]
        .iter()
        .filter(|r| r.url.path() == "/screenshot")
        .count()
}

#[tokio::test]
async fn a_point_on_a_screen_still_arriving_is_touched_once_it_stops() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(app_tree(400.0)))
        .mount(&server)
        .await;
    let shots = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/screenshot"))
        .respond_with(Arriving {
            shots: shots.clone(),
            moving: 4,
        })
        .mount(&server)
        .await;
    ok_tap(&server).await;
    let at = driver(&server).tap_at_norm_coord(0.5, 0.5).await.unwrap();
    assert!(matches!(at, ScreenAtTouch::Still { .. }), "{at:?}");
    assert!(at.caveat().is_none());
    assert_eq!(
        shots_before_the_touch(&server).await,
        6,
        "four changing frames, the first still one, and one agreeing with it — all before the touch"
    );
}

#[tokio::test]
async fn a_runner_with_no_screenshot_still_touches_and_says_it_did_not_look() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(app_tree(400.0)))
        .mount(&server)
        .await;
    ok_tap(&server).await;
    let at = driver(&server).tap_at_norm_coord(0.5, 0.5).await.unwrap();
    assert_eq!(at, ScreenAtTouch::Unseen);
    server.verify().await;
}

#[tokio::test]
async fn an_app_with_no_frame_is_not_touched() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tree"))
        .respond_with(ResponseTemplate::new(200).set_body_json(app_tree(0.0)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tap-at-norm-coord"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(0)
        .mount(&server)
        .await;
    let e = driver(&server)
        .tap_at_norm_coord(0.5, 0.5)
        .await
        .unwrap_err();
    assert!(e.message.contains("no frame yet"), "{}", e.message);
    server.verify().await;
}
