//! Whether `smix sim boot --cold` can do what it says on this device.
//!
//! Only an Android emulator has a Quick Boot snapshot to skip. Anywhere
//! else the flag would change nothing, and saying nothing about that
//! would read as a cold boot that happened.

use smix_simctl::registry::DeviceKind;

/// Why `--cold` cannot be honoured, or `None` when it can.
pub(crate) fn cold_boot_refusal(device: &str, kind: DeviceKind, running: bool) -> Option<String> {
    match kind {
        DeviceKind::Emulator if running => Some(format!(
            "{device} is already running, so there is no boot to make cold. Stop it \
             first — `smix sim shutdown {device}` — then `smix sim boot {device} --cold`."
        )),
        DeviceKind::Emulator => None,
        DeviceKind::Simulator => Some(format!(
            "{device} is an iOS simulator, which has no boot snapshot to skip: every \
             `simctl boot` starts it from its data. Boot it without --cold."
        )),
        DeviceKind::PhysicalIos | DeviceKind::PhysicalAndroid => Some(format!(
            "{device} is a physical device; smix does not boot phones, so there is \
             nothing for --cold to change."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emulator_that_is_off_boots_cold() {
        assert_eq!(cold_boot_refusal("e", DeviceKind::Emulator, false), None);
    }

    #[test]
    fn a_running_emulator_is_told_to_stop_first() {
        let why = cold_boot_refusal("e", DeviceKind::Emulator, true).expect("refused");
        assert!(why.contains("smix sim shutdown e"), "{why}");
    }

    #[test]
    fn every_other_kind_is_refused_by_name() {
        for kind in [
            DeviceKind::Simulator,
            DeviceKind::PhysicalIos,
            DeviceKind::PhysicalAndroid,
        ] {
            assert!(cold_boot_refusal("d", kind, false).is_some(), "{kind:?}");
        }
    }
}
