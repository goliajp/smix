//! What a run changed about a device's environment, and what it was before.
//!
//! A flow's `setEnvironment` restores what it changed when the run ends.
//! A run that is killed never gets there, so the values from before are
//! kept in the device's ledger as they are changed: `smix cleanup` puts
//! them back from there, and `smix status` shows the device as changed
//! and by whom.

use serde::{Deserialize, Serialize};

use crate::Resource;
use crate::store::{self, LeaseDir, LeaseError};

pub mod commands;
use commands::{Command, Tool};

/// Makes the `adb` command a step runs, the device not yet named.
pub type Adb<'a> = &'a dyn Fn() -> Result<std::process::Command, String>;

/// One setting a run changed, and its value before the run touched it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvChange {
    /// Which setting: `appearance`, `textSize`, `timeZone`, `locale`,
    /// `network`.
    pub setting: String,
    /// Its value before, as the device reported it.
    pub before: String,
    /// The app a per-app setting (Android's app locale) belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

/// `changes` with `change` added. A setting changed twice keeps the value
/// from before the first change: that is the one to put back.
#[must_use]
pub fn with_change(changes: &[EnvChange], change: EnvChange) -> Vec<EnvChange> {
    let mut out = changes.to_vec();
    if !out
        .iter()
        .any(|c| c.setting == change.setting && c.app == change.app)
    {
        out.push(change);
    }
    out
}

/// The changes recorded against `device_id`, oldest first.
///
/// # Errors
///
/// The ledger could not be read.
pub fn recorded(dir: &LeaseDir, device_id: &str) -> Result<Vec<EnvChange>, LeaseError> {
    Ok(store::read(dir, device_id)?
        .and_then(|l| {
            l.known_resources().find_map(|r| match r {
                Resource::Environment { changes } => Some(changes.clone()),
                _ => None,
            })
        })
        .unwrap_or_default())
}

/// Write `change` into `device_id`'s ledger before the setting is changed.
///
/// # Errors
///
/// The ledger could not be read or written.
pub fn record(dir: &LeaseDir, device_id: &str, change: EnvChange) -> Result<(), LeaseError> {
    let changes = with_change(&recorded(dir, device_id)?, change);
    store::add_resource(dir, device_id, Resource::Environment { changes })
}

/// Forget the changes once they are put back.
///
/// # Errors
///
/// The ledger could not be read or written.
pub fn forget(dir: &LeaseDir, device_id: &str) -> Result<(), LeaseError> {
    store::drop_resource_kind(
        dir,
        device_id,
        &Resource::Environment {
            changes: Vec::new(),
        },
    )
}

/// Run one command against `device_id`; its standard output.
///
/// # Errors
///
/// It could not start, or it exited unsuccessfully; the words say which.
pub fn run(device_id: &str, c: &Command, adb: Adb<'_>) -> Result<String, String> {
    let mut p = match c.tool {
        Tool::Simctl => {
            let mut p = std::process::Command::new("xcrun");
            p.arg("simctl");
            p
        }
        Tool::AdbShell => {
            let mut p = adb()?;
            p.args(["-s", device_id, "shell"]);
            p
        }
        Tool::AdbEmu => {
            let mut p = adb()?;
            p.args(["-s", device_id, "emu"]);
            p
        }
    };
    let out = p
        .args(&c.args)
        .output()
        .map_err(|e| format!("{}: {e}", c.args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() && !stdout.starts_with("KO:") {
        Ok(stdout)
    } else {
        Err(format!(
            "`{}` answered {}{}",
            c.args.join(" "),
            stdout.trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// What `setting` holds on `device_id` now, in the device's spelling.
///
/// # Errors
///
/// It cannot be read there, or its answer was not understood.
pub fn current(
    device_id: &str,
    setting: &str,
    app: Option<&str>,
    adb: Adb<'_>,
) -> Result<String, String> {
    let answers = commands::read(device_id, setting, app)?
        .iter()
        .map(|c| run(device_id, c, adb))
        .collect::<Result<Vec<_>, _>>()?;
    commands::parse(device_id, setting, &answers)
}

/// Set `setting` to `value` (the device's spelling) and read it back.
///
/// # Errors
///
/// A command failed, or the setting reads back as something else.
pub fn put(
    device_id: &str,
    setting: &str,
    value: &str,
    app: Option<&str>,
    adb: Adb<'_>,
) -> Result<(), String> {
    for c in commands::set(device_id, setting, value, app)? {
        run(device_id, &c, adb)?;
    }
    let now = current(device_id, setting, app, adb)?;
    if now == value {
        Ok(())
    } else {
        Err(format!(
            "{setting} was set to {value:?} and reads back {now:?}"
        ))
    }
}

/// Put back every change, newest first; one line for each that failed.
pub fn restore(device_id: &str, changes: &[EnvChange], adb: Adb<'_>) -> Vec<String> {
    changes
        .iter()
        .rev()
        .filter_map(|c| {
            put(device_id, &c.setting, &c.before, c.app.as_deref(), adb)
                .err()
                .map(|e| format!("{} not put back to {:?}: {e}", c.setting, c.before))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(setting: &str, before: &str) -> EnvChange {
        EnvChange {
            setting: setting.into(),
            before: before.into(),
            app: None,
        }
    }

    #[test]
    fn a_setting_changed_twice_keeps_the_first_before() {
        let once = with_change(&[], change("appearance", "light"));
        let twice = with_change(&once, change("appearance", "dark"));
        assert_eq!(twice, vec![change("appearance", "light")]);
    }

    #[test]
    fn different_settings_and_apps_are_kept_apart() {
        let mut a = change("locale", "en-US");
        a.app = Some("a.b".into());
        let mut b = change("locale", "");
        b.app = Some("c.d".into());
        let all = with_change(
            &with_change(&[a.clone()], b.clone()),
            change("timeZone", "UTC"),
        );
        assert_eq!(all, vec![a, b, change("timeZone", "UTC")]);
    }

    #[test]
    fn changes_are_kept_and_forgotten_in_the_ledger() {
        let t = tempfile::tempdir().expect("tempdir");
        let d = LeaseDir::at(t.path());
        record(&d, "emulator-5554", change("appearance", "no")).expect("record");
        record(&d, "emulator-5554", change("textSize", "1.0")).expect("record");
        record(&d, "emulator-5554", change("appearance", "yes")).expect("record");
        assert_eq!(
            recorded(&d, "emulator-5554").expect("read"),
            vec![change("appearance", "no"), change("textSize", "1.0")]
        );
        forget(&d, "emulator-5554").expect("forget");
        assert!(recorded(&d, "emulator-5554").expect("read").is_empty());
    }
}
