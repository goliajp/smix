//! A platform date picker keeps the months beside the shown one in its
//! tree: each page hidden, its days visible. Centred on a tablet, the
//! hidden pages' days sit on the screen too, and `text: "1"` found the
//! previous month's first day.

use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};
use smix_selector_resolver::resolve_selector;

fn node(label: &str, text: &str, x: f64, visible: bool, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        visible_bounds: None,
        hittable: None,
        window: None,
        unreadable_windows: None,
        raw_type: "android.view.View".into(),
        element_type_raw: 0,
        role: None,
        identifier: None,
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: (!text.is_empty()).then(|| text.into()),
        bounds: Rect {
            x,
            y: 1052.0,
            w: 97.0,
            h: 110.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible,
        children,
    }
}

fn picker() -> A11yNode {
    // the bounds of the three pages as they stand with the dialog centred
    // on a 2560-wide tablet: all three on the screen
    let october = node(
        "",
        "",
        700.0,
        false,
        vec![node("01 October 2026", "1", 700.0, true, vec![])],
    );
    let november = node(
        "",
        "",
        1100.0,
        true,
        vec![node("01 November 2026", "1", 1100.0, true, vec![])],
    );
    let december = node(
        "",
        "",
        1500.0,
        false,
        vec![node("01 December 2026", "1", 1500.0, true, vec![])],
    );
    let mut root = node("", "", 0.0, true, vec![october, november, december]);
    root.bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 2560.0,
        h: 1600.0,
    };
    root
}

fn one() -> Selector {
    Selector::Text {
        text: Pattern::Text("1".into()),
        modifiers: Modifiers::default(),
    }
}

#[test]
fn the_shown_page_is_found_before_a_hidden_one() {
    let tree = picker();
    let found = resolve_selector(&tree, &one()).expect("found");
    assert_eq!(found.label.as_deref(), Some("01 November 2026"));
}

#[test]
fn a_match_under_a_hidden_ancestor_alone_is_still_found() {
    // only the hidden page carries the text: not set aside for nothing
    let mut tree = picker();
    tree.children.remove(1);
    let found = resolve_selector(&tree, &one()).expect("still found");
    assert_eq!(found.label.as_deref(), Some("01 October 2026"));
}
