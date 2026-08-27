//! A `fallback:` chain is tried layer by layer, wherever a selector is
//! resolved.
//!
//! The resolver used to answer `false` for a chain, on the grounds that
//! "the adapter dispatches it". Three verbs kept that bargain —
//! `extendedWaitUntil`, `tapOn`, and the OCR probe — and every other
//! one never heard of it, so `assertVisible` with a chain matched
//! nothing. A consumer reported it on Android; this repository's own
//! iOS corpus had noticed the same thing months earlier and left it
//! unexplained. An invariant a comment asserts and nothing enforces is
//! the shape of both.
//!
//! Order is a promise, not an implementation detail: `[id, text]` means
//! "prefer the id", and a chain that resolved to whichever matched
//! would silently pick differently as an app's copy changed.

use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::{resolve_selector, resolve_selector_all};

fn node(id: &str, label: &str) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "staticText".into(),
        element_type_raw: 48,
        role: None,
        identifier: Some(id.into()),
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn screen() -> A11yNode {
    let mut root = node("root", "root");
    root.bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 400.0,
        h: 800.0,
    };
    root.children = vec![node("tab-device", "Devices"), node("other", "tab-device")];
    root
}

fn by_id(id: &str) -> Selector {
    Selector::Id {
        id: id.into(),
        modifiers: Modifiers::default(),
    }
}

fn by_label(label: &str) -> Selector {
    Selector::Label {
        label: label.into(),
        modifiers: Modifiers::default(),
    }
}

fn chain(items: Vec<Selector>) -> Selector {
    Selector::Fallback { fallback: items }
}

#[test]
fn the_second_layer_is_tried_when_the_first_misses() {
    let tree = screen();
    let hit = resolve_selector(&tree, &chain(vec![by_id("nope"), by_label("tab-device")]));
    assert_eq!(
        hit.and_then(|n| n.identifier.as_deref()),
        Some("other"),
        "the id misses and the label matches — the chain must reach it"
    );
}

#[test]
fn the_first_layer_wins_when_both_match() {
    let tree = screen();
    // `tab-device` is an id on one node and a label on another. The
    // chain says prefer the id.
    let hit = resolve_selector(
        &tree,
        &chain(vec![by_id("tab-device"), by_label("tab-device")]),
    );
    assert_eq!(
        hit.and_then(|n| n.identifier.as_deref()),
        Some("tab-device"),
        "order is the promise: the first layer that matches is the answer"
    );
}

#[test]
fn a_chain_that_matches_nothing_resolves_to_nothing() {
    let tree = screen();
    assert!(resolve_selector(&tree, &chain(vec![by_id("nope"), by_label("also-nope")])).is_none());
}

#[test]
fn an_empty_chain_matches_nothing_rather_than_everything() {
    // A chain with no layers offers no way to identify anything. The
    // empty case must not fall through to "no constraint, so the first
    // node will do".
    let tree = screen();
    assert!(resolve_selector(&tree, &chain(vec![])).is_none());
    assert!(resolve_selector_all(&tree, &chain(vec![])).is_empty());
}

#[test]
fn a_nested_chain_is_flattened_in_order() {
    let tree = screen();
    let nested = chain(vec![
        chain(vec![by_id("nope"), by_id("still-nope")]),
        by_label("tab-device"),
    ]);
    assert_eq!(
        resolve_selector(&tree, &nested).and_then(|n| n.identifier.as_deref()),
        Some("other"),
    );
}

#[test]
fn all_returns_the_first_layer_that_matched_not_the_union() {
    // Both layers match, on different nodes. A union would hand back
    // two elements for a selector whose whole meaning is "use the first
    // of these that works".
    let tree = screen();
    let got = resolve_selector_all(
        &tree,
        &chain(vec![by_id("tab-device"), by_label("tab-device")]),
    );
    assert_eq!(
        got.len(),
        1,
        "the chain picks a layer, it does not merge them"
    );
    assert_eq!(got[0].identifier.as_deref(), Some("tab-device"));
}

#[test]
fn a_pattern_that_cannot_compile_does_not_take_the_whole_chain_down() {
    // The layer is unusable; the ones after it are not.
    let tree = screen();
    let bad = Selector::Text {
        text: Pattern::Regex {
            regex: "(".into(),
            flags: "i".into(),
        },
        modifiers: Modifiers::default(),
    };
    assert_eq!(
        resolve_selector(&tree, &chain(vec![bad, by_label("tab-device")]))
            .and_then(|n| n.identifier.as_deref()),
        Some("other"),
    );
}

#[test]
fn the_compiled_path_walks_the_chain_too() {
    // `wait_for` — and so `assertVisible` — polls through the compiled
    // variant with a context built once. Fixing only the plain entry
    // points would have left the verb the defect was reported against
    // answering exactly as before.
    use smix_selector_resolver::{
        ResolverContext, resolve_selector_all_compiled, resolve_selector_compiled,
    };

    let tree = screen();
    let sel = chain(vec![by_id("nope"), by_label("tab-device")]);
    let ctx = ResolverContext::new(&sel).expect("a chain must build a context");

    assert_eq!(
        resolve_selector_compiled(&tree, &sel, &ctx).and_then(|n| n.identifier.as_deref()),
        Some("other"),
    );
    assert_eq!(resolve_selector_all_compiled(&tree, &sel, &ctx).len(), 1);
}
