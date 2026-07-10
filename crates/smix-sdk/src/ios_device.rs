//! v6.0 c1b — iOS `DeviceControl` impl backed by `SimctlClient`.
//!
//! Owns the active recording handle internally (`Mutex<Option<RecordingHandle>>`)
//! so the trait surface stays platform-agnostic. App no longer holds
//! recording state — delegates to `self.device.start_recording / stop_recording`.

use async_trait::async_trait;
use std::path::Path;
use tokio::sync::Mutex;

use smix_driver::Platform;
use smix_simctl::{RecordingHandle, SimctlClient, SimctlError};

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

    async fn launch(&self, udid: &str, bundle_id: &str) -> Result<u32, SimctlError> {
        self.client.launch(udid, bundle_id).await.map(|res| res.pid)
    }

    async fn launch_with_args(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
    ) -> Result<u32, SimctlError> {
        self.client
            .launch_with_args(udid, bundle_id, args)
            .await
            .map(|res| res.pid)
    }

    async fn terminate(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        self.client.terminate(udid, bundle_id).await
    }

    async fn install(&self, udid: &str, app_path: &str) -> Result<(), SimctlError> {
        self.client.install(udid, app_path).await
    }

    async fn uninstall(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        self.client.uninstall(udid, bundle_id).await
    }

    async fn keychain_reset(&self, udid: &str) -> Result<(), SimctlError> {
        self.client.keychain_reset(udid).await
    }

    /// v1.0.4 §D12 — reset all privacy grants for the bundle via
    /// `simctl privacy <udid> reset all <bundle-id>`.
    async fn privacy_reset_all(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        self.client.privacy_reset_all(udid, bundle_id).await
    }

    /// v1.0.4 §D12 — wipe the app's sandbox (`Documents/`, `Library/`,
    /// `tmp/`) via `simctl spawn <udid> rm -rf` under the app's
    /// Containers/Data path. Preserves XCUITest binding.
    async fn clear_app_sandbox(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        self.client.clear_app_sandbox(udid, bundle_id).await
    }

    // === Lifecycle ancillary ===

    async fn open_url(&self, udid: &str, url: &str) -> Result<(), SimctlError> {
        self.client.open_url(udid, url).await
    }

    async fn send_push(
        &self,
        udid: &str,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), SimctlError> {
        self.client.send_push(udid, bundle_id, apns_json_path).await
    }

    async fn screenshot(&self, udid: &str) -> Result<Vec<u8>, SimctlError> {
        self.client.screenshot(udid).await
    }

    // === Clipboard / Media / Location ===

    async fn pasteboard_set(&self, udid: &str, text: &str) -> Result<(), SimctlError> {
        self.client.pasteboard_set(udid, text).await
    }

    async fn pasteboard_get(&self, udid: &str) -> Result<String, SimctlError> {
        self.client.pasteboard_get(udid).await
    }

    async fn add_media(&self, udid: &str, paths: &[String]) -> Result<(), SimctlError> {
        self.client.add_media(udid, paths).await
    }

    async fn location_set(&self, udid: &str, lat: f64, lon: f64) -> Result<(), SimctlError> {
        self.client.location_set(udid, lat, lon).await
    }

    async fn location_start(
        &self,
        udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), SimctlError> {
        self.client.location_start(udid, points, speed_mps).await
    }

    // === Permissions ===

    async fn set_permission(
        &self,
        udid: &str,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), SimctlError> {
        let Some(simctl_perm) = permission.to_simctl() else {
            // Android-only permission on iOS → no-op (don't fail; matches
            // cross-platform yaml expectation that `Storage` on iOS does
            // nothing rather than crashes the test).
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

    async fn start_recording(&self, udid: &str, output_path: &Path) -> Result<(), SimctlError> {
        let mut guard = self.recording.lock().await;
        if guard.is_some() {
            // Caller must stop the existing one first; surface via SimctlError
            // (App layer will translate to ExpectationFailure with hint per §13).
            return Err(SimctlError::NonZeroExit {
                subcommand: "io recordVideo".to_string(),
                code: -1,
                stderr: "a recording is already in progress (call stop_recording first)"
                    .to_string(),
            });
        }
        let path_str = output_path.to_string_lossy();
        let handle = self.client.record_video_start(udid, &path_str).await?;
        *guard = Some(handle);
        Ok(())
    }

    async fn stop_recording(&self) -> Result<(), SimctlError> {
        let mut guard = self.recording.lock().await;
        let handle = guard.take().ok_or_else(|| SimctlError::NonZeroExit {
            subcommand: "io recordVideo".to_string(),
            code: -1,
            stderr: "no recording in progress (call start_recording first)".to_string(),
        })?;
        self.client.record_video_stop(handle).await
    }
}
