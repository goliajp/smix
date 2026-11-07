//! Entering the lock-screen credential of an emulator that booted locked.
//!
//! With a PIN, pattern or password set, the user stays locked after a
//! boot until the credential is entered, and no app that is not
//! direct-boot aware can start. Once it has been entered the user stays
//! unlocked, but the lock screen comes back whenever the screen goes off,
//! and an app launched under it is not on screen. So the device is
//! unlocked only when both are true: the user is unlocked and the lock
//! screen is gone. The credential is typed on the bouncer, which needs no
//! window on the host: wake, dismiss the keyguard, type, Enter.

use std::time::{Duration, Instant};

use thiserror::Error;

use crate::{AdbClient, AdbError, user_is_locked};

/// How long a device without a credential takes to unlock itself after
/// `sys.boot_completed`, at most.
const UNLOCKS_ITSELF_WITHIN: Duration = Duration::from_secs(30);
/// How long the bouncer takes to come up. Characters typed before it is
/// up go nowhere, which is what happened when the credential was typed
/// right after a boot.
const BOUNCER_UP_WITHIN: Duration = Duration::from_secs(20);
/// How long the user state takes to turn and the lock screen to leave
/// after Enter on the bouncer. The lock screen leaves up to 3 s after
/// the user state turns.
const ENTRY_TAKES_HOLD_WITHIN: Duration = Duration::from_secs(12);
/// How long a lock screen without a credential takes to leave once
/// dismissed.
const DISMISSED_WITHIN: Duration = Duration::from_secs(8);
/// Where the bouncer is read: system UI's own state, which a running smix
/// runner does not stand in the way of. A `uiautomator dump` did, and
/// was killed whenever the runner held the device's UI automation.
const BOUNCER_READ: &str =
    "dumpsys activity service com.android.systemui | grep -F 'isBouncerShowing()'; true";

/// `CredentialType: PIN` in `dumpsys lock_settings`; `NONE` without one.
#[must_use]
pub fn credential_type(dumpsys_lock_settings: &str) -> Option<&str> {
    dumpsys_lock_settings
        .lines()
        .find_map(|l| l.trim().strip_prefix("CredentialType:"))
        .map(str::trim)
}

/// `isKeyguardShowing=true` in `dumpsys window`.
#[must_use]
pub fn keyguard_showing(dumpsys_window: &str) -> Option<bool> {
    dumpsys_window
        .lines()
        .find_map(|l| l.trim().strip_prefix("isKeyguardShowing="))
        .and_then(|v| match v.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

/// Whether `input text` types `credential` as it is: 1–16 ASCII letters
/// or digits. Anything else would be read by the shell or by `input`.
#[must_use]
pub fn typeable_credential(credential: &str) -> bool {
    (1..=16).contains(&credential.len()) && credential.chars().all(|c| c.is_ascii_alphanumeric())
}

/// What [`AdbClient::unlock_user`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unlock {
    /// The user was unlocked and the lock screen gone already, or the
    /// user unlocked itself having no credential.
    WasNotLocked,
    /// The credential was entered: the user is unlocked and the lock
    /// screen gone.
    Entered,
    /// The lock screen had no credential behind it and was dismissed.
    Dismissed,
    /// `dumpsys user` or `dumpsys window` did not say.
    Unknown,
}

/// Why the user is still locked. Never carries the credential.
#[derive(Debug, Error)]
pub enum UnlockError {
    #[error(
        "{serial} is locked: its {credential} has not been entered since it booted, and no \
         app that is not direct-boot aware can start until it is. Give it with \
         --unlock-pin <pin> or SMIX_UNLOCK_PIN"
    )]
    NeedsCredential { serial: String, credential: String },
    #[error(
        "{serial} shows its lock screen again (it was unlocked once since it booted), and \
         an app launched under it is not on screen. Give its {credential} with \
         --unlock-pin <pin> or SMIX_UNLOCK_PIN"
    )]
    NeedsCredentialAgain { serial: String, credential: String },
    #[error(
        "{serial} is locked by a pattern, which smix cannot enter; a PIN or a password \
         of letters and digits can be"
    )]
    Pattern { serial: String },
    #[error(
        "{serial} is locked and its bouncer did not come up within {} s of waking it and \
         dismissing the keyguard",
        .after.as_secs()
    )]
    NoBouncer { serial: String, after: Duration },
    #[error(
        "{serial} is still locked {} s after its credential was entered — is it the \
         device's {credential}?",
        .after.as_secs()
    )]
    StillLocked {
        serial: String,
        credential: String,
        after: Duration,
    },
    #[error("{serial} has no credential and stayed locked for {} s", .after.as_secs())]
    DidNotUnlockItself { serial: String, after: Duration },
    #[error(
        "{serial} has no credential and its lock screen was still showing {} s after it \
         was dismissed",
        .after.as_secs()
    )]
    LockScreenStayed { serial: String, after: Duration },
    #[error("{serial}: typing the credential failed (adb exited {code})")]
    Typing { serial: String, code: i32 },
    #[error(transparent)]
    Adb(#[from] AdbError),
}

/// `PIN` stays `PIN`; `PASSWORD` reads `password`.
fn spoken(kind: &str) -> String {
    if kind == "PIN" {
        kind.to_string()
    } else {
        kind.to_ascii_lowercase().replace('_', " ")
    }
}

/// `isBouncerShowing(): true` in system UI's dump, from its
/// `StatusBarKeyguardViewManager` section.
#[must_use]
pub fn bouncer_showing(systemui_dump: &str) -> Option<bool> {
    systemui_dump
        .lines()
        .find_map(|l| l.trim().strip_prefix("isBouncerShowing():"))
        .and_then(|v| match v.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

impl AdbClient {
    /// Leaves `serial` unlocked — its user unlocked and its lock screen
    /// gone — entering `credential` on the bouncer when it has one.
    ///
    /// # Errors
    ///
    /// [`UnlockError::NeedsCredential`] / [`UnlockError::NeedsCredentialAgain`]
    /// when it is locked behind a credential and none was given;
    /// [`UnlockError::StillLocked`] when the one given did not unlock it.
    pub async fn unlock_user(
        &self,
        serial: &str,
        credential: Option<&str>,
    ) -> Result<Unlock, UnlockError> {
        let Some(user_locked) = self.user_locked(serial).await? else {
            return Ok(Unlock::Unknown);
        };
        // a `dumpsys window` without the line leaves the user's state to decide, as
        // it did before the lock screen was read
        match (user_locked, self.keyguard_showing(serial).await?) {
            (false, Some(false)) => return Ok(Unlock::WasNotLocked),
            (false, None) => return Ok(Unlock::Unknown),
            _ => {}
        }
        let settings = self.shell(serial, &["dumpsys", "lock_settings"]).await?;
        let kind = credential_type(&settings)
            .unwrap_or("credential")
            .to_string();
        let serial_s = || serial.to_string();
        match kind.as_str() {
            "NONE" => {
                if user_locked && !self.unlocked_within(serial, UNLOCKS_ITSELF_WITHIN).await? {
                    return Err(UnlockError::DidNotUnlockItself {
                        serial: serial_s(),
                        after: UNLOCKS_ITSELF_WITHIN,
                    });
                }
                if self.keyguard_showing(serial).await? != Some(true) {
                    return Ok(Unlock::WasNotLocked);
                }
                self.shell(serial, &["input", "keyevent", "KEYCODE_WAKEUP"])
                    .await?;
                self.shell(serial, &["wm", "dismiss-keyguard"]).await?;
                return if self.open_within(serial, DISMISSED_WITHIN).await? {
                    Ok(Unlock::Dismissed)
                } else {
                    Err(UnlockError::LockScreenStayed {
                        serial: serial_s(),
                        after: DISMISSED_WITHIN,
                    })
                };
            }
            "PATTERN" => return Err(UnlockError::Pattern { serial: serial_s() }),
            _ => {}
        }
        let Some(credential) = credential else {
            return Err(if user_locked {
                UnlockError::NeedsCredential {
                    serial: serial_s(),
                    credential: spoken(&kind),
                }
            } else {
                UnlockError::NeedsCredentialAgain {
                    serial: serial_s(),
                    credential: spoken(&kind),
                }
            });
        };
        self.raise_bouncer(serial).await?;
        // the error of a failed `input text` names its arguments
        self.shell(serial, &["input", "text", credential])
            .await
            .map_err(|e| UnlockError::Typing {
                serial: serial_s(),
                code: match e {
                    AdbError::NonZeroExit { code, .. } => code,
                    _ => -1,
                },
            })?;
        self.shell(serial, &["input", "keyevent", "KEYCODE_ENTER"])
            .await?;
        if self.open_within(serial, ENTRY_TAKES_HOLD_WITHIN).await? {
            return Ok(Unlock::Entered);
        }
        Err(UnlockError::StillLocked {
            serial: serial_s(),
            credential: spoken(&kind),
            after: ENTRY_TAKES_HOLD_WITHIN,
        })
    }

    /// Wakes the device and dismisses the keyguard until the bouncer's
    /// entry field is on screen.
    async fn raise_bouncer(&self, serial: &str) -> Result<(), UnlockError> {
        let deadline = Instant::now() + BOUNCER_UP_WITHIN;
        loop {
            let looked = Instant::now();
            self.shell(serial, &["input", "keyevent", "KEYCODE_WAKEUP"])
                .await?;
            self.shell(serial, &["wm", "dismiss-keyguard"]).await?;
            if bouncer_showing(&self.shell(serial, &[BOUNCER_READ]).await?) == Some(true) {
                return Ok(());
            }
            if looked >= deadline {
                return Err(UnlockError::NoBouncer {
                    serial: serial.to_string(),
                    after: BOUNCER_UP_WITHIN,
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn user_locked(&self, serial: &str) -> Result<Option<bool>, AdbError> {
        Ok(user_is_locked(
            &self.shell(serial, &["dumpsys", "user"]).await?,
        ))
    }

    async fn keyguard_showing(&self, serial: &str) -> Result<Option<bool>, AdbError> {
        Ok(keyguard_showing(
            &self.shell(serial, &["dumpsys", "window"]).await?,
        ))
    }

    /// Whether the user is unlocked and the lock screen not showing within
    /// `within`.
    async fn open_within(&self, serial: &str, within: Duration) -> Result<bool, AdbError> {
        let deadline = Instant::now() + within;
        loop {
            let looked = Instant::now();
            if self.user_locked(serial).await? == Some(false)
                && self.keyguard_showing(serial).await? != Some(true)
            {
                return Ok(true);
            }
            if looked >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn unlocked_within(&self, serial: &str, within: Duration) -> Result<bool, AdbError> {
        let deadline = Instant::now() + within;
        loop {
            let looked = Instant::now();
            if self.user_locked(serial).await? == Some(false) {
                return Ok(true);
            }
            if looked >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credential_type_is_read_and_only_plain_credentials_are_typed() {
        // `dumpsys lock_settings` on an API 36 emulator, the PIN set
        let set = "User State:\n  User 0\n    CredentialType: PIN\n    Quality: 0\n";
        assert_eq!(credential_type(set), Some("PIN"));
        assert_eq!(credential_type("User State:\n  User 0\n"), None);
        assert!(typeable_credential("1357"));
        assert!(typeable_credential("abc123"));
        for refused in ["", "12 34", "1;reboot", "$(id)", "12345678901234567"] {
            assert!(!typeable_credential(refused), "{refused:?}");
        }
    }

    #[test]
    fn the_bouncer_is_read_from_system_ui() {
        // what BOUNCER_READ prints on the API 36.1 emulator, the PIN pad up
        // after `wm dismiss-keyguard`, then after the PIN went in
        assert_eq!(
            bouncer_showing("      isBouncerShowing(): true\n"),
            Some(true)
        );
        assert_eq!(
            bouncer_showing("      isBouncerShowing(): false\n"),
            Some(false)
        );
        assert_eq!(bouncer_showing(""), None);
        assert_eq!(spoken("PIN"), "PIN");
        assert_eq!(spoken("PASSWORD"), "password");
    }

    #[test]
    fn the_lock_screen_is_read_from_dumpsys_window() {
        // `dumpsys window` on the API 36.1 emulator: its user RUNNING_UNLOCKED,
        // the lock screen back after the screen went off and on, then entered
        let shown = "    mKeyguardDrawComplete=false mWindowManagerDrawComplete=false\n    isKeyguardShowing=true\n";
        let gone = "    mKeyguardDrawComplete=true mWindowManagerDrawComplete=true\n    isKeyguardShowing=false\n";
        assert_eq!(keyguard_showing(shown), Some(true));
        assert_eq!(keyguard_showing(gone), Some(false));
        assert_eq!(keyguard_showing("    deviceHasKeyguard=true\n"), None);
    }
}
