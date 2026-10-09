//! Android's animation scales: zeroed for a run, put back after it.
//!
//! The scales are global settings of the device, and they outlive the
//! run that wrote them. A consumer's app reads `animator_duration_scale
//! == 0` as the user's reduce-motion preference, so after one `smix
//! run` their emulator's app stayed in reduced motion until somebody
//! opened developer options. Putting `1` back is not putting back: a
//! fresh emulator has no `animator_duration_scale` key at all, and
//! `settings get` prints `null` for it. What is put back is what was
//! read, key by key — a value verbatim, an absent key deleted.
//!
//! Which values those were is kept on the machine, not in this
//! process: a run killed by SIGKILL never reaches its restore, and the
//! next run on that device would otherwise read the zeros it left as
//! the device's own. See `smix_lease::held`.

use smix_adb::AdbClient;
use smix_lease::ProcIdentity;
use smix_lease::held::{self, HeldDir, Settings};
use smix_lease::store::{identify_self, probe};
use smix_simctl::DeviceControlError;

/// The animation settings smix zeroes on Android, in the order it
/// writes them.
///
/// All three matter and they are separately settable: window covers
/// activity windows, transition covers activity-to-activity, animator
/// covers `ObjectAnimator` — a screen can sit still by two of these
/// and keep moving by the third.
pub const ANDROID_ANIMATION_SCALES: [&str; 3] = [
    "window_animation_scale",
    "transition_animation_scale",
    "animator_duration_scale",
];

/// Whether an Android scale reads as off, however the device spells
/// zero (`0`, `0.0`).
fn scale_is_off(value: &str) -> bool {
    value.trim().parse::<f64>().is_ok_and(|n| n == 0.0)
}

/// Judge what a device gave back after being told to stop animating.
///
/// Takes `(setting, value-read-back)` pairs and returns the ones that
/// did not take. A device is not trusted to have honoured a write:
/// `simctl ui appearance` is documented per-simulator and behaves
/// globally, and this project has been bitten by exactly that shape
/// before. A switch that reports success while the device kept its
/// animations is worse than no switch, because the run that follows
/// looks deterministic and is not.
///
/// Two vocabularies, because two platforms: an Android scale is off at
/// `0`, however it is spelled (`0`, `0.0`); iOS Reduce Motion is on at
/// `1`. Anything else — including the empty string and `null` that
/// `settings get` prints for a key never written — is a failure that
/// names itself.
pub fn animation_settings_verified(read_back: &[(&str, &str)]) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for (setting, value) in read_back {
        let v = value.trim();
        let ok = if setting.starts_with("UIAccessibility") {
            v == "1"
        } else {
            scale_is_off(v)
        };
        if !ok {
            let seen = if v.is_empty() { "<empty>" } else { v };
            bad.push(format!(
                "{setting} read back as {seen}, so the device did not take it"
            ));
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

/// What `settings get global` printed, as a value or an absence.
///
/// `null` is the literal the command prints for a key the device does
/// not have; nothing a scale can hold spells that way.
fn setting_read(printed: &str) -> Option<String> {
    let v = printed.trim();
    if v == "null" {
        None
    } else {
        Some(v.to_string())
    }
}

fn shell_err(e: smix_adb::AdbError, subcommand: &str) -> DeviceControlError {
    super::android_device::adb_to_simctl_err(e, subcommand)
}

fn held_dir() -> Result<HeldDir, DeviceControlError> {
    HeldDir::machine().ok_or_else(|| {
        DeviceControlError::non_zero_exit(
            "settings",
            -1,
            "this machine has nowhere to record the device's animation \
             settings before changing them: neither SMIX_MACHINE_DIR, \
             XDG_DATA_HOME nor HOME is set",
        )
    })
}

fn holder_alive(h: &ProcIdentity) -> bool {
    probe(h).identity_matches
}

async fn read_scales(client: &AdbClient, serial: &str) -> Result<Settings, DeviceControlError> {
    let mut current = Settings::new();
    for setting in ANDROID_ANIMATION_SCALES {
        let printed = client
            .shell(serial, &["settings", "get", "global", setting])
            .await
            .map_err(|e| shell_err(e, "shell settings get"))?;
        current.insert(setting.to_string(), setting_read(&printed));
    }
    Ok(current)
}

/// Zero the scales for this process, recording what they were if this
/// process is the first holder. Read back and refused if they did not
/// take; a later holder of a device another run already quietened
/// registers and writes nothing.
pub(crate) async fn hold(client: &AdbClient, serial: &str) -> Result<(), DeviceControlError> {
    let current = read_scales(client, serial).await?;
    let joined = held::join(
        &held_dir()?,
        serial,
        identify_self(),
        &current,
        scale_is_off,
        holder_alive,
    )
    .map_err(|e| DeviceControlError::non_zero_exit("settings", -1, e.to_string()))?;
    if !joined.first {
        return Ok(());
    }
    for setting in ANDROID_ANIMATION_SCALES {
        client
            .shell(serial, &["settings", "put", "global", setting, "0"])
            .await
            .map_err(|e| shell_err(e, "shell settings put"))?;
    }
    let read_back = read_scales(client, serial).await?;
    let pairs: Vec<(&str, &str)> = read_back
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_deref().unwrap_or("null")))
        .collect();
    animation_settings_verified(&pairs).map_err(|bad| {
        DeviceControlError::non_zero_exit(
            "shell settings get",
            1,
            format!(
                "animations were not quietened on {serial}: {}",
                bad.join("; ")
            ),
        )
    })
}

/// Leave the device's holders; when this process was the last one
/// alive, put the recorded scales back exactly — a value verbatim, an
/// absent key deleted — and only then let the record go. Nothing
/// recorded means nothing to put back.
pub(crate) async fn release(client: &AdbClient, serial: &str) -> Result<(), DeviceControlError> {
    let lease_err = |e: smix_lease::store::LeaseError| {
        DeviceControlError::non_zero_exit("settings", -1, e.to_string())
    };
    let last =
        held::leave(&held_dir()?, serial, &identify_self(), holder_alive).map_err(lease_err)?;
    let Some(last) = last else {
        return Ok(());
    };
    for (setting, value) in last.originals() {
        let cmd: Vec<&str> = match value {
            Some(v) => vec!["settings", "put", "global", setting, v],
            None => vec!["settings", "delete", "global", setting],
        };
        client
            .shell(serial, &cmd)
            .await
            .map_err(|e| shell_err(e, "shell settings"))?;
    }
    last.restored().map_err(lease_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_is_an_absence_and_anything_else_is_a_value() {
        assert_eq!(setting_read("null\n"), None);
        assert_eq!(setting_read("1.0\n").as_deref(), Some("1.0"));
        assert_eq!(setting_read("0").as_deref(), Some("0"));
        assert_eq!(setting_read("").as_deref(), Some(""));
    }

    #[test]
    fn off_is_zero_however_spelled() {
        assert!(scale_is_off("0"));
        assert!(scale_is_off("0.0\n"));
        assert!(!scale_is_off("1"));
        assert!(!scale_is_off("null"));
        assert!(!scale_is_off(""));
    }
}
