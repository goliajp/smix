//! Folding the per-checkout registries into the machine one.
//!
//! Four checkouts on this machine each kept a `.smix/` because
//! `registry_path()` walked up from the working directory. Migration is
//! a one-way door: whatever it drops, the workspace that relied on it
//! stops working, and the person will not know which of their four trees
//! to look in. So it is built to be safe rather than tidy — it adds, it
//! never removes, and it leaves every source file exactly where it was.
//!
//! The merge rules themselves are `registry_merge.rs`'s subject. What is
//! checked here is that migration applies them to real books on disk and
//! that nothing is lost on the way.

use smix_simctl::registry::{DeviceKind, RegisteredSim, SimRegistry};
use std::path::{Path, PathBuf};

fn sim(udid: &str, opt_in: bool) -> RegisteredSim {
    RegisteredSim {
        device_name: format!("dev {udid}"),
        kind: DeviceKind::Simulator,
        destructive_opt_in: opt_in,
        udid: udid.into(),
        runtime: "com.apple.CoreSimulator.SimRuntime.iOS-26-5".into(),
        device_type: String::new(),
        locale: None,
        runner_port: None,
    }
}

/// A `.smix`-shaped registry directory with the given rows in it.
fn book(root: &Path, name: &str, rows: &[(&str, RegisteredSim)]) -> PathBuf {
    let dir = root.join(name).join(".smix");
    std::fs::create_dir_all(&dir).unwrap();
    for (alias, s) in rows {
        SimRegistry::register(&dir, alias, s.clone()).unwrap();
    }
    dir
}

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "smix-migrate-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The whole point: nothing disappears.
#[test]
fn every_device_in_every_book_survives() {
    let root = tmp("union");
    let a = book(&root, "a", &[("dev", sim("UDID-1", false))]);
    let b = book(&root, "b", &[("phone", sim("UDID-2", false))]);
    let c = book(&root, "c", &[("dev", sim("UDID-1", false))]);
    let machine = root.join("machine");

    let report = SimRegistry::migrate(&machine, &[a, b, c]).unwrap();

    let got = SimRegistry::load(&machine).unwrap();
    let udids: std::collections::BTreeSet<String> =
        got.sims().values().map(|s| s.udid.clone()).collect();
    assert_eq!(
        udids,
        ["UDID-1".to_string(), "UDID-2".to_string()].into(),
        "report was {report:?}"
    );
}

/// Two trees, two names, one device. Both names keep working.
#[test]
fn a_device_known_by_two_names_answers_to_both() {
    let root = tmp("aliases");
    let a = book(&root, "a", &[("dev", sim("UDID-1", false))]);
    let b = book(&root, "b", &[("sim-01", sim("UDID-1", false))]);
    let machine = root.join("machine");

    SimRegistry::migrate(&machine, &[a, b]).unwrap();

    let got = SimRegistry::load(&machine).unwrap();
    assert_eq!(got.resolve("dev").unwrap(), "UDID-1");
    assert_eq!(got.resolve("sim-01").unwrap(), "UDID-1");
}

/// Consent granted in one tree and withheld in another does not widen.
///
/// §9 #1: allowing destruction is a per-device authorisation. Merging
/// two books is not a moment to hand one out — somebody who granted it
/// can grant it again, somebody who did not cannot un-wipe a phone.
#[test]
fn conflicting_consent_lands_on_the_stricter_answer() {
    let root = tmp("consent");
    let a = book(&root, "a", &[("phone", sim("UDID-9", true))]);
    let b = book(&root, "b", &[("phone", sim("UDID-9", false))]);
    let machine = root.join("machine");

    SimRegistry::migrate(&machine, &[a, b]).unwrap();

    let got = SimRegistry::load(&machine).unwrap();
    assert!(
        !got.lookup("phone").unwrap().destructive_opt_in,
        "consent widened during a migration"
    );
}

/// One unreadable source must not strand the other three.
#[test]
fn a_source_that_will_not_open_is_named_not_fatal() {
    let root = tmp("corrupt");
    let good = book(&root, "good", &[("dev", sim("UDID-1", false))]);
    let missing = root.join("nope").join(".smix");
    let machine = root.join("machine");

    let report = SimRegistry::migrate(&machine, &[good, missing.clone()]).unwrap();

    let got = SimRegistry::load(&machine).unwrap();
    assert!(got.lookup("dev").is_some(), "the readable book was dropped");
    assert!(
        report.empty.iter().any(|p| p == &missing),
        "a source that gave nothing was not reported: {report:?}"
    );
}

/// Migration adds; it never takes the source away.
///
/// Somebody who has to go back to a smix from before this move must
/// still find their registry where they left it.
#[test]
fn the_source_book_is_left_where_it_was() {
    let root = tmp("nondestructive");
    let a = book(&root, "a", &[("dev", sim("UDID-1", false))]);
    let machine = root.join("machine");

    SimRegistry::migrate(&machine, std::slice::from_ref(&a)).unwrap();

    let still = SimRegistry::load(&a).unwrap();
    assert_eq!(still.resolve("dev").unwrap(), "UDID-1");
}

/// Running it twice changes nothing the second time.
///
/// Migration is offered on first use, so it will be run again — by a
/// script, by somebody who forgot, by two shells at once. A second run
/// that renamed or duplicated anything would make "run it again if
/// unsure" bad advice.
#[test]
fn a_second_run_is_a_no_op() {
    let root = tmp("idempotent");
    let a = book(&root, "a", &[("dev", sim("UDID-1", false))]);
    let machine = root.join("machine");

    SimRegistry::migrate(&machine, std::slice::from_ref(&a)).unwrap();
    let first: Vec<String> = SimRegistry::load(&machine)
        .unwrap()
        .sims()
        .keys()
        .cloned()
        .collect();

    let second_report = SimRegistry::migrate(&machine, &[a]).unwrap();
    let second: Vec<String> = SimRegistry::load(&machine)
        .unwrap()
        .sims()
        .keys()
        .cloned()
        .collect();

    assert_eq!(first, second, "a second migration changed the aliases");
    assert!(
        second_report.added.is_empty(),
        "a second migration claimed to add something: {second_report:?}"
    );
}
