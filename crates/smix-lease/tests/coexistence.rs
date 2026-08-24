//! Two books about one device, and what may be done while they differ.
//!
//! The ledgers moved to the machine on 2026-08-11. The copy of smix
//! already installed here did not: `~/.local/bin/smix` is 3.0.0 and
//! still writes into whichever `.smix/` is above the working directory.
//! Ninety-one minutes after the migration,
//! `qualcomm/insight/.smix/leases/FFC57DAE-….json` had been rewritten —
//! recording a runner at pid 28529, alive and serving on port 22087,
//! while the machine ledger for the same device still named a pid that
//! had exited and read `abandoned — 2 close(s) owed`. In that window a
//! `lease reconcile` from any tree would have shut the simulator down
//! and killed the runner. It closed when that session ended.
//!
//! The shapes below are that window, built rather than observed. A test
//! that waited for some session to still be alive would be green
//! tomorrow for a reason nobody could name.
//!
//! The rule these check: the machine ledger is the only input to a
//! decision, and a checkout's book has exactly one power — to stop one.

use smix_lease::store::{CheckoutLedgers, LeaseDir, LedgerDivergence, compare, survey};
use smix_lease::{Lease, ProcIdentity, Resource, Row};

fn proc(pid: u32, started: &str) -> ProcIdentity {
    ProcIdentity {
        pid,
        started_at: started.into(),
        cmd: "xcodebuild test".into(),
    }
}

fn lease(device: &str, runner_pid: u32) -> Lease {
    Lease {
        device_id: device.into(),
        holder: proc(50057, "Sun Aug  9 07:18:59 2026"),
        acquired_at: "2026-08-09T07:18:59Z".into(),
        heartbeat_at: "2026-08-11T08:00:55Z".into(),
        resources: vec![
            Row::Known(Resource::Booted { by_us: true }),
            Row::Known(Resource::Runner {
                port: 22087,
                proc: proc(runner_pid, "Tue Aug 11 19:54:21 2026"),
            }),
        ],
    }
}

/// The window, in one assertion: two books, two runners, one device.
#[test]
fn two_books_naming_different_runners_disagree() {
    let machine = lease("FFC57DAE", 49224);
    let checkout = lease("FFC57DAE", 28529);
    let got = compare("FFC57DAE", Some(&machine), Some(&checkout));
    match got {
        Some(LedgerDivergence::Disagrees { device_id, detail }) => {
            assert_eq!(device_id, "FFC57DAE");
            assert!(
                detail.contains("49224") && detail.contains("28529"),
                "the detail has to name both pids — somebody reading this has to \
                 be able to go look at each. Got: {detail}"
            );
        }
        other => panic!(
            "two books recording different runners on one device read as {other:?} \
             — this is the shape that would have killed pid 28575"
        ),
    }
}

/// A source left in place after a successful migration is not a complaint.
///
/// `stables/mailrs` is exactly this: its ledger was folded in and the
/// file stayed where it was, byte for byte. Reporting that as a
/// divergence would make the report useless on the machine it was
/// written for.
#[test]
fn two_books_that_agree_are_not_a_divergence() {
    let same = lease("172AC305", 24428);
    assert_eq!(compare("172AC305", Some(&same), Some(&same.clone())), None);
}

/// A device only the tree knows about is the completeness answer.
#[test]
fn a_device_only_the_checkout_has_is_reported_as_unmigrated() {
    let checkout = lease("ONLY-HERE", 111);
    match compare("ONLY-HERE", None, Some(&checkout)) {
        Some(LedgerDivergence::OnlyInCheckout { device_id }) => {
            assert_eq!(device_id, "ONLY-HERE");
        }
        other => panic!("expected OnlyInCheckout, got {other:?}"),
    }
}

/// A device only the machine knows about is the ordinary state.
///
/// Every device registered since the move is this. Calling it a
/// divergence would put a line on every report for ever.
#[test]
fn a_device_only_the_machine_has_is_not_a_divergence() {
    let machine = lease("MACHINE-ONLY", 222);
    assert_eq!(compare("MACHINE-ONLY", Some(&machine), None), None);
}

/// The survey walks the union of both books, not one of them.
#[test]
fn the_survey_covers_devices_from_either_side() {
    let dir = std::env::temp_dir().join(format!(
        "smix-coex-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let m = LeaseDir::at(dir.join("machine"));
    let c_path = dir.join("tree").join(".smix").join("leases");
    std::fs::create_dir_all(&c_path).unwrap();

    smix_lease::store::write(&m, &lease("BOTH", 1)).unwrap();
    smix_lease::store::write(&LeaseDir::at(&c_path), &lease("BOTH", 2)).unwrap();
    smix_lease::store::write(&LeaseDir::at(&c_path), &lease("TREE-ONLY", 3)).unwrap();
    smix_lease::store::write(&m, &lease("MACHINE-ONLY", 4)).unwrap();

    let found = survey(&m, &CheckoutLedgers::at(&c_path));
    let mut ids: Vec<&str> = found
        .iter()
        .map(|d| match d {
            LedgerDivergence::Disagrees { device_id, .. } => device_id.as_str(),
            LedgerDivergence::OnlyInCheckout { device_id } => device_id.as_str(),
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        ["BOTH", "TREE-ONLY"],
        "the survey must report the disagreement and the tree-only device, and \
         must not report the device only the machine has"
    );
    std::fs::remove_dir_all(&dir).ok();
}
