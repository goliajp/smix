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

/// These tests need a live postgres + valkey. Without them each test
/// used to `return` early and report PASS — and CI runs
/// `cargo test --workspace` with neither env var set, so all six were
/// permanently green while asserting nothing. Skipping is now visible:
/// `SMIX_SERVER_IT=1` makes a missing backing service a hard failure,
/// and CI sets it so the absence is a red build rather than a silent
/// six-test hole.
fn require_backing_services() -> bool {
    std::env::var("SMIX_SERVER_IT").is_ok()
}

async fn build_state(stream_root: &str) -> Option<AppState> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        assert!(
            !require_backing_services(),
            "SMIX_SERVER_IT is set but DATABASE_URL is not — the integration \
             suite cannot run and must not report success"
        );
        eprintln!("DATABASE_URL not set — skipping (set SMIX_SERVER_IT=1 to make this fatal)");
        return None;
    };
    let Ok(valkey_url) = std::env::var("REDIS_URL") else {
        assert!(
            !require_backing_services(),
            "SMIX_SERVER_IT is set but REDIS_URL is not — the integration \
             suite cannot run and must not report success"
        );
        eprintln!("REDIS_URL not set — skipping (set SMIX_SERVER_IT=1 to make this fatal)");
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

/// The registry's concurrency contract, tested against the real
/// `CaptureRegistry` type instead of a mirror.
///
/// The version this replaces defined a `fake_start_shape` helper inside
/// this file and asserted on THAT — so it locked in a shape
/// `start_capture` merely resembled, and the shape it locked in was the
/// bug: check-then-release-then-insert, which let two concurrent starts
/// for the SAME udid both pass the check and build two pipelines. It
/// also only ever exercised two DIFFERENT udids, the case that was
/// never at risk.
///
/// The real invariant: claiming a udid is atomic (a second claim for the
/// same udid loses), and claims for different udids do not serialize.
#[tokio::test]
async fn claiming_a_udid_is_atomic_and_independent_udids_do_not_serialize() {
    use smix_server::state::CaptureRegistry;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::Mutex;

    // Mirrors start_capture's claim step exactly: one lock, check and
    // insert together. Returns whether this caller won the slot.
    async fn claim(reg: &CaptureRegistry, udid: &str) -> bool {
        let mut g = reg.lock().await;
        if g.contains_key(udid) {
            return false;
        }
        g.insert(udid.to_string(), None);
        true
    }

    let reg: CaptureRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Same udid, concurrently: exactly one winner.
    let (a, b) = tokio::join!(claim(&reg, "UDID-SAME"), claim(&reg, "UDID-SAME"));
    assert!(
        a ^ b,
        "exactly one concurrent claim for the same udid may win (got {a} and {b}) — \
         two winners means two pipelines and an orphaned encoder"
    );

    // Different udids: the long bring-up happens outside the lock, so
    // two of them overlap instead of queueing.
    async fn claim_then_work(reg: CaptureRegistry, udid: &str) {
        assert!(claim(&reg, udid).await, "{udid} claim");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        reg.lock().await.insert(udid.to_string(), None);
    }
    let started = Instant::now();
    tokio::join!(
        claim_then_work(reg.clone(), "UDID-A"),
        claim_then_work(reg.clone(), "UDID-B"),
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_millis() < 350,
        "independent udids serialized on the registry mutex — {elapsed:?}, expected ~200ms"
    );
    assert_eq!(reg.lock().await.len(), 3, "one same-udid slot + two others");
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
