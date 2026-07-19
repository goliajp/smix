//! Spatial modifiers must survive the yaml parser.
//!
//! `Modifiers` carries near / below / above / leftOf / rightOf /
//! inside / ancestor / nth / first / last; the resolver honours all of
//! them, `smix-selector` serializes all of them, the selector guide
//! documents them and the SDK READMEs print the wire JSON with them.
//! The yaml parser read none of them. Every documented modifier parsed
//! without error and arrived as `Modifiers::default()`, so a flow
//! written to disambiguate — `below: { text: "Settings" }` — resolved
//! as if the line were not there and tapped whatever matched first.
//! `tapOn` honoured a single one, spelled `index:`, which the guide
//! calls `nth:`.
//!
//! Silent, because a dropped modifier cannot fail: it just widens the
//! match.

use smix_adapter_maestro::{Step, parse_flow_yaml};
use smix_selector::Selector;

fn selector_of(body: &str) -> Selector {
    let yaml = format!("appId: com.example.app\n---\n{body}");
    let flow = parse_flow_yaml(&yaml).expect("parses");
    match flow.steps.into_iter().next().expect("one step") {
        Step::TapOn { selector, .. } => selector,
        Step::AssertVisible { selector } => selector,
        other => panic!("unexpected step {other:?}"),
    }
}

fn modifiers_of(body: &str) -> smix_selector::Modifiers {
    match selector_of(body) {
        Selector::Text { modifiers, .. } => modifiers,
        Selector::Id { modifiers, .. } => modifiers,
        Selector::Label { modifiers, .. } => modifiers,
        Selector::Role { modifiers, .. } => modifiers,
        other => panic!("selector without modifiers: {other:?}"),
    }
}

#[test]
fn tap_on_keeps_a_below_modifier() {
    let m = modifiers_of("- tapOn:\n    text: \"Edit\"\n    below: { text: \"Settings\" }\n");
    assert!(
        m.below.is_some(),
        "below: dropped — the tap resolves against every `Edit` on screen"
    );
}

#[test]
fn assert_visible_keeps_a_below_modifier() {
    let m =
        modifiers_of("- assertVisible:\n    text: \"Edit\"\n    below: { text: \"Settings\" }\n");
    assert!(m.below.is_some(), "below: dropped on the assert path");
}

#[test]
fn every_spatial_modifier_survives() {
    for (key, present) in [
        (
            "near",
            (|m: &smix_selector::Modifiers| m.near.is_some())
                as fn(&smix_selector::Modifiers) -> bool,
        ),
        ("below", |m| m.below.is_some()),
        ("above", |m| m.above.is_some()),
        ("leftOf", |m| m.left_of.is_some()),
        ("rightOf", |m| m.right_of.is_some()),
        ("inside", |m| m.inside.is_some()),
        ("ancestor", |m| m.ancestor.is_some()),
    ] {
        let body = format!("- tapOn:\n    text: \"Edit\"\n    {key}: {{ text: \"Anchor\" }}\n");
        assert!(
            present(&modifiers_of(&body)),
            "`{key}:` parsed but did not arrive"
        );
    }
}

#[test]
fn nth_is_the_documented_spelling_and_index_still_works() {
    // The guide says `nth`; the parser only ever read `index`. Both
    // land, because flows in the wild were written against the code.
    assert_eq!(
        modifiers_of("- tapOn:\n    text: \"Edit\"\n    nth: 2\n").nth,
        Some(2)
    );
    assert_eq!(
        modifiers_of("- tapOn:\n    text: \"Edit\"\n    index: 2\n").nth,
        Some(2)
    );
    assert_eq!(
        modifiers_of("- assertVisible:\n    text: \"Edit\"\n    nth: 2\n").nth,
        Some(2)
    );
}

#[test]
fn first_and_last_survive() {
    assert_eq!(
        modifiers_of("- tapOn:\n    text: \"Edit\"\n    first: true\n").first,
        Some(true)
    );
    assert_eq!(
        modifiers_of("- tapOn:\n    text: \"Edit\"\n    last: true\n").last,
        Some(true)
    );
}

#[test]
fn a_modifier_on_an_id_selector_survives_too() {
    let m = modifiers_of("- tapOn:\n    id: \"row-btn\"\n    inside: { id: \"row-3\" }\n");
    assert!(m.inside.is_some(), "modifiers must not be text-only");
}

#[test]
fn an_unknown_key_beside_a_selector_is_refused() {
    // The reason the drop went unnoticed for so long: an unread key is
    // indistinguishable from an honoured one. Refusing what is not
    // understood is what makes the next drift loud.
    let yaml =
        "appId: com.example.app\n---\n- tapOn:\n    text: \"Edit\"\n    beneath: { text: \"x\" }\n";
    let err = parse_flow_yaml(yaml).expect_err("unknown selector key must not be swallowed");
    assert!(
        format!("{err}").contains("beneath"),
        "the error must name the key: {err}"
    );
}
