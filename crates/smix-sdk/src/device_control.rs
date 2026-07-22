//! `DeviceControl` trait: cross-platform sim/host control.
//!
//! Two-trait architecture: pairs with [`smix_driver::Driver`]
//! (sense+act).
//!
//! Methods on this trait wrap host-side simulator/emulator control
//! commands (`xcrun simctl` for iOS, `adb` for Android).
//! Sense+act methods (tap/find/etc) live on [`smix_driver::Driver`].

use async_trait::async_trait;
use smix_simctl::{DeviceControlError, SimctlClient, SimctlPermission};
use std::path::Path;

pub use crate::PermissionAction;

/// Platform-agnostic permission name used in [`DeviceControl::set_permission`]
/// and the cross-platform yaml `launchApp.permissions:` shape. Avoids
/// leaking iOS-specific `SimctlPermission` into the trait signature.
///
/// Naming follows iOS convention where present; Android-only permissions
/// (Storage, PostNotifications) have explicit variants. Cross-platform
/// permissions (Camera/Location/etc.) map both ways via `to_simctl`
/// and `to_android`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Permission {
    Camera,
    Microphone,
    PhotoLibrary,
    Location,
    LocationAlways,
    Notifications,
    Contacts,
    Calendar,
    Reminders,
    Bluetooth,
    Motion,
    Media,
    Health,
    /// iOS-only — FaceID / TouchID biometric prompt.
    FaceId,
    /// iOS-only — HomeKit accessory access.
    HomeKit,
    /// Android-only — storage / files (iOS-side returns `None` from
    /// `to_simctl`).
    Storage,
    /// Android-only POST_NOTIFICATIONS (API 33+). On iOS aliases to
    /// `Notifications` for cross-platform yaml convenience.
    PostNotifications,
}

impl Permission {
    /// Map to iOS `SimctlPermission`. Returns `None` for Android-only
    /// permissions (`Storage`).
    #[must_use]
    pub fn to_simctl(self) -> Option<SimctlPermission> {
        match self {
            Permission::Camera => Some(SimctlPermission::Camera),
            Permission::Microphone => Some(SimctlPermission::Microphone),
            Permission::PhotoLibrary => Some(SimctlPermission::Photos),
            Permission::Location => Some(SimctlPermission::Location),
            Permission::LocationAlways => Some(SimctlPermission::LocationAlways),
            Permission::Notifications | Permission::PostNotifications => {
                Some(SimctlPermission::Notifications)
            }
            Permission::Contacts => Some(SimctlPermission::Contacts),
            Permission::Calendar => Some(SimctlPermission::Calendar),
            Permission::Reminders => Some(SimctlPermission::Reminders),
            Permission::Bluetooth => Some(SimctlPermission::Bluetooth),
            Permission::Motion => Some(SimctlPermission::Motion),
            Permission::Media => Some(SimctlPermission::Media),
            Permission::Health => Some(SimctlPermission::Health),
            Permission::FaceId => Some(SimctlPermission::Faceid),
            Permission::HomeKit => Some(SimctlPermission::HomeKit),
            Permission::Storage => None,
        }
    }

    /// Reverse: map iOS `SimctlPermission` → `Permission`. Used by App
    /// back-compat shim accepting `SimctlPermission` arg.
    #[must_use]
    pub fn from_simctl(perm: SimctlPermission) -> Self {
        match perm {
            SimctlPermission::Camera => Permission::Camera,
            SimctlPermission::Microphone => Permission::Microphone,
            SimctlPermission::Photos => Permission::PhotoLibrary,
            SimctlPermission::Location => Permission::Location,
            SimctlPermission::LocationAlways => Permission::LocationAlways,
            SimctlPermission::Notifications => Permission::Notifications,
            SimctlPermission::Contacts => Permission::Contacts,
            SimctlPermission::Calendar => Permission::Calendar,
            SimctlPermission::Reminders => Permission::Reminders,
            SimctlPermission::Bluetooth => Permission::Bluetooth,
            SimctlPermission::Motion => Permission::Motion,
            SimctlPermission::Media => Permission::Media,
            SimctlPermission::Health => Permission::Health,
            SimctlPermission::Faceid => Permission::FaceId,
            SimctlPermission::HomeKit => Permission::HomeKit,
            SimctlPermission::AddressBook => Permission::Contacts,
        }
    }

    /// Map to Android `android.permission.X` string. Returns `None` for
    /// iOS-only permissions (`FaceId`, `HomeKit`). Wired by
    /// `AndroidDeviceControl`; the iOS impl ignores it.
    #[must_use]
    pub fn to_android(self) -> Option<&'static str> {
        match self {
            Permission::Camera => Some("android.permission.CAMERA"),
            Permission::Microphone => Some("android.permission.RECORD_AUDIO"),
            Permission::PhotoLibrary => Some("android.permission.READ_MEDIA_IMAGES"),
            Permission::Location => Some("android.permission.ACCESS_FINE_LOCATION"),
            Permission::LocationAlways => Some("android.permission.ACCESS_BACKGROUND_LOCATION"),
            Permission::Notifications | Permission::PostNotifications => {
                Some("android.permission.POST_NOTIFICATIONS")
            }
            Permission::Contacts => Some("android.permission.READ_CONTACTS"),
            Permission::Calendar => Some("android.permission.READ_CALENDAR"),
            Permission::Bluetooth => Some("android.permission.BLUETOOTH_CONNECT"),
            Permission::Motion => Some("android.permission.ACTIVITY_RECOGNITION"),
            Permission::Media => Some("android.permission.READ_MEDIA_AUDIO"),
            Permission::Storage => Some("android.permission.WRITE_EXTERNAL_STORAGE"),
            Permission::Reminders
            | Permission::Health
            | Permission::FaceId
            | Permission::HomeKit => None,
        }
    }
}

/// Sim/host control trait. The iOS impl wraps `xcrun simctl`; the
/// Android impl wraps `adb`.
///
/// Methods take `udid: &str` first (iOS terminology; Android maps this
/// to the device serial). All return `Result<_, DeviceControlError>` — the
/// Android impl wraps adb errors into the same enum.
#[async_trait]
pub trait DeviceControl: Send + Sync {
    /// Platform identifier. Returns `smix_driver::Platform`.
    fn platform(&self) -> smix_driver::Platform;

    /// iOS-only escape hatch: downcast to `&SimctlClient` for legacy
    /// `App::simctl()` API surface. Android impl returns `None`.
    fn as_ios_simctl(&self) -> Option<&SimctlClient> {
        None
    }

    // === Lifecycle ===

    async fn launch(&self, udid: &str, bundle_id: &str) -> Result<u32, DeviceControlError>;
    /// Launch with process arguments, and on Android with an explicit
    /// entry point.
    ///
    /// `activity` is `None` unless a flow's app config named one. The
    /// Android side resolved every launch to `<pkg>/.MainActivity`
    /// before this parameter existed, which is right for a scaffolded
    /// app and wrong for every AOSP one; `None` now means "ask the
    /// package manager" rather than "assume". iOS ignores it — a
    /// bundle id already names what to launch.
    async fn launch_with_args(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
        activity: Option<&str>,
    ) -> Result<u32, DeviceControlError>;
    async fn terminate(&self, udid: &str, bundle_id: &str) -> Result<(), DeviceControlError>;
    async fn install(&self, udid: &str, app_path: &str) -> Result<(), DeviceControlError>;
    async fn uninstall(&self, udid: &str, bundle_id: &str) -> Result<(), DeviceControlError>;
    async fn keychain_reset(&self, udid: &str) -> Result<(), DeviceControlError>;

    /// Push the device's animations as low as this platform allows,
    /// then read the settings back and refuse if they did not take.
    ///
    /// `quiet = true` is the default a run gets; `false` restores the
    /// device's own settings for `--animations`.
    ///
    /// How low differs by platform and the difference is not papered
    /// over. Android zeroes three scales, which really is off. iOS gets
    /// Reduce Motion, which is weaker: XCUITest runs in its own
    /// process, so `UIView.setAnimationsEnabled(false)` cannot reach
    /// the app under test, and Reduce Motion is the strongest lever
    /// smix can pull on its own. Saying "animations are off" would be
    /// false on one of the two platforms.
    ///
    /// Reading back is not belt-and-braces. `simctl ui appearance` is
    /// documented per-simulator and behaves globally; a setting written
    /// by smix is not believed until the device repeats it. A switch
    /// that reports success while the device kept animating is worse
    /// than no switch — the run that follows looks deterministic and is
    /// not.
    async fn set_animations_quiet(
        &self,
        _id: &str,
        _quiet: bool,
    ) -> Result<(), DeviceControlError> {
        Ok(())
    }

    /// Revoke every privacy permission the app has been granted.
    ///
    /// Companion to [`Self::clear_app_sandbox`]; together they are the
    /// in-place replacement for `launchApp: clearState: true`, which avoids
    /// uninstall-and-reinstall and the XCUITest binding loss that follows.
    ///
    /// Required, deliberately. This defaulted to `Ok(())` "so non-iOS
    /// device controls keep compiling", and the result was that
    /// `clearState: true` on Android reported success while clearing
    /// nothing — the planner emits this op whatever the platform. A device
    /// control that cannot do this has to say so out loud.
    async fn privacy_reset_all(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), DeviceControlError>;

    /// Wipe the app's persisted data without uninstalling it, so the
    /// test binding survives.
    ///
    /// Required for the same reason as [`Self::privacy_reset_all`].
    async fn clear_app_sandbox(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), DeviceControlError>;

    /// Delete a single key from the target app's persisted
    /// user-defaults / preferences store. iOS: `simctl spawn defaults
    /// delete <bundle> <key>` (NSUserDefaults via the sim's cfprefsd).
    /// Returns `Ok(true)` when the key existed, `Ok(false)` when
    /// already absent (both are the "ensure absent" target state).
    ///
    /// Default impl errors explicitly — Android SharedPreferences has
    /// no host-side per-key deletion path (files are app-private;
    /// `pm clear` is the whole-store hammer, which is `clearAppData`'s
    /// job, not this verb's). NOT a silent no-op: a consumer relying
    /// on the deletion for test correctness must hear that it didn't
    /// happen.
    async fn user_defaults_delete(
        &self,
        _udid: &str,
        _bundle_id: &str,
        _key: &str,
    ) -> Result<bool, DeviceControlError> {
        Err(DeviceControlError::non_zero_exit(
            "user-defaults-delete",
            1,
            "clearUserDefaults is not supported on this platform (iOS simulator only — \
             Android SharedPreferences has no host-side per-key deletion; use clearAppData \
             for a full store wipe)",
        ))
    }

    // === Lifecycle ancillary ===

    async fn open_url(&self, udid: &str, url: &str) -> Result<(), DeviceControlError>;
    async fn send_push(
        &self,
        udid: &str,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), DeviceControlError>;
    async fn screenshot(&self, udid: &str) -> Result<Vec<u8>, DeviceControlError>;

    // === Clipboard / Media / Location ===

    async fn pasteboard_set(&self, udid: &str, text: &str) -> Result<(), DeviceControlError>;
    async fn pasteboard_get(&self, udid: &str) -> Result<String, DeviceControlError>;
    async fn add_media(&self, udid: &str, paths: &[String]) -> Result<(), DeviceControlError>;
    async fn location_set(&self, udid: &str, lat: f64, lon: f64) -> Result<(), DeviceControlError>;
    async fn location_start(
        &self,
        udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError>;

    // === Permissions (cross-platform `Permission` enum) ===

    async fn set_permission(
        &self,
        udid: &str,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), DeviceControlError>;

    // === Recording (state owned internally by impl, see IosDeviceControl) ===

    async fn start_recording(
        &self,
        udid: &str,
        output_path: &Path,
    ) -> Result<(), DeviceControlError>;
    async fn stop_recording(&self) -> Result<(), DeviceControlError>;
}
