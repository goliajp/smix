//! A tap the runner says nothing about is not a tap that landed.
//!
//! Every Android selector tap used to come back with no chain, and the
//! host recorded "could not be judged" — which the flow counted as a
//! pass. A consumer's dialog confirm was pressed below the dialog,
//! dismissed it, and was reported as `tapped`, exit 0. Both halves of
//! that are gone: the Android runner now reports what it touched, and a
//! runner that reports nothing fails the step, naming itself.

use smix_driver::{ActVerdict, HitElement, landing_outcome};
use smix_runner_wire::{HitChainEntry, TapAtCoordResult};
use smix_screen::Rect;
use smix_selector::Selector;

fn aim() -> Selector {
    Selector::Id {
        id: "button1".into(),
        modifiers: Default::default(),
    }
}

fn button1() -> HitElement {
    HitElement {
        identifier: "button1".into(),
        label: String::new(),
        frame: (773.0, 1192.0, 203.0, 149.0),
    }
}

fn entry(id: &str, x: f64, y: f64, w: f64, h: f64) -> HitChainEntry {
    HitChainEntry {
        identifier: id.into(),
        label: String::new(),
        frame: Rect { x, y, w, h },
    }
}

#[test]
fn a_selector_tap_the_runner_reports_nothing_about_fails_and_names_the_runner() {
    let landed = TapAtCoordResult {
        chain: vec![],
        complete: false,
    };
    let err = landing_outcome(&aim(), Some(button1()), &landed)
        .expect_err("an empty chain was a pass until now, and must not be");
    let prompt = err.to_prompt();
    assert!(
        prompt.contains("runner"),
        "the failure names the runner: {prompt}"
    );
}

#[test]
fn a_selector_tap_delivered_below_its_dialog_fails() {
    let landed = TapAtCoordResult {
        chain: vec![entry("", 0.0, 0.0, 1080.0, 2340.0)],
        complete: true,
    };
    let err = landing_outcome(&aim(), Some(button1()), &landed)
        .expect_err("the touch went to the activity behind the dialog");
    assert!(
        err.to_prompt().contains("button1"),
        "the failure says what was aimed at: {}",
        err.to_prompt()
    );
}

#[test]
fn a_selector_tap_delivered_to_its_button_is_confirmed() {
    let landed = TapAtCoordResult {
        chain: vec![
            entry("button1", 773.0, 1192.0, 203.0, 149.0),
            entry("buttonPanel", 44.0, 1150.0, 992.0, 246.0),
        ],
        complete: true,
    };
    let outcome =
        landing_outcome(&aim(), Some(button1()), &landed).expect("the touch went to the button");
    assert_eq!(outcome.verdict, ActVerdict::Confirmed);
}

#[test]
fn a_touch_delivered_outside_every_readable_window_is_a_miss_not_an_old_runner() {
    // Measured with gesture navigation and the system's uninstall dialog in
    // front: the point landed below the dialog, where no window the runner
    // can read reaches. An Android chain lists everything under the point,
    // so empty means "nothing there" — a miss — and blaming the runner's
    // age sends the reader to rebuild something that is not broken.
    let landed = TapAtCoordResult {
        chain: vec![],
        complete: true,
    };
    let err = landing_outcome(&aim(), Some(button1()), &landed)
        .expect_err("the touch went to no window at all");
    let prompt = err.to_prompt();
    assert!(
        prompt.contains("TAP_MISSED"),
        "a miss, not a driver error: {prompt}"
    );
    assert!(
        !prompt.contains("runner up --force"),
        "not the old-runner advice: {prompt}"
    );
}
