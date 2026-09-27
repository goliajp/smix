//! A tap is judged from the tree it was aimed from.
//!
//! On the fixture's Compose screen coming in (2026-09-27), the semantics
//! probe had `compose_input` 150-300 ms before the accessibility projection
//! did. A tap aimed from the probe in that gap focused the field every time
//! (8 of 8), and the chain — read from the projection — held only the
//! containers, so the step failed `TAP_MISSED`. The aim and the verdict now
//! name one reader, and a runner that judged from another is not believed.

use smix_driver::{ActVerdict, HitElement, landing_outcome};
use smix_error::FailureCode;
use smix_runner_wire::{HitChainEntry, TapAtCoordResult, TreeReader};
use smix_screen::Rect;
use smix_selector::Selector;

fn field() -> Selector {
    Selector::Id {
        id: "compose_input".into(),
        modifiers: Default::default(),
    }
}

fn compose_input() -> HitElement {
    HitElement {
        identifier: "compose_input".into(),
        label: String::new(),
        frame: (44.0, 225.0, 770.0, 154.0),
    }
}

fn entry(id: &str, x: f64, y: f64, w: f64, h: f64) -> HitChainEntry {
    HitChainEntry {
        identifier: id.into(),
        label: String::new(),
        frame: Rect { x, y, w, h },
    }
}

/// The chain under (429,302) with the field in it, and without.
fn with_field() -> Vec<HitChainEntry> {
    vec![
        entry("compose_input", 44.0, 225.0, 770.0, 154.0),
        entry("", 44.0, 180.0, 992.0, 1212.0),
        entry("", 0.0, 0.0, 1080.0, 2340.0),
    ]
}

fn containers_only() -> Vec<HitChainEntry> {
    vec![
        entry("", 44.0, 180.0, 992.0, 1212.0),
        entry("", 0.0, 136.0, 1080.0, 1300.0),
        entry("content", 0.0, 136.0, 1080.0, 2072.0),
    ]
}

fn landed(chain: Vec<HitChainEntry>, reader: Option<TreeReader>) -> TapAtCoordResult {
    TapAtCoordResult {
        chain,
        complete: true,
        reader,
        x: Some(429),
        y: Some(302),
        ..Default::default()
    }
}

fn aimed_from(reader: TreeReader) -> (f64, f64, Option<HitElement>, TreeReader) {
    (0.3972, 0.1291, Some(compose_input()), reader)
}

#[test]
fn aimed_from_the_semantics_tree_and_found_there_is_a_hit() {
    let outcome = landing_outcome(
        &field(),
        aimed_from(TreeReader::Semantics),
        &landed(with_field(), Some(TreeReader::Semantics)),
    )
    .expect("the field is under the point in the tree that aimed");
    assert_eq!(outcome.verdict, ActVerdict::Confirmed);
}

#[test]
fn aimed_from_the_semantics_tree_and_absent_there_is_still_a_miss() {
    let err = landing_outcome(
        &field(),
        aimed_from(TreeReader::Semantics),
        &landed(containers_only(), Some(TreeReader::Semantics)),
    )
    .expect_err("a touch the aiming tree puts nowhere near the field is a miss");
    assert_eq!(err.code, FailureCode::TapMissed, "{}", err.message);
    assert!(err.message.contains("(0.3972,0.1291)"), "{}", err.message);
    assert!(err.message.contains("pixel (429,302)"), "{}", err.message);
    assert!(
        err.message.contains("judged from the semantics tree"),
        "{}",
        err.message
    );
}

#[test]
fn a_verdict_read_from_another_tree_is_not_taken_either_way() {
    // What the runner did before: aimed from the probe, judged from the
    // projection, and said nothing about which.
    let err = landing_outcome(
        &field(),
        aimed_from(TreeReader::Semantics),
        &landed(containers_only(), None),
    )
    .expect_err("a verdict from the other reader is not a verdict on this aim");
    assert_eq!(err.code, FailureCode::DriverError, "{}", err.message);
    assert!(err.message.contains("semantics"), "{}", err.message);
}

#[test]
fn a_semantics_tree_the_runner_could_not_read_is_said_as_such() {
    let mut r = landed(Vec::new(), None);
    r.complete = false;
    r.reader_error = Some("the probe did not answer".into());
    let err = landing_outcome(&field(), aimed_from(TreeReader::Semantics), &r)
        .expect_err("nothing was read, so nothing is judged");
    assert_eq!(err.code, FailureCode::DriverError, "{}", err.message);
    assert!(
        err.message.contains("the probe did not answer"),
        "{}",
        err.message
    );
}

#[test]
fn a_runner_with_one_reader_answers_for_the_accessibility_tree() {
    // iOS: the chain is the tree it aimed from, and the runner does not say.
    let outcome = landing_outcome(
        &field(),
        aimed_from(TreeReader::Accessibility),
        &landed(with_field(), None),
    )
    .expect("the one reader aimed and judged");
    assert_eq!(outcome.verdict, ActVerdict::Confirmed);
}
