//! Waiting is a question the screen can answer, when something is there to ask.
//!
//! smix polls: every 250 ms it asks whether a selector resolves yet, and
//! between those asks it knows nothing. Compose can say directly — it knows
//! whether a frame still has measure or layout outstanding — and when the
//! probe is present that answer is available for the asking.
//!
//! The strategy is a pure function so the three cases can be judged without
//! a device, and so the third one cannot be reached by accident: with no
//! probe, smix polls exactly as it always has, and says that it is.

use smix_runner_client::{WaitPlan, WaitStrategy};

#[test]
fn an_idle_signal_ends_the_wait_now() {
    assert_eq!(
        WaitStrategy::decide(Some(true)),
        WaitPlan::Settled,
        "the screen said it has no layout work outstanding and the wait \
         continued anyway"
    );
}

#[test]
fn a_busy_signal_keeps_waiting() {
    assert_eq!(
        WaitStrategy::decide(Some(false)),
        WaitPlan::AskAgainWhenTold
    );
}

#[test]
fn no_probe_falls_back_to_the_interval_smix_has_always_used() {
    // Not a new number: this is the behaviour every release so far has
    // had, and a fallback that quietly changed the cadence would be a
    // different product for anyone without the probe.
    assert_eq!(WaitStrategy::decide(None), WaitPlan::PollEvery(250));
}

#[test]
fn the_fallback_says_it_is_a_fallback() {
    let note = WaitPlan::PollEvery(250)
        .why()
        .expect("polling is the lesser answer and has to say so");
    assert!(
        note.contains("probe"),
        "the note does not say what would make it better: {note}"
    );
    assert!(
        WaitPlan::Settled.why().is_none(),
        "being told outright has nothing to explain"
    );
}

// --- what the runner reports, turned into a plan ------------------------

use smix_runner_client::ProbeStatus;

fn status(json: &str) -> ProbeStatus {
    serde_json::from_str(json).expect("the runner's /probe body parses")
}

#[test]
fn a_settled_screen_is_waited_on_by_being_told() {
    let s = status(r#"{"present":true,"version":"1","roots":1,"idle":true}"#);
    assert_eq!(s.plan(), WaitPlan::Settled);
}

#[test]
fn a_busy_screen_is_too() {
    let s = status(r#"{"present":true,"version":"1","roots":1,"idle":false}"#);
    assert_eq!(s.plan(), WaitPlan::AskAgainWhenTold);
}

#[test]
fn an_app_without_the_probe_polls() {
    let s = status(
        r#"{"present":false,"why":"com.example declares no smix probe — it is not in this build"}"#,
    );
    assert_eq!(s.plan(), WaitPlan::PollEvery(250));
    assert!(s.why.unwrap().contains("not in this build"));
}

#[test]
fn a_probe_that_is_present_but_says_nothing_still_polls() {
    // `present: true, idle: null` is what a probe answers when it is there
    // and could not tell. That is not "settled", and reading it as one
    // would end every wait instantly on a screen still moving.
    let s = status(r#"{"present":true,"version":"1","roots":0,"idle":null}"#);
    assert_eq!(s.plan(), WaitPlan::PollEvery(250));
}

#[test]
fn a_probe_with_no_compose_on_screen_is_not_the_same_as_no_probe() {
    // Both answer "poll", but for opposite reasons, and only one of them
    // is fixed by editing a build file. The distinction has to survive on
    // the struct even where the plan collapses it.
    let none = status(r#"{"present":false,"why":"no probe"}"#);
    let empty = status(r#"{"present":true,"version":"1","roots":0,"idle":null}"#);
    assert!(!none.present && empty.present);
    assert_eq!(empty.roots, 0);
}

#[test]
fn an_idle_flag_from_an_absent_probe_counts_for_nothing() {
    // `present:false` with an `idle` beside it should not exist, and a
    // mutation sweep found that nothing here would have noticed if it did:
    // every other case pairs absence with a missing flag, so reading only
    // `idle` gave the same answers. A probe that is not there did not tell
    // us the screen had settled, whatever else came back in the envelope.
    let s = status(r#"{"present":false,"why":"no probe","idle":true}"#);
    assert_eq!(
        s.plan(),
        WaitPlan::PollEvery(250),
        "an absent probe's idle flag was believed"
    );
}

// What the probe reports, turned into whether the screen has settled.
// Measured on an emulator with its guest's CPUs held by busy loops: the
// tree can hold still for over a second in the middle of an injected
// drag, and for a few hundred milliseconds between two frames of a fling,
// so stillness alone is not the answer.

#[test]
fn still_long_enough_on_time_and_nothing_waiting_for_a_frame_is_settled() {
    assert_eq!(
        smix_runner_client::settled(Some(400), Some(20), Some(false)),
        Some(true)
    );
}

#[test]
fn an_animation_waiting_for_a_frame_is_not_settled_however_still_the_tree() {
    assert_eq!(
        smix_runner_client::settled(Some(600), Some(20), Some(true)),
        Some(false)
    );
}

#[test]
fn a_sampler_running_late_is_not_believed() {
    assert_eq!(
        smix_runner_client::settled(Some(600), Some(400), Some(false)),
        Some(false)
    );
}

#[test]
fn not_still_long_enough_is_not_settled() {
    assert_eq!(
        smix_runner_client::settled(Some(120), Some(20), Some(false)),
        Some(false)
    );
}

#[test]
fn a_probe_that_never_looked_is_not_settled() {
    assert_eq!(
        smix_runner_client::settled(Some(-1), Some(-1), None),
        Some(false)
    );
}

#[test]
fn a_probe_too_old_to_say_how_late_it_is_gives_no_answer() {
    // Its stillness ran on the wall clock, which kept counting while the
    // app's main thread was starved.
    assert_eq!(smix_runner_client::settled(Some(4000), None, None), None);
}

#[tokio::test]
async fn the_status_read_from_the_runner_carries_the_verdict() {
    use smix_runner_client::HttpRunnerClient;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/probe"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "present": true, "app": "dev.smix.fixture", "version": "1", "roots": 1,
            "quietMs": 900, "sampleLateMs": 10, "framesPending": true,
        })))
        .mount(&server)
        .await;
    let client = HttpRunnerClient::with_base(server.uri());
    let status = client
        .probe_status("dev.smix.fixture")
        .await
        .expect("status");
    assert_eq!(
        status.idle,
        Some(false),
        "a fling in flight was read as settled"
    );
    assert_eq!(status.plan(), WaitPlan::AskAgainWhenTold);
}
