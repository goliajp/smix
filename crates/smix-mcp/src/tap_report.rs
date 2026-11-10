//! What `smix_tap` answers once the touch has gone.

use smix_sdk::{ActOutcome, ActVerdict, ScreenAtTouch, Selector};

/// What the touch landed on, not just that one was sent. This used to
/// answer "tapped: <selector>" — a claim about the selector, made on the
/// evidence that a touch had been synthesised somewhere, which is the
/// reading a consumer found out the hard way was not the same thing.
///
/// The elements are listed even when the verdict passed: the verdict
/// cannot see an element covered by something else and this list can,
/// and an agent deciding whether to retry or to report upward is exactly
/// who needs to know. A touch aimed at a point says what the screen was
/// doing when it went (`screen`), because a screen still arriving drops
/// the touch and answers nothing.
pub(crate) fn tapped(
    sel: &Selector,
    outcome: &ActOutcome,
    screen: Option<ScreenAtTouch>,
) -> String {
    let mut report = format!("tapped: {}", smix_selector::describe_selector(sel));
    if !outcome.observed.is_empty() {
        let at: Vec<String> = outcome
            .observed
            .iter()
            .map(|e| {
                if !e.identifier.is_empty() {
                    e.identifier.clone()
                } else if !e.label.is_empty() {
                    format!("{:?}", e.label)
                } else {
                    "<unnamed>".to_string()
                }
            })
            .collect();
        report.push_str(&format!("\nthe tapped point is inside: {}", at.join(" < ")));
    }
    if let Some(caveat) = screen.as_ref().and_then(ScreenAtTouch::caveat) {
        report.push_str(&format!("\n{caveat}"));
    }
    if let ActVerdict::Unconfirmable(why) = &outcome.verdict {
        report.push_str(&format!("\nnot verified: {why}"));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn point() -> Selector {
        Selector::Point { nx: 0.5, ny: 0.72 }
    }

    #[test]
    fn a_point_touched_on_a_moving_screen_says_so() {
        let r = tapped(
            &point(),
            &ActOutcome::unjudged(),
            Some(ScreenAtTouch::Moving {
                waited: Duration::from_millis(3000),
            }),
        );
        assert!(r.contains("still moving after 3000 ms"), "{r}");
    }

    #[test]
    fn a_point_touched_on_a_still_screen_adds_nothing_about_the_screen() {
        let r = tapped(
            &point(),
            &ActOutcome::unjudged(),
            Some(ScreenAtTouch::Still {
                waited: Duration::from_millis(400),
            }),
        );
        assert!(!r.contains("screen was"), "{r}");
    }
}
