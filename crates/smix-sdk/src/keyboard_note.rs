//! What to add when a wait for the keyboard runs out on a simulator with
//! `AutomaticMinimizationEnabled` on.

use smix_screen::Role;
use smix_selector::Selector;

/// The sentence a keyboard wait's failure carries when the simulator's
/// `AutomaticMinimizationEnabled` is on; `None` otherwise.
///
/// A fact about the device, not a verdict on the timeout. On 2026-09-25 a
/// simulator with the setting on showed no keyboard and did once it was
/// deleted; that was never reproduced, and on iOS 27 (and a later iOS 26.5
/// simulator) a keyboard appears with it on, after relaunching the app or
/// rebooting the simulator. So it is named, with the way to rule it out,
/// and not called the cause. Not acted on: the setting is the owner's.
#[must_use]
pub fn keyboard_minimized_note(
    selector: &Selector,
    minimization: Option<bool>,
    udid: &str,
) -> Option<String> {
    if !waits_for_keyboard(selector) || minimization != Some(true) {
        return None;
    }
    Some(format!(
        "this simulator has com.apple.keyboard.preferences \
         AutomaticMinimizationEnabled = 1. It is reported as a fact about the \
         device, not as the cause of this timeout: a keyboard has been seen both \
         hidden and shown with it on. smix does not change the setting. To rule it out: \
         `xcrun simctl spawn {udid} defaults delete com.apple.keyboard.preferences \
         AutomaticMinimizationEnabled`, then relaunch the app."
    ))
}

/// Whether `selector` asks for the software keyboard.
pub(crate) fn waits_for_keyboard(selector: &Selector) -> bool {
    matches!(
        selector,
        Selector::Role {
            role: Role::Keyboard,
            ..
        }
    )
}
