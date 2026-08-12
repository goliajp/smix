//! Who turned this device on outlives whoever was using it.
//!
//! `smix lease owner` exits 3, and on 2026-08-12 that one code carried
//! two meanings: "nobody here booted it" and "somebody here booted it and
//! the row is gone". `pick-dev-sim` reads the code, so the second meaning
//! made a perfectly good simulator look busy and the 4.1.0 ship failed on
//! it.
//!
//! A lease covers what a holder started; whether smix turned the device
//! on is a fact with a longer life than any session, and `store.rs:578`
//! already says so in prose. These are the places that can end that fact
//! early. Each test pins the same property — a ledger saying
//! `Booted { by_us: true }` still says it after a teardown, unless the
//! device was actually turned off — against one named site.
//!
//! Two of the six candidates cannot be reached from here: they are
//! command bodies that shell out. They are judged by reading, and by the
//! live sequence in the checkpoint, and the decomposition record says
//! which is which rather than letting a passing file imply coverage of
//! all six.

use smix_lease::store::{self, LeaseDir};
use smix_lease::{Lease, ProcIdentity, Resource};

fn dead_holder() -> ProcIdentity {
    // A pid that exists nowhere, with a start time nothing can match. The
    // ledger is then in the state every one of these sites reacts to:
    // whoever opened this is gone.
    ProcIdentity {
        pid: 4_294_967_294,
        started_at: "Tue Aug 12 00:00:00 2026".into(),
        cmd: "xcodebuild test … id=UDID".into(),
    }
}

fn ledger(dir: &LeaseDir, device: &str, resources: Vec<Resource>) {
    store::write(
        dir,
        &Lease {
            device_id: device.into(),
            holder: dead_holder(),
            acquired_at: store::now_rfc3339(),
            heartbeat_at: store::now_rfc3339(),
            resources,
        },
    )
    .expect("write ledger");
}

fn runner_row() -> Resource {
    Resource::Runner {
        port: 22087,
        proc: dead_holder(),
    }
}

fn booted_by_us(dir: &LeaseDir, device: &str) -> bool {
    store::read(dir, device).expect("read").is_some_and(|l| {
        l.resources
            .iter()
            .any(|r| matches!(r, Resource::Booted { by_us: true }))
    })
}

/// Candidate (c) — `smix-capsule/src/runner.rs:2096` via
/// `smix-lease/src/store.rs:585`.
///
/// `runner down` drops the runner, supervisor and forwarder rows and
/// never shuts the device down. If dropping the last process row took the
/// ledger with it, the device would be left running with nothing on the
/// machine able to say who turned it on.
#[test]
fn dropping_the_last_process_row_keeps_the_boot_row() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    ledger(
        &dir,
        "UDID-C",
        vec![Resource::Booted { by_us: true }, runner_row()],
    );

    store::drop_process_rows(&dir, "UDID-C").expect("drop");

    assert!(
        booted_by_us(&dir, "UDID-C"),
        "`runner down` left a device running and took the record of who \
         booted it with the runner row — that is `lease owner` exit 3 \
         meaning 'we lost the note' rather than 'nobody booted it'"
    );
}

/// The same site, reached the way `forget_runner_lease` reaches it: one
/// `drop_resource_kind` per row, so the boot row is alone at the end.
#[test]
fn dropping_rows_one_kind_at_a_time_keeps_the_boot_row() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    ledger(
        &dir,
        "UDID-K",
        vec![Resource::Booted { by_us: true }, runner_row()],
    );

    store::drop_resource_kind(&dir, "UDID-K", &runner_row()).expect("drop runner");
    store::drop_resource_kind(
        &dir,
        "UDID-K",
        &Resource::Supervisor {
            proc: store::identify_self(),
        },
    )
    .expect("drop supervisor");

    assert!(
        booted_by_us(&dir, "UDID-K"),
        "the boot row did not survive `forget_runner_lease`'s row-by-row \
         teardown, which is the exact sequence `runner down` runs"
    );
}

/// A device smix did NOT turn on is the other half of the same rule. The
/// ledger has nothing to keep once the processes are gone, and keeping it
/// would make the next `down` look like it had work to do.
#[test]
fn a_ledger_for_a_device_we_did_not_boot_is_not_kept() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    ledger(
        &dir,
        "UDID-N",
        vec![Resource::Booted { by_us: false }, runner_row()],
    );

    store::drop_process_rows(&dir, "UDID-N").expect("drop");

    assert!(
        store::read(&dir, "UDID-N").expect("read").is_none(),
        "a ledger recording only that the device was already up outlived \
         its processes — `by_us: false` is not a fact worth a file"
    );
}

/// The whole sequence, end to end, because each site above passing on its
/// own is what the ledger already did before any of this: the device came
/// out of it running and unclaimed anyway.
///
/// `lease owner` exits 3 for two different reasons and `pick-dev-sim`
/// cannot tell them apart. Only one of them may survive — "nobody here
/// booted it". This walks the other one's route and demands the row is
/// still there at the end of it.
#[test]
fn a_device_left_running_still_says_who_booted_it() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());

    // `smix runner up`: turn it on, write that down, then the runner.
    ledger(&dir, "UDID-SEQ", vec![Resource::Booted { by_us: true }]);
    store::add_resource(&dir, "UDID-SEQ", runner_row()).expect("runner row");

    // `smix runner down`: stops the runner, never touches the device.
    store::drop_process_rows(&dir, "UDID-SEQ").expect("down");
    assert!(
        booted_by_us(&dir, "UDID-SEQ"),
        "teardown took the boot row with the runner row"
    );

    // `smix lease prune`, holder long gone, device still on.
    let facts = store::collect_facts(&dir, "UDID-SEQ").expect("facts");
    let held = facts.existing.expect("ledger is there");
    assert!(
        matches!(
            smix_lease::prune_verdict(&held, Some(true)),
            smix_lease::PruneVerdict::Keep(_)
        ),
        "prune would clear it"
    );

    // Which is the condition `lease owner` answers 0 on.
    assert!(
        booted_by_us(&dir, "UDID-SEQ"),
        "a device left running came out of a full session with nothing on \
         this machine able to say who turned it on — that is the exit 3 \
         that failed the 4.1.0 ship"
    );
}

/// Candidate (e) — `smix-cli/src/lease_cmd.rs:402` reading
/// `smix-lease/src/store.rs:684`.
///
/// `prune` removes a ledger when the holder is gone and
/// `any_resource_alive` is false. `Booted` is hard-coded false there, so
/// a ledger whose only row says smix booted a device reads as describing
/// nothing at all — and gets deleted while the device is still on.
///
/// `smix lease prune --help` says it removes "a boot row for a device
/// that is off". Nothing in this path asks the device. That is the same
/// defect the retired-claims gate was built for, one layer down: a rule
/// whose own description is more careful than its code.
///
/// `any_resource_alive` itself is NOT the thing to change, and the first
/// draft of this test said it was. That field also decides admission
/// (`lib.rs:395`), and making a boot row count as a live process would
/// move what "in use" means for every command. The field is right about
/// what it names — a boot is not a process and cannot be probed. It was
/// simply never the last word on whether a ledger is empty, and `prune`
/// treated it as though it were.
#[test]
fn a_boot_row_is_not_nothing() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    ledger(&dir, "UDID-E", vec![Resource::Booted { by_us: true }]);

    let facts = store::collect_facts(&dir, "UDID-E").expect("facts");
    let held = facts.existing.expect("ledger is there");
    assert!(
        !(held.holder.pid_exists && held.holder.identity_matches),
        "the fixture holder is supposed to be gone — this test is not \
         asking what it thinks it is asking"
    );

    assert!(
        matches!(
            smix_lease::prune_verdict(&held, Some(true)),
            smix_lease::PruneVerdict::Keep(_)
        ),
        "a ledger saying smix booted a device that is STILL ON was judged \
         removable — the device stays up and the only record of who turned \
         it on goes away, which is `lease owner` exit 3 meaning 'we lost \
         the note'"
    );
}

/// The device is not in this machine's simulator list at all — an Android
/// serial, a phone. "Not listed" must never read as "off".
#[test]
fn a_device_this_machine_cannot_ask_about_is_kept() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    ledger(&dir, "UDID-U", vec![Resource::Booted { by_us: true }]);

    let facts = store::collect_facts(&dir, "UDID-U").expect("facts");
    let held = facts.existing.expect("ledger is there");

    assert!(
        matches!(
            smix_lease::prune_verdict(&held, None),
            smix_lease::PruneVerdict::Keep(_)
        ),
        "a device whose power state this machine cannot see was pruned — \
         not being in simctl's list is not evidence that a device is off"
    );
}

/// And the converse, twice, so none of the above can be satisfied by
/// making every ledger un-prunable. `prune --help` promises it removes
/// "a boot row for a device that is off"; that promise has to keep
/// working, and it is what nothing in this path used to check.
#[test]
fn a_ledger_that_claims_nothing_is_still_prunable() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    ledger(&dir, "UDID-P", vec![Resource::Booted { by_us: false }]);

    let facts = store::collect_facts(&dir, "UDID-P").expect("facts");
    let held = facts.existing.expect("ledger is there");

    assert!(
        !held.any_resource_alive,
        "a ledger recording only that the device was already up now reads \
         as live, which would make `prune` keep every ledger forever"
    );
    assert_eq!(
        smix_lease::prune_verdict(&held, Some(false)),
        smix_lease::PruneVerdict::Remove,
        "a ledger that records nothing but 'the device was already up' \
         survived pruning"
    );
}

/// The boot row of a device that has since been switched off is the case
/// `--help` names. It goes.
#[test]
fn a_boot_row_for_a_device_that_is_off_is_prunable() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    ledger(&dir, "UDID-O", vec![Resource::Booted { by_us: true }]);

    let facts = store::collect_facts(&dir, "UDID-O").expect("facts");
    let held = facts.existing.expect("ledger is there");

    assert_eq!(
        smix_lease::prune_verdict(&held, Some(false)),
        smix_lease::PruneVerdict::Remove,
        "the device is off and its boot row still kept the ledger alive — \
         that is the stale file `prune` exists to clear, and `--help` says \
         so in as many words"
    );
}
