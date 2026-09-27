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

#[test]
fn a_count_past_what_a_tap_burst_can_carry_is_refused_not_wrapped() {
    // 4294967296 is u32::MAX + 1: cast rather than checked, it becomes 0.
    for (body, names) in [
        ("{ id: logo, repeat: 4294967296 }", "repeat"),
        ("{ id: logo, repeat: 3, delay: 4294967296 }", "delay"),
    ] {
        let err = step(body).expect_err(body);
        assert!(err.contains(names), "{body}: {err}");
    }
}

fn repeat_tap(body: &str) -> Result<Step, String> {
    parse_flow_yaml(&format!("appId: com.example\n---\n- repeatTap: {body}\n"))
        .map(|f| f.steps.into_iter().next().expect("one step"))
        .map_err(|e| e.to_string())
}

#[test]
fn repeat_tap_reads_its_counts() {
    match repeat_tap("{ id: logo, times: 4, intervalMs: 50, holdMs: 20 }").expect("parses") {
        Step::RepeatTap {
            times,
            interval_ms,
            hold_ms,
            ..
        } => assert_eq!((times, interval_ms, hold_ms), (4, Some(50), Some(20))),
        other => panic!("expected a repeated tap: {other:?}"),
    }
}

#[test]
fn repeat_tap_refuses_what_is_not_a_count_by_name() {
    for (body, names) in [
        ("{ id: logo, times: 0 }", "times"),
        ("{ id: logo, times: 4294967296 }", "times"),
        ("{ id: logo, times: -1 }", "times"),
        ("{ id: logo, times: 2.5 }", "times"),
        ("{ id: logo, times: three }", "times"),
        (
            "{ id: logo, times: 2, intervalMs: 4294967296 }",
            "intervalMs",
        ),
        ("{ id: logo, times: 2, intervalMs: soon }", "intervalMs"),
        ("{ id: logo, times: 2, intervalMs: -5 }", "intervalMs"),
        ("{ id: logo, times: 2, holdMs: 4294967296 }", "holdMs"),
        ("{ id: logo, times: 2, holdMs: 1.5 }", "holdMs"),
    ] {
        let err = repeat_tap(body).expect_err(body);
        assert!(err.contains(names), "{body}: {err}");
    }
}
