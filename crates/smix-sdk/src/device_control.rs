//! `DeviceControl` trait: cross-platform sim/host control.
//!
//! Two-trait architecture: pairs with [`smix_driver::Driver`]
//! (sense+act).
//!
//! Methods on this trait wrap host-side simulator/emulator control
//! commands (`xcrun simctl` for iOS, `adb` for Android).
//! Sense+act methods (tap/find/etc) live on [`smix_driver::Driver`].

use async_trait::async_trait;
use smix_simctl::registry::DeviceKind;
use smix_simctl::{DeviceControlError, SimctlClient, SimctlPermission};
use std::path::Path;

pub use crate::PermissionAction;

/// The app a device has in front, as the device names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmost {
    /// The package or bundle that owns what is on screen.
    pub package: String,
    /// The entry point inside it, spelled the way the platform spells
    /// it — on Android that is the resumed activity, leading dot and
    /// all, so it compares against what a manifest says.
    pub activity: String,
}

/// One crash a device recorded. Re-exported from the adb layer so a
/// caller holds one type whatever kind of device answered.
pub use smix_adb::CrashReport;

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
    /// Every permission this enum names.
    ///
    /// The spelling a caller types is derived from this list rather than
    /// kept beside it, so a new variant cannot be reachable in one
    /// surface and unknown in another.
    pub const ALL: &'static [Permission] = &[
        Permission::Camera,
        Permission::Microphone,
        Permission::PhotoLibrary,
        Permission::Location,
        Permission::LocationAlways,
        Permission::Notifications,
        Permission::Contacts,
        Permission::Calendar,
        Permission::Reminders,
        Permission::Bluetooth,
        Permission::Motion,
        Permission::Media,
        Permission::Health,
        Permission::FaceId,
        Permission::HomeKit,
        Permission::Storage,
        Permission::PostNotifications,
    ];

    /// The name a caller writes, in yaml or on the command line.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Permission::Camera => "camera",
            Permission::Microphone => "microphone",
            Permission::PhotoLibrary => "photos",
            Permission::Location => "location",
            Permission::LocationAlways => "location-always",
            Permission::Notifications => "notifications",
            Permission::Contacts => "contacts",
            Permission::Calendar => "calendar",
            Permission::Reminders => "reminders",
            Permission::Bluetooth => "bluetooth",
            Permission::Motion => "motion",
            Permission::Media => "media",
            Permission::Health => "health",
            Permission::FaceId => "faceid",
            Permission::HomeKit => "homekit",
            Permission::Storage => "storage",
            Permission::PostNotifications => "post-notifications",
        }
    }

    /// Every spelling accepted for a permission, `name()` first.
    ///
    /// The aliases are the ones the guides and older flows already use.
    #[must_use]
    fn aliases(self) -> &'static [&'static str] {
        match self {
            Permission::LocationAlways => &["location-always", "locationalways"],
            Permission::Media => &["media", "media-library"],
            _ => &[],
        }
    }

    /// Parse a caller's spelling. Never guesses: an unknown name is an
    /// error listing every name there is, because silently doing nothing
    /// with a misspelled permission is how a flow comes to pass while
    /// granting nothing.
    ///
    /// # Errors
    ///
    /// The name matches no permission.
    pub fn from_name(name: &str) -> Result<Permission, String> {
        let wanted = name.trim().to_ascii_lowercase();
        for permission in Permission::ALL {
            if permission.name() == wanted || permission.aliases().contains(&wanted.as_str()) {
                return Ok(*permission);
            }
        }
        let known: Vec<&str> = Permission::ALL.iter().map(|p| p.name()).collect();
        Err(format!(
            "unknown permission name '{name}' — supported: {}",
            known.join(", ")
        ))
    }

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
    // Device, not App: the route is the device's, not the app's. It
    // serves whatever runs there, and it stays open after the test that
    // opened it has finished — which is what this level names.
    ("reverse_port", ActionLevel::Device),
    ("reverse_port_remove", ActionLevel::Device),
    // Arranging the device itself. Both outlive the test that set them
    // and neither takes anything away, which is this level exactly.
    ("wake", ActionLevel::Device),
    ("set_stay_awake", ActionLevel::Device),
    // Asking the device what it is doing. Neither changes anything, so
    // neither needs the lease — a look must not fail because somebody
    // else is driving.
    ("frontmost_app", ActionLevel::Observe),
    ("crash_reports", ActionLevel::Observe),
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
/// Whether one action can be carried out on one kind of device.
///
/// Two answers, never three. A cell either works or refuses by name, and
/// "refuses by name" means it can say what it refused, why, and what to
/// do instead — a message with only the first is a dead end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// There is an implementation for this kind of device.
    Works,
    /// There is not, and here is what to say about it.
    RefusedByName {
        /// Why this device cannot do it.
        why: &'static str,
        /// What to do instead. Never empty: `adb-guard` and the
        /// destructive gate both learned that a refusal without a way
        /// out gets worked around rather than read.
        instead: &'static str,
    },
}

/// Which actions each kind of device can carry out.
///
/// The gap this closes is not that the refusals were missing —
/// `DevicectlClient` already refused seventeen of these by name. It is
/// that they were seventeen sentences in seventeen method bodies, with
/// nothing able to say whether the set was complete, and with the other
/// three kinds of device never asked the question at all. §9 #1 has
/// required loud refusal since physical devices landed, and nothing was
/// watching it.
///
/// Rows are in [`DeviceKind::ALL`] order. The reconciliation tests below
/// hold three things: every trait method has a row, no row names a
/// method that does not exist, and every refusal on `PhysicalIos` is one
/// `DevicectlClient` actually makes — so the table cannot drift from the
/// code it describes.
///
/// What a cell encodes is a **fact about the code**: is there an
/// implementation for this kind of device. Whether it works on hardware
/// somebody is holding is a different question, and e2e answers that
/// one. Filling cells with "nobody has measured this" would refuse
/// paths that work today.
///
/// The other direction holds too: a verb that devicectl has and smix does
/// not drive is still a refusal, but the refusal has to name the verb.
/// Xcode 27's devicectl grew `capture screenshot`, `pasteboard copy` and
/// `simulate location route` while six cells here said devicectl could
/// not; `a-refusal-devicectl-outgrew` asks the installed devicectl so
/// that cannot happen quietly again. All six are driven as of 10.2, so no
/// cell is in that state today; the rule is for the next one.
pub const ACTION_PLATFORMS: &[(&str, [Availability; 4])] = {
    use Availability::{RefusedByName as No, Works as Yes};

    // Reasons, written once. Several actions are refused for the same
    // reason on the same device, and repeating the sentence is how two
    // of them come to disagree.
    const NO_DEVICECTL_VERB: &str = "devicectl has no verb for it, and it is a CoreSimulator facility with no counterpart on a device";
    const THROUGH_RUNNER: &str =
        "do it through the runner session, which behaves the same on a phone as on a simulator";
    const USERS_OWN_DEVICE: &str = "on a device this is the owner's own data, and Apple exposes no way to take it away from outside";
    const BY_HAND: &str =
        "do it by hand on the device, or use a simulator for the run that needs it set";
    const ANDROID_HAS_NO_SUCH_IDEA: &str = "Android has no equivalent of this iOS facility";
    const EMULATOR_CONSOLE_ONLY: &str = "the emulator console provides this and a handset does not";
    const ANDROID_CLIPBOARD_IS_SEALED: &str = "since Android 10 the clipboard is readable and writable only by the app in the foreground, and the test runner is not it";
    // Not "cannot": a simulator needs no route because it is already on
    // this side of one. Refusing with the reason a caller can act on is
    // the difference between a dead end and an answer.
    const SIMULATOR_SHARES_THIS_HOST: &str =
        "a simulator shares this machine's network stack, so there is no channel to open";
    const SIMULATOR_USE_LOOPBACK: &str = "reach the service at 127.0.0.1:<port> from inside the simulator — it is the same loopback this machine's service is bound to";
    const NO_REVERSE_OVER_USB: &str = "Apple's USB channel carries host-to-device connections only, and exposes nothing in the other direction";
    const PUT_IT_ON_THE_LAN: &str = "bind the service to an address the phone can reach over the network and name that address in the app under test";
    // A simulator's screen is drawn by the host and never sleeps, so
    // there is no sleeping to undo and nothing to hold awake.
    const SIMULATOR_NEVER_SLEEPS: &str =
        "a simulator's screen is drawn by this machine and never sleeps";
    const NOTHING_TO_DO_ON_A_SIMULATOR: &str =
        "nothing needs doing — a simulator is always awake and stays that way";
    const NO_DEVICECTL_POWER_VERB: &str =
        "devicectl can read a device's lock state but has no verb that changes it";
    const WAKE_IT_BY_HAND: &str =
        "press the side button, or check `xcrun devicectl device info lockState` to see where it stands";
    const NO_DEVICECTL_DISPLAY_SETTING: &str =
        "devicectl exposes no display or auto-lock setting";
    const SET_AUTOLOCK_BY_HAND: &str =
        "set Settings > Display & Brightness > Auto-Lock to Never on the device";
    // Neither Apple tool answers "which app is in front". simctl has no
    // verb at all, and devicectl's `info processes` lists what is
    // running without saying which one the user is looking at.
    const NO_FRONTMOST_FROM_SIMCTL: &str =
        "simctl has no verb that reports which app is in front";
    const NO_FRONTMOST_FROM_DEVICECTL: &str = "devicectl lists a device's processes but does not say which one is frontmost";
    const ASK_THE_RUNNER_WHAT_IT_SEES: &str =
        "ask the runner instead — `smix tree --device <udid>` names the app it read the screen from";
    // The two Apple refusals here are about attribution, not access.
    const SIM_CRASHES_ARE_NOT_DEVICE_SCOPED: &str = "a simulator's crash reports land in this machine's own folder, which is not divided by device, so no answer here could honestly be about one simulator";
    const READ_THE_HOST_CRASH_FOLDER: &str =
        "open ~/Library/Logs/DiagnosticReports and match on the app name and the time of the run";
    const DEVICE_CRASHES_STAY_ON_THE_DEVICE: &str = "crash reports stay on the device and devicectl has no verb that reads them (`sysdiagnose` gathers an archive, which is a different thing)";
    const USE_XCODE_DEVICES_WINDOW: &str =
        "download them with Xcode's Window > Devices and Simulators";

    &[
        // Metadata about the binding, not an action on a device.
        ("platform", [Yes, Yes, Yes, Yes]),
        ("as_ios_simctl", [Yes, Yes, Yes, Yes]),
        // The app under test. This half is what "physical devices are a
        // first-class backend" rests on, and it is whole.
        ("launch", [Yes, Yes, Yes, Yes]),
        ("launch_with_args", [Yes, Yes, Yes, Yes]),
        ("install", [Yes, Yes, Yes, Yes]),
        ("uninstall", [Yes, Yes, Yes, Yes]),
        ("open_url", [Yes, Yes, Yes, Yes]),
        (
            "terminate",
            [
                Yes,
                Yes,
                No {
                    why: "devicectl stops processes by pid and cannot find the pid of a running app from its bundle id",
                    instead: THROUGH_RUNNER,
                },
                Yes,
            ],
        ),
        (
            "send_push",
            [
                Yes,
                No {
                    why: ANDROID_HAS_NO_SUCH_IDEA,
                    instead: "deliver the notification through the app's own push path",
                },
                No {
                    why: "a real push has to come from APNs; devicectl cannot inject one",
                    instead: "send it through APNs, or use a simulator",
                },
                No {
                    why: ANDROID_HAS_NO_SUCH_IDEA,
                    instead: "deliver the notification through the app's own push path",
                },
            ],
        ),
        // Reads.
        (
            "screenshot",
            // A phone answers this since 10.2: `devicectl device capture
            // screenshot`, driven and read back on an iPhone on iOS 26.6.2
            // (2026-09-19, 1179x2556). The device must list
            // `com.apple.coredevice.feature.capturescreenshot`.
            [Yes, Yes, Yes, Yes],
        ),
        (
            "capture_bgra",
            [
                Yes,
                Yes,
                No {
                    why: "surface capture is a CoreSimulator facility with no device counterpart",
                    instead: THROUGH_RUNNER,
                },
                Yes,
            ],
        ),
        // The device's own state. This is where the gap is, and it is
        // wide: driving is complete, arranging the device is nearly
        // absent.
        (
            "set_animations_quiet",
            [
                Yes,
                Yes,
                No {
                    why: NO_DEVICECTL_VERB,
                    instead: BY_HAND,
                },
                Yes,
            ],
        ),
        (
            "pasteboard_set",
            [
                Yes,
                No {
                    why: ANDROID_CLIPBOARD_IS_SEALED,
                    instead: "type the text into the field instead of pasting it",
                },
                // `devicectl device pasteboard copy`, text on stdin. Read back
                // byte for byte on an iPhone (iOS 26.6.2, Xcode 27.0).
                Yes,
                No {
                    why: ANDROID_CLIPBOARD_IS_SEALED,
                    instead: "type the text into the field instead of pasting it",
                },
            ],
        ),
        (
            "pasteboard_get",
            [
                Yes,
                No {
                    why: ANDROID_CLIPBOARD_IS_SEALED,
                    instead: "assert on what the app renders instead of on the clipboard",
                },
                // `devicectl device pasteboard paste`: stdout is the text, and
                // its JSON says how many bytes that should be.
                Yes,
                No {
                    why: ANDROID_CLIPBOARD_IS_SEALED,
                    instead: "assert on what the app renders instead of on the clipboard",
                },
            ],
        ),
        (
            "add_media",
            [
                Yes,
                Yes,
                No {
                    why: NO_DEVICECTL_VERB,
                    instead: "put the file on the device by hand, or use a simulator",
                },
                Yes,
            ],
        ),
        (
            "location_set",
            [
                Yes,
                Yes,
                // `devicectl device simulate location coordinate`; what devicectl
                // says it set is held against what was sent. A phone keeps the
                // location until `simulate location clear`.
                Yes,
                No {
                    why: EMULATOR_CONSOLE_ONLY,
                    instead: BY_HAND,
                },
            ],
        ),
        (
            "location_start",
            [
                Yes,
                Yes,
                // `devicectl device simulate location route --route-file`: returns
                // at once and the device keeps travelling.
                Yes,
                No {
                    why: EMULATOR_CONSOLE_ONLY,
                    instead: BY_HAND,
                },
            ],
        ),
        (
            "set_permission",
            [
                Yes,
                Yes,
                No {
                    why: "TCC grants on a device are the owner's, and devicectl has no equivalent of `simctl privacy`",
                    instead: "grant it on the device the first time the app asks",
                },
                Yes,
            ],
        ),
        (
            "start_recording",
            // Implemented since 10.2 over `devicectl device capture
            // screen-record`, and the cell says so. Whether a given
            // device can be recorded is the device's to answer: it lists
            // `com.apple.coredevice.feature.screenrecording` or it does
            // not, and one that does not is refused by name before
            // anything is started. Measured 2026-09-19: a booted simulator
            // lists it; an iPhone on iOS 26.6.2 does not.
            [Yes, Yes, Yes, Yes],
        ),
        ("stop_recording", [Yes, Yes, Yes, Yes]),
        ("recording_pid", [Yes, Yes, Yes, Yes]),
        // A route from the device back to a service on this machine.
        // The two Apple cells refuse for opposite reasons — one because
        // there is nothing to open, one because nothing can be opened —
        // and both say where the caller should go instead.
        (
            "reverse_port",
            [
                No {
                    why: SIMULATOR_SHARES_THIS_HOST,
                    instead: SIMULATOR_USE_LOOPBACK,
                },
                Yes,
                No {
                    why: NO_REVERSE_OVER_USB,
                    instead: PUT_IT_ON_THE_LAN,
                },
                Yes,
            ],
        ),
        (
            "reverse_port_remove",
            [
                No {
                    why: SIMULATOR_SHARES_THIS_HOST,
                    instead: SIMULATOR_USE_LOOPBACK,
                },
                Yes,
                No {
                    why: NO_REVERSE_OVER_USB,
                    instead: PUT_IT_ON_THE_LAN,
                },
                Yes,
            ],
        ),
        // Arranging a handset so a run can reach it, and asking it what
        // it is doing. Android on both counts: neither Apple tool has a
        // verb for any of the four, and each cell says which tool was
        // asked and what it answered instead.
        (
            "wake",
            [
                No {
                    why: SIMULATOR_NEVER_SLEEPS,
                    instead: NOTHING_TO_DO_ON_A_SIMULATOR,
                },
                Yes,
                No {
                    why: NO_DEVICECTL_POWER_VERB,
                    instead: WAKE_IT_BY_HAND,
                },
                Yes,
            ],
        ),
        (
            "set_stay_awake",
            [
                No {
                    why: SIMULATOR_NEVER_SLEEPS,
                    instead: NOTHING_TO_DO_ON_A_SIMULATOR,
                },
                Yes,
                No {
                    why: NO_DEVICECTL_DISPLAY_SETTING,
                    instead: SET_AUTOLOCK_BY_HAND,
                },
                Yes,
            ],
        ),
        (
            "frontmost_app",
            [
                No {
                    why: NO_FRONTMOST_FROM_SIMCTL,
                    instead: ASK_THE_RUNNER_WHAT_IT_SEES,
                },
                Yes,
                No {
                    why: NO_FRONTMOST_FROM_DEVICECTL,
                    instead: ASK_THE_RUNNER_WHAT_IT_SEES,
                },
                Yes,
            ],
        ),
        (
            "crash_reports",
            [
                No {
                    why: SIM_CRASHES_ARE_NOT_DEVICE_SCOPED,
                    instead: READ_THE_HOST_CRASH_FOLDER,
                },
                Yes,
                No {
                    why: DEVICE_CRASHES_STAY_ON_THE_DEVICE,
                    instead: USE_XCODE_DEVICES_WINDOW,
                },
                Yes,
            ],
        ),
        // Taking data away. Most of these should not exist on somebody's
        // own phone, which is a reason and not an accident.
        (
            "keychain_reset",
            [
                Yes,
                No {
                    why: ANDROID_HAS_NO_SUCH_IDEA,
                    instead: "clear the app's data, which takes its credentials with it",
                },
                No {
                    why: USERS_OWN_DEVICE,
                    instead: "sign out inside the app, or use a simulator",
                },
                No {
                    why: ANDROID_HAS_NO_SUCH_IDEA,
                    instead: "clear the app's data, which takes its credentials with it",
                },
            ],
        ),
        (
            "privacy_reset_all",
            [
                Yes,
                Yes,
                No {
                    why: USERS_OWN_DEVICE,
                    instead: "revoke the permissions in Settings, or use a simulator",
                },
                Yes,
            ],
        ),
        (
            "clear_app_sandbox",
            [
                Yes,
                Yes,
                No {
                    why: "devicectl can uninstall an app but cannot empty its container in place",
                    instead: "uninstall and install again, which empties it",
                },
                Yes,
            ],
        ),
        (
            "user_defaults_delete",
            [
                Yes,
                No {
                    why: ANDROID_HAS_NO_SUCH_IDEA,
                    instead: "clear the app's data, which takes its preferences with it",
                },
                No {
                    why: "a device's defaults live inside the app container, which devicectl cannot write",
                    instead: "uninstall and install again, which empties them",
                },
                No {
                    why: ANDROID_HAS_NO_SUCH_IDEA,
                    instead: "clear the app's data, which takes its preferences with it",
                },
            ],
        ),
    ]
};

/// What one action does on one kind of device, or `None` if the table
/// has never heard of the action.
#[must_use]
pub fn availability(action: &str, kind: DeviceKind) -> Option<Availability> {
    let idx = DeviceKind::ALL.iter().position(|k| *k == kind)?;
    ACTION_PLATFORMS
        .iter()
        .find(|(name, _)| *name == action)
        .map(|(_, row)| row[idx])
}

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
    ///
    /// The default refuses. It used to answer `Ok(())`, three lines under
    /// the sentence above saying that a switch reporting success while
    /// the device keeps animating is worse than no switch — the comment
    /// was right and the code was the thing it warned about. Every
    /// backend in the tree overrides this, so the default was only ever
    /// waiting for the next one, which would have inherited a silent
    /// no-op for free (§9 #1: loud error, never quiet degradation).
    async fn set_animations_quiet(
        &self,
        _id: &str,
        _quiet: bool,
    ) -> Result<(), DeviceControlError> {
        Err(DeviceControlError::non_zero_exit(
            "set_animations_quiet",
            -1,
            "this device backend has not said whether it can quiet animations. \
             Answering yes without doing anything would make the run that \
             follows look deterministic when it is not.",
        ))
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

    // === A route from the device to this machine ===

    /// Let the app under test reach `127.0.0.1:<host_port>` on this
    /// machine by dialling `127.0.0.1:<device_port>` on the device.
    ///
    /// The device port comes first because that is the end the app
    /// dials, and it is the end that identifies the route later.
    ///
    /// What is opened outlives this call: it is not a process, and
    /// nothing here holds it. A caller that opens one owes a
    /// [`Self::reverse_port_remove`], and a caller that writes it into a
    /// ledger gives the next teardown the same ability.
    ///
    /// Since smix 10.2.0.
    async fn reverse_port(
        &self,
        udid: &str,
        device_port: u16,
        host_port: u16,
    ) -> Result<(), DeviceControlError>;

    /// Close a route opened by [`Self::reverse_port`], named by the port
    /// the device dials.
    ///
    /// Since smix 10.2.0.
    async fn reverse_port_remove(
        &self,
        udid: &str,
        device_port: u16,
    ) -> Result<(), DeviceControlError>;

    /// Turn the device's screen on, and answer once it is on.
    ///
    /// This lights the screen. It does **not** unlock: a device with a
    /// passcode keeps its keyguard, and a caller who needs past it has
    /// to get past it some other way. The name says what it does for
    /// that reason.
    ///
    /// Since smix 10.2.0.
    async fn wake(&self, udid: &str) -> Result<(), DeviceControlError>;

    /// Keep the screen on while the device is on a charger, or stop.
    ///
    /// Outlives the run that set it — it is a device setting, not a
    /// property of the session, and the same call with `false` is what
    /// puts it back.
    ///
    /// Since smix 10.2.0.
    async fn set_stay_awake(&self, udid: &str, on: bool) -> Result<(), DeviceControlError>;

    /// Which app is in front, or `None` when nothing is.
    ///
    /// `None` is an ordinary answer: a locked screen and a device still
    /// booting both have nothing resumed.
    ///
    /// Since smix 10.2.0.
    async fn frontmost_app(&self, udid: &str) -> Result<Option<Frontmost>, DeviceControlError>;

    /// Every crash the device has recorded, oldest first.
    ///
    /// An empty list means the device recorded none — the ordinary
    /// answer on a healthy run, and not a failure.
    ///
    /// Since smix 10.2.0.
    async fn crash_reports(&self, udid: &str) -> Result<Vec<CrashReport>, DeviceControlError>;
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
    fn every_trait_method_says_what_it_does_on_every_kind_of_device() {
        let missing: Vec<_> = trait_methods()
            .into_iter()
            .filter(|m| !ACTION_PLATFORMS.iter().any(|(name, _)| name == m))
            .collect();
        assert!(
            missing.is_empty(),
            "DeviceControl methods with no platform row: {missing:?}\n\
             §9 #1 requires a loud refusal when a capability is not there. A \
             method nobody answered for is the quiet degradation it forbids."
        );
    }

    /// A refusal on an Apple device has to leave the caller somewhere
    /// to go, and for these two the somewhere differs: a simulator
    /// needs no route because it already shares this machine's
    /// loopback; a phone has no route to open at all.
    ///
    /// Asserted on the text because that is what a caller reads. A cell
    /// that merely said `RefusedByName` with an empty `instead` would
    /// satisfy the type and strand the reader — which is the dead end
    /// `Availability`'s own doc says it exists to prevent.
    #[test]
    fn the_two_apple_refusals_of_a_reverse_say_where_to_go_instead() {
        use smix_simctl::registry::DeviceKind;

        for action in ["reverse_port", "reverse_port_remove"] {
            let Some(Availability::RefusedByName { why, instead }) =
                availability(action, DeviceKind::Simulator)
            else {
                panic!("{action} on a simulator should refuse by name");
            };
            assert!(why.contains("network stack"), "{action}: {why}");
            assert!(instead.contains("127.0.0.1"), "{action}: {instead}");

            let Some(Availability::RefusedByName { why, instead }) =
                availability(action, DeviceKind::PhysicalIos)
            else {
                panic!("{action} on a phone should refuse by name");
            };
            assert!(why.contains("host-to-device"), "{action}: {why}");
            assert!(instead.contains("over the"), "{action}: {instead}");

            for kind in [DeviceKind::Emulator, DeviceKind::PhysicalAndroid] {
                assert_eq!(
                    availability(action, kind),
                    Some(Availability::Works),
                    "{action} is driven on {kind:?}"
                );
            }
        }
    }

    #[test]
    fn the_platform_table_names_no_method_that_does_not_exist() {
        let methods = trait_methods();
        let phantom: Vec<_> = ACTION_PLATFORMS
            .iter()
            .map(|(m, _)| *m)
            .filter(|m| !methods.iter().any(|t| t == m))
            .collect();
        assert!(
            phantom.is_empty(),
            "the platform table names methods the trait does not have: {phantom:?}"
        );
    }

    #[test]
    fn a_refusal_that_cannot_say_what_to_do_instead_is_a_dead_end() {
        let mut mute = Vec::new();
        for (name, row) in ACTION_PLATFORMS {
            for (idx, cell) in row.iter().enumerate() {
                if let Availability::RefusedByName { why, instead } = cell
                    && (why.trim().is_empty() || instead.trim().is_empty())
                {
                    mute.push(format!("{name} on {:?}", DeviceKind::ALL[idx]));
                }
            }
        }
        assert!(
            mute.is_empty(),
            "refusals with nothing to say: {mute:?}\n\
             'Refused by name' means it can name what it refused, why, and \
             what to do instead. Two out of three is a dead end."
        );
    }

    #[test]
    fn the_table_is_indexed_the_way_it_says_it_is() {
        // The rows are `[Availability; 4]` and the reader is told they
        // are in `DeviceKind::ALL` order. Nothing in the type says so,
        // so a fifth kind — or a reordering — would silently shift every
        // cell by one, and each cell would still be a valid answer to
        // the wrong question.
        assert_eq!(
            DeviceKind::ALL.len(),
            4,
            "the platform rows are fixed-width; a new DeviceKind needs a column, \
             and every row filled in for it"
        );
        assert_eq!(
            availability("send_push", DeviceKind::Simulator),
            Some(Availability::Works),
            "a simulator can be sent a push"
        );
        assert!(
            matches!(
                availability("send_push", DeviceKind::PhysicalIos),
                Some(Availability::RefusedByName { .. })
            ),
            "a phone cannot; if this passes as Works the row is off by one"
        );
        assert_eq!(availability("no_such_action", DeviceKind::Simulator), None);
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
        for m in [
            "screenshot",
            "capture_bgra",
            "pasteboard_get",
            "frontmost_app",
            "crash_reports",
        ] {
            assert_eq!(
                action_level(m),
                Some(ActionLevel::Observe),
                "{m} only reads"
            );
        }
    }

    /// Arranging a handset is Android's alone, and each Apple cell has
    /// to say which tool was asked and what to do instead.
    ///
    /// The `instead` half is what makes a refusal usable: a message that
    /// only says no gets worked around rather than read, which is why
    /// the table's own type demands one. Asserting it is non-empty keeps
    /// a future cell from satisfying the type with `""`.
    #[test]
    fn arranging_a_phone_is_android_only_and_apple_says_where_to_go() {
        use smix_simctl::registry::DeviceKind::{
            Emulator, PhysicalAndroid, PhysicalIos, Simulator,
        };

        for action in ["wake", "set_stay_awake", "frontmost_app", "crash_reports"] {
            for kind in [Emulator, PhysicalAndroid] {
                assert_eq!(
                    availability(action, kind),
                    Some(Availability::Works),
                    "{action} is driven over adb on {kind:?}"
                );
            }
            for kind in [Simulator, PhysicalIos] {
                match availability(action, kind) {
                    Some(Availability::RefusedByName { why, instead }) => {
                        assert!(!why.is_empty(), "{action} on {kind:?} refuses without a reason");
                        assert!(
                            !instead.is_empty(),
                            "{action} on {kind:?} refuses without a way forward"
                        );
                    }
                    other => panic!("{action} on {kind:?} should refuse by name, got {other:?}"),
                }
            }
        }
    }

    /// One name table, and every variant reachable through it.
    ///
    /// The count is the number of variants, so adding one without a
    /// spelling fails here rather than in whichever surface asks first.
    #[test]
    fn every_permission_has_exactly_one_spelling_that_parses() {
        assert_eq!(Permission::ALL.len(), 17, "one entry per enum variant");
        for permission in Permission::ALL {
            assert_eq!(
                Permission::from_name(permission.name()),
                Ok(*permission),
                "{} does not parse from its own name",
                permission.name()
            );
        }
    }

    /// Case and the older spellings both land, and a misspelling is an
    /// error naming every alternative — never a silent no-op, which
    /// would grant nothing while the flow went on reporting success.
    #[test]
    fn a_misspelled_permission_is_an_error_that_lists_the_real_ones() {
        assert_eq!(Permission::from_name("CAMERA"), Ok(Permission::Camera));
        assert_eq!(
            Permission::from_name(" media-library "),
            Ok(Permission::Media)
        );
        assert_eq!(
            Permission::from_name("locationalways"),
            Ok(Permission::LocationAlways)
        );
        // Both of these were unreachable from the yaml name table until
        // it was folded into this one.
        assert_eq!(Permission::from_name("storage"), Ok(Permission::Storage));
        assert_eq!(
            Permission::from_name("post-notifications"),
            Ok(Permission::PostNotifications)
        );

        let err = Permission::from_name("camra").expect_err("a misspelling is not a permission");
        assert!(err.contains("camra"), "says what it could not read: {err}");
        assert!(err.contains("camera"), "lists the real ones: {err}");
    }
}
