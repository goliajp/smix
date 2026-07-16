//! `adb devices -l` stdout parser unit tests.
//! Mirrors the smix-simctl test pattern: pure-function parsing on canned stdout,
//! no live adb spawn (those go to integration tests with #[ignore]).

use smix_adb::{AdbDevice, parse_devices_stdout};

#[test]
fn parses_empty_list() {
    let stdout = "List of devices attached\n\n";
    let devices = parse_devices_stdout(stdout).expect("parse ok");
    assert_eq!(devices.len(), 0);
}

#[test]
fn parses_single_emulator() {
    let stdout = "List of devices attached
emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:1
";
    let devices = parse_devices_stdout(stdout).expect("parse ok");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].serial, "emulator-5554");
    assert_eq!(devices[0].state, "device");
    assert_eq!(devices[0].model.as_deref(), Some("sdk_gphone64_arm64"));
}

#[test]
fn parses_offline_state() {
    let stdout = "List of devices attached
emulator-5556          offline
";
    let devices = parse_devices_stdout(stdout).expect("parse ok");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].serial, "emulator-5556");
    assert_eq!(devices[0].state, "offline");
    assert!(devices[0].model.is_none());
}

#[test]
fn parses_multiple_devices() {
    let stdout = "List of devices attached
emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64
emulator-5556          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64
0a1b2c3d               device product:walleye model:Pixel_2 device:walleye
";
    let devices = parse_devices_stdout(stdout).expect("parse ok");
    assert_eq!(devices.len(), 3);
    assert_eq!(devices[0].serial, "emulator-5554");
    assert_eq!(devices[1].serial, "emulator-5556");
    assert_eq!(devices[2].serial, "0a1b2c3d");
    assert_eq!(devices[2].model.as_deref(), Some("Pixel_2"));
}

#[test]
fn ignores_header_only() {
    let devices: Vec<AdbDevice> = parse_devices_stdout("\n\n").expect("parse ok");
    assert_eq!(devices.len(), 0);
}
