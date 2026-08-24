//! A claim says "drive it", never "switch it off".
//!
//! The ledger could say "I booted it" and "I found it up" — two
//! observations — and the decision between them had nowhere to go. A
//! machine's own dedicated emulator, running, started by a hand that
//! wrote no ledger, was drivable by nobody: the only row that made a
//! device drivable was the same row that made it shut-downable, and
//! nothing may claim to have booted what it did not. The release gates
//! got through that on an environment variable which records nothing and
//! is forgotten when the command exits, so the next run makes the same
//! decision with no way to read who made it last.
//!
//! `Resource::Claimed` is the third state, and its whole value is what it
//! is NOT: `may_shut_down` must keep reading only the boot row. A claim
//! that leaked into teardown entitlement would hand a sweep somebody
//! else's device — the exact harm `by_us` exists to prevent, reached by
//! a cheaper spelling of the fix.
//!
//! Each test falsifies one judgement on its own.

use smix_lease::store::{self, LeaseDir};
use smix_lease::{
    CleanupAction, Held, HolderProbe, Lease, ProcIdentity, PruneVerdict, Resource, Row,
};

fn holder() -> ProcIdentity {
    ProcIdentity {
        pid: 4_294_967_293,
        started_at: "Mon Aug 24 00:00:00 2026".into(),
        cmd: "smix lease claim emulator-5554".into(),
    }
}

fn lease_with(resources: Vec<Resource>) -> Lease {
    Lease {
        device_id: "emulator-5554".into(),
        holder: holder(),
        acquired_at: "2026-08-24T00:00:00Z".into(),
        heartbeat_at: "2026-08-24T00:00:00Z".into(),
        resources: resources.into_iter().map(Row::Known).collect(),
    }
}

fn claimed() -> Resource {
    Resource::Claimed {
        at: "2026-08-24T00:00:00Z".into(),
    }
}

fn dir() -> (tempfile::TempDir, LeaseDir) {
    let t = tempfile::tempdir().expect("tempdir");
    let d = LeaseDir::at(t.path().to_path_buf());
    (t, d)
}

#[test]
fn a_claim_is_not_permission_to_shut_the_device_down() {
    let l = lease_with(vec![claimed()]);
    assert!(
        !smix_lease::may_shut_down(Some(&l)),
        "a claim states that nothing here booted the device; reading it as \
         teardown entitlement would switch off somebody else's session"
    );
}

#[test]
fn a_boot_row_still_is() {
    let l = lease_with(vec![Resource::Booted { by_us: true }]);
    assert!(
        smix_lease::may_shut_down(Some(&l)),
        "the claim must not have moved what the boot row means"
    );
}

#[test]
fn a_claim_owes_no_close() {
    let l = lease_with(vec![claimed()]);
    let plan = smix_lease::plan_cleanup(&l);
    assert!(
        !plan
            .iter()
            .any(|a| matches!(a, CleanupAction::ShutdownSim { .. })),
        "cleanup planned a shutdown for a device the ledger says it did not \
         boot: {plan:?}"
    );
}

#[test]
fn a_claim_ends_when_the_device_goes_off() {
    let held = Held {
        lease: lease_with(vec![claimed()]),
        holder: HolderProbe {
            pid_exists: false,
            identity_matches: false,
        },
        any_resource_alive: false,
    };
    assert_eq!(
        smix_lease::prune_verdict(&held, Some(false)),
        PruneVerdict::Remove,
        "a claim that outlived its device is an escape hatch nobody checks \
         the far side of"
    );
    assert!(
        matches!(
            smix_lease::prune_verdict(&held, Some(true)),
            PruneVerdict::Keep(_)
        ),
        "while the device is on, this ledger is the only record that anybody \
         answered for it"
    );
}

#[test]
fn a_claim_never_blocks_the_next_caller() {
    assert!(
        smix_lease::is_service(&claimed()),
        "a claim is a statement about who answers, not an activity that \
         excludes somebody — treating it as one would make a claimed device \
         look busy to every later command"
    );
    assert!(
        !smix_lease::is_process_backed(&claimed()),
        "nothing about a claim can be probed for liveness"
    );
}

#[test]
fn claiming_twice_restates_when_rather_than_stacking_rows() {
    let (_t, d) = dir();
    store::record_claim(&d, "emulator-5554").expect("first claim");
    store::record_claim(&d, "emulator-5554").expect("second claim");
    let l = store::read(&d, "emulator-5554")
        .expect("read")
        .expect("ledger");
    let claims = l
        .known_resources()
        .filter(|r| matches!(r, Resource::Claimed { .. }))
        .count();
    assert_eq!(claims, 1, "got {:?}", l.resources);
}

#[test]
fn releasing_a_claim_leaves_a_real_boot_row_alone() {
    let (_t, d) = dir();
    store::record_boot(&d, "UDID-BOTH", true).expect("boot");
    store::record_claim(&d, "UDID-BOTH").expect("claim");
    store::drop_resource_kind(&d, "UDID-BOTH", &Resource::Claimed { at: String::new() })
        .expect("release");
    let l = store::read(&d, "UDID-BOTH")
        .expect("read")
        .expect("ledger survives");
    assert!(
        l.known_resources()
            .any(|r| matches!(r, Resource::Booted { by_us: true })),
        "releasing a claim took the answer to a different question with it: {:?}",
        l.resources
    );
    assert!(
        !l.known_resources()
            .any(|r| matches!(r, Resource::Claimed { .. })),
        "the claim survived its own release"
    );
}

#[test]
fn a_lone_claim_is_worth_a_ledger() {
    // `Booted { by_us: false }` is not: it records somebody else's device
    // and the store deletes a file holding only that. A claim is the
    // opposite kind of thing — a decision this machine made — so the file
    // has to survive the process that wrote it, or the next run is back
    // to having nowhere to put the answer.
    let (_t, d) = dir();
    store::record_claim(&d, "emulator-5554").expect("claim");
    store::drop_process_rows(&d, "emulator-5554").expect("teardown");
    assert!(
        store::read(&d, "emulator-5554")
            .expect("read")
            .is_some_and(|l| l
                .known_resources()
                .any(|r| matches!(r, Resource::Claimed { .. }))),
        "the claim did not outlive a teardown that closed no processes"
    );
}
