//! A device the ledger says is here, and the machine says is not.
//!
//! The user saw an Android emulator exit abnormally three or four times in
//! a week and could not say whose it was or when: no crash report, an
//! empty crash database, and a ledger that went on describing a device
//! that was gone. Nothing spoke, so nothing could be asked afterwards.
//!
//! These pin the two halves: judging presence without guessing, and
//! keeping the fact once judged.

use smix_lease::store::{self, LeaseDir};
use smix_lease::vanish::{self, AdbDevice, LiveDevices, Presence};
use smix_lease::{Held, HolderProbe, Lease, ProcIdentity, Resource, Row};

fn holder() -> ProcIdentity {
    ProcIdentity {
        pid: 4_294_967_291,
        started_at: "Thu Sep 24 09:00:00 2026".into(),
        cmd: "smix run flows/login.yaml".into(),
    }
}

fn lease(device_id: &str, resources: Vec<Resource>) -> Lease {
    Lease {
        device_id: device_id.into(),
        holder: holder(),
        acquired_at: "2026-09-24T00:00:00Z".into(),
        heartbeat_at: "2026-09-24T00:05:00Z".into(),
        resources: resources.into_iter().map(Row::Known).collect(),
    }
}

fn emulator(avd: &str) -> Resource {
    Resource::Emulator {
        avd: avd.into(),
        console_log: Some(format!("/tmp/console/{avd}.log")),
    }
}

fn live(emulators: &[(&str, &str)], simulators: Option<&[(&str, bool)]>) -> LiveDevices {
    LiveDevices {
        adb: Some(
            emulators
                .iter()
                .map(|(s, a)| AdbDevice {
                    serial: (*s).to_string(),
                    avd: Some((*a).to_string()),
                })
                .collect(),
        ),
        simulators: simulators.map(|ss| {
            ss.iter()
                .map(|(u, booted)| ((*u).to_string(), *booted))
                .collect()
        }),
    }
}

#[test]
fn an_emulator_adb_no_longer_lists_is_gone() {
    let l = lease("emulator-5560", vec![emulator("sim-smix-android-01")]);
    assert_eq!(
        vanish::presence(&l, &live(&[], None)),
        Presence::Gone { slot_now: None }
    );
}

#[test]
fn a_slot_now_answering_for_another_avd_is_gone_and_names_it() {
    // The serial is a port. The same port answering for a different AVD
    // is not the device this ledger was written about.
    let l = lease("emulator-5554", vec![emulator("sim-smix-android-01")]);
    assert_eq!(
        vanish::presence(&l, &live(&[("emulator-5554", "qip-consumer-36")], None)),
        Presence::Gone {
            slot_now: Some("qip-consumer-36".into())
        }
    );
}

#[test]
fn the_same_avd_on_the_same_slot_is_present() {
    let l = lease("emulator-5560", vec![emulator("sim-smix-android-01")]);
    assert_eq!(
        vanish::presence(&l, &live(&[("emulator-5560", "sim-smix-android-01")], None)),
        Presence::Present
    );
}

#[test]
fn an_emulator_when_adb_could_not_be_asked_cannot_be_judged() {
    // Not "gone". A machine where adb is missing, or its server would not
    // start, would otherwise record every emulator it ever had as having
    // left — a history of things that did not happen.
    let l = lease("emulator-5560", vec![emulator("sim-smix-android-01")]);
    let nobody_asked = LiveDevices {
        adb: None,
        simulators: None,
    };
    assert_eq!(vanish::presence(&l, &nobody_asked), Presence::CannotTell);
}

#[test]
fn a_simulator_nobody_could_list_cannot_be_judged() {
    let l = lease(
        "5D087114-ECB3-443C-8DDB-40EEF9CFB90C",
        vec![Resource::Booted { by_us: true }],
    );
    assert_eq!(vanish::presence(&l, &live(&[], None)), Presence::CannotTell);
}

#[test]
fn a_simulator_listed_as_shut_down_is_gone() {
    let udid = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";
    let l = lease(udid, vec![Resource::Booted { by_us: true }]);
    assert_eq!(
        vanish::presence(&l, &live(&[], Some(&[(udid, false)]))),
        Presence::Gone { slot_now: None }
    );
    assert_eq!(
        vanish::presence(&l, &live(&[], Some(&[(udid, true)]))),
        Presence::Present
    );
}

fn held(l: Lease) -> Held {
    Held {
        lease: l,
        holder: HolderProbe {
            pid_exists: false,
            identity_matches: false,
        },
        any_resource_alive: false,
    }
}

fn dir() -> (tempfile::TempDir, LeaseDir) {
    let t = tempfile::tempdir().expect("tempdir");
    let d = LeaseDir::at(t.path().to_path_buf());
    (t, d)
}

#[test]
fn a_departure_is_kept_once_and_read_back_whole() {
    let (_t, d) = dir();
    let h = held(lease(
        "emulator-5560",
        vec![
            Resource::Booted { by_us: true },
            emulator("sim-smix-android-01"),
        ],
    ));
    let v = vanish::vanished_from(
        &h,
        None,
        "2026-09-24T01:00:00Z",
        "smix lease list",
        vec!["FATAL | qemu: something".into()],
    );
    assert!(
        vanish::record(&d, &v).expect("record"),
        "the first sighting is new"
    );
    assert!(
        !vanish::record(&d, &v).expect("record"),
        "the same departure noticed twice is one fact, not two"
    );
    let back = vanish::history(&d).expect("history");
    assert_eq!(back, vec![v.clone()]);
    assert_eq!(back[0].avd.as_deref(), Some("sim-smix-android-01"));
    assert!(back[0].booted_by_smix);
    assert!(!back[0].holder_alive);
    assert_eq!(back[0].last_heartbeat, "2026-09-24T00:05:00Z");
    assert_eq!(
        back[0].console_tail,
        vec!["FATAL | qemu: something".to_string()]
    );
}

#[test]
fn the_record_of_departures_is_not_mistaken_for_a_device() {
    let (_t, d) = dir();
    store::add_resource(&d, "emulator-5560", Resource::Booted { by_us: true }).expect("ledger");
    let h = held(lease(
        "emulator-5560",
        vec![Resource::Booted { by_us: true }],
    ));
    let v = vanish::vanished_from(&h, None, "2026-09-24T01:00:00Z", "smix lease list", vec![]);
    vanish::record(&d, &v).expect("record");
    // Exactly one device: the ledger. A count rather than "contains", so
    // the history file appearing as a second device is red.
    assert_eq!(d.device_ids(), vec!["emulator-5560".to_string()]);
}

#[test]
fn no_history_is_an_empty_answer_not_an_error() {
    let (_t, d) = dir();
    assert!(vanish::history(&d).expect("history").is_empty());
}

#[test]
fn a_shutdown_leaves_no_husk_to_be_mistaken_for_a_departure() {
    // Shutdown drops the boot row. If the identity row alone kept the
    // ledger alive, the next command would find an emulator ledger whose
    // device is gone and record smix's own shutdown as a disappearance.
    let (_t, d) = dir();
    store::record_boot(&d, "emulator-5560", true).expect("boot");
    store::add_resource(&d, "emulator-5560", emulator("sim-smix-android-01")).expect("identity");
    store::drop_resource_kind(&d, "emulator-5560", &Resource::Booted { by_us: true })
        .expect("shutdown");
    assert_eq!(d.device_ids(), Vec::<String>::new());
}
