//! v3.3 c5 — property-based tests for smix-host-coord-resolver.
//!
//! Invariants:
//! - `resolve_to_norm_coord` Ok result `(nx, ny)` always in `(0, 1)`.
//! - Centroid of a positive-w-positive-h Rect lies geometrically inside it.

use proptest::prelude::*;
use smix_host_coord_resolver::resolve_to_norm_coord;
use smix_screen::{A11yNode, Rect};
use smix_selector::{Modifiers, Pattern, Selector};

fn mk(label: Option<String>, bounds: Rect) -> A11yNode {
    A11yNode {
        raw_type: "other".into(),
        role: None,
        identifier: None,
        label,
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

proptest! {
    /// Successful resolve always yields `(nx, ny)` in open interval `(0, 1)`.
    #[test]
    fn ok_result_in_unit_square(
        // app frame
        aw in 100.0f64..2000.0,
        ah in 100.0f64..2000.0,
        // target node within app frame
        tx in 1.0f64..50.0,
        ty in 1.0f64..50.0,
        tw in 10.0f64..100.0,
        th in 10.0f64..100.0,
    ) {
        // Build app frame anchored at (0, 0).
        let mut root = mk(Some("root".into()), Rect { x: 0.0, y: 0.0, w: aw, h: ah });
        // Place target inside.
        let target_x = tx.min(aw / 2.0);
        let target_y = ty.min(ah / 2.0);
        let target_w = tw.min(aw - target_x - 1.0).max(1.0);
        let target_h = th.min(ah - target_y - 1.0).max(1.0);
        root.children.push(mk(
            Some("Login".into()),
            Rect { x: target_x, y: target_y, w: target_w, h: target_h },
        ));

        let sel = Selector::Text {
            text: Pattern::text("Login"),
            modifiers: Modifiers::default(),
        };
        let r = resolve_to_norm_coord(&root, &sel);
        if let Ok((nx, ny)) = r {
            prop_assert!(nx > 0.0 && nx < 1.0, "nx={} out of unit interval", nx);
            prop_assert!(ny > 0.0 && ny < 1.0, "ny={} out of unit interval", ny);
        }
    }

    /// Centroid of (x, y, w, h) with w > 0, h > 0 is at (x + w/2, y + h/2),
    /// which lies strictly inside the rect for any positive w, h.
    #[test]
    fn centroid_geometric_invariant(
        ax in 1.0f64..500.0,
        ay in 1.0f64..500.0,
        aw in 100.0f64..500.0,
        ah in 100.0f64..500.0,
    ) {
        // app frame anchored at origin
        let mut root = mk(Some("root".into()), Rect { x: 0.0, y: 0.0, w: aw + ax + 100.0, h: ah + ay + 100.0 });
        root.children.push(mk(
            Some("Target".into()),
            Rect { x: ax, y: ay, w: aw, h: ah },
        ));
        let sel = Selector::Text {
            text: Pattern::text("Target"),
            modifiers: Modifiers::default(),
        };
        let (nx, ny) = resolve_to_norm_coord(&root, &sel).expect("Target resolves");
        let app_w = aw + ax + 100.0;
        let app_h = ah + ay + 100.0;
        let abs_x = nx * app_w;
        let abs_y = ny * app_h;
        // Centroid should be inside (ax, ay, aw, ah).
        prop_assert!(abs_x >= ax && abs_x <= ax + aw,
            "centroid x={} not in [{}, {}]", abs_x, ax, ax + aw);
        prop_assert!(abs_y >= ay && abs_y <= ay + ah,
            "centroid y={} not in [{}, {}]", abs_y, ay, ay + ah);
    }
}
