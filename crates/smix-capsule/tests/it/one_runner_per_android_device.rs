//! Whose device is this? The question `runner up` and `runner down` did
//! not ask.
//!
//! An Android device has exactly one runner: one instrumentation
//! package, one fixed device-side port, every host port forwarding onto
//! the same in-process server. So neither verb is scoped by port -- and
//! `down` does not even read its `port` argument, it force-stops the
//! package.
//!
//! Measured 2026-08-29: `runner down --device emulator-5554` printed
//! `host ports 60752, 28080 closed`. 60752 was ours; 28080 belonged to a
//! consumer's suite, mid-batch, and its two `smix run` processes went
//! with it. The ledger had recorded whose it was the whole time --
//! `holder.cmd` is the driving process's command line -- and nothing on
//! this path ever read it.
//!
//! This drives the decision against a fabricated ledger, because the
//! only other way to watch it fail is to let a real one through.

use smix_capsule::runner_android::live_foreign_holder_in;
use smix_lease::store::LeaseDir;
use smix_lease::{Lease, ProcIdentity};

fn lease_for(dir: &LeaseDir, serial: &str, holder: ProcIdentity) {
    let now = smix_lease::store::now_rfc3339();
    smix_lease::store::write(
        dir,
        &Lease {
            device_id: serial.to_string(),
            holder,
            acquired_at: now.clone(),
            heartbeat_at: now,
            resources: Vec::new(),
        },
    )
    .expect("write lease");
}

/// A live process that is not this one, identified the way the ledger
/// identifies things.
fn a_live_stranger() -> (std::process::Child, ProcIdentity) {
    let child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    // The start time has to come from the same reader the check uses, or
    // the identity would fail to match for a reason that has nothing to
    // do with what is being tested.
    let identity = smix_lease::store::identify(child.id()).expect("identify the stranger");
    (child, identity)
}

#[test]
fn a_live_holder_that_is_not_us_stops_the_verb() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    let (mut child, stranger) = a_live_stranger();
    lease_for(&leases, "emulator-5554", stranger.clone());

    let refused = live_foreign_holder_in(&leases, "emulator-5554")
        .expect("a live holder that is not this process must stop the verb");
    assert_eq!(refused.pid, stranger.pid);
    assert!(
        refused.cmd.contains("sleep"),
        "the refusal has to carry the holder's command line, or the person \
         reading it cannot tell whose run they were about to end: {refused:?}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_holder_that_has_exited_does_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    let (mut child, stranger) = a_live_stranger();
    let _ = child.kill();
    let _ = child.wait();
    lease_for(&leases, "emulator-5554", stranger);

    // The ordinary case: a flow exits when it is done, and tearing down
    // after one is what every gate does. A check that refused here would
    // refuse the caller its own device, which is worse than the accident
    // it prevents -- it would be hit every run instead of rarely.
    assert!(
        live_foreign_holder_in(&leases, "emulator-5554").is_none(),
        "a holder whose process has ended is not a holder"
    );
}

#[test]
fn our_own_pid_is_not_a_stranger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    lease_for(&leases, "emulator-5554", smix_lease::store::identify_self());

    assert!(
        live_foreign_holder_in(&leases, "emulator-5554").is_none(),
        "this process holding the device is not a reason to refuse this process"
    );
}

#[test]
fn a_device_with_no_lease_is_free() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    assert!(
        live_foreign_holder_in(&leases, "emulator-9999").is_none(),
        "nothing recorded means nothing to protect"
    );
}

#[test]
fn a_reused_pid_is_not_the_holder() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    let (mut child, mut stranger) = a_live_stranger();
    // The pid is alive; the process at it is not the one recorded. This
    // is what pid reuse looks like from here, and it is why the ledger
    // records a start time at all -- without comparing it, a device
    // would stay "held" by whatever unrelated process inherited the
    // number, and the refusal would be permanent and about nothing.
    stranger.started_at = "Thu Jan  1 00:00:00 1970".to_string();
    lease_for(&leases, "emulator-5554", stranger);

    assert!(
        live_foreign_holder_in(&leases, "emulator-5554").is_none(),
        "a pid that now belongs to something else is not the recorded holder"
    );

    let _ = child.kill();
    let _ = child.wait();
}
