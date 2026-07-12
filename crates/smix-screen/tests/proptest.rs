//! v3.3 c5 — property-based tests for smix-screen visibility primitives.
//!
//! Geometric invariants the implementation must hold for any valid Rect:
//! - `visible_area >= 0` always
//! - `visible_area > 0` ⇒ `is_visible_enough = true` (when tree.bounds is known)
//! - `visible_area == 0` ⇒ `is_visible_enough = false` (when tree.bounds is known)
//! - Zero-or-negative bounds always yield `is_visible_enough = false`
//! - Unknown root (tree.bounds w/h ≤ 0) ⇒ conservative pass on positive-bounds node

use proptest::prelude::*;
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

prop_compose! {
    fn arb_finite_rect()(
        x in -1000.0f64..1000.0,
        y in -1000.0f64..1000.0,
        w in -100.0f64..1000.0,
        h in -100.0f64..1000.0,
    ) -> Rect {
        Rect { x, y, w, h }
    }
}

proptest! {
    /// `visible_area` is always non-negative for finite inputs.
    #[test]
    fn visible_area_non_negative(
        node_bounds in arb_finite_rect(),
        tree_bounds in arb_finite_rect(),
    ) {
        let node = mk(node_bounds);
        let tree = mk(tree_bounds);
        let a = visible_area(&node, &tree);
        prop_assert!(a >= 0.0, "visible_area returned negative: {}", a);
    }

    /// Zero-or-negative w/h always means invisible.
    #[test]
    fn zero_bounds_always_invisible(
        x in -1000.0f64..1000.0,
        y in -1000.0f64..1000.0,
        zero_or_neg in -100.0f64..=0.0,
        tree_bounds in arb_finite_rect(),
    ) {
        let node = mk(Rect { x, y, w: zero_or_neg, h: 50.0 });
        let tree = mk(tree_bounds);
        prop_assert!(!is_visible_enough(&node, &tree));

        let node2 = mk(Rect { x, y, w: 50.0, h: zero_or_neg });
        prop_assert!(!is_visible_enough(&node2, &tree));
    }

    /// `is_visible_enough` ↔ `visible_area > 0` when tree.bounds is known.
    #[test]
    fn visible_area_matches_predicate(
        node_bounds in arb_finite_rect(),
        // tree.bounds with positive w/h (known root)
        tx in -100.0f64..100.0,
        ty in -100.0f64..100.0,
        tw in 1.0f64..1000.0,
        th in 1.0f64..1000.0,
    ) {
        let node = mk(node_bounds);
        let tree = mk(Rect { x: tx, y: ty, w: tw, h: th });
        let a = visible_area(&node, &tree);
        let v = is_visible_enough(&node, &tree);
        if node_bounds.w > 0.0 && node_bounds.h > 0.0 {
            prop_assert_eq!(a > 0.0, v,
                "visible_area={} vs is_visible_enough={} mismatch (node={:?}, tree={:?})",
                a, v, node_bounds, Rect { x: tx, y: ty, w: tw, h: th });
        }
    }

    /// Unknown root (tree.bounds w/h ≤ 0) ⇒ positive-bounds node is conservatively visible.
    #[test]
    fn unknown_root_conservative_pass(
        nx in -1000.0f64..1000.0,
        ny in -1000.0f64..1000.0,
        nw in 0.001f64..1000.0,
        nh in 0.001f64..1000.0,
        tx in -1000.0f64..1000.0,
        ty in -1000.0f64..1000.0,
    ) {
        let node = mk(Rect { x: nx, y: ny, w: nw, h: nh });
        // tree bounds zero — conservative pass.
        let tree = mk(Rect { x: tx, y: ty, w: 0.0, h: 0.0 });
        prop_assert!(is_visible_enough(&node, &tree));
    }
}
