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

/// What a device action can do, and therefore what it takes to be allowed
/// to do it.
///
/// Before this existed, `screenshot` and `keychain_reset` were the same
/// kind of thing: two methods on one trait, either callable by anyone
/// holding it. On a simulator that is merely untidy. On a physical device
/// — which is where this project is headed — the difference between those
/// two is the difference between a picture and somebody's data.
///
/// The level is not a comment. [`ACTION_LEVELS`] is checked against the
/// trait itself, so a method added without a level fails the build's
/// tests rather than quietly defaulting to harmless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionLevel {
    /// Reads the device, changes nothing.
    Observe,
    /// Touches the app under test and nothing wider.
    App,
    /// Changes the device's own state — outside the app, and outside what
    /// a test is nominally about.
    Device,
    /// Irreversible, or wider than the app: data goes away, or the whole
    /// device is affected.
    Destructive,
}

/// Every [`DeviceControl`] method, and what it is allowed to do.
///
/// Kept beside the trait deliberately. A table in a document drifts from
/// the code silently; this one is compared against the trait's own source
/// by a test, in both directions.
pub const ACTION_LEVELS: &[(&str, ActionLevel)] = &[
    // Metadata about the binding, not an action on the device.
    ("platform", ActionLevel::Observe),
    ("as_ios_simctl", ActionLevel::Observe),
    // The app under test.
    ("launch", ActionLevel::App),
    ("launch_with_args", ActionLevel::App),
    ("terminate", ActionLevel::App),
    ("install", ActionLevel::App),
    ("open_url", ActionLevel::App),
    ("send_push", ActionLevel::App),
    ("set_permission", ActionLevel::App),
    // Reads.
    ("screenshot", ActionLevel::Observe),
    ("capture_bgra", ActionLevel::Observe),
    ("pasteboard_get", ActionLevel::Observe),
    // The device's own state. None of these are about the app, and all of
    // them outlive the test that set them.
    ("set_animations_quiet", ActionLevel::Device),
    ("pasteboard_set", ActionLevel::Device),
    ("add_media", ActionLevel::Device),
    ("location_set", ActionLevel::Device),
    ("location_start", ActionLevel::Device),
    ("start_recording", ActionLevel::Device),
    ("stop_recording", ActionLevel::Device),
    ("recording_pid", ActionLevel::Observe),
    // Data goes away. `uninstall` takes the app's container with it;
    // `keychain_reset` is device-wide, not app-scoped.
    ("uninstall", ActionLevel::Destructive),
    ("keychain_reset", ActionLevel::Destructive),
    ("privacy_reset_all", ActionLevel::Destructive),
    ("clear_app_sandbox", ActionLevel::Destructive),
    ("user_defaults_delete", ActionLevel::Destructive),
];

/// Look up a method's level.
pub fn action_level(method: &str) -> Option<ActionLevel> {
    ACTION_LEVELS
        .iter()
        .find(|(m, _)| *m == method)
        .map(|(_, l)| *l)
}

/// Sim/host control trait. The iOS impl wraps `xcrun simctl`; the
/// Android impl wraps `adb`.
///
/// Methods take `udid: &str` first (iOS terminology; Android maps this
/// to the device serial). All return `Result<_, DeviceControlError>` — the
/// Android impl wraps adb errors into the same enum.
///
/// # Levels
///
/// Every method here is classified in [`ACTION_LEVELS`], and the two
/// heavier classes have a gated counterpart on
/// [`crate::leased::Leased`], which can only be obtained by taking the
/// device's lease:
///
/// - `Device` and `Destructive` methods change the device outside the app
///   under test, or take data away. Call them through `Leased` so a
///   second process cannot do it to a device you are using, and so an
///   abandoned session is settled before yours begins.
/// - `Observe` and `App` methods need no lease.
///
/// The heavier methods remain callable here because they are published
/// API and removing them is a major-version change. New call sites should
/// go through `Leased`.
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
    /// over. Android zeroes three scales, which really is off. **iOS
    /// does nothing**, because nothing on the host can: `simctl ui` has
    /// no motion option, `simctl spawn … defaults write` cannot write
    /// any domain, and XCUITest runs in its own process so
    /// `UIView.setAnimationsEnabled(false)` cannot reach the app. This
    /// interface first claimed iOS got Reduce Motion; a device said
    /// otherwise.
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

    /// Capture a frame preferring the fast raw-BGRA path (iOS: resident
    /// IOSurface host, ~0.3 ms, skips the PNG encode+decode round-trip for
    /// diff-loop consumers). The default impl wraps [`screenshot`](Self::screenshot)
    /// as a PNG frame, so backends without a direct path (Android) keep
    /// working unchanged.
    ///
    /// Since smix 2.0.0.
    async fn capture_bgra(
        &self,
        udid: &str,
    ) -> Result<smix_simctl::surface_capture::CapturedFrame, DeviceControlError> {
        self.screenshot(udid)
            .await
            .map(smix_simctl::surface_capture::CapturedFrame::Png)
    }

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

    /// The `simctl io … recordVideo` child this impl is holding, if any.
    ///
    /// Reads state this impl already owns; it starts nothing and stops
    /// nothing. It exists so a caller that must write the recording into
    /// a ledger can name the process it will later have to signal —
    /// without that, the only record of a running recording is a struct
    /// that dies with this process.
    ///
    /// Default `None`: an impl with no recording state has nothing to
    /// report, which is different from having a recording it declines to
    /// name.
    async fn recording_pid(&self) -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod action_level_tests {
    use super::*;

    /// The trait's method names, read out of this file's own source.
    ///
    /// Reading the source rather than listing them again is the point: a
    /// second hand-written list is a second thing to forget to update,
    /// and the failure mode of forgetting is a device action nobody
    /// classified — which, once admission is enforced, means an action
    /// that slipped in without anyone deciding what it costs.
    fn trait_methods() -> Vec<String> {
        let src = include_str!("device_control.rs");
        let start = src
            .find("pub trait DeviceControl")
            .expect("trait declaration");
        // Brace-count to the end of the trait. Stopping at the first `}`
        // would stop inside `as_ios_simctl`'s default body and report a
        // trait with two methods — a parser that finds nothing agrees
        // with a table that lists nothing, and the test would pass while
        // checking air.
        let body = &src[start..];
        let mut depth = 0usize;
        let mut end = body.len();
        let mut seen_open = false;
        for (i, c) in body.char_indices() {
            match c {
                '{' => {
                    depth += 1;
                    seen_open = true;
                }
                '}' => {
                    depth -= 1;
                    if seen_open && depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &body[..end];
        let mut names = Vec::new();
        for line in body.lines() {
            let t = line.trim();
            let sig = t
                .strip_prefix("async fn ")
                .or_else(|| t.strip_prefix("fn "));
            if let Some(sig) = sig
                && let Some(name) = sig.split(['(', '<', ' ']).next()
                && !name.is_empty()
            {
                names.push(name.to_string());
            }
        }
        names
    }

    #[test]
    fn the_parser_actually_finds_the_trait() {
        // The check above is only as good as this: a parser that returned
        // an empty list would make both parity tests vacuously true.
        let methods = trait_methods();
        assert!(
            methods.len() > 15,
            "parsed only {} methods — the parser, not the trait, is wrong: {methods:?}",
            methods.len()
        );
        assert!(methods.iter().any(|m| m == "uninstall"));
        assert!(methods.iter().any(|m| m == "screenshot"));
    }

    #[test]
    fn every_trait_method_has_a_level() {
        let missing: Vec<_> = trait_methods()
            .into_iter()
            .filter(|m| action_level(m).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "DeviceControl methods with no level: {missing:?}\n\
             An unclassified action is one nobody decided the cost of."
        );
    }

    #[test]
    fn the_table_names_no_method_that_does_not_exist() {
        let methods = trait_methods();
        let phantom: Vec<_> = ACTION_LEVELS
            .iter()
            .map(|(m, _)| *m)
            .filter(|m| !methods.iter().any(|t| t == m))
            .collect();
        assert!(
            phantom.is_empty(),
            "the level table names methods the trait does not have: {phantom:?}"
        );
    }

    #[test]
    fn the_methods_that_destroy_data_are_marked_as_such() {
        // Named one by one rather than derived, because this is the list
        // whose wrongness costs the most, and deriving it from the same
        // table it checks would prove nothing.
        for m in [
            "uninstall",
            "keychain_reset",
            "privacy_reset_all",
            "clear_app_sandbox",
            "user_defaults_delete",
        ] {
            assert_eq!(
                action_level(m),
                Some(ActionLevel::Destructive),
                "{m} takes data away and must be classed Destructive"
            );
        }
    }

    #[test]
    fn reads_are_not_dressed_up_as_writes() {
        for m in ["screenshot", "capture_bgra", "pasteboard_get"] {
            assert_eq!(
                action_level(m),
                Some(ActionLevel::Observe),
                "{m} only reads"
            );
        }
    }
}
