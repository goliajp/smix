//! One reading of where to touch a target.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_host_coord_resolver::{Cover, Covering, Touchable, touch_point};
use smix_screen::{A11yNode, WindowKind, screen_facts};
use smix_selector::{Selector, describe_selector};
use smix_selector_resolver::resolve_selector;

use crate::{element_not_found, hit_element, settle};

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
