//! Unit tests for smix-screen visibility primitives.
//!
//! Behavioral contract covered: zero-bounds reject / unknown-root pass /
//! intersection check / cases F-K viewport contains children.

use smix_screen::{A11yNode, Rect, is_visible_enough, visible_area};

fn mk(bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds,
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

// ---- is_visible_enough --------------------------------------------------

#[test]
fn is_visible_enough_zero_width_returns_false() {
    let node = mk(rect(0.0, 0.0, 0.0, 10.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert!(!is_visible_enough(&node, &tree));
}

#[test]
fn is_visible_enough_zero_height_returns_false() {
    let node = mk(rect(0.0, 0.0, 10.0, 0.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert!(!is_visible_enough(&node, &tree));
}

#[test]
fn is_visible_enough_negative_width_returns_false() {
    let node = mk(rect(0.0, 0.0, -1.0, 10.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert!(!is_visible_enough(&node, &tree));
}

#[test]
fn is_visible_enough_unknown_root_conservative_pass() {
    // A tree whose bounds have w/h ≤ 0 means the viewport is unknown, so
    // visibility can't be judged — pass conservatively rather than reject.
    let node = mk(rect(50.0, 50.0, 10.0, 10.0));
    let tree = mk(rect(0.0, 0.0, 0.0, 0.0));
    assert!(is_visible_enough(&node, &tree));
}

#[test]
fn is_visible_enough_in_view_passes() {
    let node = mk(rect(20.0, 100.0, 200.0, 44.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert!(is_visible_enough(&node, &tree));
}

#[test]
fn is_visible_enough_completely_offscreen_returns_false() {
    let node = mk(rect(500.0, 1000.0, 50.0, 50.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert!(!is_visible_enough(&node, &tree));
}

#[test]
fn is_visible_enough_partial_overlap_passes() {
    // node.x range = [380, 400] overlaps tree.x range = [0, 390] in (380, 390).
    let node = mk(rect(380.0, 100.0, 20.0, 20.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert!(is_visible_enough(&node, &tree));
}

#[test]
fn is_visible_enough_edge_touching_not_enough() {
    // node.x = 390 (= tree.x + tree.w), 0-width intersection → false.
    let node = mk(rect(390.0, 100.0, 20.0, 20.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert!(!is_visible_enough(&node, &tree));
}

// ---- visible_area -------------------------------------------------------

#[test]
fn visible_area_zero_bounds_returns_zero() {
    let node = mk(rect(50.0, 50.0, 0.0, 0.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert_eq!(visible_area(&node, &tree), 0.0);
}

#[test]
fn visible_area_unknown_root_returns_node_area() {
    let node = mk(rect(50.0, 50.0, 10.0, 5.0));
    let tree = mk(rect(0.0, 0.0, 0.0, 0.0));
    assert_eq!(visible_area(&node, &tree), 50.0); // 10 * 5
}

#[test]
fn visible_area_fully_inside_returns_node_area() {
    let node = mk(rect(50.0, 100.0, 10.0, 20.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert_eq!(visible_area(&node, &tree), 200.0); // 10 * 20
}

#[test]
fn visible_area_partial_overlap_returns_intersection() {
    // node.x range = [380, 410], tree.x range = [0, 390]. overlap x = [380, 390], width = 10.
    // node.y range = [100, 120], fully inside tree.y range. height = 20.
    let node = mk(rect(380.0, 100.0, 30.0, 20.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert_eq!(visible_area(&node, &tree), 200.0); // 10 * 20
}

#[test]
fn visible_area_offscreen_returns_zero() {
    let node = mk(rect(500.0, 1000.0, 50.0, 50.0));
    let tree = mk(rect(0.0, 0.0, 390.0, 844.0));
    assert_eq!(visible_area(&node, &tree), 0.0);
}

// ---- serde JSON round-trip (wire compat with swift-bridge /tree) --------

#[test]
fn a11y_node_serde_round_trip_camel_case() {
    // Wire (Swift side) emits camelCase (rawType, placeholderValue, hasFocus).
    // Our serde rename_all = "camelCase" must round-trip identically.
    let json = r#"{
        "rawType": "staticText",
        "role": "staticText",
        "label": "Hello",
        "bounds": { "x": 0, "y": 0, "w": 10, "h": 10 },
        "enabled": true,
        "selected": false,
        "hasFocus": false,
        "visible": true,
        "children": []
    }"#;
    let node: A11yNode = serde_json::from_str(json).expect("parse");
    assert_eq!(node.label.as_deref(), Some("Hello"));
    assert!(node.children.is_empty());
    let reserialized = serde_json::to_string(&node).expect("serialize");
    let parsed_back: A11yNode = serde_json::from_str(&reserialized).expect("re-parse");
    assert_eq!(node, parsed_back);
}

#[test]
fn a11y_node_serde_terminal_node_no_children_field() {
    // Forward-compat: a JSON terminal that omits "children" entirely should
    // still parse (default-derived to Vec::new()).
    let json = r#"{
        "rawType": "other",
        "bounds": { "x": 0, "y": 0, "w": 10, "h": 10 },
        "enabled": true,
        "selected": false,
        "hasFocus": false,
        "visible": true
    }"#;
    let node: A11yNode = serde_json::from_str(json).expect("parse");
    assert!(node.children.is_empty());
}
