//! A simulator's hardware keyboard: keys sent the way Simulator.app sends
//! the Mac's, through SimulatorKit's HID client.
//!
//! Not typed text. A typed tab is a character in the focused field; a
//! pressed Tab is what moves Full Keyboard Access from one control to the
//! next.

use std::str::FromStr;
use std::time::Duration;

/// Keys pressed together, by USB HID usage: modifiers first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    usages: Vec<u32>,
    spelled: String,
}

impl Chord {
    /// As it was written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.spelled
    }
}

const MODIFIERS: &[(&str, u32)] = &[
    ("ctrl", 0xE0),
    ("shift", 0xE1),
    ("alt", 0xE2),
    ("cmd", 0xE3),
];

const KEYS: &[(&str, u32)] = &[
    ("return", 0x28),
    ("escape", 0x29),
    ("delete", 0x2A),
    ("tab", 0x2B),
    ("space", 0x2C),
    ("right", 0x4F),
    ("left", 0x50),
    ("down", 0x51),
    ("up", 0x52),
];

fn key_usage(name: &str) -> Option<u32> {
    if let Some((_, u)) = KEYS.iter().find(|(k, _)| *k == name) {
        return Some(*u);
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c @ 'a'..='z'), None) => Some(0x04 + (c as u32 - 'a' as u32)),
        _ => None,
    }
}

impl FromStr for Chord {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let spelled = s.to_ascii_lowercase();
        let mut parts: Vec<&str> = spelled.split('+').collect();
        let key = parts.pop().unwrap_or_default();
        let mut usages = Vec::new();
        for m in &parts {
            let (_, u) = MODIFIERS
                .iter()
                .find(|(n, _)| n == m)
                .ok_or_else(|| unknown(s))?;
            usages.push(*u);
        }
        usages.push(key_usage(key).ok_or_else(|| unknown(s))?);
        Ok(Self { usages, spelled })
    }
}

fn unknown(s: &str) -> String {
    let names = |t: &[(&str, u32)]| t.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ");
    format!(
        "unknown key {s:?}: use {}, or a letter a–z, with {} before it and `+` between, as in shift+tab",
        names(KEYS),
        names(MODIFIERS)
    )
}

/// How long a key stays down, and how long between chords: a focus move is
/// animated, and a key sent during it lands on the control being left.
#[cfg(target_os = "macos")]
const DOWN: Duration = Duration::from_millis(50);
#[cfg(target_os = "macos")]
const BETWEEN: Duration = Duration::from_millis(500);

/// Press `chords` one after another on the booted simulator `udid`.
///
/// Sends and returns; what the app did with them is the caller's to look at.
#[cfg(target_os = "macos")]
pub async fn press(udid: &str, chords: &[Chord]) -> Result<(), String> {
    // one client for the whole sequence; messages are sent asynchronously,
    // so the last key is given its gap before the client goes
    let keyboard = crate::keyboard_sys::Keyboard::open(udid)?;
    for chord in chords {
        for u in &chord.usages {
            keyboard.send(*u, true)?;
        }
        tokio::time::sleep(DOWN).await;
        for u in chord.usages.iter().rev() {
            keyboard.send(*u, false)?;
        }
        tokio::time::sleep(BETWEEN).await;
    }
    Ok(())
}

/// Press `chords` one after another on the booted simulator `udid`.
#[cfg(not(target_os = "macos"))]
pub async fn press(_: &str, _: &[Chord]) -> Result<(), String> {
    Err("a simulator's keyboard needs macOS".into())
}

/// Press and let go of the lock button.
#[cfg(target_os = "macos")]
async fn press_lock(udid: &str) -> Result<(), String> {
    let keyboard = crate::keyboard_sys::Keyboard::open(udid)?;
    keyboard.lock_button(true)?;
    tokio::time::sleep(DOWN).await;
    keyboard.lock_button(false)?;
    tokio::time::sleep(BETWEEN).await;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
async fn press_lock(_: &str) -> Result<(), String> {
    Err("a simulator's lock button needs macOS".into())
}

/// `notifyutil -g`'s answer: `<name> <state>`.
fn notify_state(said: &str) -> Option<u64> {
    said.split_whitespace().last()?.parse().ok()
}

const LOCK_STATE: &str = "com.apple.springboard.lockstate";
const ACCESSIBILITY: &str = "com.apple.Accessibility";
const FULL_ACCESS: &str = "FullKeyboardAccessEnabled";

impl crate::SimctlClient {
    /// Whether the simulator's screen is locked.
    pub async fn is_locked(&self, udid: &str) -> Result<bool, crate::DeviceControlError> {
        let said = crate::simctl_run(&["spawn", udid, "notifyutil", "-g", LOCK_STATE]).await?;
        match notify_state(&said) {
            Some(n) => Ok(n != 0),
            None => Err(crate::DeviceControlError::non_zero_exit(
                "notifyutil",
                1,
                format!("{LOCK_STATE} read as {said:?}"),
            )),
        }
    }

    /// Lock the simulator's screen. Returns whether it was unlocked: the
    /// button pressed on a locked one would only light it up.
    pub async fn lock(&self, udid: &str) -> Result<bool, crate::DeviceControlError> {
        let failed = |e: String| crate::DeviceControlError::non_zero_exit("lock", 1, e);
        if self.is_locked(udid).await? {
            return Ok(false);
        }
        press_lock(udid).await.map_err(failed)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            // a look that began before the deadline still counts
            let looked = std::time::Instant::now();
            if self.is_locked(udid).await? {
                return Ok(true);
            }
            if looked >= deadline {
                return Err(failed(format!(
                    "{udid} is not locked 5 s after its lock button"
                )));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Whether Full Keyboard Access is written as on. A simulator where
    /// nobody set it has it off.
    pub async fn full_keyboard_access(
        &self,
        udid: &str,
    ) -> Result<bool, crate::DeviceControlError> {
        match self
            .spawn_defaults(udid, &["read", ACCESSIBILITY, FULL_ACCESS])
            .await
        {
            Ok(s) => Ok(s.trim() == "1"),
            Err(crate::DeviceControlError::NonZeroExit { ref stderr, .. })
                if crate::defaults_key_is_absent(stderr) =>
            {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    /// Write Full Keyboard Access. On takes effect at the next boot
    /// (measured, iPadOS 27); off at once.
    pub async fn set_full_keyboard_access(
        &self,
        udid: &str,
        on: bool,
    ) -> Result<(), crate::DeviceControlError> {
        let value = if on { "true" } else { "false" };
        self.spawn_defaults(udid, &["write", ACCESSIBILITY, FULL_ACCESS, "-bool", value])
            .await
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notify_state_is_the_number_at_the_end() {
        assert_eq!(notify_state("com.apple.springboard.lockstate 1\n"), Some(1));
        assert_eq!(notify_state("com.apple.springboard.lockstate 0"), Some(0));
        assert_eq!(notify_state("Failed"), None);
        assert_eq!(notify_state(""), None);
    }

    #[test]
    fn a_chord_holds_its_modifiers_before_its_key() {
        assert_eq!(
            "Shift+Tab".parse::<Chord>().unwrap().usages,
            vec![0xE1, 0x2B]
        );
        assert_eq!(
            "cmd+shift+a".parse::<Chord>().unwrap().usages,
            vec![0xE3, 0xE1, 0x04]
        );
        assert_eq!("down".parse::<Chord>().unwrap().usages, vec![0x51]);
        assert_eq!("z".parse::<Chord>().unwrap().usages, vec![0x1D]);
    }

    #[test]
    fn what_is_not_a_key_is_refused_with_the_list() {
        for bad in ["", "tabs", "tab+shift", "shift", "shift+", "é", "hyper+a"] {
            let e = bad.parse::<Chord>().unwrap_err();
            assert!(e.contains("tab") && e.contains("shift+tab"), "{bad:?}: {e}");
        }
    }
}
