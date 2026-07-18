//! Two claims from the boundary audit, probed rather than reasoned about.
//!
//! 1. `AnchorBox` has no `ancestor` key while all three SDKs emit one,
//!    so `serde` drops it and the candidate set is silently wider than
//!    the caller asked for — the quietest possible failure.
//! 2. `Selector` is untagged, `Anchor` is declared before
//!    `AnchorRelative`, and `AnchorBox`'s fields are all optional with
//!    `#[serde(default)]`, so an `AnchorRelative` payload may be
//!    swallowed by the `Anchor` arm and never reach its own.

use smix_selector::Selector;

#[test]
fn an_anchor_box_keeps_its_ancestor_key() {
    let json = r#"{"anchor":{"ancestor":{"role":"dialog"}},"nth":0}"#;
    let sel: Selector = serde_json::from_str(json).expect("parses");
    let Selector::Anchor { anchor, .. } = &sel else {
        panic!("expected Anchor, got {sel:?}");
    };
    assert!(
        anchor.ancestor.is_some(),
        "the ancestor sub-selector was dropped on the way in: {anchor:?}"
    );
}

#[test]
fn an_anchor_relative_payload_reaches_the_anchor_relative_arm() {
    let json = r#"{"anchor":{"id":"icon-row"},"dx":-0.15,"dy":0.0}"#;
    let sel: Selector = serde_json::from_str(json).expect("parses");
    assert!(
        matches!(sel, Selector::AnchorRelative { .. }),
        "untagged matching sent an AnchorRelative payload to {sel:?}"
    );
}
