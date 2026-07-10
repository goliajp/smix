//! v6.0 c1b — `DeviceControl` trait: cross-platform sim/host control.
//!
//! Per docs/plan-cold/v6-cross-platform-yaml-design.md §4.2 + §7
//! (audit-revised 2026-06-23). Two-trait architecture: pair with
//! [`smix_driver::Driver`] (sense+act).
//!
//! Methods on this trait wrap host-side simulator/emulator control
//! commands (`xcrun simctl` for iOS, `adb` for Android in v6.0 c2).
//! Sense+act methods (tap/find/etc) live on [`smix_driver::Driver`].

use async_trait::async_trait;
use smix_simctl::{SimctlClient, SimctlError, SimctlPermission};
use std::path::Path;

pub use crate::PermissionAction;

/// Platform-agnostic permission name used in [`DeviceControl::set_permission`]
/// and the cross-platform yaml `launchApp.permissions:` shape. Avoids
/// leaking iOS-specific `SimctlPermission` into the trait signature.
///
/// Naming follows iOS convention where present; Android-only permissions
/// (Storage, PostNotifications) have explicit variants. Cross-platform
/// permissions (Camera/Location/etc.) map both ways via `to_simctl` and
/// (v6.0 c2) `to_android`.
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
    /// `AndroidDeviceControl` in v6.0 c2; iOS impl ignores.
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

/// Sim/host control trait. iOS impl wraps `xcrun simctl`; Android impl
/// (v6.0 c2) wraps `adb`.
///
/// Methods take `udid: &str` first (iOS terminology; Android maps to
/// device serial). All return `Result<_, SimctlError>` for v6.0 c1b —
/// Android impl in v6.0 c2 wraps adb errors into the same enum (or
/// a new `DeviceError` introduced if needed).
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

    async fn launch(&self, udid: &str, bundle_id: &str) -> Result<u32, SimctlError>;
    async fn launch_with_args(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
    ) -> Result<u32, SimctlError>;
    async fn terminate(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError>;
    async fn install(&self, udid: &str, app_path: &str) -> Result<(), SimctlError>;
    async fn uninstall(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError>;
    async fn keychain_reset(&self, udid: &str) -> Result<(), SimctlError>;

    /// v1.0.4 §D12 — reset all granted privacy permissions for a
    /// bundle. Companion to `clear_app_sandbox`; together they form
    /// the in-place `launchApp: clearState: true` replacement that
    /// avoids `simctl uninstall + install` and its downstream
    /// XCUITest binding loss (feedback §F) + ReportCrash dialog
    /// (feedback §H). Default impl no-ops so non-iOS device controls
    /// (Android) keep compiling; iOS override supplies real behavior.
    async fn privacy_reset_all(&self, _udid: &str, _bundle_id: &str) -> Result<(), SimctlError> {
        Ok(())
    }

    /// v1.0.4 §D12 — wipe an app's `Documents/`, `Library/`, `tmp/`
    /// directories in the sim's Containers/Data root, without
    /// uninstalling the app. Preserves the XCUITest binding.
    /// Default impl no-ops; iOS override does the real wipe.
    async fn clear_app_sandbox(&self, _udid: &str, _bundle_id: &str) -> Result<(), SimctlError> {
        Ok(())
    }

    // === Lifecycle ancillary ===

    async fn open_url(&self, udid: &str, url: &str) -> Result<(), SimctlError>;
    async fn send_push(
        &self,
        udid: &str,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), SimctlError>;
    async fn screenshot(&self, udid: &str) -> Result<Vec<u8>, SimctlError>;

    // === Clipboard / Media / Location ===

    async fn pasteboard_set(&self, udid: &str, text: &str) -> Result<(), SimctlError>;
    async fn pasteboard_get(&self, udid: &str) -> Result<String, SimctlError>;
    async fn add_media(&self, udid: &str, paths: &[String]) -> Result<(), SimctlError>;
    async fn location_set(&self, udid: &str, lat: f64, lon: f64) -> Result<(), SimctlError>;
    async fn location_start(
        &self,
        udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), SimctlError>;

    // === Permissions (cross-platform `Permission` enum) ===

    async fn set_permission(
        &self,
        udid: &str,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), SimctlError>;

    // === Recording (state owned internally by impl, see IosDeviceControl) ===

    async fn start_recording(&self, udid: &str, output_path: &Path) -> Result<(), SimctlError>;
    async fn stop_recording(&self) -> Result<(), SimctlError>;
}
