//! The ledger crosses process boundaries — these are the properties that
//! make that safe.

use smix_lease::store::{self, LeaseDir, LeaseError};
use smix_lease::{Admission, Lease, ProcIdentity, Resource, StaleReason};

fn lease(device_id: &str, holder: ProcIdentity) -> Lease {
    Lease {
        device_id: device_id.into(),
        holder,
        acquired_at: store::now_rfc3339(),
        heartbeat_at: store::now_rfc3339(),
        resources: vec![Resource::Booted { by_us: true }],
    }
}

#[test]
fn write_then_read_roundtrips() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    let l = lease("UDID-A", store::identify_self());
    store::write(&dir, &l).expect("write");
    let back = store::read(&dir, "UDID-A").expect("read").expect("some");
    assert_eq!(back, l);
}

#[test]
fn absent_ledger_is_none_not_error() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    assert!(store::read(&dir, "UDID-NONE").expect("read").is_none());
}

#[test]
fn release_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    let l = lease("UDID-B", store::identify_self());
    store::write(&dir, &l).expect("write");
    store::remove(&dir, "UDID-B").expect("first remove");
    store::remove(&dir, "UDID-B").expect("second remove");
    assert!(store::read(&dir, "UDID-B").expect("read").is_none());
}

#[test]
fn corrupt_ledger_is_loud_not_treated_as_absent() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    let path = store::lease_path(&dir, "UDID-C").expect("path");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, b"{\"deviceId\": \"UDID-C\", \"holder\"").expect("write junk");
    match store::read(&dir, "UDID-C") {
        Err(LeaseError::Malformed { .. }) => {}
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn device_id_that_is_not_a_filename_is_refused() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    for bad in ["../escape", "a/b", ""] {
        match store::lease_path(&dir, bad) {
            Err(LeaseError::BadDeviceId { .. }) => {}
            other => panic!("expected BadDeviceId for {bad:?}, got {other:?}"),
        }
    }
}

#[test]
fn this_process_identifies_and_probes_as_itself() {
    let me = store::identify_self();
    assert_eq!(me.pid, std::process::id());
    assert!(!me.started_at.is_empty(), "no start time for self");
    let probe = store::probe(&me);
    assert!(probe.pid_exists && probe.identity_matches);
}

#[test]
fn a_pid_that_is_not_running_probes_as_gone() {
    // pid 0 is never a userland process to signal on macOS.
    let ghost = ProcIdentity {
        pid: 0,
        started_at: "Thu Aug  6 10:00:00 2026".into(),
        cmd: "long gone".into(),
    };
    let probe = store::probe(&ghost);
    assert!(!probe.pid_exists);
}

#[test]
fn a_recycled_pid_probes_as_not_matching() {
    // Same pid as this process, a start time that is not ours: exactly
    // the shape of a reused pid.
    let impostor = ProcIdentity {
        pid: std::process::id(),
        started_at: "Thu Jan  1 00:00:00 1970".into(),
        cmd: "whatever".into(),
    };
    let probe = store::probe(&impostor);
    assert!(probe.pid_exists);
    assert!(!probe.identity_matches);
}

#[test]
fn facts_from_a_dead_holder_reclaim_with_cleanup() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    let dead = ProcIdentity {
        pid: 0,
        started_at: "Thu Aug  6 10:00:00 2026".into(),
        cmd: "smix run gone.yaml".into(),
    };
    let mut l = lease("UDID-D", dead);
    // A dead process-backed row is what makes this an abandoned session.
    // A ledger holding only a boot is a device left on, not an orphan —
    // see `boot_only_tests` in the crate.
    l.resources.push(Resource::Runner {
        port: 1,
        proc: ProcIdentity {
            pid: 0,
            started_at: "Thu Aug  6 10:00:05 2026".into(),
            cmd: "xcodebuild test".into(),
        },
    });
    store::write(&dir, &l).expect("write");
    let facts = store::collect_facts(&dir, "UDID-D").expect("facts");
    match smix_lease::assess(&facts) {
        Admission::Reclaimable { reason, cleanup } => {
            assert_eq!(reason, StaleReason::HolderExited);
            assert_eq!(cleanup.len(), 2, "the runner and the boot are both owed");
        }
        other => panic!("expected Reclaimable, got {other:?}"),
    }
}

#[test]
fn a_second_resource_of_the_same_kind_replaces_the_first() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    let first = Resource::Runner {
        port: 1,
        proc: store::identify_self(),
    };
    let second = Resource::Runner {
        port: 2,
        proc: store::identify_self(),
    };
    store::add_resource(&dir, "UDID-E", first).expect("first");
    store::add_resource(&dir, "UDID-E", second).expect("second");
    let lease = store::read(&dir, "UDID-E").expect("read").expect("some");
    assert_eq!(lease.resources.len(), 1, "one runner per device");
    assert!(matches!(
        lease.resources[0],
        Resource::Runner { port: 2, .. }
    ));
}

#[test]
fn dropping_the_last_meaningful_row_clears_the_ledger() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    store::add_resource(
        &dir,
        "UDID-F",
        Resource::Runner {
            port: 1,
            proc: store::identify_self(),
        },
    )
    .expect("runner");
    // Someone else's boot: recorded, but never a reason to keep a ledger.
    store::add_resource(&dir, "UDID-F", Resource::Booted { by_us: false }).expect("boot");
    store::drop_resource_kind(
        &dir,
        "UDID-F",
        &Resource::Runner {
            port: 0,
            proc: store::identify_self(),
        },
    )
    .expect("drop");
    assert!(
        store::read(&dir, "UDID-F").expect("read").is_none(),
        "nothing left that this process owes a teardown"
    );
}

#[test]
fn a_boot_we_performed_keeps_the_ledger_alive_on_its_own() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    store::add_resource(&dir, "UDID-G", Resource::Booted { by_us: true }).expect("boot");
    store::drop_resource_kind(
        &dir,
        "UDID-G",
        &Resource::Runner {
            port: 0,
            proc: store::identify_self(),
        },
    )
    .expect("drop");
    let lease = store::read(&dir, "UDID-G").expect("read");
    assert!(lease.is_some(), "we booted it, so we still owe a shutdown");
}
