//! A recording is started only on a simulator that is booted.
//!
//! `simctl io <udid> recordVideo` on a shut-down simulator prints
//! "Recording started", later "Wrote video to: …", and leaves a zero-byte
//! file behind (2026-09-25, sim-smix-03, LD1). smix reported "recording"
//! on the strength of that, and the next ledger check correctly said the
//! device was not there. The simulator's state is the only witness that
//! does not lie about this, so it is asked before anything is spawned.

use smix_simctl::recording_may_start;

const UDID: &str = "11111111-2222-3333-4444-555555555555";

#[test]
fn a_booted_simulator_may_be_recorded() {
    assert!(recording_may_start(UDID, Some("Booted")).is_ok());
}

#[test]
fn a_simulator_that_is_not_booted_is_refused_by_its_state() {
    for state in ["Shutdown", "Booting", "Shutting Down", "Creating"] {
        let err = recording_may_start(UDID, Some(state))
            .expect_err("only a booted simulator has a screen to record");
        let text = err.to_string();
        assert!(
            text.contains(state),
            "names the state it saw ({state}): {text}"
        );
        assert!(text.contains(UDID), "names the device: {text}");
    }
}

#[test]
fn a_simulator_simctl_does_not_list_is_refused_as_unknown() {
    let err = recording_may_start(UDID, None).expect_err("an unknown device has no screen");
    assert!(err.to_string().contains("does not list"), "{err}");
}
