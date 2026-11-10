//! Why a touch that missed its element missed, read once more after it.
//!
//! The touch point comes from where the element was when the tree was
//! read, so a miss is one of three things: the element moved, it went, or
//! it stayed and something drawn over it took the touch. The chain alone
//! cannot tell them apart — it says what holds the point now, not where
//! the element is — and the hint used to lead with "the element moved"
//! for all three. A consumer's button was still exactly where it had been,
//! under a toast whose close button took the touch.
//!
//! So on a miss, and only then, the screen is read again by the reader the
//! touch was aimed from, and the hint says which of the three it was.

use smix_error::{ExpectationFailure, FailureCode};
use smix_runner_wire::TapAtCoordResult;
use smix_screen::A11yNode;
use smix_selector::Selector;
use smix_selector_resolver::resolve_selector;

use super::describe_hit;
use crate::{
    ActOutcome, Aimed, AndroidDriver, HitElement, IosDriver, hit_element, landing_outcome,
};

/// How far, in points, a frame may differ and still be where it was.
const SAME_PLACE_PT: f64 = 2.0;

/// Where the element a missed touch aimed at is, read again after it.
#[derive(Debug, PartialEq)]
pub(crate) enum Since {
    /// The screen could not be read again, or not by the same reader.
    Unread,
    /// Where it was.
    Stayed,
    /// Somewhere else.
    Moved(HitElement),
    /// Not on the screen.
    Gone,
}

pub(crate) fn since(aimed: &HitElement, now: Option<HitElement>) -> Since {
    let Some(now) = now else {
        return Since::Gone;
    };
    let close = |a: f64, b: f64| (a - b).abs() <= SAME_PLACE_PT;
    let (a, b) = (aimed.frame, now.frame);
    if close(a.0, b.0) && close(a.1, b.1) && close(a.2, b.2) && close(a.3, b.3) {
        Since::Stayed
    } else {
        Since::Moved(now)
    }
}

fn frame(e: &HitElement) -> String {
    let (x, y, w, h) = e.frame;
    format!("({x:.0},{y:.0} {w:.0}x{h:.0})")
}

/// The innermost named element holding the touched point.
fn over(landed: &TapAtCoordResult) -> Option<String> {
    landed
        .chain
        .iter()
        .find(|e| !e.identifier.is_empty() || !e.label.is_empty())
        .map(|e| {
            describe_hit(&HitElement {
                identifier: e.identifier.clone(),
                label: e.label.clone(),
                frame: (e.frame.x, e.frame.y, e.frame.w, e.frame.h),
            })
        })
}

/// The hint for a miss; `None` keeps the one it had.
pub(crate) fn hint(aimed: &HitElement, since: &Since, over: Option<&str>) -> Option<String> {
    let it = describe_hit(aimed);
    let why = match since {
        Since::Unread => return None,
        Since::Stayed => format!(
            "{it} is still where it was aimed at {}, and {} is drawn over it there and took \
             the touch. Dismiss or wait out what is in front, then touch it again.",
            frame(aimed),
            over.unwrap_or("something"),
        ),
        Since::Moved(now) => format!(
            "{it} moved between the tree fetch and the tap: it was at {} and is now at {}; \
             wait for the screen to settle first.",
            frame(aimed),
            frame(now),
        ),
        Since::Gone => format!(
            "{it} is no longer on the screen: it went between the tree fetch and the tap, \
             or the touch changed the screen."
        ),
    };
    Some(format!(
        "{why} Set SMIX_TAP_HIT_MISMATCH=warn to downgrade this to a warning while migrating \
         a suite."
    ))
}

/// The element `selector` names in `tree`, if it is there.
fn find(tree: &A11yNode, selector: &Selector) -> Option<HitElement> {
    resolve_selector(tree, selector).map(hit_element)
}

/// `failure` with its hint from a second reading; any other failure as it is.
fn explained(
    mut failure: ExpectationFailure,
    aimed: Option<&HitElement>,
    landed: &TapAtCoordResult,
    since_of: impl FnOnce(&HitElement) -> Since,
) -> ExpectationFailure {
    if failure.code != FailureCode::TapMissed {
        return failure;
    }
    if let Some(aimed) = aimed
        && let Some(h) = hint(aimed, &since_of(aimed), over(landed).as_deref())
    {
        failure.hint = Some(h);
    }
    failure
}

impl IosDriver {
    /// [`landing_outcome`], and on a miss, why.
    pub(crate) async fn landed(
        &self,
        selector: &Selector,
        aim: Aimed,
        landed: &TapAtCoordResult,
    ) -> Result<ActOutcome, ExpectationFailure> {
        let target = aim.2.clone();
        match landing_outcome(selector, aim, landed) {
            Err(e) if e.code == FailureCode::TapMissed => {
                let now = self.tree(None).await.ok();
                Err(explained(e, target.as_ref(), landed, |aimed| match &now {
                    Some(tree) => since(aimed, find(tree, selector)),
                    None => Since::Unread,
                }))
            }
            other => other,
        }
    }
}

impl AndroidDriver {
    /// [`landing_outcome`], and on a miss, why — read again by the reader
    /// the touch was aimed from, since two readers can place one element
    /// differently.
    pub(crate) async fn landed(
        &self,
        selector: &Selector,
        aim: Aimed,
        landed: &TapAtCoordResult,
    ) -> Result<ActOutcome, ExpectationFailure> {
        let (target, reader) = (aim.2.clone(), aim.3);
        match landing_outcome(selector, aim, landed) {
            Err(e) if e.code == FailureCode::TapMissed => {
                let now = self
                    .perceive(None)
                    .await
                    .ok()
                    .filter(|p| crate::verdict_reader(p.source) == reader);
                Err(explained(e, target.as_ref(), landed, |aimed| match &now {
                    Some(p) => since(aimed, find(&p.root, selector)),
                    None => Since::Unread,
                }))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(x: f64, y: f64) -> HitElement {
        HitElement {
            identifier: "btn_open_menu".into(),
            label: "Open menu".into(),
            frame: (x, y, 126.0, 126.0),
        }
    }

    #[test]
    fn an_element_where_it_was_is_covered_and_the_cover_is_named() {
        let s = since(&button(932.0, 142.0), Some(button(932.5, 142.0)));
        assert_eq!(s, Since::Stayed);
        let h = hint(&button(932.0, 142.0), &s, Some("id=toast_close")).unwrap();
        assert!(
            h.contains("still where it was aimed at (932,142 126x126)"),
            "{h}"
        );
        assert!(h.contains("id=toast_close is drawn over it"), "{h}");
        assert!(!h.contains("moved"), "{h}");
    }

    #[test]
    fn an_element_elsewhere_moved_and_both_places_are_given() {
        let s = since(&button(932.0, 142.0), Some(button(932.0, 300.0)));
        let h = hint(&button(932.0, 142.0), &s, Some("id=toast_close")).unwrap();
        assert!(h.contains("moved"), "{h}");
        assert!(
            h.contains("(932,142 126x126)") && h.contains("(932,300 126x126)"),
            "{h}"
        );
        assert!(!h.contains("drawn over"), "{h}");
    }

    #[test]
    fn an_element_not_found_again_is_gone() {
        let s = since(&button(932.0, 142.0), None);
        assert_eq!(s, Since::Gone);
        assert!(
            hint(&button(932.0, 142.0), &s, None)
                .unwrap()
                .contains("no longer on the screen")
        );
    }

    #[test]
    fn a_screen_not_read_again_keeps_the_hint_it_had() {
        assert_eq!(hint(&button(0.0, 0.0), &Since::Unread, None), None);
    }

    #[test]
    fn only_a_miss_is_reworded() {
        let landed = TapAtCoordResult::default();
        let other = ExpectationFailure::new(smix_error::FailureInit {
            code: Some(FailureCode::DriverError),
            hint: Some("as it was".into()),
            ..Default::default()
        });
        let e = explained(other, Some(&button(0.0, 0.0)), &landed, |_| Since::Gone);
        assert_eq!(e.hint.as_deref(), Some("as it was"));
    }
}
