//! What of an element its scrolling container lets show.
//!
//! XCUITest gives an element inside a scroll view, table or collection
//! view its frame where the content puts it, not the part the container
//! shows. A row scrolled up past its list's top edge — behind a
//! navigation bar, or under a header above the list — kept `visible:
//! true` and its whole frame, so it was found as shown and a tap at its
//! centre went to whatever was over it, reported as tapped. Each such
//! element is cut to its containers here, as the Android probe has cut
//! its nodes since 11.0.

use crate::{A11yNode, Rect};

/// The iOS element types that scroll their content and clip it.
const SCROLLERS: [&str; 3] = ["scrollView", "table", "collectionView"];

/// Cut every element inside a scrolling container to what the container
/// shows: `visible_bounds` becomes the part inside, and an element with no
/// part inside is not visible.
pub fn clip_to_scrollers(root: &mut A11yNode) {
    clip(root, None);
}

fn clip(node: &mut A11yNode, within: Option<Rect>) {
    if let Some(window) = within {
        let shown = node.visible_bounds.unwrap_or(node.bounds);
        match intersect(shown, window) {
            Some(part) => node.visible_bounds = Some(part),
            None => {
                node.visible_bounds = Some(Rect {
                    w: 0.0,
                    h: 0.0,
                    ..shown
                });
                node.visible = false;
            }
        }
    }
    let inner = if SCROLLERS.contains(&node.raw_type.as_str()) {
        let own = node.visible_bounds.unwrap_or(node.bounds);
        Some(within.and_then(|w| intersect(own, w)).unwrap_or(own))
    } else {
        within
    };
    for c in &mut node.children {
        clip(c, inner);
    }
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let (x1, y1) = (a.x.max(b.x), a.y.max(b.y));
    let (x2, y2) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    (x2 > x1 && y2 > y1).then_some(Rect {
        x: x1,
        y: y1,
        w: x2 - x1,
        h: y2 - y1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(raw: &str, y: f64, h: f64, children: Vec<A11yNode>) -> A11yNode {
        let mut n: A11yNode = serde_json::from_value(serde_json::json!({
            "rawType": raw, "elementTypeRaw": 0,
            "bounds": {"x": 0, "y": y, "w": 402, "h": h},
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "children": [],
        }))
        .expect("a node");
        n.children = children;
        n
    }

    /// The fixture's scroll screen, measured on iOS 27: the list starts at
    /// 152 under a title and a label, and rows 0 and 1 had scrolled up past it.
    fn screen() -> A11yNode {
        let rows = (0..4)
            .map(|i| node("button", -52.7 + 100.0 * f64::from(i), 100.0, vec![]))
            .collect();
        node(
            "application",
            0.0,
            874.0,
            vec![
                node("navigationBar", 62.0, 54.0, vec![]),
                node("scrollView", 152.3, 721.7, rows),
            ],
        )
    }

    #[test]
    fn a_row_scrolled_past_the_list_top_is_not_visible() {
        let mut s = screen();
        clip_to_scrollers(&mut s);
        let rows = &s.children[1].children;
        assert!(!rows[0].visible, "{:?}", rows[0].visible_bounds);
        assert!(!rows[1].visible, "{:?}", rows[1].visible_bounds);
    }

    #[test]
    fn a_row_cut_by_the_list_top_shows_its_lower_part() {
        let mut s = screen();
        clip_to_scrollers(&mut s);
        let row = &s.children[1].children[2];
        assert!(row.visible);
        let shown = row.visible_bounds.expect("cut");
        assert!(
            (shown.y - 152.3).abs() < 1e-9 && (shown.h - 95.0).abs() < 1e-6,
            "{shown:?}"
        );
        assert_eq!(row.bounds.h, 100.0, "the frame is left as it was");
    }

    #[test]
    fn what_is_not_in_a_scroller_is_left_alone() {
        let mut s = screen();
        clip_to_scrollers(&mut s);
        assert!(s.children[0].visible && s.children[0].visible_bounds.is_none());
    }

    #[test]
    fn a_scroller_inside_a_scroller_shows_only_what_both_show() {
        let inner = node(
            "collectionView",
            100.0,
            400.0,
            vec![node("cell", 80.0, 60.0, vec![])],
        );
        let mut s = node("scrollView", 120.0, 300.0, vec![inner]);
        clip_to_scrollers(&mut s);
        let cell = &s.children[0].children[0];
        assert_eq!(cell.visible_bounds.map(|r| (r.y, r.h)), Some((120.0, 20.0)));
    }
}
