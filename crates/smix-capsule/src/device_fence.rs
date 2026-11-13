//! Refuse to drive a device whose life smix does not answer for.
//!
//! A hook sees the commands a session types; it cannot see a script that
//! spawns the emulator binary or calls `simctl boot` itself. What every
//! such device has in common is that the ledger has no row for it: smix
//! did not boot it and nobody claimed it. So the check sits where smix
//! starts driving, and reads the ledger rather than the command line.

use smix_lease::store::{self, LeaseDir};

/// Why `device` may not be driven, or `None` when it may.
///
/// `device` is a simulator UDID or an emulator serial. A ledger that
/// cannot be read refuses too: saying yes there would make the fence as
/// good as absent on any machine whose ledger is broken.
pub fn refusal(ledger: &LeaseDir, device: &str) -> Option<String> {
    let lease = match store::read(ledger, device) {
        Ok(lease) => lease,
        Err(e) => return Some(format!("the device ledger could not be read: {e}")),
    };
    if smix_lease::may_drive(lease.as_ref()) {
        return None;
    }
    Some(unanswered_message(device))
}

/// The refusal for a running device nothing here booted or claimed.
#[must_use]
pub fn unanswered_message(device: &str) -> String {
    format!(
        "{device} was not started by smix and nobody has claimed it, so smix \
         will not drive it. A device started outside smix (a bare `simctl \
         boot`, `emulator -avd`, a script spawning it) has no owner that will \
         unlock it, shut it down or know it is there.\n\
         Either have whoever started it stop it, then start it with \
         `smix sim boot {device}`, or take it on, with the reason, which is \
         written down:\n  \
         smix lease claim {device} --reason \"<why>\""
    )
}

/// [`refusal`] against this machine's ledger.
pub fn check(device: &str) -> Result<(), String> {
    match refusal(&crate::runner::machine_leases()?, device) {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> (tempfile::TempDir, LeaseDir) {
        let t = tempfile::tempdir().expect("tempdir");
        let d = LeaseDir::at(t.path().to_path_buf());
        (t, d)
    }

    #[test]
    fn a_device_with_no_ledger_is_refused_and_told_the_way_out() {
        let (_t, d) = ledger();
        let why = refusal(&d, "emulator-5560").expect("refused");
        assert!(
            why.contains("smix lease claim emulator-5560 --reason"),
            "{why}"
        );
        assert!(why.contains("smix sim boot emulator-5560"), "{why}");
    }

    #[test]
    fn a_device_found_already_up_is_refused() {
        let (_t, d) = ledger();
        store::record_boot(&d, "emulator-5560", false).expect("record");
        assert!(refusal(&d, "emulator-5560").is_some());
    }

    #[test]
    fn a_device_smix_booted_or_someone_claimed_is_driven() {
        let (_t, d) = ledger();
        store::record_boot(&d, "emulator-5554", true).expect("record");
        assert_eq!(refusal(&d, "emulator-5554"), None);
        store::record_claim(&d, "emulator-5556", "taken on by hand").expect("claim");
        assert_eq!(refusal(&d, "emulator-5556"), None);
    }
}
