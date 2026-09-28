//! Clearing a dead runner row must not take the record of who turned the
//! device on.
//!
//! A consumer's ledger held four Android runner rows whose processes had
//! been dead for days, and nothing cleared them. `runner down` refuses a
//! runner this workspace never recorded, so the ledger is what decides
//! whether a device may be shut down — and one filling with dead rows
//! makes that decision rest on a list nobody maintains.
//!
//! `smix runner list --prune` clears them, and the boundary is the part
//! worth pinning: a boot row and a claim answer a different question —
//! who may switch this device off — and a tidy-up that took them with it
//! would hand the next teardown a device it has no record of starting.
//! That boundary is `drop_resource_kind`'s, not the command's, which is
//! why it is asserted here rather than through the CLI.

use smix_lease::store::{self, LeaseDir};
use smix_lease::{Lease, ProcIdentity, Resource, Row};

fn proc() -> ProcIdentity {
    ProcIdentity {
        pid: 4_294_967_291,
        started_at: "Mon Aug 25 00:00:00 2026".into(),
        cmd: "smix runner up emulator-5560 --platform android".into(),
    }
}

fn ledger(d: &LeaseDir, device: &str, resources: Vec<Resource>) {
    store::write(
        d,
        &Lease {
            device_id: device.into(),
            holder: proc(),
            acquired_at: "2026-08-25T00:00:00Z".into(),
            heartbeat_at: "2026-08-25T00:00:00Z".into(),
            resources: resources.into_iter().map(Row::Known).collect(),
        },
    )
    .expect("write");
}

fn runner_row(serial: &str) -> Resource {
    Resource::AndroidRunner {
        port: 28080,
        serial: serial.into(),
        proc: proc(),
    }
}

fn drop_runner(d: &LeaseDir, device: &str) {
    store::drop_resource_kind(d, device, &runner_row(device)).expect("drop");
}

fn left(d: &LeaseDir, device: &str) -> Vec<&'static str> {
    store::read(d, device)
        .expect("read")
        .map(|l| {
            l.known_resources()
                .map(|r| match r {
                    Resource::AndroidRunner { .. } => "androidRunner",
                    Resource::Booted { .. } => "booted",
                    Resource::Claimed { .. } => "claimed",
                    _ => "other",
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_runner_row_goes_and_the_claim_stays() {
    let t = tempfile::tempdir().unwrap();
    let d = LeaseDir::at(t.path().to_path_buf());
    ledger(
        &d,
        "emulator-5560",
        vec![
            runner_row("emulator-5560"),
            Resource::Claimed {
                at: "2026-08-25T00:00:00Z".into(),
            },
        ],
    );

    drop_runner(&d, "emulator-5560");

    let rest = left(&d, "emulator-5560");
    assert!(
        !rest.contains(&"androidRunner"),
        "the dead row survived: {rest:?}"
    );
    assert!(
        rest.contains(&"claimed"),
        "the claim went with it — it answers who may switch this device off, \
         which clearing a runner row has not looked at: {rest:?}"
    );
}

#[test]
fn the_boot_row_stays_too() {
    let t = tempfile::tempdir().unwrap();
    let d = LeaseDir::at(t.path().to_path_buf());
    ledger(
        &d,
        "emulator-5562",
        vec![
            runner_row("emulator-5562"),
            Resource::Booted { by_us: true },
        ],
    );

    drop_runner(&d, "emulator-5562");

    let rest = left(&d, "emulator-5562");
    assert!(
        rest.contains(&"booted"),
        "the record of who turned this device on went with the runner row: {rest:?}"
    );
}

#[test]
fn a_ledger_holding_only_the_dead_row_goes_entirely() {
    // Nothing worth a file is left, and an empty ledger is one more row
    // in the list this exists to keep short.
    let t = tempfile::tempdir().unwrap();
    let d = LeaseDir::at(t.path().to_path_buf());
    ledger(&d, "emulator-5564", vec![runner_row("emulator-5564")]);

    drop_runner(&d, "emulator-5564");

    assert!(
        store::read(&d, "emulator-5564").expect("read").is_none(),
        "an empty ledger was kept"
    );
}
