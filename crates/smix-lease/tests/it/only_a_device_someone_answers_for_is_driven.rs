//! A device is drivable when smix booted it or a person claimed it, and
//! not otherwise. The device that started this was an emulator a script
//! spawned on its own: running, no ledger, and before this rule smix
//! drove it like any other.

use smix_lease::store::{self, LeaseDir};
use smix_lease::{Lease, ProcIdentity, Resource, Row, may_drive};

fn lease_with(resources: Vec<Resource>) -> Lease {
    Lease {
        device_id: "emulator-5560".into(),
        holder: ProcIdentity {
            pid: 4_294_967_292,
            started_at: "Fri Nov 13 00:00:00 2026".into(),
            cmd: "smix sim boot".into(),
        },
        acquired_at: "2026-11-13T00:00:00Z".into(),
        heartbeat_at: "2026-11-13T00:00:00Z".into(),
        resources: resources.into_iter().map(Row::Known).collect(),
    }
}

#[test]
fn no_ledger_is_not_drivable() {
    assert!(!may_drive(None));
}

#[test]
fn a_device_smix_found_already_up_is_not_drivable() {
    let l = lease_with(vec![Resource::Booted { by_us: false }]);
    assert!(!may_drive(Some(&l)), "found up is not started by smix");
}

#[test]
fn a_device_smix_booted_is_drivable() {
    let l = lease_with(vec![Resource::Booted { by_us: true }]);
    assert!(may_drive(Some(&l)));
}

#[test]
fn a_claimed_device_is_drivable() {
    let l = lease_with(vec![
        Resource::Booted { by_us: false },
        Resource::Claimed {
            at: "2026-11-13T00:00:00Z".into(),
            reason: Some("its owner asked".into()),
        },
    ]);
    assert!(may_drive(Some(&l)));
}

#[test]
fn a_claim_keeps_its_reason_on_disk() {
    let t = tempfile::tempdir().expect("tempdir");
    let d = LeaseDir::at(t.path().to_path_buf());
    store::record_claim(&d, "emulator-5560", "started by a script before the fence")
        .expect("claim");
    let back = store::read(&d, "emulator-5560")
        .expect("read")
        .expect("a ledger");
    let reason = back.known_resources().find_map(|r| match r {
        Resource::Claimed { reason, .. } => reason.clone(),
        _ => None,
    });
    assert_eq!(
        reason.as_deref(),
        Some("started by a script before the fence")
    );
    assert!(may_drive(Some(&back)));
}

#[test]
fn a_claim_written_before_reasons_existed_still_reads() {
    let json = r#"{"kind":"claimed","at":"2026-08-24T00:00:00Z"}"#;
    let r: Resource = serde_json::from_str(json).expect("an old claim row parses");
    assert!(matches!(r, Resource::Claimed { reason: None, .. }));
}
