//! Whether `smix sim boot --cold` can do what it says on this device.
//!
//! Only an Android emulator has a Quick Boot snapshot to skip. Anywhere
//! else the flag would change nothing, and saying nothing about that
//! would read as a cold boot that happened.

use smix_simctl::registry::DeviceKind;

/// Why the flags given cannot be honoured, or `None` when they can.
pub(crate) fn boot_refusal(
    device: &str,
    kind: DeviceKind,
    running: bool,
    cold: bool,
    no_window: bool,
) -> Option<String> {
    let cold = cold
        .then(|| cold_boot_refusal(device, kind, running))
        .flatten();
    cold.or_else(|| {
        no_window
            .then(|| no_window_refusal(device, kind, running))
            .flatten()
    })
}

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

/// Whether `SMIX_EMULATOR_NO_WINDOW` asks every emulator boot to leave its
/// window out, as `--no-window` does for one. A preference rather than a
/// demand: an emulator already running is left as it is, not refused. Set
/// by the e2e scripts, so a suite run on a machine somebody is using does
/// not open windows on their screen.
pub(crate) fn no_window_asked_by_env() -> bool {
    std::env::var("SMIX_EMULATOR_NO_WINDOW").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Why `--no-window` cannot be honoured, or `None` when it can. A
/// simulator `simctl` boots has no window to begin with.
pub(crate) fn no_window_refusal(device: &str, kind: DeviceKind, running: bool) -> Option<String> {
    match kind {
        DeviceKind::Emulator if running => Some(format!(
            "{device} is already running, with whatever window it was started with. Stop \
             it first — `smix sim shutdown {device}` — then `smix sim boot {device} --no-window`."
        )),
        DeviceKind::Emulator | DeviceKind::Simulator => None,
        DeviceKind::PhysicalIos | DeviceKind::PhysicalAndroid => Some(format!(
            "{device} is a physical device; smix does not boot phones, so there is \
             no window for --no-window to leave out."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_window_is_refused_where_it_would_change_nothing() {
        assert_eq!(no_window_refusal("e", DeviceKind::Emulator, false), None);
        assert_eq!(no_window_refusal("s", DeviceKind::Simulator, false), None);
        let why = no_window_refusal("e", DeviceKind::Emulator, true).expect("refused");
        assert!(why.contains("smix sim shutdown e"), "{why}");
        for kind in [DeviceKind::PhysicalIos, DeviceKind::PhysicalAndroid] {
            assert!(no_window_refusal("d", kind, false).is_some(), "{kind:?}");
        }
    }

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
