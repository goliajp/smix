//! Why an Android runner that answers `/health` sees no application.
//!
//! "No readable application window" has more than one cause, and the
//! remedies do not overlap. A display that is off draws nothing, and a
//! lock screen covers the app; restarting the instrumentation fixes
//! neither, and telling someone it will sends them round the same loop.
//! So the screen is read before the runner is blamed.

use smix_adb::{Wakefulness, parse_wakefulness};

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
    ScreenState {
        wakefulness: dump(&["shell", "dumpsys", "power"]).and_then(|d| parse_wakefulness(&d)),
        keyguard_showing: dump(&["shell", "dumpsys", "window"])
            .and_then(|d| parse_keyguard_showing(&d)),
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
             emulator -avd <its AVD> -no-snapshot-load"
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
        }
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
