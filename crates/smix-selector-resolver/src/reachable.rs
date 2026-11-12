//! Of several matches, the ones a touch reaches.
//!
//! A form sheet on an iPad is a card the window draws over the screen that
//! presented it, and the rows of that screen stay in the tree. With the
//! same identifier on a row behind the card and on a row in it, the first
//! match was the one behind — covered by the card and the keyboard — and a
//! tap went there. The runner says, for an element under something drawn
//! over it, that a touch does not reach it (`hittable: false`). It does not
//! say it of a row whose centre has gone past the screen's edge or under
//! the keyboard: a row one point on screen was taken next, and its centre
//! was off the screen.

use smix_screen::{A11yNode, Rect, is_keyboard};

/// Whether a touch at `node`'s centre reaches it, as far as the tree can
/// tell: not reported covered, the centre on the screen, and not under the
/// keyboard or the bar above it.
fn reachable(node: &A11yNode, screen: Rect, keyboards: &[Rect]) -> bool {
    let b = node.bounds;
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let inside = |r: &Rect| cx >= r.x && cx < r.x + r.w && cy >= r.y && cy < r.y + r.h;
    node.hittable != Some(false) && inside(&screen) && !keyboards.iter().any(inside)
}

fn keyboards(n: &A11yNode, out: &mut Vec<Rect>) {
    if is_keyboard(n) || n.identifier.as_deref() == Some("SystemInputAssistantView") {
        out.push(n.bounds);
        return;
    }
    for c in &n.children {
        keyboards(c, out);
    }
}

/// `candidates` without the ones a touch does not reach, when some of them
/// it does; all of them otherwise, so that a step aimed at an element that
/// is covered still finds it and says it is covered.
pub(crate) fn reachable_first<'t>(
    tree: &A11yNode,
    candidates: Vec<&'t A11yNode>,
) -> Vec<&'t A11yNode> {
    if candidates.len() <= 1 {
        return candidates;
    }
    let mut kb = Vec::new();
    keyboards(tree, &mut kb);
    let reached: Vec<&A11yNode> = candidates
        .iter()
        .copied()
        .filter(|n| reachable(n, tree.bounds, &kb))
        .collect();
    if reached.is_empty() {
        candidates
    } else {
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, y: f64, hittable: Option<bool>) -> A11yNode {
        let mut n: A11yNode = serde_json::from_value(serde_json::json!({
            "rawType": "button", "elementTypeRaw": 9, "identifier": "device-row", "label": label,
            "bounds": {"x": 0, "y": y, "w": 1032, "h": 70},
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "children": [],
        }))
        .expect("a node");
        n.hittable = hittable;
        n
    }

    /// An iPad Pro 13 in portrait with the keyboard up, as a consumer's tree
    /// read it.
    fn screen() -> A11yNode {
        let mut s = row("screen", 0.0, None);
        s.bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: 1032.0,
            h: 1376.0,
        };
        let mut kb = row("keyboard", 1028.0, None);
        kb.raw_type = "keyboard".into();
        kb.bounds.h = 345.0;
        let mut bar = row("", 973.0, None);
        bar.identifier = Some("SystemInputAssistantView".into());
        bar.bounds.h = 55.0;
        s.children = vec![bar, kb];
        s
    }

    fn labels(v: &[&A11yNode]) -> Vec<String> {
        v.iter()
            .map(|n| n.label.clone().unwrap_or_default())
            .collect()
    }

    fn on_the_card() -> A11yNode {
        let mut c = row("Lobby Elevator", 223.0, None);
        c.bounds.x = 236.0;
        c.bounds.w = 560.0;
        c
    }

    #[test]
    fn a_match_reported_covered_gives_way_to_one_that_is_not() {
        let (behind, on) = (row("Lobby Entrance", 400.0, Some(false)), on_the_card());
        assert_eq!(
            labels(&reachable_first(&screen(), vec![&behind, &on])),
            ["Lobby Elevator"]
        );
    }

    #[test]
    fn a_centre_past_the_edge_or_under_the_keyboard_gives_way() {
        // one point on screen, under the keyboard, under the bar above it
        let edge = row("3F Conference Room", 1375.0, None);
        let under = row("Lobby Reception", 1095.0, None);
        let bar = row("Lobby Entrance", 960.0, None);
        let card = on_the_card();
        assert_eq!(
            labels(&reachable_first(
                &screen(),
                vec![&edge, &under, &bar, &card]
            )),
            ["Lobby Elevator"]
        );
    }

    #[test]
    fn when_no_match_is_reachable_all_stay_to_be_refused() {
        let (a, b) = (row("a", 400.0, Some(false)), row("b", 1375.0, None));
        assert_eq!(
            labels(&reachable_first(&screen(), vec![&a, &b])),
            ["a", "b"]
        );
    }

    #[test]
    fn reachable_matches_keep_their_order() {
        let (a, b) = (row("a", 100.0, None), row("b", 200.0, Some(true)));
        assert_eq!(
            labels(&reachable_first(&screen(), vec![&a, &b])),
            ["a", "b"]
        );
    }
}
