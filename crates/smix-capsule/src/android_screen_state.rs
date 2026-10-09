//! Why an Android runner that answers `/health` sees no application.
//!
//! "No readable application window" has more than one cause, and the
//! remedies do not overlap. A display that is off draws nothing, and a
//! lock screen covers the app; restarting the instrumentation fixes
//! neither, and telling someone it will sends them round the same loop.
//! So the screen is read before the runner is blamed.

use smix_adb::{Wakefulness, parse_wakefulness};

/// The package the status bar, the shade and the lock screen belong to.
const SYSTEM_UI: &str = "com.android.systemui";

/// What the device said about its screen when no app could be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenState {
    /// `mWakefulness=` from `dumpsys power`; `None` when it could not be read.
    pub wakefulness: Option<Wakefulness>,
    /// `isKeyguardShowing=` from `dumpsys window`; `None` when it could not be read.
    pub keyguard_showing: Option<bool>,
    /// Whether `com.android.systemui` has a process; `None` when it could not be read.
    pub system_ui_running: Option<bool>,
    /// The package a system "isn't responding" dialog names, when one is on screen.
    pub not_responding: Option<String>,
    /// The title of the window holding the input focus (`mCurrentFocus=`
    /// in `dumpsys window`); `None` when no window holds it or it could
    /// not be read.
    pub focus: Option<String>,
    /// The package of the window the runner reports holding the focus,
    /// when that is neither system UI nor unreadable — set by the caller,
    /// which has the runner's window list.
    pub in_front: Option<String>,
}

impl ScreenState {
    /// The package of whatever is in front of the app: the runner's
    /// focused window, or else the platform's focused activity. A focused
    /// window title that is not `package/activity` names no package
    /// (`NotificationShade` is system UI's), so it is not guessed at.
    fn covering(&self) -> Option<&str> {
        if let Some(p) = &self.in_front {
            return Some(p);
        }
        let (package, _) = self.focus.as_deref()?.split_once('/')?;
        (package != SYSTEM_UI).then_some(package)
    }
}

/// The focused window's title out of `dumpsys window`: what follows the
/// user id in `mCurrentFocus=Window{<hash> u<user> <title>}`.
pub fn parse_current_focus(dump: &str) -> Option<String> {
    let rest = dump
        .lines()
        .find_map(|line| line.trim().strip_prefix("mCurrentFocus="))?
        .trim()
        .strip_prefix("Window{")?
        .strip_suffix('}')?;
    let mut words = rest.splitn(3, ' ');
    let (_hash, user, title) = (words.next()?, words.next()?, words.next()?);
    (user.starts_with('u') && !title.is_empty()).then(|| title.to_string())
}

/// `isKeyguardShowing=` out of `dumpsys window`.
pub fn parse_keyguard_showing(dump: &str) -> Option<bool> {
    let value = dump
        .lines()
        .find_map(|line| line.trim().strip_prefix("isKeyguardShowing="))?
        .trim();
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// The package named by an "Application Not Responding" window in `dumpsys window windows`.
pub fn parse_not_responding(dump: &str) -> Option<String> {
    let rest = dump.split("Application Not Responding: ").nth(1)?;
    let name: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '}')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Read the screen state of `serial` through adb.
pub fn read(serial: &str) -> ScreenState {
    let dump = |args: &[&str]| {
        crate::runner_android::adb(serial)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let window = dump(&["shell", "dumpsys", "window"]);
    ScreenState {
        wakefulness: dump(&["shell", "dumpsys", "power"]).and_then(|d| parse_wakefulness(&d)),
        keyguard_showing: window.as_deref().and_then(parse_keyguard_showing),
        focus: window.as_deref().and_then(parse_current_focus),
        in_front: None,
        system_ui_running: crate::runner_android::adb(serial)
            .args(["shell", "pidof", "com.android.systemui"])
            .output()
            .ok()
            .map(|o| o.status.success() && !o.stdout.trim_ascii().is_empty()),
        not_responding: dump(&["shell", "dumpsys", "window", "windows"])
            .and_then(|d| parse_not_responding(&d)),
    }
}

/// The refusal `runner up` gives when the automation sees no application.
pub fn no_app_refusal(port: u16, serial: &str, why: &str, screen: ScreenState) -> String {
    let head =
        format!("the runner answers /health on {port} but its automation is not usable: {why}");
    match &screen {
        ScreenState {
            system_ui_running: Some(false),
            ..
        } => format!(
            "{head}\nThe device's system UI is not running: with no status bar, lock \
             screen or launcher surface there is nothing an app can be drawn under, \
             and the screen stays black. The runner is fine. Android does not bring \
             it back by itself once it has been killed after an error, and an \
             emulator's Quick Boot snapshot saves this state and restores it on the \
             next boot. Shut it down and start it cold:\n  \
             smix sim shutdown {serial}\n  \
             smix sim boot <its alias> --cold"
        ),
        ScreenState {
            not_responding: Some(app),
            ..
        } => format!(
            "{head}\nA system \"{app} isn't responding\" dialog is covering the \
             screen. The runner is fine. Answer the dialog on the device (Wait or \
             Close app) and bring the runner up again."
        ),
        ScreenState {
            wakefulness: Some(w @ (Wakefulness::Asleep | Wakefulness::Dozing)),
            ..
        } => format!(
            "{head}\nThe display is off ({w:?}): nothing is drawn, so there is no \
             application window to read. The runner is fine. Wake the device and \
             bring the runner up again:\n  smix sim wake {serial}\n\
             smix does not turn a device's screen on by itself."
        ),
        ScreenState {
            wakefulness: Some(Wakefulness::Awake),
            keyguard_showing: Some(true),
            ..
        } => format!(
            "{head}\nThe lock screen is showing: the app is behind it, where the \
             automation cannot read it. The runner is fine. Unlock the device — an \
             emulator without a PIN unlocks with a swipe up — and bring the runner up \
             again. smix does not unlock devices."
        ),
        ScreenState {
            wakefulness: Some(Wakefulness::Awake),
            keyguard_showing: Some(false),
            ..
        } if screen.covering().is_some() => format!(
            "{head}\nA window of {} holds the focus (the platform's focused window: \
             {}): a system dialog, or an input method's own screen, is in front of \
             the app, and the app's window cannot be read behind it. The runner is \
             fine. smix does not dismiss it: a BACK key would answer it for whoever \
             uses the device. Close it on the device and bring the runner up again.",
            screen.covering().unwrap_or_default(),
            screen.focus.as_deref().unwrap_or("not read"),
        ),
        ScreenState {
            wakefulness: Some(Wakefulness::Awake),
            keyguard_showing: Some(false),
            ..
        } => format!(
            "{head}\nThe display is on and unlocked, so this is what a \
             crashed-and-restarted instrumentation looks like — the server is up, \
             `getWindows()` is not. Stop it and bring it up again:\n  \
             smix runner down --platform android --device {serial}"
        ),
        _ => format!(
            "{head}\nsmix could not read whether the display is on or locked \
             (dumpsys power / window gave no answer), so the cause is not known: a \
             screen that is off or locked looks the same as a crashed \
             instrumentation from here. Check the device's screen; if it is on and \
             unlocked, stop the runner and bring it up again:\n  \
             smix runner down --platform android --device {serial}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHY: &str = "no readable application window is attached — only nothing at all";

    fn state(w: Option<Wakefulness>, k: Option<bool>) -> ScreenState {
        ScreenState {
            wakefulness: w,
            keyguard_showing: k,
            system_ui_running: Some(true),
            not_responding: None,
            focus: None,
            in_front: None,
        }
    }

    /// A consumer's screen: a password manager's "save password?" prompt
    /// in front of their app, and the refusal blamed the instrumentation.
    #[test]
    fn a_window_of_another_package_in_front_is_named_and_the_runner_is_not_blamed() {
        let mut s = state(Some(Wakefulness::Awake), Some(false));
        s.focus = Some(
            "com.google.android.gms/com.google.android.gms.autofill.ui.AutofillSaveActivity".into(),
        );
        let text = no_app_refusal(1, "emulator-5554", WHY, s.clone());
        assert!(
            text.contains("A window of com.google.android.gms holds the focus"),
            "{text}"
        );
        assert!(!text.contains("crashed-and-restarted"), "{text}");
        // the runner's own answer, when it has one, is the one named
        s.in_front = Some("android".into());
        let text = no_app_refusal(1, "emulator-5554", WHY, s);
        assert!(
            text.contains("A window of android holds the focus"),
            "{text}"
        );
        assert!(
            text.contains("AutofillSaveActivity"),
            "the platform's title is evidence: {text}"
        );
    }

    #[test]
    fn system_ui_holding_the_focus_is_not_called_something_in_front() {
        let mut s = state(Some(Wakefulness::Awake), Some(false));
        for title in [
            "NotificationShade",
            "com.android.systemui/com.android.systemui.Foo",
        ] {
            s.focus = Some(title.into());
            let text = no_app_refusal(1, "emulator-5554", WHY, s.clone());
            assert!(text.contains("crashed-and-restarted"), "{title}: {text}");
        }
    }

    #[test]
    fn the_focused_window_is_read_from_a_real_dump() {
        // measured on emulator-5554, API 36, with a permission dialog up
        let dump = "  mCurrentFocus=Window{cdcd6de u0 com.google.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity}\n  mFocusedApp=ActivityRecord{130209139 u0 x/y t1996}\n";
        assert_eq!(
            parse_current_focus(dump).as_deref(),
            Some(
                "com.google.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity"
            )
        );
        assert_eq!(parse_current_focus("  mCurrentFocus=null\n"), None);
        assert_eq!(parse_current_focus("nothing\n"), None);
    }

    #[test]
    fn a_system_ui_that_is_not_running_is_named_before_the_lock_screen() {
        let mut s = state(Some(Wakefulness::Awake), Some(true));
        s.system_ui_running = Some(false);
        let text = no_app_refusal(1, "emulator-5554", WHY, s);
        assert!(text.contains("system UI is not running"), "{text}");
        assert!(!text.contains("crashed-and-restarted"), "{text}");
        assert!(!text.contains("lock screen is showing"), "{text}");
    }

    #[test]
    fn a_not_responding_dialog_is_named_with_its_app() {
        let mut s = state(Some(Wakefulness::Awake), Some(false));
        s.not_responding = Some("com.android.systemui".into());
        let text = no_app_refusal(1, "emulator-5554", WHY, s);
        assert!(text.contains("isn't responding"), "{text}");
        assert!(text.contains("com.android.systemui"), "{text}");
        assert!(!text.contains("crashed-and-restarted"), "{text}");
    }

    #[test]
    fn the_not_responding_window_is_read_from_a_real_dump() {
        let dump =
            "  Window #3 Window{f7edd3 u0 Application Not Responding: com.android.systemui}:\n";
        assert_eq!(
            parse_not_responding(dump).as_deref(),
            Some("com.android.systemui")
        );
        assert_eq!(
            parse_not_responding("  Window #1 Window{16dbb46 u0 com.android.launcher3}:\n"),
            None
        );
    }

    #[test]
    fn a_display_that_is_off_is_named_and_the_runner_is_not_blamed() {
        for w in [Wakefulness::Asleep, Wakefulness::Dozing] {
            let text = no_app_refusal(1, "emulator-5554", WHY, state(Some(w), Some(true)));
            assert!(text.contains("display is off"), "{text}");
            assert!(text.contains("smix sim wake emulator-5554"), "{text}");
            assert!(!text.contains("crashed-and-restarted"), "{text}");
        }
    }

    #[test]
    fn a_lock_screen_is_named_and_the_runner_is_not_blamed() {
        let text = no_app_refusal(
            1,
            "emulator-5554",
            WHY,
            state(Some(Wakefulness::Awake), Some(true)),
        );
        assert!(text.contains("lock screen is showing"), "{text}");
        assert!(!text.contains("crashed-and-restarted"), "{text}");
    }

    #[test]
    fn an_awake_unlocked_device_with_no_app_is_the_instrumentation() {
        let text = no_app_refusal(
            1,
            "emulator-5554",
            WHY,
            state(Some(Wakefulness::Awake), Some(false)),
        );
        assert!(text.contains("crashed-and-restarted"), "{text}");
        assert!(text.contains("smix runner down --platform android --device emulator-5554"));
    }

    #[test]
    fn a_screen_that_could_not_be_read_says_so() {
        let text = no_app_refusal(1, "emulator-5554", WHY, state(None, None));
        assert!(text.contains("could not read"), "{text}");
    }

    #[test]
    fn the_keyguard_line_is_read_from_a_real_dump() {
        let dump = "    mKeyguardDrawComplete=true mWindowManagerDrawComplete=true\n    isKeyguardShowing=true\n";
        assert_eq!(parse_keyguard_showing(dump), Some(true));
        assert_eq!(
            parse_keyguard_showing("    isKeyguardShowing=false\n"),
            Some(false)
        );
        assert_eq!(parse_keyguard_showing("nothing here\n"), None);
    }
}
