//! Entering the lock-screen credential of an emulator that booted locked.
//!
//! With a PIN, pattern or password set, the user stays locked after a
//! boot until the credential is entered, and no app that is not
//! direct-boot aware can start. The credential is typed on the bouncer,
//! which needs no window on the host: wake, dismiss the keyguard, type,
//! Enter.

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
/// How long the user state takes to turn after Enter on the bouncer.
const ENTRY_TAKES_HOLD_WITHIN: Duration = Duration::from_secs(8);
/// Where the bouncer is read to: `/sdcard` does not exist while the user
/// is locked.
const DUMP: &str = "/data/local/tmp/smix-bouncer.xml";

/// `CredentialType: PIN` in `dumpsys lock_settings`; `NONE` without one.
#[must_use]
pub fn credential_type(dumpsys_lock_settings: &str) -> Option<&str> {
    dumpsys_lock_settings
        .lines()
        .find_map(|l| l.trim().strip_prefix("CredentialType:"))
        .map(str::trim)
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
    /// The user was unlocked already, or unlocked itself having no credential.
    WasNotLocked,
    /// The credential was entered and the user is unlocked.
    Entered,
    /// `dumpsys user` did not say.
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

/// Whether a UI dump holds the field the bouncer types into.
#[must_use]
pub fn bouncer_is_up(ui_dump: &str) -> bool {
    [
        "com.android.systemui:id/pinEntry",
        "com.android.systemui:id/passwordEntry",
    ]
    .iter()
    .any(|id| ui_dump.contains(&format!("resource-id=\"{id}\"")))
}

impl AdbClient {
    /// Leaves the user of `serial` unlocked, entering `credential` on the
    /// bouncer when the device has one and is locked.
    ///
    /// # Errors
    ///
    /// [`UnlockError::NeedsCredential`] when it is locked behind a
    /// credential and none was given; [`UnlockError::StillLocked`] when
    /// the one given did not unlock it.
    pub async fn unlock_user(
        &self,
        serial: &str,
        credential: Option<&str>,
    ) -> Result<Unlock, UnlockError> {
        match self.user_locked(serial).await? {
            None => return Ok(Unlock::Unknown),
            Some(false) => return Ok(Unlock::WasNotLocked),
            Some(true) => {}
        }
        let settings = self.shell(serial, &["dumpsys", "lock_settings"]).await?;
        let kind = credential_type(&settings)
            .unwrap_or("credential")
            .to_string();
        let serial_s = || serial.to_string();
        match kind.as_str() {
            "NONE" => {
                return if self.unlocked_within(serial, UNLOCKS_ITSELF_WITHIN).await? {
                    Ok(Unlock::WasNotLocked)
                } else {
                    Err(UnlockError::DidNotUnlockItself {
                        serial: serial_s(),
                        after: UNLOCKS_ITSELF_WITHIN,
                    })
                };
            }
            "PATTERN" => return Err(UnlockError::Pattern { serial: serial_s() }),
            _ => {}
        }
        let Some(credential) = credential else {
            return Err(UnlockError::NeedsCredential {
                serial: serial_s(),
                credential: spoken(&kind),
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
        if self
            .unlocked_within(serial, ENTRY_TAKES_HOLD_WITHIN)
            .await?
        {
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
        let read = format!("uiautomator dump {DUMP} >/dev/null && cat {DUMP}; rm -f {DUMP}");
        loop {
            let looked = Instant::now();
            self.shell(serial, &["input", "keyevent", "KEYCODE_WAKEUP"])
                .await?;
            self.shell(serial, &["wm", "dismiss-keyguard"]).await?;
            if bouncer_is_up(&self.shell(serial, &[read.as_str()]).await?) {
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
    fn the_bouncer_is_up_when_its_entry_field_is_on_screen() {
        // from `uiautomator dump` on the API 36 emulator, locked, before
        // and after `wm dismiss-keyguard` brought the PIN pad up
        let pad = r#"<node resource-id="com.android.systemui:id/keyguard_pin_view"><node resource-id="com.android.systemui:id/pinEntry" class="android.widget.EditText"/></node>"#;
        let lock = r#"<node resource-id="com.android.systemui:id/keyguard_long_press"/><node resource-id="com.android.systemui:id/keyguard_indication_area"/>"#;
        assert!(bouncer_is_up(pad));
        assert!(!bouncer_is_up(lock));
        assert_eq!(spoken("PIN"), "PIN");
        assert_eq!(spoken("PASSWORD"), "password");
    }
}
