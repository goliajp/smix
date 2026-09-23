//! Scroll until a selector's target is far enough into view — the one
//! loop every caller shares.
//!
//! There were three: the iOS driver's, the Android driver's, and the
//! flow adapter's OCR variant. Each had its own idea of when to stop, and
//! the Android one stopped as soon as the target was anywhere in the
//! tree — which, with a semantics tree that reports layout rather than
//! what is on screen, is at once, before any swipe, with the target's
//! middle below the edge. Whether to stop is now one pure answer
//! ([`smix_host_coord_resolver::verdict`]); what differs between callers
//! is only how they look, and looking is a trait.

use async_trait::async_trait;
use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_host_coord_resolver::{
    HostResolveError, NormBox, Reach, Verdict, norm_box, verdict, visible_share,
};
use smix_input::SwipeDirection;
use smix_screen::{ElementSummary, collect_visible_summaries};
use smix_selector::{Selector, describe_selector};
use smix_selector_resolver::{ResolverContext, resolve_selector_compiled};
use std::time::Duration;
use tokio::time::Instant;

use crate::{Driver, OCR_RECOGNITION_LEVEL, base_text_or_id, ocr_locales};

/// What a scroll-until-visible asks for, and how long it may take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollUntil {
    /// When the target counts as reached.
    pub reach: Reach,
    /// How long to keep swiping. The only limit: a second, swipe-count
    /// limit would be a second stopping rule.
    pub timeout: Duration,
}

/// How long to wait before looking again at a target that is in place
/// but has not stopped moving. Long enough that two looks straddle a
/// frame, short enough to cost nothing on a list that is already still.
const SETTLE_INTERVAL: Duration = Duration::from_millis(150);

/// Two readings of the same box, near enough that the content is not
/// moving. A thousandth of the frame is under two pixels on any screen
/// there is.
fn same_place(a: NormBox, b: NormBox) -> bool {
    const TOLERANCE: f64 = 1e-3;
    (a.x - b.x).abs() < TOLERANCE
        && (a.y - b.y).abs() < TOLERANCE
        && (a.w - b.w).abs() < TOLERANCE
        && (a.h - b.h).abs() < TOLERANCE
}

/// maestro's `DEFAULT_TIMEOUT_IN_MILLIS` for `scrollUntilVisible`.
pub const DEFAULT_SCROLL_TIMEOUT: Duration = Duration::from_millis(20_000);

impl Default for ScrollUntil {
    fn default() -> Self {
        ScrollUntil {
            reach: Reach::default(),
            timeout: DEFAULT_SCROLL_TIMEOUT,
        }
    }
}

/// Swipe `direction` until `selector`'s target is reached, per `until`.
///
/// One look is the accessibility (or semantics) tree first, then each
/// `ocrText` the selector names, in order. A tree match counts only
/// once the driver confirms it is on screen now
/// ([`Driver::confirm_on_screen`]).
///
/// # Errors
///
/// `ElementNotFound` when the timeout passes first — the message says
/// how many swipes were made and how much of the target the last look
/// saw. Anything a look or a swipe fails with, unchanged.
pub async fn scroll_until(
    driver: &dyn Driver,
    selector: &Selector,
    direction: SwipeDirection,
    until: &ScrollUntil,
) -> Result<(), ExpectationFailure> {
    // Compiled once rather than on every look, and a pattern that does
    // not compile fails now: otherwise it matches nothing and the scroll
    // swipes out its whole timeout before saying it saw nothing.
    let Some(ctx) = ResolverContext::new(selector) else {
        return Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::ElementNotFound),
            message: format!(
                "scroll({}, '{}'): selector pattern failed to compile",
                describe_selector(selector),
                direction
            ),
            selector: Some(selector.clone()),
            hint: Some(
                "regex Pattern compile error — check selector syntax (unbalanced bracket / invalid escape / etc.)"
                    .into(),
            ),
            ..Default::default()
        }));
    };
    let mut eyes = DriverEyes {
        driver,
        selector,
        ctx,
    };
    run(&mut eyes, selector, direction, until).await
}

/// One look at the screen.
pub(crate) struct Look {
    /// The target's box, when it is seen at all.
    pub seen: Option<NormBox>,
    /// What was on screen, for the failure a timeout produces.
    pub visible: Vec<ElementSummary>,
}

#[async_trait]
pub(crate) trait Eyes: Send {
    async fn look(&mut self) -> Result<Look, ExpectationFailure>;
    async fn swipe(&mut self, direction: SwipeDirection) -> Result<(), ExpectationFailure>;
}

pub(crate) async fn run<E: Eyes>(
    eyes: &mut E,
    selector: &Selector,
    direction: SwipeDirection,
    until: &ScrollUntil,
) -> Result<(), ExpectationFailure> {
    let deadline = Instant::now() + until.timeout;
    let mut swipes = 0u32;
    let mut recentered = 0u32;
    let mut previous: Option<NormBox> = None;
    loop {
        let look = eyes.look().await?;
        // In place, but not where it was a moment ago: the content is
        // still gliding. A swipe leaves a list moving, and a tap sent
        // into the glide is spent stopping it rather than pressing what
        // it landed on — measured on the Compose fixture, where the tap
        // reported success and the row's own label never changed.
        let mut still_moving = false;
        let last_share = match look.seen {
            None => None,
            Some(b) => match verdict(b, &until.reach, direction, recentered) {
                Verdict::Reached if previous.is_some_and(|p| same_place(p, b)) => return Ok(()),
                Verdict::Reached => {
                    still_moving = true;
                    Some(visible_share(b))
                }
                Verdict::Recenter => {
                    recentered += 1;
                    Some(visible_share(b))
                }
                Verdict::Short { share } => Some(share),
            },
        };
        previous = look.seen;
        if Instant::now() >= deadline {
            return Err(not_reached(
                selector,
                direction,
                until,
                swipes,
                last_share,
                still_moving,
                look.visible,
            ));
        }
        if still_moving {
            tokio::time::sleep(SETTLE_INTERVAL).await;
            continue;
        }
        eyes.swipe(direction).await?;
        swipes += 1;
    }
}

struct DriverEyes<'a> {
    driver: &'a dyn Driver,
    selector: &'a Selector,
    ctx: ResolverContext,
}

#[async_trait]
impl Eyes for DriverEyes<'_> {
    async fn look(&mut self) -> Result<Look, ExpectationFailure> {
        let tree = self.driver.tree(None).await?;
        let visible = collect_visible_summaries(&tree, 10);
        if let Some(node) = resolve_selector_compiled(&tree, self.selector, &self.ctx)
            && self.driver.confirm_on_screen(&[node]).await
        {
            match norm_box(node.bounds, tree.bounds) {
                Ok(b) => {
                    return Ok(Look {
                        seen: Some(b),
                        visible,
                    });
                }
                // A node with no area is not on screen in any sense a
                // scroll can improve; keep looking.
                Err(HostResolveError::EmptyMatchedFrame) => {}
                Err(e) => {
                    return Err(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: format!("scroll: {e}"),
                        ..Default::default()
                    }));
                }
            }
        }
        for (text, locales) in ocr_texts(self.selector) {
            if let Some(f) = self
                .driver
                .find_text_by_ocr(text, &ocr_locales(locales), OCR_RECOGNITION_LEVEL)
                .await?
            {
                let seen = NormBox {
                    x: f.nx,
                    y: f.ny,
                    w: f.w,
                    h: f.h,
                };
                return Ok(Look {
                    seen: Some(seen),
                    visible,
                });
            }
        }
        Ok(Look {
            seen: None,
            visible,
        })
    }

    async fn swipe(&mut self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        self.driver.swipe_once(direction).await
    }
}

/// Every `ocrText` in `selector`, in the order a fallback chain names
/// them: the selector itself, or the members of its chain. A chain inside
/// a chain means the same as writing it out, so it is walked flat.
fn ocr_texts(selector: &Selector) -> Vec<(&str, &[String])> {
    match selector {
        Selector::Fallback { fallback } => fallback.iter().flat_map(ocr_texts).collect(),
        Selector::OcrText {
            ocr_text, locales, ..
        } => vec![(ocr_text.as_str(), locales.as_slice())],
        _ => Vec::new(),
    }
}

fn not_reached(
    selector: &Selector,
    direction: SwipeDirection,
    until: &ScrollUntil,
    swipes: u32,
    last_share: Option<f64>,
    moving: bool,
    visible: Vec<ElementSummary>,
) -> ExpectationFailure {
    let last = match (last_share, moving) {
        // In place on every look and never twice in the same place: the
        // content never stopped, and a tap into it would be spent
        // stopping it.
        (Some(_), true) => "it came into view and was still moving on every look".to_string(),
        (Some(share), false) => format!(
            "the last look saw {:.0}% of it, and {:.0}% was asked for",
            share * 100.0,
            until.reach.visibility * 100.0
        ),
        (None, _) => "the last look did not see it".to_string(),
    };
    let target = base_text_or_id(selector);
    let suggestions = smix_error::build_suggestions(target.as_deref(), &visible);
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: format!(
            "scroll({}, '{}'): not reached after {} swipes in {:.1} s; {last}",
            describe_selector(selector),
            direction,
            swipes,
            until.timeout.as_secs_f64(),
        ),
        selector: Some(selector.clone()),
        visible_elements: visible,
        suggestions,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_selector::Modifiers;

    /// Looks scripted in advance; the last one repeats.
    struct Scripted {
        looks: Vec<Result<Option<NormBox>, ExpectationFailure>>,
        swipes: u32,
    }

    #[async_trait]
    impl Eyes for Scripted {
        async fn look(&mut self) -> Result<Look, ExpectationFailure> {
            let next = if self.looks.len() > 1 {
                self.looks.remove(0)
            } else {
                self.looks[0].clone()
            };
            next.map(|seen| Look {
                seen,
                visible: Vec::new(),
            })
        }
        async fn swipe(&mut self, _: SwipeDirection) -> Result<(), ExpectationFailure> {
            // A swipe takes time; with the clock paused this is what
            // moves it, so the timeout is reached in a test that does
            // not wait for it.
            tokio::time::sleep(Duration::from_millis(250)).await;
            self.swipes += 1;
            Ok(())
        }
    }

    fn sel() -> Selector {
        Selector::Id {
            id: "row-39".into(),
            modifiers: Modifiers::default(),
        }
    }

    fn nb(y: f64, h: f64) -> NormBox {
        NormBox {
            x: 0.0,
            y,
            w: 1.0,
            h,
        }
    }

    async fn go(
        looks: Vec<Result<Option<NormBox>, ExpectationFailure>>,
        until: ScrollUntil,
    ) -> (Result<(), ExpectationFailure>, u32) {
        let mut eyes = Scripted { looks, swipes: 0 };
        let r = run(&mut eyes, &sel(), SwipeDirection::Down, &until).await;
        (r, eyes.swipes)
    }

    #[tokio::test(start_paused = true)]
    async fn a_target_partly_in_is_swiped_until_it_is_wholly_in() {
        let (r, swipes) = go(
            vec![Ok(Some(nb(0.95, 0.14))), Ok(Some(nb(0.6, 0.14)))],
            ScrollUntil::default(),
        )
        .await;
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(
            swipes, 1,
            "partly in is not reached; one swipe brings it in"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_target_never_seen_times_out_saying_how_many_swipes() {
        let until = ScrollUntil {
            timeout: Duration::from_secs(2),
            ..ScrollUntil::default()
        };
        let (r, swipes) = go(vec![Ok(None)], until).await;
        let f = r.expect_err("a target that never appears must fail");
        assert_eq!(f.code, FailureCode::ElementNotFound);
        assert!(swipes >= 7, "swiped for the whole timeout, got {swipes}");
        assert!(
            f.message.contains(&format!("{swipes} swipes")),
            "{}",
            f.message
        );
        assert!(f.message.contains("did not see it"), "{}", f.message);
    }

    #[tokio::test(start_paused = true)]
    async fn a_target_stuck_part_way_says_how_much_was_seen() {
        let until = ScrollUntil {
            timeout: Duration::from_secs(1),
            ..ScrollUntil::default()
        };
        // 0.042 of 0.1 on screen: the last row of a list that ends there.
        let (r, _) = go(vec![Ok(Some(nb(0.958, 0.1)))], until).await;
        let f = r.expect_err("a target that never gets wholly in must fail");
        assert!(f.message.contains("saw 42% of it"), "{}", f.message);
        assert!(f.message.contains("100% was asked for"), "{}", f.message);
    }

    #[tokio::test(start_paused = true)]
    async fn center_element_gives_up_centring_after_four_swipes() {
        let until = ScrollUntil {
            reach: Reach {
                visibility: 1.0,
                center_element: true,
            },
            ..ScrollUntil::default()
        };
        // Wholly in, low on the screen, and it never moves: the end of a list.
        let (r, swipes) = go(vec![Ok(Some(nb(0.85, 0.1)))], until).await;
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(
            swipes, 5,
            "MAX_RECENTER + 1 recentring swipes, then the share rule"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_reached_target_that_is_still_moving_is_waited_out_rather_than_swiped_at() {
        // The swipe that brings the last row in leaves the list gliding.
        // A tap sent then is eaten stopping the glide, so the row is
        // reached only once it has stopped where it is.
        let (r, swipes) = go(
            vec![
                Ok(Some(nb(0.95, 0.14))),
                Ok(Some(nb(0.60, 0.14))),
                Ok(Some(nb(0.40, 0.14))),
                Ok(Some(nb(0.30, 0.14))),
                Ok(Some(nb(0.30, 0.14))),
            ],
            ScrollUntil::default(),
        )
        .await;
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(
            swipes, 1,
            "one swipe; the rest of the looks waited out the glide"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_target_that_never_settles_times_out_rather_than_reporting_it_reached() {
        let until = ScrollUntil {
            timeout: Duration::from_secs(2),
            ..ScrollUntil::default()
        };
        // Wholly in every look, and never twice in the same place.
        let looks: Vec<_> = (0..40)
            .map(|i| Ok(Some(nb(0.30 + f64::from(i % 2) * 0.01, 0.14))))
            .collect();
        let (r, _) = go(looks, until).await;
        let f = r.expect_err("a screen that never settles has not been reached");
        assert!(f.message.contains("still moving"), "{}", f.message);
    }

    #[tokio::test(start_paused = true)]
    async fn a_look_that_fails_fails_the_scroll_rather_than_reading_as_absent() {
        let broken = ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "runner unreachable".into(),
            ..Default::default()
        });
        let (r, swipes) = go(vec![Err(broken)], ScrollUntil::default()).await;
        let f = r.expect_err("a broken look is not an absent target");
        assert_eq!(f.code, FailureCode::DriverError);
        assert_eq!(swipes, 0);
    }

    #[test]
    fn ocr_texts_are_read_in_the_order_the_chain_names_them() {
        let chain = Selector::Fallback {
            fallback: vec![
                sel(),
                Selector::OcrText {
                    ocr_text: "Row 30".into(),
                    locales: vec![],
                    modifiers: Modifiers::default(),
                },
                Selector::OcrText {
                    ocr_text: "Row 31".into(),
                    locales: vec!["ja".into()],
                    modifiers: Modifiers::default(),
                },
            ],
        };
        let got: Vec<&str> = ocr_texts(&chain).into_iter().map(|(t, _)| t).collect();
        assert_eq!(got, vec!["Row 30", "Row 31"]);
        let nested = Selector::Fallback {
            fallback: vec![chain.clone()],
        };
        let got: Vec<&str> = ocr_texts(&nested).into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            got,
            vec!["Row 30", "Row 31"],
            "a nested chain reads as written out"
        );
        assert!(ocr_texts(&sel()).is_empty());
    }
}
