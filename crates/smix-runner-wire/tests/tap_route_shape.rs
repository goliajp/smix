//! The `/tap` success body exists twice — the Swift emitter builds it by
//! string template, this crate parses it into [`TapResult`]. Every field
//! on the Rust side is `#[serde(default)]`, so a drifted emission does
//! not error: it parses to all-`None`/zero, and the capability the field
//! carries dies silently. That happened — the runner shipped a
//! nested-`matched` + snake_case body for long enough that
//! `TapMode::Resolve`'s whole purpose (returning the matched frame)
//! returned `None` in production while every test stayed green.
//!
//! These tests read the Swift source the way
//! `schema_negotiation.rs` reads `HealthRoute.swift`: the emitter's key
//! literals must be exactly the keys this crate serializes.

use smix_runner_wire::{TapResult, TapStages};
use smix_screen::Rect;

const TAP_ROUTE_SWIFT: &str =
    include_str!("../../../swift-bridge/Sources/SmixRunnerCore/TapRoute.swift");

fn fully_populated() -> TapResult {
    TapResult {
        matched_label: Some("Sign In".to_string()),
        stages: Some(TapStages {
            resolve_ms: 12.3,
            tap_call_ms: 4.5,
            total_ms: 17.1,
            wait_existence_ms: 0.2,
            frame_read_ms: 0.7,
        }),
        frame: Some(Rect {
            x: 50.0,
            y: 100.0,
            w: 200.0,
            h: 40.0,
        }),
        app_frame: Some(Rect {
            x: 0.0,
            y: 0.0,
            w: 390.0,
            h: 844.0,
        }),
    }
}

/// Every key this crate writes must appear as a quoted key literal in the
/// Swift emitter. Serializing a fully-populated TapResult enumerates the
/// keys, so a field added here without a Swift counterpart fails too.
#[test]
fn every_tap_result_key_is_emitted_by_the_swift_route() {
    let json = serde_json::to_value(fully_populated()).expect("serializes");
    let mut keys: Vec<String> = Vec::new();
    collect_keys(&json, &mut keys);
    assert!(
        keys.len() >= 8,
        "TapResult serialized to only {keys:?} — the shape shrank and this \
         check would pass by knowing nothing"
    );
    for key in keys {
        // Rect keys (x/y/w/h) are emitted via the shared rectJson helper.
        let literal = format!("\"{key}\":");
        assert!(
            TAP_ROUTE_SWIFT.contains(&literal),
            "TapRoute.swift never writes the key `{key}` — the runner's \
             /tap body and smix_runner_wire::TapResult have drifted apart"
        );
    }
}

/// The two shapes the old emitter used and this crate silently absorbed.
#[test]
fn the_swift_route_does_not_use_the_pre_v2_shape() {
    for forbidden in [
        "\"matched\":{",
        "\"resolve_ms\"",
        "\"tap_call_ms\"",
        "\"total_ms\"",
        "\"wait_existence_ms\"",
        "\"frame_read_ms\"",
    ] {
        assert!(
            !TAP_ROUTE_SWIFT.contains(forbidden),
            "TapRoute.swift still contains `{forbidden}` — that is the \
             pre-v2 emission this crate deserializes to None/zero without \
             erroring"
        );
    }
}

/// A body in the exact textual form the Swift template produces (float
/// formats included) must round-trip with every field populated.
#[test]
fn a_swift_shaped_body_parses_fully_populated() {
    let body = concat!(
        r#"{"ok":true,"matchedLabel":"Sign In","#,
        r#""frame":{"x":50.00,"y":100.00,"w":200.00,"h":40.00},"#,
        r#""appFrame":{"x":0.00,"y":0.00,"w":390.00,"h":844.00},"#,
        r#""stages":{"resolveMs":12.3,"tapCallMs":4.5,"totalMs":17.1,"#,
        r#""waitExistenceMs":0.2,"frameReadMs":0.7}}"#,
    );
    let parsed: TapResult = serde_json::from_str(body).expect("parses");
    assert_eq!(parsed.matched_label.as_deref(), Some("Sign In"));
    let stages = parsed.stages.expect("stages populated");
    assert!(stages.resolve_ms > 0.0, "resolveMs lost in transit");
    assert!(stages.total_ms > 0.0, "totalMs lost in transit");
    let frame = parsed.frame.expect("frame populated");
    assert_eq!(
        (frame.x, frame.y, frame.w, frame.h),
        (50.0, 100.0, 200.0, 40.0)
    );
    let app = parsed.app_frame.expect("appFrame populated");
    assert_eq!((app.w, app.h), (390.0, 844.0));
}

fn collect_keys(value: &serde_json::Value, out: &mut Vec<String>) {
    if let serde_json::Value::Object(map) = value {
        for (k, v) in map {
            out.push(k.clone());
            collect_keys(v, out);
        }
    }
}
