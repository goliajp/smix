//! A two-key selector resolves to the element that is both.
//!
//! `{ id: X, text: Y }` parses to `Id { X }` carrying `Y` in
//! `modifiers.and`. Parsing it is half the job: until the resolver
//! reads that list, the constraint is carried and ignored, which is
//! the same wrong answer as before with a longer paper trail.
//!
//! Two nodes sharing a label is the case that tells them apart — it is
//! also the case the guides' own examples are about, where several
//! rows read "1" and only one of them is the counter.

use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::resolve_selector_all as resolve;

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
    root.children = vec![node("x", "foo"), node("y", "foo")];
    root
}

fn id_and_text(id: &str, text: &str) -> Selector {
    Selector::Id {
        id: id.into(),
        modifiers: Modifiers {
            and: vec![Selector::Text {
                text: Pattern::Text(text.into()),
                modifiers: Modifiers::default(),
            }],
            ..Default::default()
        },
    }
}

#[test]
fn the_conjunction_picks_the_one_element_that_is_both() {
    let tree = screen();
    let hits = resolve(&tree, &id_and_text("x", "foo"));
    let ids: Vec<&str> = hits
        .iter()
        .filter_map(|n| n.identifier.as_deref())
        .collect();
    assert_eq!(ids, vec!["x"], "wanted only the node that is both");

    let hits = resolve(&tree, &id_and_text("y", "foo"));
    let ids: Vec<&str> = hits
        .iter()
        .filter_map(|n| n.identifier.as_deref())
        .collect();
    assert_eq!(
        ids,
        vec!["y"],
        "the other half has to select the other node"
    );
}

#[test]
fn a_constraint_that_no_node_satisfies_selects_nothing() {
    // The half that was being dropped: with `and` ignored, this
    // resolves to the id and reports a match for text nobody has.
    let tree = screen();
    let hits = resolve(&tree, &id_and_text("x", "bar"));
    assert!(
        hits.is_empty(),
        "matched an element whose text is not what was asked for"
    );
}

#[test]
fn no_constraint_behaves_as_before() {
    let tree = screen();
    let plain = Selector::Id {
        id: "x".into(),
        modifiers: Modifiers::default(),
    };
    let ids: Vec<&str> = resolve(&tree, &plain)
        .iter()
        .filter_map(|n| n.identifier.as_deref())
        .collect();
    assert_eq!(ids, vec!["x"]);
}
