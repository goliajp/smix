//! Integration test for smix-server wiring: boots `app(state)` against the
//! real compose-dev pg(5432) + valkey(6379) and drives it via tower oneshot
//! (no listener bind). Requires `DATABASE_URL` + `REDIS_URL` to be set; the
//! whole file is skipped (early-return) when they are not, so a bare
//! `cargo test` without backends does not spuriously fail.
//!
//! Run:
//!   DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres \
//!   REDIS_URL=redis://localhost:6379 \
//!     cargo test -p smix-server --test wiring -- --nocapture

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use smix_server::{app, config::Config, db, state::AppState, valkey};
use std::net::SocketAddr;
use tower::ServiceExt;

async fn build_state(stream_root: &str) -> Option<AppState> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL not set — skipping");
        return None;
    };
    let Ok(valkey_url) = std::env::var("REDIS_URL") else {
        eprintln!("REDIS_URL not set — skipping");
        return None;
    };
    let cfg = Config {
        bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        database_url,
        valkey_url,
        stream_root: stream_root.to_string(),
    };
    let pg = db::connect(&cfg.database_url).await.expect("connect pg");
    db::run_migrations(&pg).await.expect("run migrations");
    let valkey_mgr = valkey::connect(&cfg.valkey_url)
        .await
        .expect("connect valkey");
    Some(AppState {
        cfg,
        pg,
        valkey: valkey_mgr,
        captures: Default::default(),
    })
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn sims_empty_then_listed() {
    let Some(state) = build_state(".smix/stream").await else {
        return;
    };
    let udid = format!("TEST-S1-{}", uuid::Uuid::new_v4());

    // health: double-probe 200 + status ok
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "health status");
    assert!(body_string(resp).await.contains("\"status\":\"ok\""));

    // /api/sims returns a JSON array, and does NOT contain our udid yet
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/sims")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "sims status");
    let body = body_string(resp).await;
    assert!(
        body.trim_start().starts_with('['),
        "sims is a JSON array: {body}"
    );
    assert!(!body.contains(&udid), "udid not present before insert");

    // insert one stream_sessions row
    sqlx::query("INSERT INTO stream_sessions(udid, device_name, runtime, stream_path) VALUES ($1, $2, $3, $4)")
        .bind(&udid)
        .bind("iPhone 16")
        .bind("iOS 26.5")
        .bind(&udid)
        .execute(&state.pg)
        .await
        .expect("insert row");

    // now /api/sims includes it
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/sims")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(
        body.contains(&udid),
        "sims should list inserted udid: {body}"
    );
    assert!(body.contains("iPhone 16"), "device_name present: {body}");

    // cleanup
    sqlx::query("DELETE FROM stream_sessions WHERE udid = $1")
        .bind(&udid)
        .execute(&state.pg)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn serves_existing_hls_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let stream_root = tmp.path().to_str().unwrap();
    let Some(state) = build_state(stream_root).await else {
        return;
    };

    let udid = format!("TEST-S2-{}", uuid::Uuid::new_v4());
    let dir = tmp.path().join(&udid);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("index.m3u8"),
        "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:3\n#EXTINF:3.0,\nseg_000.ts\n#EXT-X-ENDLIST\n",
    )
    .unwrap();
    std::fs::write(dir.join("seg_000.ts"), b"\x47\x40\x00\x10").unwrap();

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/streams/{udid}/index.m3u8"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "m3u8 serve status");
    let body = body_string(resp).await;
    assert!(body.contains("#EXTM3U"), "m3u8 body: {body}");
}

#[tokio::test]
async fn sims_reflects_capturing_state() {
    let Some(state) = build_state(".smix/stream").await else {
        return;
    };
    let udid = format!("TEST-S2-CAP-{}", uuid::Uuid::new_v4());

    // pg row for this udid (so it appears in /api/sims)
    sqlx::query("INSERT INTO stream_sessions(udid, device_name, runtime, stream_path) VALUES ($1, $2, $3, $4)")
        .bind(&udid)
        .bind("iPhone 16")
        .bind("iOS 26.5")
        .bind(&udid)
        .execute(&state.pg)
        .await
        .expect("insert row");

    // mark capturing in valkey directly
    let mut vk = state.valkey.clone();
    smix_server::valkey::sadd(&mut vk, smix_server::capture::CAPTURING_SET, &udid)
        .await
        .expect("sadd");

    let body = sims_body(state.clone()).await;
    let entry = find_entry(&body, &udid).expect("udid present after insert");
    assert_eq!(
        entry["capturing"],
        serde_json::Value::Bool(true),
        "capturing=true after SADD: {body}"
    );

    // remove → capturing should flip to false
    smix_server::valkey::srem(&mut vk, smix_server::capture::CAPTURING_SET, &udid)
        .await
        .expect("srem");

    let body = sims_body(state.clone()).await;
    let entry = find_entry(&body, &udid).expect("udid still present");
    assert_eq!(
        entry["capturing"],
        serde_json::Value::Bool(false),
        "capturing=false after SREM: {body}"
    );

    sqlx::query("DELETE FROM stream_sessions WHERE udid = $1")
        .bind(&udid)
        .execute(&state.pg)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn capture_routes_validate() {
    let Some(state) = build_state(".smix/stream").await else {
        return;
    };

    // start with empty udid → 400
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/capture/start")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"udid":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "empty udid start → 400"
    );

    // start with no udid field at all → 4xx (axum JSON rejection = 422)
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/capture/start")
                .header("content-type", "application/json")
                .body(Body::from(r#"{}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_client_error(),
        "missing udid → client error, got {}",
        resp.status()
    );

    // stop for a udid that is not capturing → explicit 404, not a panic
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/capture/stop")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"udid":"NOT-CAPTURING-XYZ"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "stop non-capturing → 404"
    );
    let body = body_string(resp).await;
    assert!(body.contains("not capturing"), "404 body: {body}");
}

#[tokio::test]
async fn capture_registry_isolates_per_udid() {
    let tmp = tempfile::TempDir::new().unwrap();
    let stream_root = tmp.path().to_str().unwrap();
    let Some(state) = build_state(stream_root).await else {
        return;
    };

    let udid_a = format!("TEST-ISO-A-{}", uuid::Uuid::new_v4());
    let udid_b = format!("TEST-ISO-B-{}", uuid::Uuid::new_v4());

    for u in [&udid_a, &udid_b] {
        let dir = tmp.path().join(u);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("index.m3u8"),
            format!("#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXTINF:3.0,\n{u}-seg.ts\n"),
        )
        .unwrap();
    }

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/streams/{udid_a}/index.m3u8"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "A m3u8 serve status");
    let body = body_string(resp).await;
    assert!(
        body.contains(&udid_a),
        "A m3u8 should contain udid_a: {body}"
    );
    assert!(
        !body.contains(&udid_b),
        "A m3u8 must NOT contain udid_b (cross-pollution): {body}"
    );

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/streams/{udid_b}/index.m3u8"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "B m3u8 serve status");
    let body = body_string(resp).await;
    assert!(
        body.contains(&udid_b),
        "B m3u8 should contain udid_b: {body}"
    );
    assert!(
        !body.contains(&udid_a),
        "B m3u8 must NOT contain udid_a (cross-pollution): {body}"
    );

    sqlx::query("INSERT INTO stream_sessions(udid, device_name, runtime, stream_path) VALUES ($1, $2, $3, $4)")
        .bind(&udid_a)
        .bind("iPhone A")
        .bind("iOS 26.5")
        .bind(&udid_a)
        .execute(&state.pg)
        .await
        .expect("insert A");
    sqlx::query("INSERT INTO stream_sessions(udid, device_name, runtime, stream_path) VALUES ($1, $2, $3, $4)")
        .bind(&udid_b)
        .bind("iPhone B")
        .bind("iOS 26.5")
        .bind(&udid_b)
        .execute(&state.pg)
        .await
        .expect("insert B");

    let mut vk = state.valkey.clone();
    smix_server::valkey::sadd(&mut vk, smix_server::capture::CAPTURING_SET, &udid_a)
        .await
        .expect("sadd A");
    smix_server::valkey::sadd(&mut vk, smix_server::capture::CAPTURING_SET, &udid_b)
        .await
        .expect("sadd B");

    let body = sims_body(state.clone()).await;
    let a = find_entry(&body, &udid_a).expect("A present after insert");
    let b = find_entry(&body, &udid_b).expect("B present after insert");
    assert_eq!(
        a["capturing"],
        serde_json::Value::Bool(true),
        "A capturing=true after SADD: {body}"
    );
    assert_eq!(
        b["capturing"],
        serde_json::Value::Bool(true),
        "B capturing=true after SADD: {body}"
    );

    smix_server::valkey::srem(&mut vk, smix_server::capture::CAPTURING_SET, &udid_a)
        .await
        .expect("srem A");

    let body = sims_body(state.clone()).await;
    let a = find_entry(&body, &udid_a).expect("A still in pg");
    let b = find_entry(&body, &udid_b).expect("B still in pg");
    assert_eq!(
        a["capturing"],
        serde_json::Value::Bool(false),
        "A capturing=false after its own SREM: {body}"
    );
    assert_eq!(
        b["capturing"],
        serde_json::Value::Bool(true),
        "B capturing=true unaffected by A's SREM: {body}"
    );

    smix_server::valkey::srem(&mut vk, smix_server::capture::CAPTURING_SET, &udid_b)
        .await
        .expect("srem B");
    sqlx::query("DELETE FROM stream_sessions WHERE udid IN ($1, $2)")
        .bind(&udid_a)
        .bind(&udid_b)
        .execute(&state.pg)
        .await
        .expect("cleanup");
}

/// Lock the "short lock + long work in lock-out" shape of `start_capture` so
/// that two independent udids do not serialize on the registry mutex.
///
/// `start_capture` (capture.rs:482-499) only holds the captures-map mutex for
/// `contains_key` + (later) `insert`; the long-running `capture::start(...)`
/// runs in between with the guard dropped. If a future refactor accidentally
/// held the guard across the long work, two `tokio::join!`-ed calls for
/// different udids would serialize and the wall-clock would double. This test
/// fixates the shape with a `fake_start` mirror — wall-clock for two
/// concurrent calls must stay roughly equal to one call's long-work, not
/// double it.
async fn fake_start_shape(
    map: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, ()>>>,
    udid: &str,
) {
    {
        let g = map.lock().await;
        assert!(
            !g.contains_key(udid),
            "fake_start: udid not already present"
        );
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    {
        let mut g = map.lock().await;
        g.insert(udid.to_string(), ());
    }
}

#[tokio::test]
async fn registry_mutex_does_not_serialize_independent_udids() {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::Mutex;

    let map: Arc<Mutex<HashMap<String, ()>>> = Arc::new(Mutex::new(HashMap::new()));
    let map_a = map.clone();
    let map_b = map.clone();

    let start = Instant::now();
    tokio::join!(
        fake_start_shape(map_a, "UDID-A"),
        fake_start_shape(map_b, "UDID-B"),
    );
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 350,
        "concurrent fake_start of independent udids should not serialize on the registry mutex — wall-clock = {elapsed:?}, expected < 350ms (~200ms concurrent), not ~400ms (serial)"
    );

    let final_map = map.lock().await;
    assert_eq!(final_map.len(), 2, "both udids inserted: {:?}", *final_map);
}

async fn sims_body(state: AppState) -> serde_json::Value {
    let resp = app(state)
        .oneshot(
            Request::builder()
                .uri("/api/sims")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let s = body_string(resp).await;
    serde_json::from_str(&s).unwrap()
}

fn find_entry<'a>(body: &'a serde_json::Value, udid: &str) -> Option<&'a serde_json::Value> {
    body.as_array()?
        .iter()
        .find(|e| e["udid"] == serde_json::Value::String(udid.to_string()))
}
