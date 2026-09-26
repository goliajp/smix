//! A device smix boots after it left is a new lease, not the old one.
//!
//! An emulator smix booted exits; the next command notices and keeps its
//! departure. The ledger stays until `lease prune`, describing a device
//! that is gone. `smix sim boot` then starts the same AVD on the same
//! serial — and every row it wrote went into that old ledger: the holder
//! of a process long dead, the old `acquiredAt`. A departure is one
//! (device, lease) pair, so when this second life ended too it matched
//! the first one's record and was never kept.
//!
//! Only a boot this call performed starts a new life, and only a ledger
//! nobody is holding is replaced: a ledger this process wrote a moment
//! ago, or one a live process holds, is written into as before.

use smix_lease::store::{self, LeaseDir};
use smix_lease::vanish;
use smix_lease::{Lease, ProcIdentity, Resource, Row};

fn dir() -> (tempfile::TempDir, LeaseDir) {
    let t = tempfile::tempdir().expect("tempdir");
    let d = LeaseDir::at(t.path().to_path_buf());
    (t, d)
}

/// A process that is not running: no pid is this large.
fn gone() -> ProcIdentity {
    ProcIdentity {
        pid: 4_294_967_291,
        started_at: "Fri Sep 25 22:09:03 2026".into(),
        cmd: "smix sim boot sim-smix-android-02".into(),
    }
}

fn earlier_life(holder: ProcIdentity) -> Lease {
    Lease {
        device_id: "emulator-5558".into(),
        holder,
        acquired_at: "2026-09-25T13:09:04Z".into(),
        heartbeat_at: "2026-09-25T13:09:04Z".into(),
        resources: vec![
            Row::Known(Resource::Booted { by_us: true }),
            Row::Known(Resource::Emulator {
                avd: "sim-smix-android-02".into(),
                console_log: Some("/tmp/console/first.log".into()),
            }),
        ],
    }
}

fn keep_departure(d: &LeaseDir) -> bool {
    let facts = store::collect_facts(d, "emulator-5558").expect("facts");
    let held = facts.existing.expect("a ledger to depart from");
    let v = vanish::vanished_from(
        &held,
        None,
        &store::now_rfc3339(),
        "smix lease list",
        vec![],
    );
    vanish::record(d, &v).expect("record")
}

#[test]
fn a_boot_over_a_ledger_nobody_holds_starts_a_new_one() {
    let (_t, d) = dir();
    store::write(&d, &earlier_life(gone())).expect("the first life's ledger");
    assert!(keep_departure(&d), "the first departure is kept");

    store::record_boot(&d, "emulator-5558", true).expect("boot");

    let now = store::read(&d, "emulator-5558")
        .expect("read")
        .expect("a ledger");
    assert_eq!(
        now.holder.pid,
        std::process::id(),
        "the booting process holds it"
    );
    assert_ne!(now.acquired_at, "2026-09-25T13:09:04Z", "the lease is new");
    assert!(
        now.known_resources()
            .all(|r| !matches!(r, Resource::Emulator { .. })),
        "the first life's rows are not carried over: {:?}",
        now.resources
    );
    assert!(
        keep_departure(&d),
        "the second life leaving is a second departure"
    );
    assert_eq!(vanish::history(&d).expect("history").len(), 2);
}

#[test]
fn a_boot_after_this_process_claimed_the_device_keeps_the_claim() {
    let (_t, d) = dir();
    store::record_claim(&d, "emulator-5558").expect("claim");
    store::record_boot(&d, "emulator-5558", true).expect("boot");
    let now = store::read(&d, "emulator-5558")
        .expect("read")
        .expect("a ledger");
    assert!(
        now.known_resources()
            .any(|r| matches!(r, Resource::Claimed { .. })),
        "this process's own claim survives its boot: {:?}",
        now.resources
    );
}

#[test]
fn a_boot_over_a_ledger_a_live_process_holds_is_written_into() {
    let (_t, d) = dir();
    let parent = store::identify(std::os::unix::process::parent_id())
        .expect("the test's parent process can be identified");
    store::write(&d, &earlier_life(parent.clone())).expect("their ledger");
    store::record_boot(&d, "emulator-5558", true).expect("boot");
    let now = store::read(&d, "emulator-5558")
        .expect("read")
        .expect("a ledger");
    assert_eq!(now.holder, parent, "a live holder keeps its ledger");
    assert_eq!(now.acquired_at, "2026-09-25T13:09:04Z");
}
