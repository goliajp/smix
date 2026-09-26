//! A cold boot starts the system from scratch.
//!
//! An emulator resumes its Quick Boot snapshot by default, and the snapshot
//! keeps whatever state the device stopped in — a system UI killed after an
//! error comes back dead on every later boot. `-no-snapshot-load` is the
//! emulator's own switch for booting without it.

use smix_adb::{BootFrom, emulator_args};

#[test]
fn a_cold_boot_does_not_load_the_snapshot() {
    let args = emulator_args("sim-a", Some(5556), BootFrom::Cold);
    assert!(args.iter().any(|a| a == "-no-snapshot-load"), "{args:?}");
}

#[test]
fn an_ordinary_boot_resumes_the_snapshot() {
    let args = emulator_args("sim-a", Some(5556), BootFrom::Snapshot);
    assert!(!args.iter().any(|a| a.contains("snapshot")), "{args:?}");
}

#[test]
fn the_port_and_avd_are_passed_either_way() {
    for boot in [BootFrom::Snapshot, BootFrom::Cold] {
        let args = emulator_args("sim-a", Some(5556), boot);
        assert_eq!(&args[..2], ["-avd", "sim-a"]);
        let p = args
            .iter()
            .position(|a| a == "-port")
            .expect("a port was given");
        assert_eq!(args[p + 1], "5556");
    }
}
