//! What to say when a wait for the keyboard runs out on a simulator that
//! keeps its keyboard minimized.

use smix_screen::Role;
use smix_selector::Selector;

/// The sentence a keyboard wait's failure carries when the simulator's
/// `AutomaticMinimizationEnabled` is on; `None` otherwise.
///
/// With the setting on, a focused field shows no software keyboard, so
/// the timeout is a fact about the device and not about the app — and
/// the failure used to read like the latter. Named, with the way back,
/// and not acted on: the setting is the owner's. How it gets switched on
/// is not known; neither `pressKey` nor `inputText` did it on a clean
/// simulator (2026-09-25).
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
        "this simulator keeps its software keyboard minimized \
         (com.apple.keyboard.preferences AutomaticMinimizationEnabled = 1), so a \
         focused field shows no keyboard and this wait cannot succeed on it. smix \
         does not change the setting. To turn it off: `xcrun simctl spawn {udid} \
         defaults delete com.apple.keyboard.preferences AutomaticMinimizationEnabled`, \
         then relaunch the app."
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
