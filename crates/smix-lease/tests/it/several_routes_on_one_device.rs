//! A device may dial several host services at once, and the ledger has
//! to hold all of them.
//!
//! Every other resource a holder opens is one per device: one runner,
//! one recording, one boot state. `add_resource` was built around that
//! and replaces a row of the same kind, which for those is right — a
//! second row would be a second thing to tear down that never existed.
//!
//! A reverse route is the first kind where a device really can have
//! several. A consumer's round opens one for its API stub and another
//! for its asset server; if the second row replaced the first, the
//! ledger would owe one close and the device would be left holding two,
//! with nothing anywhere able to name the one that got lost.
//!
//! So the key is the port the device dials, not the kind.

use smix_lease::store::{self, LeaseDir};
use smix_lease::{Resource, Row};

fn dir() -> (tempfile::TempDir, LeaseDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let d = LeaseDir::at(tmp.path());
    (tmp, d)
}

fn routes(d: &LeaseDir, device: &str) -> Vec<(u16, u16)> {
    store::read(d, device)
        .expect("read")
        .map(|l| {
            l.resources
                .iter()
                .filter_map(Row::known)
                .filter_map(|r| match r {
                    Resource::ReversePort {
                        device_port,
                        host_port,
                        ..
                    } => Some((*device_port, *host_port)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn two_routes_on_different_ports_both_stay() {
    let (_tmp, d) = dir();
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 8080, 8080).expect("first");
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 3000, 3001).expect("second");

    let open = routes(&d, "emulator-5554");
    assert_eq!(
        open.len(),
        2,
        "both routes are open on the device: {open:?}"
    );
    assert!(
        open.contains(&(8080, 8080)) && open.contains(&(3000, 3001)),
        "{open:?}"
    );
}

/// Re-pointing a route the device already dials is one route, not two —
/// adb replaces it, and a ledger that stacked a second row would owe a
/// close for a route that no longer exists.
#[test]
fn reopening_the_same_device_port_restates_it() {
    let (_tmp, d) = dir();
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 8080, 8080).expect("first");
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 8080, 4000).expect("again");

    assert_eq!(
        routes(&d, "emulator-5554"),
        vec![(8080, 4000)],
        "the later host port is the one in force"
    );
}

#[test]
fn closing_one_route_leaves_the_other() {
    let (_tmp, d) = dir();
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 8080, 8080).expect("first");
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 3000, 3000).expect("second");
    store::drop_reverse(&d, "emulator-5554", 8080).expect("drop one");

    assert_eq!(routes(&d, "emulator-5554"), vec![(3000, 3000)]);
}

/// A ledger whose last row goes is a file describing nothing, and the
/// rest of the store already removes those rather than leaving a husk
/// that reads like an occupied device.
#[test]
fn closing_the_last_route_takes_the_ledger_with_it() {
    let (_tmp, d) = dir();
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 8080, 8080).expect("open");
    store::drop_reverse(&d, "emulator-5554", 8080).expect("close");

    assert!(
        store::read(&d, "emulator-5554").expect("read").is_none(),
        "nothing is open on this device any more"
    );
}

/// The boot row answers a different question — who may switch this
/// device off — and closing a route must not answer it by accident.
#[test]
fn closing_a_route_leaves_the_record_of_who_booted_the_device() {
    let (_tmp, d) = dir();
    store::record_boot(&d, "emulator-5554", true).expect("boot");
    store::record_reverse(&d, "emulator-5554", "emulator-5554", 8080, 8080).expect("open");
    store::drop_reverse(&d, "emulator-5554", 8080).expect("close");

    let lease = store::read(&d, "emulator-5554")
        .expect("read")
        .expect("ledger");
    assert!(
        lease
            .known_resources()
            .any(|r| matches!(r, Resource::Booted { by_us: true })),
        "the boot row is still there: {:?}",
        lease.resources
    );
}
