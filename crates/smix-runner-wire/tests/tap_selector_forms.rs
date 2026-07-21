//! The tap-family routes accept the selector forms the wire doc claims.
//!
//! They accepted one key, `text`, while the actions guide documented
//! `dispatch: daemonProxy` with an `id` — the escape hatch for React
//! Native views addressed by testID, which is precisely what a text
//! selector cannot reach. The guide taught a flow that could not run.
//!
//! Reading the Swift source rather than the running server, the way
//! tap_route_shape.rs does: this is a contract check, and it has to
//! fail in CI where no simulator exists.

const ROUTE_SELECTOR: &str =
    include_str!("../../../swift-bridge/Sources/SmixRunnerCore/RouteSelector.swift");
const TAP: &str = include_str!("../../../swift-bridge/Sources/SmixRunnerCore/TapRoute.swift");
const DOUBLE_TAP: &str =
    include_str!("../../../swift-bridge/Sources/SmixRunnerCore/DoubleTapRoute.swift");
const LONG_PRESS: &str =
    include_str!("../../../swift-bridge/Sources/SmixRunnerCore/LongPressRoute.swift");

/// Reads the decoder's own key list, not the file at large.
///
/// The first version asserted the file merely *contained* each key,
/// which stayed true after the key was removed from the list — the
/// switch arms and the wireKey property still mention it. A check
/// satisfied by text elsewhere in the file is not checking the thing
/// it names.
#[test]
fn tap_route_decodes_three_selector_keys() {
    let start = ROUTE_SELECTOR
        .find("for key in [")
        .expect("RouteSelector no longer iterates a key list; re-read the decoder");
    let rest = &ROUTE_SELECTOR[start..];
    let end = rest.find(']').expect("unterminated key list");
    let list = &rest[..end];

    for key in ["\"text\"", "\"id\"", "\"label\""] {
        assert!(
            list.contains(key),
            "RouteSelector's decoder no longer accepts {key}; the wire doc promises it. \
             Key list is: {list}"
        );
    }
}

/// One decoder, not three. Three copies would drift, and the defect
/// being closed here was exactly one route's worth of a shape the
/// other two shared.
#[test]
fn all_three_tap_routes_share_one_selector_decoder() {
    for (name, src) in [
        ("TapRoute", TAP),
        ("DoubleTapRoute", DOUBLE_TAP),
        ("LongPressRoute", LONG_PRESS),
    ] {
        assert!(
            src.contains("RouteSelector.decode"),
            "{name} decodes its selector itself instead of sharing RouteSelector"
        );
    }
}

/// The Swift keys are the Rust enum's keys. `Selector` is untagged, so
/// the key IS the discriminant on both sides; if they drifted, a body
/// the Rust side produced would decode to a different form — or to
/// nothing — in the runner.
#[test]
fn selector_wire_keys_match_rust_selector_enum() {
    use smix_selector::{Modifiers, Pattern, Selector};

    let cases = [
        (
            Selector::Text {
                text: Pattern::Text("x".into()),
                modifiers: Modifiers::default(),
            },
            "text",
        ),
        (
            Selector::Id {
                id: "x".into(),
                modifiers: Modifiers::default(),
            },
            "id",
        ),
        (
            Selector::Label {
                label: "x".into(),
                modifiers: Modifiers::default(),
            },
            "label",
        ),
    ];

    for (selector, key) in cases {
        let json = serde_json::to_value(&selector).expect("selector serializes");
        assert!(
            json.get(key).is_some(),
            "Rust {selector:?} does not serialize under {key:?}, but the runner decodes that key"
        );
    }
}
