//! `AndroidDeviceControl` impl smoke. Verifies trait surface
//! compiles + Box<dyn DeviceControl> can hold an Android impl. Live
//! `adb` invocations are integration-tested under `#[ignore]` when an
//! emulator is booted.

use smix_sdk::{AndroidDeviceControl, DeviceControl};

#[test]
fn android_device_control_dyn_compat() {
    let _: Box<dyn DeviceControl> = Box::new(AndroidDeviceControl::new());
}

#[test]
fn android_platform_reports_android() {
    let device = AndroidDeviceControl::new();
    assert_eq!(device.platform(), smix_driver::Platform::Android);
}

#[test]
fn android_as_ios_simctl_returns_none() {
    let device = AndroidDeviceControl::new();
    assert!(device.as_ios_simctl().is_none());
}
