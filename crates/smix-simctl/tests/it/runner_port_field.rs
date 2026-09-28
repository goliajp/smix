//! Regression test for the optional `runnerPort` field on the
//! `.smix/sims.json` `RegisteredSim` entry.
//!
//! Purpose:
//! - Confirm the field is deserialized when present
//! - Confirm the field is `None` when absent
//! - Confirm two sims can coexist with distinct ports (the whole point
//!   of the field is concurrent-runner scenarios)

use std::fs;
use tempfile::TempDir;

use smix_simctl::registry::SimRegistry;

fn write_sims(tmp: &TempDir, json: &str) -> std::path::PathBuf {
    let path = tmp.path().join("sims.json");
    fs::write(&path, json).unwrap();
    path
}

#[test]
fn runner_port_absent_deserializes_none() {
    let tmp = TempDir::new().unwrap();
    let path = write_sims(
        &tmp,
        r#"{
        "version": 1,
        "sims": {
            "a": {
                "deviceName": "sim-a",
                "udid": "AAAAAAAA-4AAA-4AAA-AAAA-BBBBBBBBBBBB",
                "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
                "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"
            }
        }
    }"#,
    );
    let reg = SimRegistry::load(&path).unwrap();
    let sim = reg.lookup("a").unwrap();
    assert_eq!(sim.runner_port, None);
}

#[test]
fn runner_port_present_deserializes_value() {
    let tmp = TempDir::new().unwrap();
    let path = write_sims(
        &tmp,
        r#"{
        "version": 1,
        "sims": {
            "a": {
                "deviceName": "sim-a",
                "udid": "AAAAAAAA-4AAA-4AAA-AAAA-BBBBBBBBBBBB",
                "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
                "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
                "runnerPort": 22088
            }
        }
    }"#,
    );
    let reg = SimRegistry::load(&path).unwrap();
    let sim = reg.lookup("a").unwrap();
    assert_eq!(sim.runner_port, Some(22088));
}

#[test]
fn two_sims_can_have_distinct_runner_ports() {
    let tmp = TempDir::new().unwrap();
    let path = write_sims(
        &tmp,
        r#"{
        "version": 1,
        "sims": {
            "a": {
                "deviceName": "sim-a",
                "udid": "AAAAAAAA-4AAA-4AAA-AAAA-BBBBBBBBBBBB",
                "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
                "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
                "runnerPort": 22087
            },
            "b": {
                "deviceName": "sim-b",
                "udid": "CCCCCCCC-4CCC-4CCC-CCCC-DDDDDDDDDDDD",
                "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
                "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
                "runnerPort": 22088
            }
        }
    }"#,
    );
    let reg = SimRegistry::load(&path).unwrap();
    let a = reg.lookup("a").unwrap();
    let b = reg.lookup("b").unwrap();
    assert_eq!(a.runner_port, Some(22087));
    assert_eq!(b.runner_port, Some(22088));
    assert_ne!(a.runner_port, b.runner_port);
}
