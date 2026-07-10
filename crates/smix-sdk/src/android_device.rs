//! Android `DeviceControl` impl backed by `smix_adb::AdbClient`.
//!
//! Mirror of [`crate::IosDeviceControl`]. Wraps `adb` shell commands;
//! routes the cross-platform `Permission` enum to `android.permission.*`
//! grant/revoke via `pm`.

use async_trait::async_trait;
use std::path::Path;
use tokio::sync::Mutex;

use smix_adb::{AdbClient, AdbError};
use smix_driver::Platform;
use smix_simctl::SimctlError;

use crate::PermissionAction;
use crate::device_control::{DeviceControl, Permission};

/// Android `DeviceControl` impl. Wraps `smix_adb::AdbClient` + holds an
/// active recording handle internally.
///
/// Some methods for operations not yet fully implemented surface
/// `SimctlError::NonZeroExit` with a clear message; `smix-adb`
/// translation lives in `adb_to_simctl_err`.
pub struct AndroidDeviceControl {
    client: AdbClient,
    /// Active `adb shell screenrecord` child handle (PID), if recording.
    /// Placeholder; real recording wiring lands when Android video
    /// capture becomes a requirement.
    #[allow(dead_code)]
    recording: Mutex<Option<u32>>,
}

impl Default for AndroidDeviceControl {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidDeviceControl {
    #[must_use]
    pub fn new() -> Self {
        AndroidDeviceControl {
            client: AdbClient::new(),
            recording: Mutex::new(None),
        }
    }

    /// Construct with an existing `AdbClient` (tests / non-PATH adb).
    #[must_use]
    pub fn with_client(client: AdbClient) -> Self {
        AndroidDeviceControl {
            client,
            recording: Mutex::new(None),
        }
    }
}

/// Translate `AdbError` → `SimctlError` so the trait surface stays
/// consistent (single error type across platforms — App layer maps to
/// `ExpectationFailure`). Android-specific detail preserved in message.
fn adb_to_simctl_err(e: AdbError, subcommand: &str) -> SimctlError {
    match e {
        AdbError::BinaryNotFound => SimctlError::non_zero_exit(
            subcommand,
            -1,
            "adb binary not found in PATH; install Android SDK platform-tools",
        ),
        AdbError::Spawn(io) => SimctlError::non_zero_exit(
            subcommand,
            -1,
            format!("adb spawn failed: {io}"),
        ),
        AdbError::NonZeroExit {
            subcommand: sub,
            code,
            stderr,
            serial,
        } => SimctlError::non_zero_exit(
            format!("adb {sub} (serial={serial:?})"),
            code,
            stderr,
        ),
        AdbError::Malformed {
            subcommand: sub,
            detail,
        } => SimctlError::Malformed {
            subcommand: sub,
            detail,
        },
    }
}

#[async_trait]
impl DeviceControl for AndroidDeviceControl {
    fn platform(&self) -> Platform {
        Platform::Android
    }

    // === Lifecycle ===

    async fn launch(&self, serial: &str, bundle_id: &str) -> Result<u32, SimctlError> {
        // AdbClient::start_activity itself builds `package/activity` for
        // `am start -n`. Pass the activity name RELATIVE to the package
        // (".MainActivity") so the wire ends up `<pkg>/.MainActivity`
        // rather than the double-prefix form `<pkg>/<pkg>/.MainActivity`.
        self.client
            .start_activity(serial, bundle_id, ".MainActivity", &[])
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell am start"))?;
        Ok(0)
    }

    async fn launch_with_args(
        &self,
        serial: &str,
        bundle_id: &str,
        args: &[String],
    ) -> Result<u32, SimctlError> {
        let extras: Vec<(String, String)> = args
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some((chunk[0].clone(), chunk[1].clone()))
                } else {
                    None
                }
            })
            .collect();
        let extra_refs: Vec<(&str, &str)> = extras
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.client
            .start_activity(serial, bundle_id, ".MainActivity", &extra_refs)
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell am start"))?;
        Ok(0)
    }

    async fn terminate(&self, serial: &str, bundle_id: &str) -> Result<(), SimctlError> {
        self.client
            .force_stop(serial, bundle_id)
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell am force-stop"))
    }

    async fn install(&self, serial: &str, app_path: &str) -> Result<(), SimctlError> {
        self.client
            .install(serial, Path::new(app_path))
            .await
            .map_err(|e| adb_to_simctl_err(e, "install"))
    }

    async fn uninstall(&self, serial: &str, bundle_id: &str) -> Result<(), SimctlError> {
        self.client
            .uninstall(serial, bundle_id)
            .await
            .map_err(|e| adb_to_simctl_err(e, "uninstall"))
    }

    async fn keychain_reset(&self, _udid: &str) -> Result<(), SimctlError> {
        // No direct Android analog; KeyChain/AccountManager require app-side
        // intent or root. Surface as a platform no-op (matches the
        // cross-platform yaml `clearKeychain: true` expectation that
        // Android silently no-ops rather than crashing).
        Ok(())
    }

    // === Lifecycle ancillary ===

    async fn open_url(&self, serial: &str, url: &str) -> Result<(), SimctlError> {
        self.client
            .shell(
                serial,
                &["am", "start", "-a", "android.intent.action.VIEW", "-d", url],
            )
            .await
            .map(|_| ())
            .map_err(|e| adb_to_simctl_err(e, "shell am start -a VIEW"))
    }

    async fn send_push(
        &self,
        _serial: &str,
        _bundle_id: &str,
        _apns_json_path: &str,
    ) -> Result<(), SimctlError> {
        // Android push = FCM not APNs. Cross-platform yaml `sendPush:` on
        // Android requires FCM credentials + Firebase project setup —
        // deferred. Surface an explicit error so yaml authors know it
        // is not silently no-op.
        Err(SimctlError::non_zero_exit("send_push", -1, "Android FCM push not implemented (cross-platform yaml `sendPush:` is iOS APNs only)"))
    }

    async fn screenshot(&self, serial: &str) -> Result<Vec<u8>, SimctlError> {
        self.client
            .screenshot(serial)
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell screencap"))
    }

    // === Clipboard / Media / Location ===

    async fn pasteboard_set(&self, _serial: &str, _text: &str) -> Result<(), SimctlError> {
        // Android clipboard via `cmd clipboard set-text` (API 29+) or
        // `service call clipboard` (older). v6.x dedicated cycle; skeleton
        // returns error to avoid silent failure.
        Err(SimctlError::non_zero_exit("pasteboard_set", -1, "Android clipboard set wiring deferred to a future cycle"))
    }

    async fn pasteboard_get(&self, _serial: &str) -> Result<String, SimctlError> {
        Err(SimctlError::non_zero_exit("pasteboard_get", -1, "Android clipboard get wiring deferred to a future cycle"))
    }

    async fn add_media(&self, _serial: &str, _paths: &[String]) -> Result<(), SimctlError> {
        // `adb push <local> /sdcard/Pictures/` then `am broadcast -a
        // android.intent.action.MEDIA_SCANNER_SCAN_FILE`.
        Err(SimctlError::non_zero_exit("add_media", -1, "Android media library push deferred to a future cycle"))
    }

    async fn location_set(&self, serial: &str, lat: f64, lon: f64) -> Result<(), SimctlError> {
        // `adb emu geo fix <lon> <lat>` — emu console (note arg order:
        // lon first, then lat).
        self.client
            .shell(
                serial,
                &["emu", "geo", "fix", &lon.to_string(), &lat.to_string()],
            )
            .await
            .map(|_| ())
            .map_err(|e| adb_to_simctl_err(e, "emu geo fix"))
    }

    async fn location_start(
        &self,
        _serial: &str,
        _points: &[(f64, f64)],
        _speed_mps: Option<f64>,
    ) -> Result<(), SimctlError> {
        // Multi-waypoint route requires Android emulator GPX/KML upload
        // via emu console `geo gpx`.
        Err(SimctlError::non_zero_exit("location_start", -1, "Android multi-waypoint route deferred to a future cycle"))
    }

    // === Permissions ===

    async fn set_permission(
        &self,
        serial: &str,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), SimctlError> {
        let Some(android_perm) = permission.to_android() else {
            // iOS-only permission on Android → no-op (cross-platform yaml
            // friendly; matches IosDeviceControl behavior for Storage on iOS).
            return Ok(());
        };
        match action {
            PermissionAction::Grant => self
                .client
                .pm_grant(serial, bundle_id, android_perm)
                .await
                .map_err(|e| adb_to_simctl_err(e, "shell pm grant")),
            PermissionAction::Revoke => self
                .client
                .pm_revoke(serial, bundle_id, android_perm)
                .await
                .map_err(|e| adb_to_simctl_err(e, "shell pm revoke")),
            PermissionAction::Reset => {
                // Android has no direct `pm reset <perm>` per-package; reset
                // = uninstall+reinstall OR `pm reset-permissions`. For
                // cross-platform yaml `permissions: { camera: unset }`
                // semantic, treat as revoke (closest match).
                self.client
                    .pm_revoke(serial, bundle_id, android_perm)
                    .await
                    .map_err(|e| adb_to_simctl_err(e, "shell pm revoke (Reset alias)"))
            }
        }
    }

    // === Recording ===

    async fn start_recording(&self, _serial: &str, _output_path: &Path) -> Result<(), SimctlError> {
        // `adb shell screenrecord /sdcard/sim.mp4` — runs on device,
        // max 3min default.
        Err(SimctlError::non_zero_exit("start_recording", -1, "Android screenrecord wiring deferred to a future cycle"))
    }

    async fn stop_recording(&self) -> Result<(), SimctlError> {
        Err(SimctlError::non_zero_exit("stop_recording", -1, "Android screenrecord wiring deferred to a future cycle"))
    }
}
