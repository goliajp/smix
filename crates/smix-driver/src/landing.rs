//! Where a selector act landed, judged from the tree it was aimed from.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_selector::Selector;

use crate::{ActOutcome, ActVerdict, HitElement, tap_mismatch_is_fatal};

/// Tolerance, in points, for comparing frames.
///
/// A frame makes a round trip — the host normalises the centre against
/// the app frame, the runner multiplies it back — so exact equality
/// would fail on arithmetic rather than on aim.
const FRAME_TOLERANCE_PT: f64 = 1.0;

/// What a chain of hit elements leaves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainCoverage {
    /// Named elements only (the iOS runner): an unnamed target's absence
    /// proves nothing.
    NamedOnly,
    /// Every element under the point (the Android runner): absence is a miss.
    Every,
}

/// Where a touch aimed at a selector goes: the normalised point, the
/// element it is aimed at, and the tree that aim was read from.
pub type Aimed = (f64, f64, Option<HitElement>, smix_runner_wire::TreeReader);

/// The tree a landing is judged from, given the tree the aim was read
/// from: the same one, always, and decided here and nowhere else.
///
/// The aim and the verdict are two readings of one screen, and two
/// readers can disagree about it. On a Compose screen coming in, the
/// accessibility projection lags the semantics tree by 150-300 ms
/// (measured on the fixture): a touch aimed from the semantics tree and
/// judged from the projection in that gap focused its field and was
/// reported `TAP_MISSED`.
#[must_use]
pub fn verdict_reader(aimed_from: smix_runner_client::TreeSource) -> smix_runner_wire::TreeReader {
    match aimed_from {
        smix_runner_client::TreeSource::Accessibility => {
            smix_runner_wire::TreeReader::Accessibility
        }
        smix_runner_client::TreeSource::Semantics => smix_runner_wire::TreeReader::Semantics,
    }
}

/// Where the touch went, for a failure message: the normalised point, the
/// pixel the runner turned it into when it said, and the tree it was
/// judged from.
fn describe_touch(
    nx: f64,
    ny: f64,
    landed: &smix_runner_wire::TapAtCoordResult,
    reader: smix_runner_wire::TreeReader,
) -> String {
    let pixel = match (landed.x, landed.y) {
        (Some(x), Some(y)) => format!(", pixel ({x},{y})"),
        _ => String::new(),
    };
    format!(
        "touched at ({nx:.4},{ny:.4}){pixel}, judged from the {} tree",
        reader.name()
    )
}

/// The verdict on a selector act, from what the runner said it touched.
///
/// One judgement for every platform and every act aimed by a selector —
/// tap, double tap, long press. The iOS tap used to hold this inline and
/// the Android acts had none, so an Android tap was never judged and a
/// consumer's dialog confirm, pressed below the dialog, was reported as
/// `tapped` with exit 0.
///
/// `aimed` is `None` when the selector resolved to a point rather than
/// a node (text found by OCR, for one): there is no element to compare,
/// and the outcome says so rather than pretending.
///
/// A runner that reports NOTHING under the point fails the step. Until
/// now that was "could not be judged" and counted as a pass, on the
/// reasoning that failing it would break everyone driving an older
/// runner — which is how the Android runner, which reported nothing at
/// all, turned every one of its misses into a pass.
///
/// # Errors
///
/// `TapMissed` when the touch was delivered to something other than the
/// element aimed at (unless `SMIX_TAP_HIT_MISMATCH=warn`), and
/// `DriverError` when the runner reported nothing about where it landed.
pub fn landing_outcome(
    selector: &Selector,
    aim: Aimed,
    landed: &smix_runner_wire::TapAtCoordResult,
) -> Result<ActOutcome, ExpectationFailure> {
    let (nx, ny, aimed, reader) = aim;
    let chain: Vec<HitElement> = landed
        .chain
        .iter()
        .map(|e| HitElement {
            identifier: e.identifier.clone(),
            label: e.label.clone(),
            frame: (e.frame.x, e.frame.y, e.frame.w, e.frame.h),
        })
        .collect();
    let Some(aimed) = aimed else {
        return Ok(ActOutcome {
            target: None,
            observed: chain,
            verdict: ActVerdict::Unconfirmable(
                "the selector resolved to a coordinate but not to a node, so \
                 there is nothing to compare the tapped point against"
                    .into(),
            ),
        });
    };
    // The runner read the tree it was asked to, or the verdict is on
    // something else. A runner with one reader (iOS) says nothing and
    // answers from the accessibility tree, which is only that one's answer.
    let judged_from = landed
        .reader
        .unwrap_or(smix_runner_wire::TreeReader::Accessibility);
    if judged_from != reader || landed.reader_error.is_some() {
        let why = landed
            .reader_error
            .clone()
            .unwrap_or_else(|| format!("it read the {} tree instead", judged_from.name()));
        return Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!(
                "the touch aimed at {} went in, and where it landed cannot be told: it was \
                 aimed from the {} tree and the runner could not judge it from that one ({why})",
                describe_hit(&aimed),
                reader.name(),
            ),
            selector: Some(selector.clone()),
            ..Default::default()
        }));
    }
    if chain.is_empty() && !landed.complete {
        return Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!(
                "the touch aimed at {} went in, and the runner reported \
                 nothing about what it was delivered to — so whether it \
                 landed cannot be told",
                describe_hit(&aimed)
            ),
            selector: Some(selector.clone()),
            hint: Some(
                "a runner older than the field that carries it answers this \
                 way; `smix runner up --force` rebuilds the runner from this \
                 smix"
                    .into(),
            ),
            ..Default::default()
        }));
    }
    let coverage = if landed.complete {
        ChainCoverage::Every
    } else {
        ChainCoverage::NamedOnly
    };
    // A runner that lists everything under the point and lists nothing
    // is saying where the touch went: outside every window it can read.
    // Measured with gesture navigation and a system dialog in front —
    // the point was below the dialog, where no readable window reaches.
    let verdict = if chain.is_empty() {
        ActVerdict::Missed(format!(
            "aimed at {} and the touch was delivered outside every window \
             the runner can read",
            describe_hit(&aimed)
        ))
    } else {
        tap_landed_within(&aimed, &chain, coverage)
    };
    if let ActVerdict::Missed(why) = &verdict {
        if tap_mismatch_is_fatal() {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::TapMissed),
                message: format!(
                    "tap did not land where it aimed: {why} ({})",
                    describe_touch(nx, ny, landed, reader)
                ),
                selector: Some(selector.clone()),
                hint: Some(
                    "the element moved between the tree fetch and the tap, or \
                     the touch went to something over it; wait for the screen \
                     to settle first. Set SMIX_TAP_HIT_MISMATCH=warn to \
                     downgrade this to a warning while migrating a suite."
                        .into(),
                ),
                ..Default::default()
            }));
        }
        eprintln!("smix: warning: tap did not land where it aimed: {why}");
    }
    Ok(ActOutcome {
        target: Some(aimed),
        observed: chain,
        verdict,
    })
}

/// Did the touch land inside the element it aimed at?
///
/// `chain` is every named element containing the tapped point, as the
/// runner found them after synthesising the touch.
///
/// # Why containment and not identity
///
/// The first version of this asked whether the element at the point
/// *was* the element aimed at. A live tree says why that is wrong. At
/// the centre of the first row of Settings, the named elements
/// containing the point are:
///
/// ```text
/// staticText  "登录以访问iCloud数据…"                      area 7283
/// button      id=com.apple.settings.primaryAppleAccount   area 33423
/// application id=com.apple.Preferences                    area 351348
/// ```
///
/// A flow aiming at that button taps its centre, and the innermost
/// element there is the button's own label. Identity would call a
/// perfectly good tap a miss — and text nested inside a row is what
/// every list screen looks like. Containment gets it right: the button
/// is on the chain.
///
/// # What the chain leaves out
///
/// The iOS runner lists named elements only; the Android runner lists
/// every element under the point. `coverage` says which, and it decides
/// what an unnamed target's absence means: nothing on iOS (it may just
/// be unnamed), a miss on Android (the list is whole).
///
/// # WHAT THIS CANNOT SEE
///
/// **Occlusion.** A scrim covering the aimed element contains the
/// point too, so this passes. The snapshot the runner walks carries no
/// z-order (`TreeRoute.swift`: snapshots are dead frames), and
/// `isHittable` — Apple's own answer — has been rejected here twice
/// on purpose: it reports false for an element that is reachable in
/// the AX tree but visually covered, which is exactly the see-through
/// tap `SmixRunnerUITests.swift` performs deliberately, and it broke a
/// QA-overlay assertion in v1.0.27.
///
/// So this closes the stale-frame half of "the tap reported success and
/// nothing happened" and not the covered-element half. The whole chain
/// travels in the outcome regardless, so a caller can see the scrim
/// even when the verdict passes.
pub fn tap_landed_within(
    aimed: &HitElement,
    chain: &[HitElement],
    coverage: ChainCoverage,
) -> ActVerdict {
    if chain.is_empty() {
        return ActVerdict::Missed(format!(
            "aimed at {} and the tapped point held nothing — the element \
             moved between the tree fetch and the tap, or its frame was \
             stale",
            describe_hit(aimed)
        ));
    }
    if chain.iter().any(|c| same_element(aimed, c)) {
        return ActVerdict::Confirmed;
    }
    if aimed.identifier.is_empty() && aimed.label.is_empty() && coverage == ChainCoverage::NamedOnly
    {
        return ActVerdict::Unconfirmable(format!(
            "the element aimed at carries neither an identifier nor a \
             label, so it cannot be looked for among the {} element(s) \
             at the tapped point",
            chain.len()
        ));
    }
    ActVerdict::Missed(format!(
        "aimed at {} but the tapped point is inside {} instead",
        describe_hit(aimed),
        chain
            .iter()
            .map(describe_hit)
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Are these two descriptions the same element?
///
/// By the strongest field both carry: identifier, then label, then
/// geometry.
fn same_element(a: &HitElement, b: &HitElement) -> bool {
    if !a.identifier.is_empty() && !b.identifier.is_empty() {
        return a.identifier == b.identifier;
    }
    if !a.label.is_empty() && !b.label.is_empty() {
        return a.label == b.label;
    }
    if a.identifier.is_empty()
        && a.label.is_empty()
        && b.identifier.is_empty()
        && b.label.is_empty()
    {
        let close = |x: f64, y: f64| (x - y).abs() <= FRAME_TOLERANCE_PT;
        return close(a.frame.0, b.frame.0)
            && close(a.frame.1, b.frame.1)
            && close(a.frame.2, b.frame.2)
            && close(a.frame.3, b.frame.3);
    }
    // One is named and the other is not: they are describable in
    // different vocabularies, which is not evidence of sameness.
    false
}

fn describe_hit(e: &HitElement) -> String {
    if !e.identifier.is_empty() {
        format!("id={}", e.identifier)
    } else if !e.label.is_empty() {
        format!("label={:?}", e.label)
    } else {
        format!(
            "an unnamed element at ({:.0},{:.0} {:.0}x{:.0})",
            e.frame.0, e.frame.1, e.frame.2, e.frame.3
        )
    }
}
