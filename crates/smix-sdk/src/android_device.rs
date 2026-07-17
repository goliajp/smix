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
use smix_simctl::DeviceControlError;

use crate::PermissionAction;
use crate::device_control::{DeviceControl, Permission};

/// What `simctl location start` uses when `--speed` is absent, per its own
/// help text. Mirrored so a flow that omits the speed travels at the same
/// rate on both platforms.
const SIMCTL_DEFAULT_SPEED_MPS: f64 = 20.0;

/// How often the waypoint loop posts a position. A GPS receiver reports at
/// 1 Hz, and the emulator's own `mFixInterval` is 1000ms, so a faster tick
/// would only queue fixes the framework reports at this rate anyway.
const GEO_FIX_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Great-circle distance in metres. Used only to turn a leg into a
/// duration, which is why the earth is a sphere here.
fn haversine_metres(from: (f64, f64), to: (f64, f64)) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let (lat1, lat2) = (from.0.to_radians(), to.0.to_radians());
    let d_lat = lat2 - lat1;
    let d_lon = (to.1 - from.1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().asin()
}

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

/// Android `DeviceControl`, over `adb`.
///
/// Where a capability has no Android primitive, the method says so and
/// names the alternative rather than succeeding quietly. `smix-adb`'s
/// errors are translated by `adb_to_simctl_err`.
pub struct AndroidDeviceControl {
    client: AdbClient,
    /// The `adb shell screenrecord` in flight, if any.
    recording: Mutex<Option<AndroidRecording>>,
    /// The waypoint loop in flight, if any. iOS hands the route to
    /// CoreSimulator and forgets it; here nothing on the device walks a
    /// route, so this process does the walking.
    travel: Mutex<Option<tokio::task::JoinHandle<()>>>,
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
            travel: Mutex::new(None),
        }
    }

    /// Construct with an existing `AdbClient` (tests / non-PATH adb).
    #[must_use]
    pub fn with_client(client: AdbClient) -> Self {
        AndroidDeviceControl {
            client,
            recording: Mutex::new(None),
            travel: Mutex::new(None),
        }
    }
}

/// Translate an adb failure into the shared device-control error, keeping
/// the Android detail in the message.
///
/// The subcommand is prefixed with `adb` so the message names the tool that
/// actually ran. `DeviceControlError` is shared by both platforms, and a reader
/// chasing an Android failure should not be pointed at Xcode.
fn adb_to_simctl_err(e: AdbError, subcommand: &str) -> DeviceControlError {
    let subcommand = &format!("adb {subcommand}");
    match e {
        AdbError::BinaryNotFound => DeviceControlError::non_zero_exit(
            subcommand,
            -1,
            "adb binary not found in PATH; install Android SDK platform-tools",
        ),
        AdbError::Spawn(io) => DeviceControlError::non_zero_exit(
            subcommand,
            -1,
            format!("adb spawn failed: {io}"),
        ),
        AdbError::NonZeroExit {
            subcommand: sub,
            code,
            stderr,
            serial,
        } => DeviceControlError::non_zero_exit(
            match serial {
                Some(s) => format!("adb -s {s} {sub}"),
                None => format!("adb {sub}"),
            },
            code,
            stderr,
        ),
        AdbError::Malformed {
            subcommand: sub,
            detail,
        } => DeviceControlError::Malformed {
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

    async fn launch(&self, serial: &str, bundle_id: &str) -> Result<u32, DeviceControlError> {
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
    ) -> Result<u32, DeviceControlError> {
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

    async fn terminate(&self, serial: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.client
            .force_stop(serial, bundle_id)
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell am force-stop"))
    }

    async fn install(&self, serial: &str, app_path: &str) -> Result<(), DeviceControlError> {
        self.client
            .install(serial, Path::new(app_path))
            .await
            .map_err(|e| adb_to_simctl_err(e, "install"))
    }

    async fn uninstall(&self, serial: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.client
            .uninstall(serial, bundle_id)
            .await
            .map_err(|e| adb_to_simctl_err(e, "uninstall"))
    }

    async fn keychain_reset(&self, _serial: &str) -> Result<(), DeviceControlError> {
        // This used to return Ok(()) and call itself a no-op "matching the
        // expectation that Android silently no-ops rather than crashing".
        // A flow that clears the keychain does it so the next step meets a
        // signed-out app; succeeding without doing it hands that step a
        // logged-in one and blames the step.
        Err(DeviceControlError::non_zero_exit(
            "keychain_reset",
            -1,
            "clearKeychain has no Android equivalent: credentials live in each app's own \
             KeyStore, which the host cannot reach. Use clearAppData to wipe the app's \
             state, or have the app expose a sign-out path.",
        ))
    }

    async fn privacy_reset_all(&self, serial: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        // `pm reset-permissions` reverts every app on the device, including
        // whatever the runner was granted to do its job, so the grants are
        // read back and dropped one at a time.
        let granted = self
            .client
            .runtime_permissions_granted(serial, bundle_id)
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell dumpsys package"))?;
        for permission in granted {
            self.client
                .pm_revoke(serial, bundle_id, &permission)
                .await
                .map_err(|e| adb_to_simctl_err(e, "shell pm revoke"))?;
        }
        Ok(())
    }

    async fn clear_app_sandbox(&self, serial: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        // App data is app-private, so the host's only way in is `pm clear`,
        // which also reverts the runtime permissions. iOS can wipe the
        // sandbox and leave privacy alone; here the two come together, and
        // the launch-fresh plan resets privacy anyway.
        self.client
            .pm_clear(serial, bundle_id)
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell pm clear"))
    }

    // === Lifecycle ancillary ===

    async fn open_url(&self, serial: &str, url: &str) -> Result<(), DeviceControlError> {
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
    ) -> Result<(), DeviceControlError> {
        // Android push = FCM not APNs. Cross-platform yaml `sendPush:` on
        // Android requires FCM credentials + Firebase project setup —
        // deferred. Surface an explicit error so yaml authors know it
        // is not silently no-op.
        Err(DeviceControlError::non_zero_exit("send_push", -1, "Android FCM push not implemented (cross-platform yaml `sendPush:` is iOS APNs only)"))
    }

    async fn screenshot(&self, serial: &str) -> Result<Vec<u8>, DeviceControlError> {
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
    async fn pasteboard_set(&self, _serial: &str, _text: &str) -> Result<(), DeviceControlError> {
        Err(DeviceControlError::non_zero_exit(
            "pasteboard_set",
            -1,
            "Android does not let a test runner write the clipboard: since Android 10 the \
             clipboard serves only the focused app, and the runner cannot be focused while \
             driving your app. Pass the text via inputText, or have the app expose it another way.",
        ))
    }

    async fn pasteboard_get(&self, _serial: &str) -> Result<String, DeviceControlError> {
        Err(DeviceControlError::non_zero_exit(
            "pasteboard_get",
            -1,
            "Android does not let a test runner read the clipboard: since Android 10 the \
             clipboard serves only the focused app, and the runner cannot be focused while \
             driving your app. Assert on what the app renders instead.",
        ))
    }

    async fn add_media(&self, serial: &str, paths: &[String]) -> Result<(), DeviceControlError> {
        if paths.is_empty() {
            return Err(DeviceControlError::Malformed {
                subcommand: "add_media".into(),
                detail: "no paths supplied".into(),
            });
        }
        for p in paths {
            let local = Path::new(p);
            let name = local
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| DeviceControlError::Malformed {
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

    async fn location_set(&self, serial: &str, lat: f64, lon: f64) -> Result<(), DeviceControlError> {
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
        serial: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        // The emulator console has no route: `geo` takes one position
        // (`fix`), an NMEA sentence, or a GNSS sentence — no waypoint list,
        // whatever a comment here once claimed about `geo gpx`. `automation
        // play` replays a macro, but only one the emulator itself recorded.
        //
        // So the interpolation simctl does inside CoreSimulator happens here
        // instead, one `geo fix` per tick. Same contract as iOS: the route
        // starts and the call returns, so the loop outlives it.
        if points.len() < 2 {
            return Err(DeviceControlError::Malformed {
                subcommand: "location_start".into(),
                detail: format!("requires ≥2 waypoints, got {}", points.len()),
            });
        }
        let speed = speed_mps.unwrap_or(SIMCTL_DEFAULT_SPEED_MPS);
        if !(speed.is_finite() && speed > 0.0) {
            return Err(DeviceControlError::Malformed {
                subcommand: "location_start".into(),
                detail: format!("speed must be finite and positive, got {speed}"),
            });
        }

        let route: Vec<(f64, f64)> = points.to_vec();
        let client = self.client.clone();
        let serial = serial.to_string();

        // Whoever asked last is where the device is going: stop the old walk
        // before starting the new one, or the two take turns pointing the
        // device at their own routes.
        let mut slot = self.travel.lock().await;
        if let Some(previous) = slot.take() {
            previous.abort();
        }
        let handle = tokio::spawn(async move {
            for pair in route.windows(2) {
                let (from, to) = (pair[0], pair[1]);
                let seconds = haversine_metres(from, to) / speed;
                let ticks = (seconds / GEO_FIX_INTERVAL.as_secs_f64()).round() as u64;
                for tick in 1..=ticks.max(1) {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "a route long enough to lose f64 precision here would \
                                  take longer to walk than the emulator stays up"
                    )]
                    let t = tick as f64 / ticks.max(1) as f64;
                    let lat = from.0 + (to.0 - from.0) * t;
                    let lon = from.1 + (to.1 - from.1) * t;
                    if client
                        .emu(&serial, &["geo", "fix", &lon.to_string(), &lat.to_string()])
                        .await
                        .is_err()
                    {
                        // The device is gone, or the console closed. There is
                        // no one to tell — the caller returned long ago.
                        return;
                    }
                    tokio::time::sleep(GEO_FIX_INTERVAL).await;
                }
            }
        });

        *slot = Some(handle);
        Ok(())
    }

    // === Permissions ===

    async fn set_permission(
        &self,
        serial: &str,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), DeviceControlError> {
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
    async fn start_recording(&self, serial: &str, output_path: &Path) -> Result<(), DeviceControlError> {
        let mut guard = self.recording.lock().await;
        if guard.is_some() {
            return Err(DeviceControlError::non_zero_exit(
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

    async fn stop_recording(&self) -> Result<(), DeviceControlError> {
        let mut guard = self.recording.lock().await;
        let mut rec = guard.take().ok_or_else(|| {
            DeviceControlError::non_zero_exit(
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Spans whose length follows from the sphere itself, so a wrong
    /// formula cannot agree with them by construction.
    #[test]
    fn haversine_matches_the_geometry() {
        let r = 6_371_000.0_f64;

        // A degree of longitude on the equator: one 360th of the great
        // circle.
        let d = haversine_metres((0.0, 0.0), (0.0, 1.0));
        assert!((d - r * std::f64::consts::PI / 180.0).abs() < 1.0, "{d}");

        // Equator to pole: a quarter of the way round.
        let d = haversine_metres((0.0, 0.0), (90.0, 0.0));
        assert!((d - r * std::f64::consts::FRAC_PI_2).abs() < 1.0, "{d}");

        // Antipodes: half. The far side is where a formula built on
        // sin(Δ/2) rather than cos(Δ) keeps its accuracy.
        let d = haversine_metres((0.0, 0.0), (0.0, 180.0));
        assert!((d - r * std::f64::consts::PI).abs() < 1.0, "{d}");

        // A meridian degree is a longitude degree on the equator, since
        // both are the same great circle.
        let d = haversine_metres((0.0, 0.0), (1.0, 0.0));
        assert!((d - r * std::f64::consts::PI / 180.0).abs() < 1.0, "{d}");
    }

    /// One published distance, to catch a formula that is self-consistent
    /// but not about the earth.
    #[test]
    fn haversine_matches_a_known_route() {
        // Paris to New York, published as 5837 km.
        let d = haversine_metres((48.856_614, 2.352_222), (40.712_776, -74.005_974));
        assert!((d - 5_837_000.0).abs() < 20_000.0, "{d}");
    }

    #[test]
    fn a_leg_of_no_length_takes_no_time() {
        let here = (35.681_236, 139.767_125);
        assert!(haversine_metres(here, here) < 0.001);
    }

    #[tokio::test]
    async fn a_route_needs_somewhere_to_go() {
        let device = AndroidDeviceControl::new();
        let err = device
            .location_start("emulator-5554", &[(35.0, 139.0)], None)
            .await
            .expect_err("one waypoint is not a route");
        assert!(err.to_string().contains("≥2 waypoints"), "{err}");
    }

    #[tokio::test]
    async fn a_route_walked_at_zero_speed_never_arrives() {
        let device = AndroidDeviceControl::new();
        let err = device
            .location_start("emulator-5554", &[(35.0, 139.0), (36.0, 140.0)], Some(0.0))
            .await
            .expect_err("zero speed would divide by zero and tick forever");
        assert!(err.to_string().contains("positive"), "{err}");
    }
}
