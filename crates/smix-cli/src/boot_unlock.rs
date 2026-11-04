//! `smix sim boot --unlock-pin`: an emulator with a lock-screen
//! credential answers `booted` only once its user is unlocked.
//!
//! The credential is the AVD's own, so it comes from whoever boots it —
//! the flag, or `SMIX_UNLOCK_PIN` as the default — and is never written
//! to the ledger or anywhere else.

use smix_simctl::registry::DeviceKind;

const ENV: &str = "SMIX_UNLOCK_PIN";

/// The credential to enter: the flag, else the environment's default.
/// Refused when it cannot be typed as given, or when the flag names a
/// device that has no lock screen to enter it on.
pub(crate) fn credential(
    device: &str,
    kind: DeviceKind,
    flag: Option<String>,
) -> Result<Option<String>, String> {
    if flag.is_some() && kind != DeviceKind::Emulator {
        return Err(format!(
            "{device} is not an Android emulator, so there is no lock screen for \
             --unlock-pin to unlock. Boot it without the flag."
        ));
    }
    let given = flag.or_else(|| std::env::var(ENV).ok().filter(|v| !v.is_empty()));
    match given {
        Some(c) if !smix_adb::typeable_credential(&c) => Err(format!(
            "the unlock credential (--unlock-pin or {ENV}) must be 1–16 ASCII letters or \
             digits; what was given is not, and is not shown here"
        )),
        given => Ok(given),
    }
}

/// Leaves `serial`'s user unlocked, entering `credential` if it is locked
/// behind one.
pub(crate) async fn leave_unlocked(serial: &str, credential: Option<&str>) -> Result<(), String> {
    match smix_adb::AdbClient::new()
        .unlock_user(serial, credential)
        .await
    {
        Ok(smix_adb::Unlock::Entered) => {
            eprintln!("unlocked: {serial} (its credential was entered)");
            Ok(())
        }
        Ok(smix_adb::Unlock::WasNotLocked) => Ok(()),
        Ok(smix_adb::Unlock::Unknown) => {
            eprintln!(
                "note: {serial} did not say whether its user is locked (`dumpsys user`); \
                 nothing was entered"
            );
            Ok(())
        }
        Err(e) => Err(format!("booted, but {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_is_refused_where_there_is_no_lock_screen_and_bad_input_is_not_echoed() {
        let refused = credential("sim-a", DeviceKind::Simulator, Some("1357".into()));
        assert!(refused.unwrap_err().contains("not an Android emulator"));
        assert_eq!(
            credential("emulator-5554", DeviceKind::Emulator, Some("1357".into())),
            Ok(Some("1357".into()))
        );
        let bad = credential(
            "emulator-5554",
            DeviceKind::Emulator,
            Some("13 57;x".into()),
        );
        let why = bad.unwrap_err();
        assert!(why.contains("1–16 ASCII letters"), "{why}");
        assert!(!why.contains("13 57"), "{why}");
    }
}
