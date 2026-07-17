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
/// An `adb shell screenrecord` in flight.
///
/// `stop_recording` takes no serial and no path — the trait shape assumes the
/// implementation remembers. iOS remembers a child process; here the child is
/// on the *device*, writing to the device's filesystem, so stopping also means
/// knowing where to pull the file from and where the caller wanted it.
struct AndroidRecording {
    /// The local `adb` child. Killing it sends SIGINT down to screenrecord,
    /// which is what makes it flush a playable mp4.
    child: tokio::process::Child,
    serial: String,
    /// Where screenrecord is writing, on the device.
    remote_path: String,
    /// Where the caller asked for the file, on this machine.
    local_path: std::path::PathBuf,
}

pub struct AndroidDeviceControl {
    client: AdbClient,
    /// The `adb shell screenrecord` in flight, if any.
    recording: Mutex<Option<AndroidRecording>>,
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
/// Translate an adb failure into the shared device-control error.
///
/// The subcommand is prefixed with `adb` so the message names the tool that
/// actually ran. `SimctlError` is shared by both platforms, and a reader
/// chasing an Android failure should not be pointed at Xcode.
fn adb_to_simctl_err(e: AdbError, subcommand: &str) -> SimctlError {
    let subcommand = &format!("adb {subcommand}");
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

    // The clipboard is out of reach on Android, and not for want of wiring.
    //
    // Since Android 10, ClipboardService serves only the focused app.
    // Measured on SDK 33, from the runner's own instrumentation process:
    //
    //   E ClipboardService: Denying clipboard access to dev.smix.runner.test,
    //   application is not in focus nor is it a system service for user 0
    //
    // The runner can never satisfy that. Being focused would make it the
    // foreground app, and then it could not drive the app under test — which
    // is the entire job. `appops set … READ_CLIPBOARD allow` does not lift it;
    // the check is on focus, not on an app-op. adb is no better: `cmd
    // clipboard` has no shell implementation on SDK 33, and shell is not
    // focused either.
    //
    // iOS has no equivalent problem because `simctl pasteboard` is a host
    // privilege the simulator grants from outside the device. Android's
    // emulator offers nothing like it.
    //
    // So this is a platform limit, and the honest thing is to say so rather
    // than keep a skeleton that reads as unfinished work.
    async fn pasteboard_set(&self, _serial: &str, _text: &str) -> Result<(), SimctlError> {
        Err(SimctlError::non_zero_exit(
            "pasteboard_set",
            -1,
            "Android does not let a test runner write the clipboard: since Android 10 the \
             clipboard serves only the focused app, and the runner cannot be focused while \
             driving your app. Pass the text via inputText, or have the app expose it another way.",
        ))
    }

    async fn pasteboard_get(&self, _serial: &str) -> Result<String, SimctlError> {
        Err(SimctlError::non_zero_exit(
            "pasteboard_get",
            -1,
            "Android does not let a test runner read the clipboard: since Android 10 the \
             clipboard serves only the focused app, and the runner cannot be focused while \
             driving your app. Assert on what the app renders instead.",
        ))
    }

    async fn add_media(&self, serial: &str, paths: &[String]) -> Result<(), SimctlError> {
        if paths.is_empty() {
            return Err(SimctlError::Malformed {
                subcommand: "add_media".into(),
                detail: "no paths supplied".into(),
            });
        }
        for p in paths {
            let local = Path::new(p);
            let name = local
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| SimctlError::Malformed {
                    subcommand: "add_media".into(),
                    detail: format!("path has no file name: {p}"),
                })?;
            let remote = format!("/sdcard/Pictures/{name}");
            self.client
                .push(serial, local, &remote)
                .await
                .map_err(|e| adb_to_simctl_err(e, "push"))?;
            // Landing the bytes is not enough: MediaStore indexes the gallery,
            // and an unindexed file is invisible to the app under test. The
            // scan broadcast is what makes it show up.
            self.client
                .broadcast(
                    serial,
                    "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
                    Some(&format!("file://{remote}")),
                )
                .await
                .map_err(|e| adb_to_simctl_err(e, "media scan"))?;
        }
        Ok(())
    }

    async fn location_set(&self, serial: &str, lat: f64, lon: f64) -> Result<(), SimctlError> {
        // The emulator console, not the device shell. This called
        // `shell(["emu", …])` and so ran `adb shell emu geo fix`, which asks
        // the device for a program named `emu` — `sh: emu: inaccessible or
        // not found`, exit 127. The verb never worked. Note the argument
        // order: longitude first.
        self.client
            .emu(serial, &["geo", "fix", &lon.to_string(), &lat.to_string()])
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

    /// Start `adb shell screenrecord`.
    ///
    /// Unlike iOS, the recorder runs on the device and writes there, so the
    /// file only reaches `output_path` when [`Self::stop_recording`] pulls it.
    ///
    /// Android caps this at **180 seconds** — `screenrecord --time-limit`
    /// documents 180 as "Default / maximum", a ceiling rather than a default
    /// that can be raised. `simctl recordVideo` has no such limit. Past the
    /// cap the recorder exits on its own, and stop_recording pulls whatever
    /// it managed to write.
    async fn start_recording(&self, serial: &str, output_path: &Path) -> Result<(), SimctlError> {
        let mut guard = self.recording.lock().await;
        if guard.is_some() {
            return Err(SimctlError::non_zero_exit(
                "screenrecord",
                -1,
                "a recording is already in progress (call stop_recording first)",
            ));
        }
        let name = output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("smix-recording.mp4");
        let remote_path = format!("/sdcard/{name}");
        // A leftover from a previous run would be pulled instead of this one
        // if screenrecord failed to start.
        let _ = self.client.shell(serial, &["rm", "-f", &remote_path]).await;
        let child = self
            .client
            .spawn_shell(serial, &["screenrecord", &remote_path])
            .map_err(|e| adb_to_simctl_err(e, "screenrecord"))?;
        *guard = Some(AndroidRecording {
            child,
            serial: serial.to_string(),
            remote_path,
            local_path: output_path.to_path_buf(),
        });
        Ok(())
    }

    async fn stop_recording(&self) -> Result<(), SimctlError> {
        let mut guard = self.recording.lock().await;
        let mut rec = guard.take().ok_or_else(|| {
            SimctlError::non_zero_exit(
                "screenrecord",
                -1,
                "no recording in progress (call start_recording first)",
            )
        })?;

        // SIGINT, not kill: screenrecord writes the mp4's moov atom on
        // interrupt, and without it the file is unplayable. Same reason
        // simctl's recordVideo stop signals rather than terminates.
        if let Some(pid) = rec.child.id() {
            // SAFETY: a thin POSIX syscall wrapper. The pid belongs to this
            // Child, so it cannot have been recycled, and SIGINT is
            // signal-safe.
            unsafe { libc::kill(pid as i32, libc::SIGINT) };
        }
        let _ = rec.child.wait().await;
        // The device-side recorder receives the signal through adb and needs
        // a moment to finish writing before the file is worth pulling.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        self.client
            .pull(&rec.serial, &rec.remote_path, &rec.local_path)
            .await
            .map_err(|e| adb_to_simctl_err(e, "pull recording"))?;
        // Best-effort: a leftover on the device must not fail the stop.
        let _ = self
            .client
            .shell(&rec.serial, &["rm", "-f", &rec.remote_path])
            .await;
        Ok(())
    }
}
