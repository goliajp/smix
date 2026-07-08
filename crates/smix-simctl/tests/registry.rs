//! SimRegistry — deterministic device addressing: explicit UDID or an
//! alias recorded in `.smix/sims.json`. No name-resolution against the
//! live simulator set, ever — the registry file is the only source.

use smix_simctl::registry::{RegistryError, SimRegistry};
use std::fs;
use std::path::PathBuf;

const UDID_02: &str = "5D087114-ECB3-443C-8DDB-40EEF9CFB90C";
const UDID_03: &str = "89980B43-EF26-446A-A897-848C1AD3A872";

fn fixture_json() -> String {
    format!(
        r#"{{
  "version": 1,
  "sims": {{
    "02": {{
      "deviceName": "ios-sim-02",
      "udid": "{UDID_02}",
      "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
      "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"
    }},
    "03": {{
      "deviceName": "ios-sim-03",
      "udid": "{UDID_03}",
      "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
      "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"
    }}
  }}
}}"#
    )
}

fn write_fixture(dir: &std::path::Path) -> PathBuf {
    let smix_dir = dir.join(".smix");
    fs::create_dir_all(&smix_dir).unwrap();
    let path = smix_dir.join("sims.json");
    fs::write(&path, fixture_json()).unwrap();
    path
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("smix-registry-test-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn resolve_explicit_udid_passes_through_normalized() {
    let dir = tmpdir("udid-passthrough");
    let reg = SimRegistry::load(&write_fixture(&dir)).unwrap();
    assert_eq!(reg.resolve(UDID_02).unwrap(), UDID_02);
    assert_eq!(reg.resolve(&UDID_02.to_ascii_lowercase()).unwrap(), UDID_02);
}

#[test]
fn resolve_unregistered_udid_still_passes_through() {
    // An explicit UDID is always a deliberate instruction — registry
    // membership is not required for UDID-form input.
    let dir = tmpdir("udid-unregistered");
    let reg = SimRegistry::load(&write_fixture(&dir)).unwrap();
    let foreign = "AAAAAAAA-BBBB-CCCC-DDDD-EEEEFFFF0000";
    assert_eq!(reg.resolve(foreign).unwrap(), foreign);
}

#[test]
fn resolve_alias_key() {
    let dir = tmpdir("alias-key");
    let reg = SimRegistry::load(&write_fixture(&dir)).unwrap();
    assert_eq!(reg.resolve("02").unwrap(), UDID_02);
    assert_eq!(reg.resolve("03").unwrap(), UDID_03);
}

#[test]
fn resolve_device_name() {
    let dir = tmpdir("device-name");
    let reg = SimRegistry::load(&write_fixture(&dir)).unwrap();
    assert_eq!(reg.resolve("ios-sim-02").unwrap(), UDID_02);
}

#[test]
fn resolve_unknown_lists_known_aliases() {
    let dir = tmpdir("unknown");
    let reg = SimRegistry::load(&write_fixture(&dir)).unwrap();
    let err = reg.resolve("nope").unwrap_err();
    match &err {
        RegistryError::UnknownDevice { device_ref, .. } => assert_eq!(device_ref, "nope"),
        other => panic!("expected UnknownDevice, got {other:?}"),
    }
    let msg = err.to_string();
    assert!(msg.contains("02"), "msg should list alias keys: {msg}");
    assert!(
        msg.contains("ios-sim-02"),
        "msg should list device names: {msg}"
    );
}

#[test]
fn discover_walks_up_to_find_registry() {
    let dir = tmpdir("discover");
    let path = write_fixture(&dir);
    let nested = dir.join("a/b/c");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(SimRegistry::discover(&nested).unwrap(), path);
}

#[test]
fn discover_returns_none_without_registry() {
    let dir = tmpdir("discover-none");
    assert!(SimRegistry::discover(&dir).is_none());
}

// RegisteredSim.locale optional field roundtrip. Entries without
// `locale` deserialize as None (no enforcement); entries with `locale`
// populate Some(<tag>).
#[test]
fn sim_entry_locale_field_roundtrip() {
    let dir = tmpdir("locale-roundtrip");
    let smix_dir = dir.join(".smix");
    fs::create_dir_all(&smix_dir).unwrap();
    let json = format!(
        r#"{{
  "version": 1,
  "sims": {{
    "02": {{
      "deviceName": "ios-sim-02",
      "udid": "{UDID_02}",
      "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
      "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro",
      "locale": "en-US"
    }},
    "03": {{
      "deviceName": "ios-sim-03",
      "udid": "{UDID_03}",
      "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-26-5",
      "deviceType": "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro"
    }}
  }}
}}"#
    );
    let path = smix_dir.join("sims.json");
    fs::write(&path, json).unwrap();
    let reg = SimRegistry::load(&path).unwrap();

    let sim02 = reg.lookup("02").expect("02 lookup");
    assert_eq!(sim02.locale.as_deref(), Some("en-US"));

    let sim03 = reg.lookup("03").expect("03 lookup");
    assert_eq!(sim03.locale, None);
}

#[test]
fn lookup_by_alias_device_name_and_udid() {
    let dir = tmpdir("lookup-multimatch");
    let reg = SimRegistry::load(&write_fixture(&dir)).unwrap();
    assert_eq!(
        reg.lookup("02").map(|s| &s.udid),
        Some(&UDID_02.to_string())
    );
    assert_eq!(
        reg.lookup("ios-sim-02").map(|s| &s.udid),
        Some(&UDID_02.to_string())
    );
    assert_eq!(
        reg.lookup(UDID_02).map(|s| &s.udid),
        Some(&UDID_02.to_string())
    );
    assert!(reg.lookup("ghost-sim").is_none());
}
