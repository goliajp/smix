//! v2 break #3: the two run-time switches are driven by injected `App`
//! fields, and the field wins over the `SMIX_*` env fallback.
//!
//! `assert_screenshot` reads `assert_screenshot_strict`; `launch_fresh`
//! reads `launch_fresh_force_reinstall`. A `Some` field is used verbatim
//! (no env consulted); `None` keeps the legacy env read. These tests pin
//! the field and observe the branch, so they never touch process env.
//! With the env var unset the default is non-strict, so a `Some(true)`
//! that forces strict proves the field is consulted and overrides the
//! env-read path — no `set_var` race needed to demonstrate field-wins.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use smix_sdk::{
    App, AssertScreenshotOutcome, DeviceControl, DeviceControlError, HttpRunnerClient, Permission,
    PermissionAction, SimctlDriver,
};

type Calls = Arc<Mutex<Vec<String>>>;

/// Records the device-control methods `launch_fresh` drives, and returns
/// canned success for everything. `screenshot` returns opaque bytes —
/// `assert_screenshot_inner` never decodes them on the strict-error or
/// auto-record paths, so any bytes suffice. Call log is shared via `Arc`
/// so the test keeps a handle after the device moves into the `App`.
struct RecordingDevice {
    calls: Calls,
}

impl RecordingDevice {
    fn new() -> (Self, Calls) {
        let calls: Calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                calls: Arc::clone(&calls),
            },
            calls,
        )
    }
    fn record(&self, name: &str) {
        self.calls.lock().unwrap().push(name.to_string());
    }
}

#[async_trait]
impl DeviceControl for RecordingDevice {
    fn platform(&self) -> smix_driver::Platform {
        smix_driver::Platform::Ios
    }
    async fn launch(&self, _udid: &str, _bundle_id: &str) -> Result<u32, DeviceControlError> {
        self.record("launch");
        Ok(1)
    }
    async fn launch_with_args(
        &self,
        _udid: &str,
        _bundle_id: &str,
        _args: &[String],
    ) -> Result<u32, DeviceControlError> {
        self.record("launch_with_args");
        Ok(1)
    }
    async fn terminate(&self, _udid: &str, _bundle_id: &str) -> Result<(), DeviceControlError> {
        self.record("terminate");
        Ok(())
    }
    async fn install(&self, _udid: &str, _app_path: &str) -> Result<(), DeviceControlError> {
        self.record("install");
        Ok(())
    }
    async fn uninstall(&self, _udid: &str, _bundle_id: &str) -> Result<(), DeviceControlError> {
        self.record("uninstall");
        Ok(())
    }
    async fn keychain_reset(&self, _udid: &str) -> Result<(), DeviceControlError> {
        self.record("keychain_reset");
        Ok(())
    }
    async fn privacy_reset_all(
        &self,
        _udid: &str,
        _bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        self.record("privacy_reset_all");
        Ok(())
    }
    async fn clear_app_sandbox(
        &self,
        _udid: &str,
        _bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        self.record("clear_app_sandbox");
        Ok(())
    }
    async fn open_url(&self, _udid: &str, _url: &str) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn send_push(
        &self,
        _udid: &str,
        _bundle_id: &str,
        _apns_json_path: &str,
    ) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn screenshot(&self, _udid: &str) -> Result<Vec<u8>, DeviceControlError> {
        self.record("screenshot");
        Ok(b"opaque-not-a-real-png".to_vec())
    }
    async fn pasteboard_set(&self, _udid: &str, _text: &str) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn pasteboard_get(&self, _udid: &str) -> Result<String, DeviceControlError> {
        unimplemented!()
    }
    async fn add_media(&self, _udid: &str, _paths: &[String]) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn location_set(
        &self,
        _udid: &str,
        _lat: f64,
        _lon: f64,
    ) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn location_start(
        &self,
        _udid: &str,
        _points: &[(f64, f64)],
        _speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn set_permission(
        &self,
        _udid: &str,
        _bundle_id: &str,
        _permission: Permission,
        _action: PermissionAction,
    ) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn start_recording(
        &self,
        _udid: &str,
        _output_path: &Path,
    ) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
    async fn stop_recording(&self) -> Result<(), DeviceControlError> {
        unimplemented!()
    }
}

fn app_with_device(device: RecordingDevice) -> App {
    // The driver is never contacted by assert_screenshot / launch_fresh —
    // both go through `self.device`. A dummy runner client is enough.
    let runner = HttpRunnerClient::with_base("http://127.0.0.1:1");
    App::new_with(Box::new(SimctlDriver::new(runner)), Box::new(device)).with_udid("UDID-TEST")
}

/// Unique scratch path for a baseline PNG; the auto-record path creates
/// the parent dir itself.
fn scratch_baseline(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("smix-c15-{}-{}-{}", std::process::id(), tag, nanos))
        .join("baseline.png")
}

#[tokio::test]
async fn assert_screenshot_strict_field_forces_driver_error() {
    let baseline = scratch_baseline("strict");
    let (device, _calls) = RecordingDevice::new();
    let app = app_with_device(device).with_assert_screenshot_strict(Some(true));
    let err = app.assert_screenshot(&baseline, 5).await.unwrap_err();
    assert_eq!(err.code, smix_sdk::FailureCode::DriverError);
    assert!(
        !baseline.exists(),
        "strict must NOT auto-record the baseline"
    );
}

#[tokio::test]
async fn assert_screenshot_field_false_auto_records() {
    let baseline = scratch_baseline("record");
    let (device, _calls) = RecordingDevice::new();
    let app = app_with_device(device).with_assert_screenshot_strict(Some(false));
    let out = app.assert_screenshot(&baseline, 5).await.unwrap();
    assert!(matches!(out, AssertScreenshotOutcome::Recorded { .. }));
    assert!(baseline.exists(), "non-strict auto-records the baseline");
    let _ = std::fs::remove_dir_all(baseline.parent().unwrap());
}

#[tokio::test]
async fn launch_fresh_reinstall_field_drives_uninstall_install() {
    let (device, calls) = RecordingDevice::new();
    let app = app_with_device(device).with_launch_fresh_force_reinstall(Some(true));
    // clear_state + app_path present → reinstall plan = uninstall+install.
    app.launch_fresh("com.test.app", true, false, Some("/tmp/X.app"), &[])
        .await
        .unwrap();
    let calls = calls.lock().unwrap().clone();
    assert!(calls.contains(&"uninstall".to_string()), "calls: {calls:?}");
    assert!(calls.contains(&"install".to_string()), "calls: {calls:?}");
}

#[tokio::test]
async fn launch_fresh_field_false_stays_in_place() {
    let (device, calls) = RecordingDevice::new();
    let app = app_with_device(device).with_launch_fresh_force_reinstall(Some(false));
    app.launch_fresh("com.test.app", true, false, Some("/tmp/X.app"), &[])
        .await
        .unwrap();
    let calls = calls.lock().unwrap().clone();
    assert!(
        calls.contains(&"clear_app_sandbox".to_string()),
        "in-place clear expected: {calls:?}"
    );
    assert!(
        !calls.contains(&"uninstall".to_string()),
        "field Some(false) must NOT reinstall: {calls:?}"
    );
}
