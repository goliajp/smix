//! Which AVD `smix sim boot` starts for an emulator that is not running.
//!
//! An emulator serial names a console port, not a device: every AVD that
//! has ever run on 5554 was `emulator-5554`, and a machine's registry
//! ends up with several rows at that serial, each naming a different AVD.
//! Picking one of them for `smix sim boot emulator-5554` starts whichever
//! AVD the lookup happened to reach first — on 2026-09-29 that was another
//! project's emulator, because a row somebody had registered under the
//! alias `emulator-5554` sorted ahead of ours. A serial names the AVD to
//! start only when every row at that port agrees on it.

/// The AVD to start, or why the reference does not say which.
///
/// `alias` is the row an alias named, checked by identity, with the AVD it
/// records (`None` when it records none). `at_port` is `(alias, avd)` for
/// every row registered at `serial`, and is only read when no alias
/// named the device.
pub(crate) fn avd_to_start(
    device_ref: &str,
    serial: &str,
    alias: Option<Option<&str>>,
    at_port: &[(&str, &str)],
) -> Result<String, String> {
    if let Some(avd) = alias {
        return avd
            .map(str::to_string)
            .ok_or_else(|| no_avd_on_record(serial));
    }
    let mut avds: Vec<&str> = at_port.iter().map(|(_, avd)| *avd).collect();
    avds.sort_unstable();
    avds.dedup();
    match avds.as_slice() {
        [] => Err(no_avd_on_record(serial)),
        [one] => Ok((*one).to_string()),
        _ => {
            let mut rows: Vec<String> = at_port
                .iter()
                .map(|(alias, avd)| format!("  {alias} → {avd}"))
                .collect();
            rows.sort();
            Err(format!(
                "{device_ref} is a console port, not a device, and {} AVDs are registered \
                 on it:\n{}\nBoot the one you mean by its alias — `smix sim boot <alias>` — \
                 so smix starts that AVD and not whichever row it reaches first.",
                avds.len(),
                rows.join("\n")
            ))
        }
    }
}

fn no_avd_on_record(serial: &str) -> String {
    format!(
        "{serial} has no AVD name on record, so there is nothing to start. Register it \
         once while it is running — `smix sim register <alias> --udid {serial} --kind \
         emulator` — and the name is kept."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // The rows at emulator-5554 on the day this went wrong.
    const AT_5554: &[(&str, &str)] = &[
        ("android", "sim-smix-android-01"),
        ("emu5554", "sim-smix-android-01"),
        ("emulator-5554", "qip-consumer-36"),
        ("sim-smix-android-01", "sim-smix-android-01"),
    ];

    #[test]
    fn a_serial_several_avds_have_run_on_names_none_of_them() {
        let err = avd_to_start("emulator-5554", "emulator-5554", None, AT_5554).unwrap_err();
        assert!(err.contains("2 AVDs"), "{err}");
        assert!(err.contains("emulator-5554 → qip-consumer-36"), "{err}");
        assert!(
            err.contains("sim-smix-android-01 → sim-smix-android-01"),
            "{err}"
        );
    }

    #[test]
    fn a_serial_every_row_agrees_on_names_that_avd() {
        let rows = &[
            ("android", "sim-smix-android-01"),
            ("emu5554", "sim-smix-android-01"),
        ];
        assert_eq!(
            avd_to_start("emulator-5554", "emulator-5554", None, rows).unwrap(),
            "sim-smix-android-01"
        );
    }

    #[test]
    fn an_alias_names_its_own_avd_whatever_else_ran_on_the_port() {
        assert_eq!(
            avd_to_start(
                "sim-smix-android-01",
                "emulator-5554",
                Some(Some("sim-smix-android-01")),
                AT_5554
            )
            .unwrap(),
            "sim-smix-android-01"
        );
    }

    #[test]
    fn nothing_on_record_says_so() {
        let err = avd_to_start("emulator-5570", "emulator-5570", None, &[]).unwrap_err();
        assert!(err.contains("no AVD name on record"), "{err}");
        let err = avd_to_start("x", "emulator-5570", Some(None), AT_5554).unwrap_err();
        assert!(err.contains("no AVD name on record"), "{err}");
    }
}
