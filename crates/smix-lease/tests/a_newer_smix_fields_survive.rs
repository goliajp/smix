//! A newer smix's fields survive this one writing the ledger back.
//!
//! 10.1.0 read an 11.0 runner row, knew the row's kind, and wrote it back
//! without `log` and `bundle` — fields it had never heard of. Unknown kinds
//! were already kept whole; these are unknown fields inside known parts.

use serde_json::Value;
use smix_lease::store::{self, LeaseDir};
use smix_lease::{ProcIdentity, Resource};

const DEVICE: &str = "UDID-FUTURE";

// A ledger as a later smix might write it: one unknown field on the lease,
// on the holder, inside the runner row and inside that row's process.
const FUTURE: &str = r#"{
  "deviceId": "UDID-FUTURE",
  "holder": {"pid": 4242, "startedAt": "Sun Sep 27 09:00:00 2026", "cmd": "smix run", "sessionToken": "t-1"},
  "acquiredAt": "2026-09-27T00:00:00+00:00",
  "heartbeatAt": "2026-09-27T00:00:00+00:00",
  "claimEpoch": 7,
  "resources": [
    {"kind": "booted", "byUs": true},
    {"kind": "runner", "port": 40111,
     "proc": {"pid": 5151, "startedAt": "Sun Sep 27 09:00:01 2026", "cmd": "xcodebuild test", "cpuType": "arm64"},
     "bundle": "com.example", "log": "/tmp/runner.log",
     "transport": {"kind": "unix", "path": "/tmp/r.sock"}}
  ]
}"#;

fn seeded() -> (tempfile::TempDir, LeaseDir) {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    std::fs::write(tmp.path().join(format!("{DEVICE}.json")), FUTURE).expect("seed");
    (tmp, dir)
}

fn on_disk(dir: &LeaseDir) -> Value {
    let path = store::lease_path(dir, DEVICE).expect("path");
    serde_json::from_slice(&std::fs::read(path).expect("read")).expect("json")
}

fn future() -> Value {
    serde_json::from_str(FUTURE).expect("fixture")
}

fn runner_row(v: &Value) -> Option<&Value> {
    v["resources"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["kind"] == "runner")
}

#[test]
fn a_heartbeat_keeps_every_unknown_field() {
    let (_tmp, dir) = seeded();
    let mut lease = store::read(&dir, DEVICE).expect("read").expect("lease");
    lease.heartbeat_at = "2026-09-27T00:01:00+00:00".into();
    store::write(&dir, &lease).expect("write");

    let after = on_disk(&dir);
    let before = future();
    assert_eq!(after["heartbeatAt"], "2026-09-27T00:01:00+00:00");
    assert_eq!(after["claimEpoch"], before["claimEpoch"], "{after:#}");
    assert_eq!(after["holder"], before["holder"], "{after:#}");
    assert_eq!(runner_row(&after), runner_row(&before), "{after:#}");
}

#[test]
fn dropping_another_row_keeps_the_runner_row_as_it_was() {
    let (_tmp, dir) = seeded();
    store::drop_resource_kind(&dir, DEVICE, &Resource::Booted { by_us: false }).expect("drop");

    let after = on_disk(&dir);
    assert_eq!(
        after["resources"].as_array().expect("rows").len(),
        1,
        "{after:#}"
    );
    assert_eq!(runner_row(&after), runner_row(&future()), "{after:#}");
    assert_eq!(after["claimEpoch"], 7, "{after:#}");
}

#[test]
fn a_replaced_row_is_written_as_this_binary_knows_it() {
    let (_tmp, dir) = seeded();
    store::add_resource(
        &dir,
        DEVICE,
        Resource::Runner {
            port: 40222,
            proc: ProcIdentity {
                pid: 6161,
                started_at: "Sun Sep 27 09:05:00 2026".into(),
                cmd: "xcodebuild test".into(),
            },
            bundle: None,
            log: None,
        },
    )
    .expect("add");

    let after = on_disk(&dir);
    let row = runner_row(&after).expect("runner row");
    assert_eq!(row["port"], 40222);
    assert!(row.get("transport").is_none(), "{after:#}");
    // the lease and its holder are unchanged, so theirs stay
    assert_eq!(after["claimEpoch"], 7, "{after:#}");
    assert_eq!(after["holder"]["sessionToken"], "t-1", "{after:#}");
}

#[test]
fn a_new_lease_does_not_inherit_the_old_ones_fields() {
    let (_tmp, dir) = seeded();
    let mut lease = store::read(&dir, DEVICE).expect("read").expect("lease");
    lease.acquired_at = "2026-09-27T01:00:00+00:00".into();
    lease.holder = ProcIdentity {
        pid: 7070,
        started_at: "Sun Sep 27 10:00:00 2026".into(),
        cmd: "smix run".into(),
    };
    store::write(&dir, &lease).expect("write");

    let after = on_disk(&dir);
    assert!(after.get("claimEpoch").is_none(), "{after:#}");
    assert!(after["holder"].get("sessionToken").is_none(), "{after:#}");
}

#[test]
fn a_ledger_this_binary_fully_reads_is_written_in_its_own_order() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dir = LeaseDir::at(tmp.path());
    store::add_resource(&dir, "UDID-PLAIN", Resource::Booted { by_us: true }).expect("add");
    store::add_resource(&dir, "UDID-PLAIN", Resource::Booted { by_us: true }).expect("again");
    let text = std::fs::read_to_string(tmp.path().join("UDID-PLAIN.json")).expect("read");
    assert!(
        text.find("\"deviceId\"") < text.find("\"acquiredAt\""),
        "struct order, not alphabetical:\n{text}"
    );
}
