//! A device the user owns keeps its own state unless they said it may be
//! changed.
//!
//! Writing a phone's clipboard, simulating its location, adding to its
//! photo library or turning its animations off change things the person
//! is using that day — the clipboard reaches their other devices on the
//! same account — and every one outlives the test that did it. On a
//! simulator or emulator they are the test's to change. On a registered
//! phone they need the same one-time opt-in as erasing it:
//! `smix sim allow-destructive <device>`.

use std::path::Path;

use async_trait::async_trait;
use smix_simctl::{DeviceControlError, SimctlClient};

use crate::device_control::{CrashReport, DeviceControl, Frontmost, Permission, PermissionAction};

/// The actions that change what the device's user sees outside the app.
pub const USER_STATE_ACTIONS: [&str; 6] = [
    "set_animations_quiet",
    "pasteboard_set",
    "add_media",
    "location_set",
    "location_start",
    "location_clear",
];

/// A physical device's control, refusing [`USER_STATE_ACTIONS`] unless the
/// device was opted in. Everything else is passed through as it is.
pub struct UserStateGuard {
    inner: Box<dyn DeviceControl>,
    device: String,
    opted_in: bool,
}

impl UserStateGuard {
    /// Guard `inner`, which drives the physical device `device`.
    /// `opted_in` is whether the device's registry row allows destructive
    /// actions.
    #[must_use]
    pub fn new(inner: Box<dyn DeviceControl>, device: &str, opted_in: bool) -> Self {
        Self {
            inner,
            device: device.to_string(),
            opted_in,
        }
    }

    fn may_change(&self, action: &str) -> Result<(), DeviceControlError> {
        if self.opted_in {
            return Ok(());
        }
        Err(DeviceControlError::non_zero_exit(
            action,
            -1,
            format!(
                "{action} would change {device}'s own state — its clipboard, location, \
                 photo library or animations — which the person using it keeps after \
                 the test. A registered phone allows that only once it is opted in: \
                 smix sim allow-destructive {device}",
                device = self.device
            ),
        ))
    }
}

#[async_trait]
impl DeviceControl for UserStateGuard {
    fn platform(&self) -> smix_driver::Platform {
        self.inner.platform()
    }

    fn as_ios_simctl(&self) -> Option<&SimctlClient> {
        self.inner.as_ios_simctl()
    }

    async fn launch(&self, udid: &str, bundle_id: &str) -> Result<u32, DeviceControlError> {
        self.inner.launch(udid, bundle_id).await
    }

    async fn launch_with_args(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
        activity: Option<&str>,
    ) -> Result<u32, DeviceControlError> {
        self.inner
            .launch_with_args(udid, bundle_id, args, activity)
            .await
    }

    async fn terminate(&self, udid: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.inner.terminate(udid, bundle_id).await
    }

    async fn install(&self, udid: &str, app_path: &str) -> Result<(), DeviceControlError> {
        self.inner.install(udid, app_path).await
    }

    async fn uninstall(&self, udid: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.inner.uninstall(udid, bundle_id).await
    }

    async fn keychain_reset(&self, udid: &str) -> Result<(), DeviceControlError> {
        self.inner.keychain_reset(udid).await
    }

    async fn set_animations_quiet(&self, id: &str, quiet: bool) -> Result<(), DeviceControlError> {
        self.may_change("set_animations_quiet")?;
        self.inner.set_animations_quiet(id, quiet).await
    }

    async fn privacy_reset_all(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        self.inner.privacy_reset_all(udid, bundle_id).await
    }

    async fn clear_app_sandbox(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        self.inner.clear_app_sandbox(udid, bundle_id).await
    }

    async fn user_defaults_delete(
        &self,
        udid: &str,
        bundle_id: &str,
        key: &str,
    ) -> Result<bool, DeviceControlError> {
        self.inner.user_defaults_delete(udid, bundle_id, key).await
    }

    async fn open_url(&self, udid: &str, url: &str) -> Result<(), DeviceControlError> {
        self.inner.open_url(udid, url).await
    }

    async fn send_push(
        &self,
        udid: &str,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), DeviceControlError> {
        self.inner.send_push(udid, bundle_id, apns_json_path).await
    }

    async fn screenshot(&self, udid: &str) -> Result<Vec<u8>, DeviceControlError> {
        self.inner.screenshot(udid).await
    }

    async fn capture_bgra(
        &self,
        udid: &str,
    ) -> Result<smix_simctl::surface_capture::CapturedFrame, DeviceControlError> {
        self.inner.capture_bgra(udid).await
    }

    async fn pasteboard_set(&self, udid: &str, text: &str) -> Result<(), DeviceControlError> {
        self.may_change("pasteboard_set")?;
        self.inner.pasteboard_set(udid, text).await
    }

    async fn pasteboard_get(&self, udid: &str) -> Result<String, DeviceControlError> {
        self.inner.pasteboard_get(udid).await
    }

    async fn add_media(&self, udid: &str, paths: &[String]) -> Result<(), DeviceControlError> {
        self.may_change("add_media")?;
        self.inner.add_media(udid, paths).await
    }

    async fn location_set(&self, udid: &str, lat: f64, lon: f64) -> Result<(), DeviceControlError> {
        self.may_change("location_set")?;
        self.inner.location_set(udid, lat, lon).await
    }

    async fn location_start(
        &self,
        udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        self.may_change("location_start")?;
        self.inner.location_start(udid, points, speed_mps).await
    }

    async fn location_clear(&self, udid: &str) -> Result<(), DeviceControlError> {
        self.may_change("location_clear")?;
        self.inner.location_clear(udid).await
    }

    async fn set_permission(
        &self,
        udid: &str,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), DeviceControlError> {
        self.inner
            .set_permission(udid, bundle_id, permission, action)
            .await
    }

    async fn start_recording(
        &self,
        udid: &str,
        output_path: &Path,
    ) -> Result<(), DeviceControlError> {
        self.inner.start_recording(udid, output_path).await
    }

    async fn stop_recording(&self) -> Result<(), DeviceControlError> {
        self.inner.stop_recording().await
    }

    async fn recording_pid(&self) -> Option<u32> {
        self.inner.recording_pid().await
    }

    async fn reverse_port(
        &self,
        udid: &str,
        device_port: u16,
        host_port: u16,
    ) -> Result<(), DeviceControlError> {
        self.inner.reverse_port(udid, device_port, host_port).await
    }

    async fn reverse_port_remove(
        &self,
        udid: &str,
        device_port: u16,
    ) -> Result<(), DeviceControlError> {
        self.inner.reverse_port_remove(udid, device_port).await
    }

    async fn wake(&self, udid: &str) -> Result<(), DeviceControlError> {
        self.inner.wake(udid).await
    }

    async fn set_stay_awake(&self, udid: &str, on: bool) -> Result<(), DeviceControlError> {
        self.inner.set_stay_awake(udid, on).await
    }

    async fn frontmost_app(&self, udid: &str) -> Result<Option<Frontmost>, DeviceControlError> {
        self.inner.frontmost_app(udid).await
    }

    async fn crash_reports(&self, udid: &str) -> Result<Vec<CrashReport>, DeviceControlError> {
        self.inner.crash_reports(udid).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guarded(opted_in: bool) -> UserStateGuard {
        // Refused before the inner control is asked, so no device is reached.
        UserStateGuard::new(
            Box::new(crate::ios_device::IosDeviceControl::new()),
            "phone-1",
            opted_in,
        )
    }

    #[tokio::test]
    async fn a_phone_not_opted_in_keeps_its_clipboard_and_location() {
        let g = guarded(false);
        let e = g
            .pasteboard_set("phone-1", "x")
            .await
            .expect_err("written without an opt-in");
        assert!(
            e.to_string().contains("smix sim allow-destructive phone-1"),
            "{e}"
        );
        assert!(g.location_set("phone-1", 1.0, 2.0).await.is_err());
        assert!(g.location_clear("phone-1").await.is_err());
        assert!(g.add_media("phone-1", &["a.png".into()]).await.is_err());
    }

    #[test]
    fn the_guarded_actions_are_the_device_level_writes_in_the_action_table() {
        for a in USER_STATE_ACTIONS {
            assert_eq!(
                crate::device_control::action_level(a),
                Some(crate::device_control::ActionLevel::Device),
                "{a} is not a device-level action in the table"
            );
        }
    }

    // Every method of the trait is forwarded here. One that was not would
    // fall to the trait's default and answer for the phone without asking
    // it — or refuse what the phone can do.
    #[test]
    fn every_method_of_the_trait_is_forwarded() {
        let trait_src = include_str!("device_control.rs");
        let i = trait_src
            .find("pub trait DeviceControl")
            .expect("the trait");
        let j = trait_src[i..].find("\n}\n").expect("its end") + i;
        // rustfmt breaks a long call after `self.inner`; read it unbroken.
        let ours: String = include_str!("user_state.rs")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let mut missing = Vec::new();
        for line in trait_src[i..j].lines() {
            let t = line.trim_start();
            let name = t
                .strip_prefix("async fn ")
                .or_else(|| t.strip_prefix("fn "))
                .and_then(|r| r.split(['(', '<']).next());
            if let Some(name) = name
                && !ours.contains(&format!("self.inner.{name}("))
            {
                missing.push(name.to_string());
            }
        }
        assert!(missing.is_empty(), "not forwarded: {missing:?}");
    }
}
