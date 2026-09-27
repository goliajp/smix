//! `tapOn` takes maestro's `repeat` and `delay`.
//!
//! A consumer's QA gate opens on ten taps on a logo, each within 1.5 s of
//! the last. On Android every `tapOn` read the screen and waited for its
//! target to hold still before touching, and the gaps ran past 1.5 s; the
//! flow could only pass by tapping a bare coordinate. maestro's `tapOn`
//! has `repeat` (how many taps) and `delay` (ms between them) for this:
//! the target is found once and tapped that many times.

use smix_adapter_maestro::{Step, parse_flow_yaml};
use smix_selector::Selector;

fn step(body: &str) -> Result<Step, String> {
    parse_flow_yaml(&format!("appId: com.example\n---\n- tapOn: {body}\n"))
        .map(|f| f.steps.into_iter().next().expect("one step"))
        .map_err(|e| e.to_string())
}

#[test]
fn repeat_and_delay_find_once_and_tap_that_many_times() {
    match step("{ id: logo, repeat: 10, delay: 200 }").expect("parses") {
        Step::RepeatTap {
            selector: Selector::Id { id, .. },
            times,
            interval_ms,
            hold_ms,
        } => {
            assert_eq!(
                (id.as_str(), times, interval_ms, hold_ms),
                ("logo", 10, Some(200), None)
            );
        }
        other => panic!("expected a repeated tap: {other:?}"),
    }
}

#[test]
fn repeat_without_delay_takes_maestros_default() {
    match step("{ id: logo, repeat: 3 }").expect("parses") {
        Step::RepeatTap { interval_ms, .. } => assert_eq!(interval_ms, Some(100)),
        other => panic!("expected a repeated tap: {other:?}"),
    }
}

#[test]
fn repeat_one_is_a_tap() {
    assert!(matches!(
        step("{ id: logo, repeat: 1 }").expect("parses"),
        Step::TapOn { .. }
    ));
}

#[test]
fn values_that_are_not_counts_are_refused_by_name() {
    for (body, names) in [
        ("{ id: logo, repeat: 0 }", "repeat"),
        ("{ id: logo, repeat: -2 }", "repeat"),
        ("{ id: logo, repeat: two }", "repeat"),
        ("{ id: logo, repeat: 2.5 }", "repeat"),
        ("{ id: logo, repeat: 3, delay: -1 }", "delay"),
        ("{ id: logo, repeat: 3, delay: soon }", "delay"),
        ("{ id: logo, delay: 200 }", "delay"),
        ("{ point: \"50%,50%\", repeat: 3 }", "point"),
        ("{ id: logo, repeat: 3, optional: true }", "optional"),
        ("{ id: logo, repeat: 3, dispatch: xcui }", "dispatch"),
    ] {
        let err = step(body).expect_err(body);
        assert!(err.contains(names), "{body}: {err}");
    }
}
