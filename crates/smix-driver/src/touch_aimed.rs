//! A touch sent with the element it is aimed at, aimed again when the
//! runner finds that element gone from under the point before the touch.

use std::future::Future;
use std::time::Duration;

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_runner_client::{RunnerTransportError, TapTarget, target_gone_before_touch};
use smix_runner_wire::TapAtCoordResult;
use smix_selector::Selector;

use crate::{Aimed, HitElement, poll};

/// How long a tap keeps aiming again at a target that keeps leaving.
const AIM_AGAIN_FOR: Duration = Duration::from_millis(5000);
/// How long between two aims at a target that had left.
const AIM_AGAIN_AFTER: Duration = Duration::from_millis(100);

/// The target as the runner looks for it.
pub(crate) fn tap_target(h: &HitElement) -> TapTarget {
    TapTarget {
        identifier: h.identifier.clone(),
        label: h.label.clone(),
        frame: smix_screen::Rect {
            x: h.frame.0,
            y: h.frame.1,
            w: h.frame.2,
            h: h.frame.3,
        },
    }
}

/// Aim with `aim`, touch with `touch`; when the runner answers that the
/// target had left the point before the touch, nothing was touched — aim
/// again, until a look begun after [`AIM_AGAIN_FOR`] still finds it gone.
///
/// A consumer's player took its "go live" pill away by itself between the
/// aim and the touch; the touch went to the overlay under it, and the tap
/// was reported missed after the fact. A touch is no longer sent at a
/// point the target has already left.
pub(crate) async fn touch_where_aimed<A, AF, T, TF>(
    selector: &Selector,
    mut aim: A,
    mut touch: T,
    transport: fn(RunnerTransportError) -> ExpectationFailure,
) -> Result<(Aimed, TapAtCoordResult), ExpectationFailure>
where
    A: FnMut() -> AF,
    AF: Future<Output = Result<Aimed, ExpectationFailure>>,
    T: FnMut(f64, f64, smix_runner_wire::TreeReader, Option<TapTarget>) -> TF,
    TF: Future<Output = Result<TapAtCoordResult, RunnerTransportError>>,
{
    let budget = poll::Budget::new(AIM_AGAIN_FOR);
    loop {
        let look = budget.look();
        let aimed = aim().await?;
        let (nx, ny, target, reader) = &aimed;
        match crate::phases::time(
            "touch",
            touch(*nx, *ny, *reader, target.as_ref().map(tap_target)),
        )
        .await
        {
            Ok(landed) => return Ok((aimed, landed)),
            Err(e) => {
                let Some(saw) = target_gone_before_touch(&e) else {
                    return Err(transport(e));
                };
                if budget.spent_by(look) {
                    return Err(gone(selector, target.as_ref(), &saw));
                }
                // A target leaving answers at once; without a pause the
                // runner would be asked again as fast as it can answer.
                tokio::time::sleep(AIM_AGAIN_AFTER).await;
            }
        }
    }
}

fn gone(selector: &Selector, target: Option<&HitElement>, saw: &str) -> ExpectationFailure {
    let what = target.map_or_else(
        || smix_selector::describe_selector(selector),
        crate::landing::describe_hit,
    );
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: format!(
            "{what} was found each time it was aimed at and had left the point by the \
             time the touch would have gone in, for {} s; nothing was touched. Under \
             the point the runner found: {saw}",
            AIM_AGAIN_FOR.as_secs()
        ),
        selector: Some(selector.clone()),
        hint: Some(
            "the element is taking itself away (a control that hides after a while, a \
             screen in transition) — tap it as soon as it appears rather than after a \
             wait, or wait for the screen it belongs to"
                .to_string(),
        ),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_runner_wire::TreeReader;
    use std::cell::{Cell, RefCell};

    fn aimed() -> Aimed {
        (
            0.5,
            0.5,
            Some(HitElement {
                identifier: "btn_go_live".into(),
                label: "GO LIVE".into(),
                frame: (41.0, 942.0, 171.0, 126.0),
            }),
            TreeReader::Semantics,
        )
    }

    fn gone_error() -> RunnerTransportError {
        RunnerTransportError::NonSuccessStatus {
            endpoint: "/tap-at-norm-coord".into(),
            status: 409,
            body: r#"{"error":"target_gone_before_touch","saw":"id=overlay_detections"}"#.into(),
        }
    }

    fn failed(e: RunnerTransportError) -> ExpectationFailure {
        ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!("transport: {e}"),
            ..Default::default()
        })
    }

    fn selector() -> Selector {
        Selector::Id {
            id: "btn_go_live".into(),
            modifiers: smix_selector::Modifiers::default(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_target_gone_before_the_touch_is_aimed_at_again_and_then_touched() {
        let aims = Cell::new(0);
        let sent = RefCell::new(Vec::new());
        let (_, landed) = touch_where_aimed(
            &selector(),
            || {
                aims.set(aims.get() + 1);
                async { Ok(aimed()) }
            },
            |_, _, _, target| {
                sent.borrow_mut().push(target);
                let first = sent.borrow().len() == 1;
                async move {
                    if first {
                        Err(gone_error())
                    } else {
                        Ok(TapAtCoordResult::default())
                    }
                }
            },
            failed,
        )
        .await
        .expect("the second touch went in");
        assert_eq!(aims.get(), 2, "it did not aim again after the target left");
        assert!(landed.chain.is_empty());
        let sent = sent.borrow();
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent[0].as_ref().map(|t| t.identifier.as_str()),
            Some("btn_go_live"),
            "the runner was not told what the touch was aimed at"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_target_that_keeps_leaving_is_not_found_and_nothing_is_touched() {
        let e = touch_where_aimed(
            &selector(),
            || async { Ok(aimed()) },
            |_, _, _, _| async { Err(gone_error()) },
            failed,
        )
        .await
        .expect_err("a target gone at every touch was reported touched");
        assert_eq!(e.code, FailureCode::ElementNotFound);
        assert!(e.message.contains("nothing was touched"), "{}", e.message);
        assert!(
            e.message.contains("overlay_detections"),
            "what was under the point is not named: {}",
            e.message
        );
    }

    #[tokio::test(start_paused = true)]
    async fn any_other_runner_error_is_the_transport_s() {
        let e = touch_where_aimed(
            &selector(),
            || async { Ok(aimed()) },
            |_, _, _, _| async {
                Err(RunnerTransportError::NonSuccessStatus {
                    endpoint: "/tap-at-norm-coord".into(),
                    status: 500,
                    body: "boom".into(),
                })
            },
            failed,
        )
        .await
        .expect_err("a 500 was taken for success");
        assert_eq!(e.code, FailureCode::DriverError);
    }
}
