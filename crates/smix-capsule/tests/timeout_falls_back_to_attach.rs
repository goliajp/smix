//! A bring-up that ran out of time gets one more attempt, and only one.
//!
//! The first attempt asks the runner to launch the app. When that never
//! finishes, the runner host is often alive and the app is often
//! launchable by other means — so the second attempt foregrounds the app
//! and asks the runner to attach to it rather than start it.
//!
//! "Only one" is the whole of the boundary. Without the already-tried
//! row below, "at most once" is a sentence in a comment and the code is
//! an unbounded loop, and a bring-up that cannot succeed would sit there
//! doubling its own timeout forever.
//!
//! Pure on purpose: every row is reachable without a device, which is
//! what makes the table exhaustive rather than illustrative.

use smix_capsule::runner::{AfterTimeout, RunnerTarget, decide_after_timeout};

const GAP: &str = "not-running";
const LOG: &str = "…xcodebuild transcript…";

fn decide(
    target: RunnerTarget<'_>,
    bundle: Option<&str>,
    attach_requested: bool,
    attach_already_tried: bool,
) -> AfterTimeout {
    decide_after_timeout(
        target,
        bundle,
        attach_requested,
        attach_already_tried,
        Some(GAP),
        300,
        LOG,
    )
}

fn refusal(verdict: AfterTimeout) -> String {
    match verdict {
        AfterTimeout::GiveUp { message } => message,
        AfterTimeout::RetryAsAttach { because } => {
            panic!("expected to stop, and it went round again: {because}")
        }
    }
}

#[test]
fn a_simulator_with_a_bundle_gets_one_more_attempt() {
    let verdict = decide(
        RunnerTarget::Simulator,
        Some("com.example.app"),
        false,
        false,
    );
    let AfterTimeout::RetryAsAttach { because } = verdict else {
        panic!("the usual case is exactly the one worth retrying: {verdict:?}");
    };
    assert!(
        because.contains(GAP),
        "say what the first attempt was stuck on: {because}"
    );
}

#[test]
fn the_second_time_it_stops() {
    let message = refusal(decide(
        RunnerTarget::Simulator,
        Some("com.example.app"),
        false,
        true,
    ));
    assert!(
        message.contains("attach"),
        "say that the other way was tried too: {message}"
    );
    assert!(
        message.contains(GAP),
        "the reason the wait failed is still the news: {message}"
    );
}

#[test]
fn a_run_that_was_already_attaching_does_not_retry_as_attaching() {
    let message = refusal(decide(
        RunnerTarget::Simulator,
        Some("com.example.app"),
        true,
        false,
    ));
    assert!(
        message.contains("--no-launch"),
        "this attempt already was the attach one; say so by name: {message}"
    );
}

#[test]
fn without_a_bundle_there_is_nothing_to_bring_to_the_front() {
    let message = refusal(decide(RunnerTarget::Simulator, None, false, false));
    assert!(
        message.contains("--bundle"),
        "the retry foregrounds an app, and no app was named: {message}"
    );
}

#[test]
fn a_physical_device_is_told_it_does_not_have_this() {
    let message = refusal(decide(
        RunnerTarget::Physical { team: "ABCDE12345" },
        Some("com.example.app"),
        false,
        false,
    ));
    assert!(
        message.contains("physical") || message.contains("device"),
        "§9 #1 ③: say this device does not have the capability: {message}"
    );
    assert!(
        message.contains("simctl"),
        "name what is missing rather than only that something is: {message}"
    );
}

#[test]
fn the_wait_that_failed_is_still_described_when_there_is_no_gap() {
    let verdict = decide_after_timeout(
        RunnerTarget::Simulator,
        Some("com.example.app"),
        false,
        true,
        None,
        300,
        LOG,
    );
    let message = refusal(verdict);
    assert!(
        message.contains("300"),
        "how long it waited is part of the answer: {message}"
    );
    assert!(
        message.contains(LOG),
        "the log tail is what a person reads next: {message}"
    );
}

/// The retry gets a whole new deadline, not what was left of the first.
///
/// Source-level because seeing it behave would mean sitting through two
/// real timeouts. The property is in the signature: an attempt takes a
/// *duration* and starts its own clock, so two attempts cannot share
/// one. Passing an `Instant` in would make the retry inherit whatever
/// was left — which is the same as designing the fallback to fail.
#[test]
fn each_attempt_starts_its_own_clock() {
    let src = include_str!("../src/runner.rs");
    let helper = src
        .split("fn one_bring_up(")
        .nth(1)
        .expect("the single attempt is still its own function");
    let signature = helper
        .split(") -> Result<Attempt")
        .next()
        .expect("its arguments");
    assert!(
        signature.contains("timeout_secs: u64"),
        "an attempt takes how long it may take: {signature}"
    );
    assert!(
        !signature.contains("Instant"),
        "an attempt was handed a deadline, so the retry inherits the first \
         one's remains: {signature}"
    );
    let body = helper.split("\n}\n").next().expect("its body");
    assert!(
        body.contains("let deadline = std::time::Instant::now()"),
        "the attempt no longer starts its own clock: it has to, or the two \
         attempts share one"
    );
}
