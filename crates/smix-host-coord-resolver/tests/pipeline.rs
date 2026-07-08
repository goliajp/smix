//! v3.2 c5 — host-coord-resolver pure-pipeline tests.

use smix_host_coord_resolver::{HostResolveError, resolve_to_norm_coord};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn mk(label: Option<&str>, bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        role: None,
        identifier: None,
        label: label.map(String::from),
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

fn tree_with(label: &str, bounds: Rect) -> A11yNode {
    let mut root = mk(None, rect(0.0, 0.0, 390.0, 844.0));
    root.raw_type = "application".into();
    root.children.push(mk(Some(label), bounds));
    root
}

fn text_sel(t: &str) -> Selector {
    Selector::Text {
        text: Pattern::text(t),
        modifiers: Modifiers::default(),
    }
}

#[test]
fn happy_path_returns_centered_normalized_coord() {
    let tree = tree_with("Login", rect(50.0, 100.0, 200.0, 40.0));
    let (nx, ny) = resolve_to_norm_coord(&tree, &text_sel("Login")).expect("ok");
    // centroid = (150, 120); app frame 390x844 → (150/390, 120/844)
    assert!((nx - 150.0 / 390.0).abs() < 1e-9);
    assert!((ny - 120.0 / 844.0).abs() < 1e-9);
}

#[test]
fn not_found_returns_not_found_error() {
    let tree = tree_with("Other", rect(50.0, 100.0, 200.0, 40.0));
    let err = resolve_to_norm_coord(&tree, &text_sel("Login")).unwrap_err();
    assert_eq!(err, HostResolveError::NotFound);
}

#[test]
fn unknown_app_frame_returns_unknown_app_frame() {
    // tree.bounds w/h<=0; node has valid bounds. visibility filter passes
    // node through (conservative-pass branch when root is unknown), so
    // resolver returns the node. Then resolve_to_norm_coord short-
    // circuits on the `app_frame.w<=0` check → UnknownAppFrame.
    let mut tree = mk(None, rect(0.0, 0.0, 0.0, 0.0));
    tree.raw_type = "application".into();
    tree.children
        .push(mk(Some("Found"), rect(50.0, 50.0, 10.0, 10.0)));
    let err = resolve_to_norm_coord(&tree, &text_sel("Found")).unwrap_err();
    assert_eq!(err, HostResolveError::UnknownAppFrame);
}

#[test]
fn centroid_at_edge_returns_centroid_out_of_frame() {
    // tree 500x844; node (490, 100, 20, 10). Partial-overlap means
    // visibility filter passes (intersection > 0). centroid (500, 105);
    // nx = 500/500 = 1.0 → triggers `nx >= 1.0` reject.
    let mut root = mk(None, rect(0.0, 0.0, 500.0, 844.0));
    root.raw_type = "application".into();
    root.children
        .push(mk(Some("Edge"), rect(490.0, 100.0, 20.0, 10.0)));
    let err = resolve_to_norm_coord(&root, &text_sel("Edge")).unwrap_err();
    match err {
        HostResolveError::CentroidOutOfFrame { nx, .. } => {
            assert!(nx >= 1.0, "expected nx>=1, got {nx}");
        }
        other => panic!("expected CentroidOutOfFrame, got {other:?}"),
    }
}

#[test]
fn id_selector_resolves_correctly() {
    let mut root = mk(None, rect(0.0, 0.0, 390.0, 844.0));
    let mut node = mk(None, rect(100.0, 200.0, 100.0, 100.0));
    node.identifier = Some("btn-x".into());
    root.children.push(node);
    let sel = Selector::Id {
        id: "btn-x".into(),
        modifiers: Modifiers::default(),
    };
    let (nx, ny) = resolve_to_norm_coord(&root, &sel).expect("ok");
    assert!((nx - 150.0 / 390.0).abs() < 1e-9);
    assert!((ny - 250.0 / 844.0).abs() < 1e-9);
}
