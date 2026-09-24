//! The held-touch bounds on `/tap-at-norm-coord` exist twice — the
//! Swift emitter builds them by string template, this crate parses them
//! into [`TapAtCoordResult`] and reads them back as [`PressResult`]. Every field is `#[serde(default)]`, so a drifted
//! key does not error: it parses to zero, and zero is exactly what the
//! host reads as "this press cannot be placed". The capability would
//! die into a permanent "uncertain" with every test still green.
//!
//! Read the emitter's literals, the way `tap_route_shape.rs` does.

use smix_runner_wire::{PressResult, TapAtCoordResult};

const LONG_PRESS_SWIFT: &str =
    include_str!("../../../swift-bridge/Sources/SmixRunnerCore/TapAtCoordRoute.swift");

#[test]
fn every_field_this_crate_parses_is_a_key_the_emitter_writes() {
    let json = serde_json::to_value(PressResult {
        latest_down_offset_ms: 500,
        earliest_up_offset_ms: 1200,
        handler_wall_ms: 1500,
    })
    .expect("serialize");
    let keys: Vec<&String> = json.as_object().expect("object").keys().collect();
    assert_eq!(
        keys.len(),
        3,
        "a field was added without updating this gate"
    );
    for k in keys {
        assert!(
            LONG_PRESS_SWIFT.contains(&format!("\"{k}\":")),
            "the Swift emitter never writes `{k}`, so it parses to 0 and \
             every press reads as unplaceable"
        );
    }
}

/// The emitted body has to round-trip, not merely contain the words.
#[test]
fn the_emitted_body_parses_back_to_what_was_measured() {
    let body = r#"{"ok":true,"chain":[],"latestDownOffsetMs":500,"earliestUpOffsetMs":1200,"handlerWallMs":1500}"#;
    let parsed: TapAtCoordResult = serde_json::from_str(body).expect("parse");
    assert_eq!(
        parsed.press(),
        Some(PressResult {
            latest_down_offset_ms: 500,
            earliest_up_offset_ms: 1200,
            handler_wall_ms: 1500,
        })
    );
}

/// A tap's body, or an older runner's, carries no bounds and is
/// unplaceable — not a press at time zero.
#[test]
fn a_body_without_bounds_reads_as_unplaceable() {
    let parsed: TapAtCoordResult =
        serde_json::from_str(r#"{"ok":true,"chain":[]}"#).expect("parse");
    assert_eq!(parsed.press(), None);
    let one_edge: TapAtCoordResult =
        serde_json::from_str(r#"{"ok":true,"latestDownOffsetMs":500}"#).expect("parse");
    assert_eq!(
        one_edge.press(),
        None,
        "one edge of a window places nothing"
    );
}
