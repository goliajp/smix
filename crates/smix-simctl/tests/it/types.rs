//! Unit tests for smix-simctl types (subprocess integration tests run
//! against a real sim elsewhere).

use smix_simctl::{Appearance, SimctlClient, SimctlPermission};

#[test]
fn permission_as_str_covers_all_variants() {
    let cases = [
        (SimctlPermission::Camera, "camera"),
        (SimctlPermission::Photos, "photos"),
        (SimctlPermission::Location, "location"),
        (SimctlPermission::LocationAlways, "location-always"),
        (SimctlPermission::Notifications, "notifications"),
        (SimctlPermission::Microphone, "microphone"),
        (SimctlPermission::Contacts, "contacts"),
        (SimctlPermission::Calendar, "calendar"),
        (SimctlPermission::Reminders, "reminders"),
        (SimctlPermission::Media, "media-library"),
        (SimctlPermission::Motion, "motion"),
        (SimctlPermission::HomeKit, "homekit"),
        (SimctlPermission::Health, "health"),
        (SimctlPermission::Bluetooth, "bluetooth"),
        (SimctlPermission::Faceid, "faceid"),
        (SimctlPermission::AddressBook, "addressbook"),
    ];
    for (p, expected) in cases {
        assert_eq!(p.as_str(), expected);
    }
}

#[test]
fn appearance_as_str_light_dark() {
    assert_eq!(Appearance::Light.as_str(), "light");
    assert_eq!(Appearance::Dark.as_str(), "dark");
}

#[test]
fn simctl_client_default_constructable() {
    let _: SimctlClient = SimctlClient::default();
    let _: SimctlClient = SimctlClient::new();
}

// Sanity: DeviceControlError variants reachable via Display impl. These don't
// actually invoke subprocess (constructing the variants directly is safe).
#[test]
fn simctl_error_display_includes_subcommand() {
    use smix_simctl::DeviceControlError;
    let e = DeviceControlError::NonZeroExit {
        subcommand: "boot".into(),
        argv: vec!["boot".into(), "test-udid".into()],
        code: 5,
        stderr: "device not found".into(),
        wall_ms: 42,
    };
    let s = format!("{e}");
    assert!(s.contains("boot"));
    assert!(s.contains("test-udid"), "argv should appear in error: {s}");
    assert!(s.contains("5"));
    assert!(s.contains("42ms"), "wall_ms should appear: {s}");
    assert!(s.contains("device not found"));

    let e = DeviceControlError::Timeout {
        subcommand: "boot".into(),
        ms: 60000,
    };
    let s = format!("{e}");
    assert!(s.contains("60000"));

    let e = DeviceControlError::Malformed {
        subcommand: "list".into(),
        detail: "bad json".into(),
    };
    assert!(format!("{e}").contains("malformed"));
}

/// A bundle that is not installed is the first error a reader of the
/// quickstart hits — they have not built and installed their app yet,
/// or they typed the id wrong. It used to surface as the raw simctl
/// text (`NSPOSIXErrorDomain code 2: No such file or directory`), which
/// names neither the app nor the fix. This project's premise is
/// AI-readable failures; that message is the opposite of one.
#[test]
fn a_missing_app_is_named_not_dumped() {
    let err = smix_simctl::DeviceControlError::AppNotInstalled {
        bundle_id: "com.example.app".into(),
        udid: "5D087114-ECB3-443C-8DDB-40EEF9CFB90C".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("com.example.app"), "must name the app: {msg}");
    assert!(
        msg.contains("not installed"),
        "must say what is wrong in words: {msg}"
    );
    assert!(msg.contains("install"), "must point at the fix: {msg}");
    assert!(
        !msg.contains("NSPOSIXErrorDomain"),
        "must not leak the raw simctl dump: {msg}"
    );
}
