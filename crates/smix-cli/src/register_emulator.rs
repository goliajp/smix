//! `smix sim register … --kind emulator`: write down an Android emulator,
//! running or not.
//!
//! An emulator used to be registered only while it ran, because the AVD
//! name was read off the running device. With devices started only
//! through smix, that left a new AVD with no way in: it had to run before
//! smix could start it. `--avd` names the AVD outright, checked against the
//! AVD directory, and the serial's port is where `smix sim boot` puts it.

use std::path::PathBuf;

use crate::CliError;

/// Where the emulator keeps its AVDs: `ANDROID_AVD_HOME`, else
/// `~/.android/avd`.
pub(crate) fn avd_home() -> Option<PathBuf> {
    std::env::var_os("ANDROID_AVD_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".android/avd")))
}

/// What is known about the serial when it is registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Seen {
    /// adb lists it; the AVD it runs, when the emulator said.
    Running(Option<String>),
    /// adb does not list it.
    Off,
}

/// The AVD name to record for `serial`, or why it cannot be registered.
/// `asked` is `--avd`; `asked_exists` whether that AVD is on this machine.
pub(crate) fn avd_to_record(
    serial: &str,
    seen: &Seen,
    asked: Option<&str>,
    asked_exists: bool,
) -> Result<Option<String>, String> {
    if let Some(a) = asked
        && !asked_exists
    {
        return Err(format!(
            "there is no AVD named {a:?} here (looked for {a}.ini in {}). \
             `emulator -list-avds` names the ones there are.",
            avd_home().map_or_else(|| "~/.android/avd".into(), |p| p.display().to_string())
        ));
    }
    match (seen, asked) {
        (Seen::Running(Some(running)), Some(a)) if running != a => Err(format!(
            "{serial} is running the AVD {running:?}, not {a:?}; register it with \
             --avd {running}, or use another serial for {a}"
        )),
        (Seen::Running(running), asked) => Ok(asked.map(str::to_string).or(running.clone())),
        (Seen::Off, Some(a)) => Ok(Some(a.to_string())),
        (Seen::Off, None) => Err(format!(
            "adb lists no running device {serial}, so nothing says which AVD it is. \
             Name it: --avd <AVD name> registers it while it is off, and \
             `smix sim boot <alias>` then starts that AVD on {serial}'s port."
        )),
    }
}

/// Register an emulator under `alias`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn register(
    alias: &str,
    serial: &str,
    asked_avd: Option<&str>,
    name: Option<String>,
    locale: Option<String>,
    runner_port: Option<u16>,
    kind: smix_simctl::registry::DeviceKind,
) -> Result<(), CliError> {
    let seen = if crate::adb_knows(serial) {
        Seen::Running(
            smix_adb::AdbClient::new()
                .avd_name(serial)
                .await
                .ok()
                .filter(|a| !a.is_empty()),
        )
    } else {
        Seen::Off
    };
    let exists = asked_avd
        .is_some_and(|a| avd_home().is_some_and(|home| home.join(format!("{a}.ini")).is_file()));
    let avd = avd_to_record(serial, &seen, asked_avd, exists).map_err(CliError::Other)?;
    let path = crate::registry_path()?;
    let outcome = smix_simctl::registry::SimRegistry::register(
        &path,
        alias,
        smix_simctl::registry::RegisteredSim {
            device_name: name.unwrap_or_else(|| alias.to_string()),
            udid: serial.to_string(),
            runtime: String::new(),
            device_type: String::new(),
            avd_name: avd.clone(),
            locale,
            runner_port,
            kind,
            destructive_opt_in: false,
        },
    )?;
    let verb = match outcome {
        smix_simctl::registry::RegisterOutcome::Added => "registered",
        smix_simctl::registry::RegisterOutcome::Updated => "updated",
    };
    let state = match (&seen, &avd) {
        (Seen::Running(_), _) => String::new(),
        (Seen::Off, Some(a)) => format!(", AVD {a}, off — `smix sim boot {alias}` starts it"),
        (Seen::Off, None) => String::new(),
    };
    println!(
        "{verb}: {alias} → {serial} (Android emulator{state}) in {}",
        smix_simctl::registry::store_dir(&path).display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emulator_that_is_off_is_registered_by_the_avd_named() {
        let got = avd_to_record("emulator-5574", &Seen::Off, Some("qip-chromeos-01"), true);
        assert_eq!(got, Ok(Some("qip-chromeos-01".into())));
    }

    #[test]
    fn an_emulator_that_is_off_and_unnamed_is_refused_with_the_way_in() {
        let why = avd_to_record("emulator-5574", &Seen::Off, None, false).unwrap_err();
        assert!(why.contains("--avd"), "{why}");
        assert!(why.contains("emulator-5574"), "{why}");
    }

    #[test]
    fn an_avd_that_is_not_on_this_machine_is_refused() {
        let why = avd_to_record("emulator-5574", &Seen::Off, Some("nope"), false).unwrap_err();
        assert!(why.contains("no AVD named \"nope\""), "{why}");
    }

    #[test]
    fn a_running_emulator_is_recorded_as_what_it_runs() {
        let running = Seen::Running(Some("sim-a".into()));
        assert_eq!(
            avd_to_record("emulator-5554", &running, None, false),
            Ok(Some("sim-a".into()))
        );
        assert_eq!(
            avd_to_record("emulator-5554", &running, Some("sim-a"), true),
            Ok(Some("sim-a".into()))
        );
        let why = avd_to_record("emulator-5554", &running, Some("sim-b"), true).unwrap_err();
        assert!(why.contains("running the AVD \"sim-a\""), "{why}");
    }
}
