//! Whose run is on the simulator behind this port? The question iOS
//! `runner down` did not ask.
//!
//! It found the runner by port and stopped it. The ledger records who is
//! driving the device -- a `smix run`, an MCP session, an SDK process --
//! and nothing on this path read it, so a second session's `down` on the
//! same port ended the first one's run. Driven against a fabricated
//! ledger: the only other way to watch it fail is to let it through.

use smix_capsule::runner::live_foreign_holder_on;
use smix_lease::store::LeaseDir;
use smix_lease::{ProcIdentity, Resource};

const UDID: &str = "0F0F0F0F-1111-2222-3333-444444444444";
const PORT: u16 = 22187;

fn runner_on(dir: &LeaseDir, port: u16, holder: ProcIdentity) {
    smix_lease::store::add_resource(
        dir,
        UDID,
        Resource::Runner {
            port,
            proc: ProcIdentity {
                pid: 999_999,
                started_at: "Thu Sep 24 10:00:00 2026".into(),
                cmd: "xcodebuild test".into(),
            },
            bundle: None,
            log: None,
        },
    )
    .expect("record the runner");
    smix_lease::store::set_holder(dir, UDID, holder).expect("record the holder");
}

fn a_live_stranger() -> (std::process::Child, ProcIdentity) {
    let child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    let identity = smix_lease::store::identify(child.id()).expect("identify the stranger");
    (child, identity)
}

#[test]
fn a_live_driver_that_is_not_us_is_named() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    let (mut child, stranger) = a_live_stranger();
    runner_on(&leases, PORT, stranger.clone());

    let holder = live_foreign_holder_on(&leases, PORT)
        .expect("read")
        .expect("a live process driving the device behind this port must stop the verb");
    assert_eq!(holder.pid, stranger.pid);
    assert!(
        holder.cmd.contains("sleep"),
        "the refusal names whose run it is: {holder:?}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_driver_that_has_exited_is_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    let (mut child, stranger) = a_live_stranger();
    let _ = child.kill();
    let _ = child.wait();
    runner_on(&leases, PORT, stranger);

    // `runner up` and `smix run` both exit before the teardown that
    // follows them; refusing here would refuse every ordinary run.
    assert!(
        live_foreign_holder_on(&leases, PORT)
            .expect("read")
            .is_none()
    );
}

#[test]
fn we_are_not_a_stranger_to_ourselves() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    runner_on(&leases, PORT, smix_lease::store::identify_self());
    assert!(
        live_foreign_holder_on(&leases, PORT)
            .expect("read")
            .is_none()
    );
}

#[test]
fn only_the_device_behind_the_asked_port_counts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let leases = LeaseDir::at(dir.path());
    let (mut child, stranger) = a_live_stranger();
    runner_on(&leases, PORT, stranger);

    // Somebody is driving the simulator on 22187; the runner on 22087 is
    // another device's, and theirs is no reason to keep it.
    assert!(
        live_foreign_holder_on(&leases, 22087)
            .expect("read")
            .is_none()
    );

    let _ = child.kill();
    let _ = child.wait();
}
