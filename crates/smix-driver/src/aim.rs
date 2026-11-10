//! One reading of where to touch a target.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_host_coord_resolver::{Cover, Covering, Touchable, touch_point};
use smix_screen::{A11yNode, WindowKind, screen_facts};
use smix_selector::{Selector, describe_selector};
use smix_selector_resolver::resolve_selector;

use crate::{hit_element, settle};

/// One reading of where `selector` is in `tree`: the point a touch reaches
/// it at and the element, or `None` when it is not there.
///
/// The point is its centre unless something over it takes the touch
/// there; then it is the middle of the largest part nothing is over (see
/// [`touch_point`]).
///
/// # Errors
///
/// `NOT_VISIBLE`, naming what covers it, when every part of it that
/// shows is under something that would take the touch.
pub(crate) fn aim_in(
    tree: &A11yNode,
    selector: &Selector,
) -> Result<Option<settle::Aim>, ExpectationFailure> {
    let Some(node) = resolve_selector(tree, selector) else {
        return Ok(None);
    };
    match touch_point(tree, node) {
        Ok(Touchable::At(nx, ny)) => Ok(Some((nx, ny, Some(hit_element(node))))),
        Ok(Touchable::Covered(covers)) => Err(covered(tree, selector, &covers)),
        Err(_) => Ok(None),
    }
}

/// One settle reading of `tree`: where the target is, or the failure for
/// an element this screen does not have.
pub(crate) fn settle_reading(
    tree: &A11yNode,
    selector: &Selector,
) -> Result<settle::Reading, ExpectationFailure> {
    Ok(match aim_in(tree, selector)? {
        Some(aim) => settle::Reading::At(aim),
        None => settle::Reading::Gone(Box::new(element_not_found(tree, selector))),
    })
}

fn covered(tree: &A11yNode, selector: &Selector, covers: &[Cover<'_>]) -> ExpectationFailure {
    let names: Vec<String> = covers.iter().map(describe_cover).collect();
    ExpectationFailure::new(
        FailureInit {
            code: Some(FailureCode::NotVisible),
            message: format!(
                "{} is under {} wherever it shows, so a touch on it would go there instead; \
                 nothing was touched",
                describe_selector(selector),
                names.join(" and ")
            ),
            selector: Some(selector.clone()),
            hint: Some(
                "scroll it clear of what covers it, or close that first (`hideKeyboard` for the \
                 keyboard)"
                    .into(),
            ),
            ..Default::default()
        }
        .with_screen(screen_facts(tree, 10)),
    )
}

fn describe_cover(c: &Cover<'_>) -> String {
    let what = match &c.what {
        Covering::Keyboard => "the keyboard".to_owned(),
        Covering::StatusBar => "the status bar".to_owned(),
        Covering::Window(w) => {
            let kind = match w.kind {
                WindowKind::Application => "app",
                WindowKind::InputMethod => "keyboard",
                WindowKind::System => "system",
                WindowKind::Other => "overlay",
            };
            match &w.package {
                Some(p) => format!("{p}'s {kind} window"),
                None => format!("a {kind} window"),
            }
        }
    };
    let r = c.rect;
    format!("{what} ({},{} {}×{})", r.x, r.y, r.w, r.h)
}

/// `ELEMENT_NOT_FOUND` for `selector` on `tree`, with what was on screen
/// and the near misses — the shape every "not found" takes.
pub(crate) fn element_not_found(tree: &A11yNode, selector: &Selector) -> ExpectationFailure {
    let screen = screen_facts(tree, 10);
    let suggestions = suggestions_for(tree, selector);
    ExpectationFailure::new(
        FailureInit {
            code: Some(FailureCode::ElementNotFound),
            message: format!("element not found: {}", describe_selector(selector)),
            selector: Some(selector.clone()),
            suggestions,
            hint: Some(not_found_hint(tree, selector)),
            ..Default::default()
        }
        .with_screen(screen),
    )
}

/// What to do about a miss: an element the tree holds off screen is
/// scrolled to, not looked for under another name.
fn not_found_hint(tree: &A11yNode, selector: &Selector) -> String {
    let off = smix_selector_resolver::matched_off_screen(tree, selector);
    let Some(first) = off.first() else {
        return "matched 0 nodes in the current a11y tree; check selector or wait for the screen to settle"
            .into();
    };
    let b = &first.bounds;
    let s = &tree.bounds;
    format!(
        "matched {} node(s) in the a11y tree, all off screen — the first at \
         x={:.0} y={:.0} {:.0}x{:.0}, the screen {:.0}x{:.0}; scroll it into view \
         first (`scrollUntilVisible`)",
        off.len(),
        b.x,
        b.y,
        b.w,
        b.h,
        s.w,
        s.h
    )
}

/// Near misses for `selector`, from every element on screen rather than
/// the first few a failure lists: a row further down was never offered.
pub(crate) fn suggestions_for(tree: &A11yNode, selector: &Selector) -> Vec<String> {
    let pool = smix_screen::collect_visible_summaries(tree, usize::MAX);
    smix_error::build_suggestions(crate::base_text_or_id(selector).as_deref(), &pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_selector::{Modifiers, Pattern};

    /// A screen of `n` labelled rows, the last one the one looked for.
    fn rows(n: usize) -> A11yNode {
        let row = |i: usize, label: &str| {
            serde_json::json!({
                "rawType": "staticText", "elementTypeRaw": 48, "label": label,
                "enabled": true, "selected": false, "hasFocus": false, "visible": true,
                "bounds": {"x": 0.0, "y": i as f64 * 40.0, "w": 390.0, "h": 40.0}
            })
        };
        let mut children: Vec<_> = (0..n - 1).map(|i| row(i, &format!("Row {i}"))).collect();
        children.push(row(n - 1, "Sign in on Apple Watch"));
        serde_json::from_value(serde_json::json!({
            "rawType": "application", "label": "Fixture", "enabled": true, "selected": false,
            "hasFocus": false, "visible": true,
            "bounds": {"x": 0.0, "y": 0.0, "w": 390.0, "h": 844.0}, "children": children
        }))
        .expect("a screen")
    }

    fn text(t: &str) -> Selector {
        Selector::Text {
            text: Pattern::text(t),
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn a_miss_offers_a_near_name_from_anywhere_on_the_screen() {
        let f = element_not_found(&rows(18), &text("Sign in on Apple Wach"));
        assert!(
            f.suggestions
                .iter()
                .any(|s| s.contains("Sign in on Apple Watch")),
            "the 18th row is not offered: {:?}",
            f.suggestions
        );
    }

    #[test]
    fn a_miss_lists_every_piece_of_text_on_the_screen() {
        let f = element_not_found(&rows(18), &text("Log out"));
        let named: Vec<_> = f
            .visible_elements
            .iter()
            .filter_map(|e| e.name.as_deref())
            .collect();
        assert!(named.contains(&"Sign in on Apple Watch"), "{named:?}");
        assert!(named.contains(&"Row 16"), "{named:?}");
    }
}
