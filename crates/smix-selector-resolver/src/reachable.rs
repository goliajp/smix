//! Of several matches, the ones a touch reaches.
//!
//! A form sheet on an iPad is a card the window draws over the screen that
//! presented it, and the rows of that screen stay in the tree. With the
//! same identifier on a row behind the card and on a row in it, the first
//! match was the one behind — covered by the card and the keyboard — and a
//! tap went there. The runner says, for an element under something drawn
//! over it, that a touch does not reach it (`hittable: false`); the others
//! are left unsaid.

use smix_screen::A11yNode;

/// `candidates` without the ones a touch does not reach, when some of them
/// it does; all of them otherwise, so that a step aimed at an element
/// that is covered still finds it and says it is covered.
pub(crate) fn reachable_first(candidates: Vec<&A11yNode>) -> Vec<&A11yNode> {
    if candidates.len() <= 1 || !candidates.iter().any(|n| n.hittable == Some(false)) {
        return candidates;
    }
    let reached: Vec<&A11yNode> = candidates
        .iter()
        .copied()
        .filter(|n| n.hittable != Some(false))
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

    fn row(label: &str, hittable: Option<bool>) -> A11yNode {
        let mut n: A11yNode = serde_json::from_value(serde_json::json!({
            "rawType": "cell", "elementTypeRaw": 75, "identifier": "device-row", "label": label,
            "bounds": {"x": 0, "y": 0, "w": 100, "h": 40},
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "children": [],
        }))
        .expect("a node");
        n.hittable = hittable;
        n
    }

    fn labels(v: &[&A11yNode]) -> Vec<String> {
        v.iter()
            .map(|n| n.label.clone().unwrap_or_default())
            .collect()
    }

    #[test]
    fn a_match_under_a_sheet_gives_way_to_one_on_it() {
        let (behind, on) = (
            row("Lobby Entrance", Some(false)),
            row("Lobby Elevator", None),
        );
        assert_eq!(
            labels(&reachable_first(vec![&behind, &on])),
            ["Lobby Elevator"]
        );
    }

    #[test]
    fn when_every_match_is_covered_all_stay_to_be_refused() {
        let (a, b) = (row("a", Some(false)), row("b", Some(false)));
        assert_eq!(labels(&reachable_first(vec![&a, &b])), ["a", "b"]);
    }

    #[test]
    fn nothing_said_about_reach_keeps_the_order() {
        let (a, b) = (row("a", None), row("b", Some(true)));
        assert_eq!(labels(&reachable_first(vec![&a, &b])), ["a", "b"]);
    }
}
