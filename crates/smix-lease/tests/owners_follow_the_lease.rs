//! A device's resources carry the owner the command was given, and lose it
//! with the resource.
//!
//! Its own test binary: the owner comes from the process environment, and
//! the other lease tests run in threads beside each other.

use smix_lease::store::{self, LeaseDir};
use smix_lease::{ProcIdentity, Resource, owners};

fn runner() -> Resource {
    Resource::Runner {
        port: 22191,
        proc: ProcIdentity {
            pid: 4_294_967_291,
            started_at: "Tue Nov  3 15:00:00 2026".into(),
            cmd: "xcodebuild test".into(),
        },
        bundle: Some("com.example.shop".into()),
        log: None,
    }
}

fn names(d: &LeaseDir, device: &str) -> Vec<(String, String)> {
    owners::read(d, device)
        .expect("read")
        .rows
        .into_iter()
        .map(|r| (r.kind, r.owner))
        .collect()
}

#[test]
fn owners_follow_the_resources_they_were_given_with() {
    let t = tempfile::tempdir().expect("tempdir");
    let d = LeaseDir::at(t.path().join("leases"));
    let dev = "SIM-OWNED-A";

    // SAFETY: the only test in this binary that reads or writes it
    unsafe { std::env::set_var(owners::OWNER_ENV, "task-a") };
    store::record_boot(&d, dev, true).expect("boot");
    store::add_resource(&d, dev, runner()).expect("runner");
    assert_eq!(
        names(&d, dev),
        vec![
            ("booted".into(), "task-a".into()),
            ("runner".into(), "task-a".into())
        ]
    );

    // another task finds the device up: the runner is its own now, the
    // device stays the first task's
    unsafe { std::env::set_var(owners::OWNER_ENV, "task-b") };
    store::record_boot(&d, dev, false).expect("found up");
    store::add_resource(&d, dev, runner()).expect("its runner");
    assert_eq!(
        names(&d, dev),
        vec![
            ("booted".into(), "task-a".into()),
            ("runner".into(), "task-b".into())
        ]
    );

    // the runner goes, its owner with it; the device keeps its own
    store::drop_process_rows(&d, dev).expect("drop processes");
    assert_eq!(names(&d, dev), vec![("booted".into(), "task-a".into())]);

    // the ledger goes, and so does the owner file
    store::remove(&d, dev).expect("remove");
    assert!(names(&d, dev).is_empty());
    assert!(!t.path().join("owners").join(format!("{dev}.json")).exists());

    // a device somebody else turned on is nobody's here
    store::record_boot(&d, "SIM-NOT-OURS", false).expect("not ours");
    assert!(names(&d, "SIM-NOT-OURS").is_empty());

    // no owner given: nothing written, not a guess
    unsafe { std::env::remove_var(owners::OWNER_ENV) };
    store::record_boot(&d, "SIM-UNNAMED", true).expect("unnamed");
    assert!(names(&d, "SIM-UNNAMED").is_empty());

    // a name that is not one is not written either
    unsafe { std::env::set_var(owners::OWNER_ENV, "two words") };
    store::record_boot(&d, "SIM-BAD-NAME", true).expect("bad name");
    assert!(names(&d, "SIM-BAD-NAME").is_empty());
}

#[test]
fn an_owner_is_a_short_plain_name() {
    assert!(owners::validate("insight-qa:sub-2").is_ok());
    for bad in ["", "two words", "a/b", &"x".repeat(65)] {
        assert!(owners::validate(bad).is_err(), "{bad:?}");
    }
}
