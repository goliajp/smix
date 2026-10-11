//! A selector that names an element the tree holds off screen.
//!
//! Resolution drops it, as it must for acting on it; the miss is then
//! explained from `matched_off_screen`, which finds it again.

use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Selector};
use smix_selector_resolver::{matched_off_screen, resolve_selector};

fn node(id: &str, y: f64) -> A11yNode {
    A11yNode {
        visible_bounds: None,
        hittable: None,
        window: None,
        unreadable_windows: None,
        raw_type: "button".into(),
        element_type_raw: 9,
        role: None,
        identifier: Some(id.into()),
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect {
            x: 0.0,
            y,
            w: 402.0,
            h: 66.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn list() -> A11yNode {
    let mut root = node("root", 0.0);
    root.bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 402.0,
        h: 874.0,
    };
    root.children = vec![node("row_0", 106.0), node("row_20", 1426.0)];
    root
}

fn by_id(id: &str) -> Selector {
    Selector::Id {
        id: id.into(),
        modifiers: Modifiers::default(),
    }
}

#[test]
fn a_row_below_the_screen_is_not_resolved_and_is_named_off_screen() {
    let tree = list();
    assert!(resolve_selector(&tree, &by_id("row_20")).is_none());
    let off = matched_off_screen(&tree, &by_id("row_20"));
    assert_eq!(
        off.iter().map(|n| n.bounds.y).collect::<Vec<_>>(),
        vec![1426.0]
    );
}

#[test]
fn a_row_on_screen_or_absent_is_not_named_off_screen() {
    let tree = list();
    assert!(resolve_selector(&tree, &by_id("row_0")).is_some());
    assert!(matched_off_screen(&tree, &by_id("row_0")).is_empty());
    assert!(matched_off_screen(&tree, &by_id("row_99")).is_empty());
}

#[test]
fn a_chain_answers_for_its_first_layer_held_off_screen() {
    let chain = Selector::Fallback {
        fallback: vec![by_id("row_99"), by_id("row_20")],
    };
    assert_eq!(matched_off_screen(&list(), &chain).len(), 1);
}
