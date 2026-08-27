//! A swipe inside an element needs two points that are not the centre.
//!
//! `resolve_to_norm_coord` answers with the centroid, which is what a tap
//! wants. `swipe: { over: <selector>, from: 0.3, to: 0.8 }` wants three
//! tenths down the element's box and four fifths down it — so that a flow
//! dragging a timeline stops depending on where that timeline happens to
//! sit. A consumer measured theirs at 45.3–50.5% of screen height on
//! Android and 47.7–53.2% on iOS, took 49% as the overlap, and wrote down
//! that a device of a different shape would need measuring again.
//!
//! The refusals matter as much as the arithmetic. Nothing here clamps: a
//! share outside the element, or a point outside the app frame, is a
//! caller asking for somewhere they did not mean, and quietly moving it
//! to the edge would swipe there and report success.

use smix_host_coord_resolver::{HostResolveError, resolve_to_norm_point_in_box};
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Selector};

fn node(id: &str, x: f64, y: f64, w: f64, h: f64, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: Some(id.into()),
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: Rect { x, y, w, h },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

fn id_sel(id: &str) -> Selector {
    Selector::Id {
        id: id.into(),
        modifiers: Modifiers::default(),
    }
}

/// A 1000×1000 app frame with a 200-tall strip a quarter of the way down.
fn tree() -> A11yNode {
    node(
        "root",
        0.0,
        0.0,
        1000.0,
        1000.0,
        vec![node("timeline", 100.0, 250.0, 800.0, 200.0, vec![])],
    )
}

fn timeline() -> Selector {
    id_sel("timeline")
}

#[test]
fn three_tenths_across_is_three_tenths_of_the_box() {
    let (nx, ny) =
        resolve_to_norm_point_in_box(&tree(), &timeline(), (0.3, 0.5)).expect("resolves");
    // x: 100 + 800*0.3 = 340 → 0.34 of the frame. y: 250 + 200*0.5 = 350.
    assert!((nx - 0.34).abs() < 1e-9, "nx={nx}");
    assert!((ny - 0.35).abs() < 1e-9, "ny={ny}");
}

#[test]
fn the_centre_share_agrees_with_the_centre() {
    // The one share where this and `resolve_to_norm_coord` must say the
    // same thing. If they ever disagree, one of them is computing the
    // box differently.
    let (nx, ny) =
        resolve_to_norm_point_in_box(&tree(), &timeline(), (0.5, 0.5)).expect("resolves");
    let (cx, cy) =
        smix_host_coord_resolver::resolve_to_norm_coord(&tree(), &timeline()).expect("resolves");
    assert!(
        (nx - cx).abs() < 1e-9 && (ny - cy).abs() < 1e-9,
        "{nx},{ny} vs {cx},{cy}"
    );
}

#[test]
fn a_share_outside_the_element_is_refused_not_clamped() {
    // 100 + 800*1.5 = 1300, past the right edge of a 1000-wide frame.
    // Clamping would swipe at the edge and report having done what was
    // asked.
    let e = resolve_to_norm_point_in_box(&tree(), &timeline(), (1.5, 0.5)).unwrap_err();
    assert!(
        matches!(e, HostResolveError::CentroidOutOfFrame { .. }),
        "got {e:?}"
    );
}

#[test]
fn an_element_with_no_box_never_reaches_the_arithmetic() {
    // It answers NotFound rather than EmptyMatchedFrame, and that is the
    // resolver's doing: matching drops a zero-size node before this
    // function sees it. The empty-frame guard is therefore unreachable
    // through this door — kept because it is the same guard
    // `resolve_to_norm_coord` carries, and a divide it protects against
    // is worth a branch whichever door the node arrives by.
    //
    // Asserted as what happens rather than as what I first assumed, which
    // was EmptyMatchedFrame.
    let t = node(
        "root",
        0.0,
        0.0,
        1000.0,
        1000.0,
        vec![node("flat", 10.0, 10.0, 0.0, 0.0, vec![])],
    );
    let e = resolve_to_norm_point_in_box(&t, &id_sel("flat"), (0.5, 0.5)).unwrap_err();
    assert!(matches!(e, HostResolveError::NotFound), "got {e:?}");
}

#[test]
fn a_selector_that_matches_nothing_is_refused() {
    let e = resolve_to_norm_point_in_box(&tree(), &id_sel("nope"), (0.5, 0.5)).unwrap_err();
    assert!(matches!(e, HostResolveError::NotFound), "got {e:?}");
}
