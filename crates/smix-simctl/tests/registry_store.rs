//! The registry lives in the store now.
//!
//! Two things have to be true at once: a user's existing
//! `.smix/sims.json` keeps resolving (the whole existing test suite in
//! `registry.rs` exercises that path unchanged), and smix stops writing
//! that file.
//!
//! The third test is the reason for the migration. `register` used to
//! read the whole file, insert one row, and write the whole file back.
//! Two processes doing that at once — `smix sim register` in one
//! terminal while a flow registers in another — keep only whichever
//! wrote last, and the other alias is gone with no error anywhere.

use smix_simctl::registry::{RegisteredSim, SimRegistry};

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smix-registry-store-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".smix")).expect("temp root");
    dir
}

fn sim(name: &str) -> RegisteredSim {
    RegisteredSim {
        device_name: name.to_string(),
        udid: format!("UDID-{name}"),
        runtime: "iOS 26.5".to_string(),
        device_type: "iPhone 16 Pro".to_string(),
        locale: None,
        runner_port: None,
        kind: smix_simctl::registry::DeviceKind::Simulator,
        destructive_opt_in: false,
    }
}

const LEGACY: &str = r#"{"version":1,"sims":{"legacy-alias":{"deviceName":"iPhone SE","udid":"LEGACY-UDID","runtime":"iOS 26.5","deviceType":"iPhone SE"}}}"#;

#[test]
fn a_legacy_file_still_resolves_through_the_real_call_chain() {
    let root = temp_root("legacy");
    let legacy = root.join(".smix/sims.json");
    std::fs::write(&legacy, LEGACY).expect("write legacy");

    let registry = SimRegistry::load(&legacy).expect("loads");
    let udid = registry.resolve("legacy-alias").expect("resolves");
    assert_eq!(udid, "LEGACY-UDID");
}

#[test]
fn registering_does_not_write_the_legacy_file() {
    let root = temp_root("no-write");
    let legacy = root.join(".smix/sims.json");
    std::fs::write(&legacy, LEGACY).expect("write legacy");
    let before = std::fs::read_to_string(&legacy).expect("read");

    SimRegistry::register(&legacy, "fresh", sim("fresh")).expect("registers");

    assert_eq!(
        std::fs::read_to_string(&legacy).expect("read"),
        before,
        "smix still writes sims.json — the store is not the source of truth"
    );
    let registry = SimRegistry::load(&legacy).expect("loads");
    assert!(
        registry.resolve("fresh").is_ok(),
        "the new alias must be in the store"
    );
    assert!(
        registry.resolve("legacy-alias").is_ok(),
        "the imported alias must survive a later register"
    );
}

#[test]
fn two_concurrent_registers_both_survive() {
    // The read-modify-write this replaces loses one of these, silently.
    let root = temp_root("concurrent");
    let legacy = root.join(".smix/sims.json");

    let a = {
        let p = legacy.clone();
        std::thread::spawn(move || SimRegistry::register(&p, "alpha", sim("alpha")))
    };
    let b = {
        let p = legacy.clone();
        std::thread::spawn(move || SimRegistry::register(&p, "beta", sim("beta")))
    };
    a.join().expect("thread a").expect("registers alpha");
    b.join().expect("thread b").expect("registers beta");

    let registry = SimRegistry::load(&legacy).expect("loads");
    assert!(registry.resolve("alpha").is_ok(), "alpha was lost");
    assert!(registry.resolve("beta").is_ok(), "beta was lost");
}

#[test]
fn a_smix_directory_works_as_well_as_a_sims_json_path() {
    // Callers pass either: the documented SMIX_SIMS_JSON points at a
    // file, while discovery yields a directory.
    let root = temp_root("either");
    SimRegistry::register(&root.join(".smix"), "dir-form", sim("dir-form")).expect("registers");
    let registry = SimRegistry::load(&root.join(".smix/sims.json")).expect("loads via file path");
    assert!(registry.resolve("dir-form").is_ok());
}

#[test]
fn project_alias_round_trips_and_is_per_project() {
    let root = temp_root("project-alias");
    let p = root.as_path();
    SimRegistry::set_project_alias(p, "/Users/dev/app-one", "app-one").expect("set one");
    SimRegistry::set_project_alias(p, "/Users/dev/app-two", "app-two").expect("set two");
    assert_eq!(
        SimRegistry::project_alias(p, "/Users/dev/app-one").expect("get one"),
        Some("app-one".to_string())
    );
    assert_eq!(
        SimRegistry::project_alias(p, "/Users/dev/app-two").expect("get two"),
        Some("app-two".to_string())
    );
    assert_eq!(
        SimRegistry::project_alias(p, "/Users/dev/absent").expect("get none"),
        None
    );
    // same key overwrites to the latest
    SimRegistry::set_project_alias(p, "/Users/dev/app-one", "app-one-b").expect("reset");
    assert_eq!(
        SimRegistry::project_alias(p, "/Users/dev/app-one").expect("get one again"),
        Some("app-one-b".to_string())
    );
}
