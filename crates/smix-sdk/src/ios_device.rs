//! iOS `DeviceControl` impl backed by `SimctlClient`.
//!
//! Owns the active recording handle internally (`Mutex<Option<RecordingHandle>>`)
//! so the trait surface stays platform-agnostic. App no longer holds
//! recording state — delegates to `self.device.start_recording / stop_recording`.

use async_trait::async_trait;
use std::path::Path;
use tokio::sync::Mutex;

use smix_driver::Platform;
use smix_simctl::{DeviceControlError, RecordingHandle, SimctlClient};

use crate::PermissionAction;
use crate::device_control::{DeviceControl, Permission};

/// iOS `DeviceControl` impl. Wraps `SimctlClient` + owns active
/// recording handle. Constructed by `App::new` / `App::connect_to_runner`
/// when targeting iOS.
pub struct IosDeviceControl {
    client: SimctlClient,
    recording: Mutex<Option<RecordingHandle>>,
}

impl Default for IosDeviceControl {
    fn default() -> Self {
        Self::new()
    }
}

impl IosDeviceControl {
    #[must_use]
    pub fn new() -> Self {
        IosDeviceControl {
            client: SimctlClient::new(),
            recording: Mutex::new(None),
        }
    }

    /// Construct with an existing `SimctlClient` (used by
    /// `App::new(driver, simctl)` back-compat constructor).
    #[must_use]
    pub fn with_client(client: SimctlClient) -> Self {
        IosDeviceControl {
            client,
            recording: Mutex::new(None),
        }
    }

    /// Direct accessor for tests / iOS-only callers that need raw
    /// simctl access beyond the trait surface.
    #[must_use]
    pub fn simctl(&self) -> &SimctlClient {
        &self.client
    }
}

#[async_trait]
impl DeviceControl for IosDeviceControl {
    fn platform(&self) -> Platform {
        Platform::Ios
    }

    fn as_ios_simctl(&self) -> Option<&SimctlClient> {
        Some(&self.client)
    }

    // === Lifecycle ===

    async fn launch(&self, udid: &str, bundle_id: &str) -> Result<u32, DeviceControlError> {
        self.client.launch(udid, bundle_id).await.map(|res| res.pid)
    }

    /// `activity` is Android's; a bundle id is already an entry point.
    async fn launch_with_args(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
        _activity: Option<&str>,
    ) -> Result<u32, DeviceControlError> {
        self.client
            .launch_with_args(udid, bundle_id, args)
            .await
            .map(|res| res.pid)
    }

    async fn terminate(&self, udid: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.client.terminate(udid, bundle_id).await
    }

    async fn install(&self, udid: &str, app_path: &str) -> Result<(), DeviceControlError> {
        self.client.install(udid, app_path).await
    }

    async fn uninstall(&self, udid: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.client.uninstall(udid, bundle_id).await
    }

    /// Nothing, on iOS, and the docs say so rather than implying
    /// otherwise.
    ///
    /// There is no host-side lever. Measured on iOS 26.5:
    ///
    /// * `simctl ui <device>` offers `appearance`, `increase_contrast`
    ///   and `content_size` — no motion option.
    /// * `simctl spawn <device> defaults write <any domain> …` answers
    ///   `Could not write domain …; exiting`. Not a permissions detail
    ///   of one domain: `com.apple.UIKit` and `com.apple.Accessibility`
    ///   both refuse.
    /// * XCUITest runs in its own process, so
    ///   `UIView.setAnimationsEnabled(false)` cannot reach the app.
    ///
    /// This was written as "Reduce Motion, the strongest lever smix can
    /// pull on its own" and shipped without a device behind it. The
    /// first run against one aborted, because `set_reduce_motion` had
    /// passed `-bool 1` since the day it was written — `defaults` takes
    /// `true`/`false` and answers anything else with its usage text and
    /// exit 255 — and it had no callers, so nothing had ever run it.
    ///
    /// Returning `Ok` rather than an error is deliberate: a platform
    /// that cannot be quietened is not a failed run. What is not
    /// deliberate is pretending it was quietened, which is why this
    /// says so here, in the guide, and in the changelog.
    async fn set_animations_quiet(
        &self,
        _udid: &str,
        _quiet: bool,
    ) -> Result<(), DeviceControlError> {
        Ok(())
    }

    async fn keychain_reset(&self, udid: &str) -> Result<(), DeviceControlError> {
        self.client.keychain_reset(udid).await
    }

    /// Reset all privacy grants for the bundle via
    /// `simctl privacy <udid> reset all <bundle-id>`.
    async fn privacy_reset_all(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        self.client.privacy_reset_all(udid, bundle_id).await
    }

    /// Wipe the app's sandbox (`Documents/`, `Library/`,
    /// `tmp/`) via `simctl spawn <udid> rm -rf` under the app's
    /// Containers/Data path. Preserves XCUITest binding.
    async fn clear_app_sandbox(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        self.client.clear_app_sandbox(udid, bundle_id).await
    }

    /// Per-key NSUserDefaults deletion via
    /// `simctl spawn defaults delete`. See [`SimctlClient::user_defaults_delete`].
    async fn user_defaults_delete(
        &self,
        udid: &str,
        bundle_id: &str,
        key: &str,
    ) -> Result<bool, DeviceControlError> {
        self.client.user_defaults_delete(udid, bundle_id, key).await
    }

    // === Lifecycle ancillary ===

    async fn open_url(&self, udid: &str, url: &str) -> Result<(), DeviceControlError> {
        self.client.open_url(udid, url).await
    }

    async fn send_push(
        &self,
        udid: &str,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), DeviceControlError> {
        self.client.send_push(udid, bundle_id, apns_json_path).await
    }

    async fn screenshot(&self, udid: &str) -> Result<Vec<u8>, DeviceControlError> {
        self.client.screenshot(udid).await
    }

    async fn capture_bgra(
        &self,
        udid: &str,
    ) -> Result<smix_simctl::surface_capture::CapturedFrame, DeviceControlError> {
        self.client.capture_bgra(udid).await
    }

    // === Clipboard / Media / Location ===

    async fn pasteboard_set(&self, udid: &str, text: &str) -> Result<(), DeviceControlError> {
        self.client.pasteboard_set(udid, text).await
    }

    async fn pasteboard_get(&self, udid: &str) -> Result<String, DeviceControlError> {
        self.client.pasteboard_get(udid).await
    }

    async fn add_media(&self, udid: &str, paths: &[String]) -> Result<(), DeviceControlError> {
        self.client.add_media(udid, paths).await
    }

    async fn location_set(&self, udid: &str, lat: f64, lon: f64) -> Result<(), DeviceControlError> {
        self.client.location_set(udid, lat, lon).await
    }

    async fn location_start(
        &self,
        udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        self.client.location_start(udid, points, speed_mps).await
    }

    // === Permissions ===

    async fn set_permission(
        &self,
        udid: &str,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), DeviceControlError> {
        let Some(simctl_perm) = permission.to_simctl() else {
            // Android-only permission on iOS → no-op by design (a
            // cross-platform yaml's `Storage` should not crash an iOS
            // run) — but SAY so: this used to skip in total silence.
            eprintln!(
                "setPermissions: {permission:?} has no iOS mapping — skipped ({action:?} not applied)"
            );
            return Ok(());
        };
        match action {
            PermissionAction::Grant => {
                self.client
                    .grant_permission(udid, simctl_perm, bundle_id)
                    .await
            }
            PermissionAction::Revoke => {
                self.client
                    .revoke_permission(udid, simctl_perm, bundle_id)
                    .await
            }
            PermissionAction::Reset => {
                self.client
                    .reset_permission(udid, simctl_perm, bundle_id)
                    .await
            }
        }
    }

    // === Recording (state owned internally) ===

    async fn start_recording(
        &self,
        udid: &str,
        output_path: &Path,
    ) -> Result<(), DeviceControlError> {
        let mut guard = self.recording.lock().await;
        if guard.is_some() {
            // Caller must stop the existing one first; surface via DeviceControlError
            // (the App layer translates this to an ExpectationFailure with a hint).
            return Err(DeviceControlError::non_zero_exit(
                "io recordVideo",
                -1,
                "a recording is already in progress (call stop_recording first)",
            ));
        }
        let path_str = output_path.to_string_lossy();
        let handle = self.client.record_video_start(udid, &path_str).await?;
        *guard = Some(handle);
        Ok(())
    }

    async fn stop_recording(&self) -> Result<(), DeviceControlError> {
        let mut guard = self.recording.lock().await;
        let handle = guard.take().ok_or_else(|| {
            DeviceControlError::non_zero_exit(
                "io recordVideo",
                -1,
                "no recording in progress (call start_recording first)",
            )
        })?;
        self.client.record_video_stop(handle).await
    }
}
