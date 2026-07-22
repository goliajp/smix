//! The `/long-press` success body exists twice — the Swift emitter
//! builds it by string template, this crate parses it into
//! [`PressResult`]. Every field is `#[serde(default)]`, so a drifted
//! key does not error: it parses to zero, and zero is exactly what the
//! host reads as "this press cannot be placed". The capability would
//! die into a permanent "uncertain" with every test still green.
//!
//! Read the emitter's literals, the way `tap_route_shape.rs` does.

use smix_runner_wire::PressResult;

const LONG_PRESS_SWIFT: &str =
    include_str!("../../../swift-bridge/Sources/SmixRunnerCore/LongPressRoute.swift");

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
    let body =
        r#"{"ok":true,"latestDownOffsetMs":500,"earliestUpOffsetMs":1200,"handlerWallMs":1500}"#;
    let parsed: PressResult = serde_json::from_str(body).expect("parse");
    assert_eq!(
        parsed,
        PressResult {
            latest_down_offset_ms: 500,
            earliest_up_offset_ms: 1200,
            handler_wall_ms: 1500,
        }
    );
}

/// The old bare body still parses, and to zeros — an older runner is
/// unplaceable, not wrongly placed.
#[test]
fn an_older_runners_bare_ok_reads_as_unplaceable() {
    let parsed: PressResult = serde_json::from_str(r#"{"ok":true}"#).expect("parse");
    assert_eq!(parsed, PressResult::default());
}
