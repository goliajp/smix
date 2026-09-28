//! `focused` names the field input goes to, never a key of the keyboard.
//!
//! The keyboard's window is part of the tree, so its nodes are matched
//! like any other. One that reports focus answered `focused`, and a
//! fill with no field named tapped its centre first — pressing a key.

use smix_screen::{A11yNode, Rect, Role};
use smix_selector::{Selector, True};
use smix_selector_resolver::resolve_selector_all;

fn node(id: &str, y: f64, focus: bool) -> A11yNode {
    A11yNode {
        visible_bounds: None,
        hittable: None,
        window: None,
        unreadable_windows: None,
        raw_type: "android.widget.EditText".into(),
        element_type_raw: 1,
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
            w: 1080.0,
            h: 120.0,
        },
        enabled: true,
        selected: false,
        has_focus: focus,
        visible: true,
        children: vec![],
    }
}

fn keyboard_with_a_focused_key() -> A11yNode {
    let mut keyboard = node("", 1500.0, false);
    keyboard.raw_type = "android.widget.FrameLayout".into();
    keyboard.role = Some(Role::Keyboard);
    keyboard.bounds.h = 840.0;
    keyboard.children = vec![node("key_1", 1680.0, true)];
    keyboard
}

fn screen(field_has_focus: bool) -> A11yNode {
    let mut root = node("", 0.0, false);
    root.raw_type = "android.view.WindowRoot".into();
    root.bounds.h = 2340.0;
    root.children = vec![
        node("code", 546.0, field_has_focus),
        keyboard_with_a_focused_key(),
    ];
    root
}

fn focused() -> Selector {
    Selector::Focused {
        focused: True(true),
    }
}

fn ids(found: &[&A11yNode]) -> Vec<String> {
    found
        .iter()
        .map(|n| n.identifier.clone().unwrap_or_default())
        .collect()
}

#[test]
fn the_field_with_focus_is_the_answer_and_the_key_is_not() {
    let tree = screen(true);
    assert_eq!(ids(&resolve_selector_all(&tree, &focused())), vec!["code"]);
}

#[test]
fn a_focused_key_alone_is_no_answer() {
    let tree = screen(false);
    assert!(
        resolve_selector_all(&tree, &focused()).is_empty(),
        "with no field holding focus there is nothing to type into; a key \
         is not a field"
    );
}

#[test]
fn a_key_is_still_found_when_it_is_named() {
    // Only `focused` passes over the keyboard: naming a key still finds it.
    let tree = screen(false);
    let key = Selector::Id {
        id: "key_1".into(),
        modifiers: Default::default(),
    };
    assert_eq!(ids(&resolve_selector_all(&tree, &key)), vec!["key_1"]);
}
