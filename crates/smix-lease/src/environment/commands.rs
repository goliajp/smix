//! How each setting is read and set, per platform: the commands and how
//! their answers read. Pure: whoever runs them — a flow, a cleanup — runs
//! the same ones.
//!
//! Values are kept in the device's own spelling (`extra-large`, `1.15`,
//! `Night mode: auto` read as `auto`), so what was read is what is put
//! back. A flow's spelling is turned into the device's by [`device_value`].

/// Which tool a command goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// `xcrun simctl …`
    Simctl,
    /// `adb -s <serial> shell …`
    AdbShell,
    /// `adb -s <serial> emu …`, an emulator's console.
    AdbEmu,
}

/// One command, without the device-addressing prefix its tool adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The tool.
    pub tool: Tool,
    /// Its arguments.
    pub args: Vec<String>,
}

fn cmd(tool: Tool, args: &[&str]) -> Command {
    Command {
        tool,
        args: args.iter().map(|a| (*a).to_string()).collect(),
    }
}

/// Whether `device_id` is an iOS simulator: a UDID, `8-4-4-4-12` hex.
#[must_use]
pub fn is_ios(device_id: &str) -> bool {
    let parts: Vec<&str> = device_id.split('-').collect();
    parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
        && parts
            .iter()
            .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// What an emulator's console reports for `network speed edge` and
/// `network delay edge`: download, upload (bits/s), latency min, max (ms).
const SLOW: &[&str] = &["473600", "473600", "80", "400"];

/// The settings `setEnvironment` takes, in the order they are applied.
pub const SETTINGS: [&str; 5] = ["appearance", "textSize", "locale", "timeZone", "network"];

/// The text sizes a flow names, smallest first, with iOS's category and
/// the Android font scale each stands for.
const TEXT_SIZES: [(&str, &str, &str); 12] = [
    ("extraSmall", "extra-small", "0.8"),
    ("small", "small", "0.85"),
    ("medium", "medium", "0.9"),
    ("large", "large", "1.0"),
    ("extraLarge", "extra-large", "1.15"),
    ("extraExtraLarge", "extra-extra-large", "1.3"),
    ("extraExtraExtraLarge", "extra-extra-extra-large", "1.5"),
    ("accessibilityMedium", "accessibility-medium", "1.6"),
    ("accessibilityLarge", "accessibility-large", "1.7"),
    (
        "accessibilityExtraLarge",
        "accessibility-extra-large",
        "1.8",
    ),
    (
        "accessibilityExtraExtraLarge",
        "accessibility-extra-extra-large",
        "1.9",
    ),
    (
        "accessibilityExtraExtraExtraLarge",
        "accessibility-extra-extra-extra-large",
        "2.0",
    ),
];

/// A flow's value for `setting` in the device's spelling, or why it cannot
/// be set there. Language and time zone on iOS are not device settings at
/// all — the caller applies those when it launches the app.
///
/// # Errors
///
/// A value the setting does not take, or a setting this device lacks.
pub fn device_value(device_id: &str, setting: &str, value: &str) -> Result<String, String> {
    let ios = is_ios(device_id);
    match setting {
        "appearance" => match (value, ios) {
            ("light" | "dark", true) => Ok(value.into()),
            ("light", false) => Ok("no".into()),
            ("dark", false) => Ok("yes".into()),
            _ => Err(format!("appearance is light or dark, not {value:?}")),
        },
        "textSize" => TEXT_SIZES
            .iter()
            .find(|(name, _, _)| *name == value)
            .map(|(_, cat, scale)| if ios { (*cat).into() } else { (*scale).into() })
            .ok_or_else(|| {
                let names: Vec<&str> = TEXT_SIZES.iter().map(|t| t.0).collect();
                format!("textSize is one of {}, not {value:?}", names.join(", "))
            }),
        "locale" | "timeZone" if !value.is_empty() && !value.contains(char::is_whitespace) => {
            Ok(value.into())
        }
        "locale" | "timeZone" => Err(format!(
            "{setting} {value:?} is not a language tag or zone id"
        )),
        "network" if ios => Err(
            "an iOS simulator has no network conditions of its own; the \
             Mac's Network Link Conditioner would slow the whole machine, so smix does not \
             set one"
                .into(),
        ),
        "network" if !device_id.starts_with("emulator-") => Err(
            "network conditions are set through an emulator's console; this device has none".into(),
        ),
        "network" => match value {
            "online" | "offline" | "slow" => Ok(value.into()),
            _ => Err(format!("network is online, offline or slow, not {value:?}")),
        },
        _ => Err(format!(
            "no setting {setting:?}; the settings are {}",
            SETTINGS.join(", ")
        )),
    }
}

/// The commands that read `setting`, in order; [`parse`] takes their
/// answers. `app` names the app a per-app setting belongs to.
///
/// # Errors
///
/// A setting this device does not keep: language and time zone on iOS.
pub fn read(device_id: &str, setting: &str, app: Option<&str>) -> Result<Vec<Command>, String> {
    let ios = is_ios(device_id);
    Ok(match (setting, ios) {
        ("appearance", true) => vec![cmd(Tool::Simctl, &["ui", device_id, "appearance"])],
        ("textSize", true) => vec![cmd(Tool::Simctl, &["ui", device_id, "content_size"])],
        ("appearance", false) => vec![cmd(Tool::AdbShell, &["cmd", "uimode", "night"])],
        ("textSize", false) => vec![cmd(
            Tool::AdbShell,
            &["settings", "get", "system", "font_scale"],
        )],
        ("timeZone", false) => vec![cmd(Tool::AdbShell, &["getprop", "persist.sys.timezone"])],
        ("locale", false) => {
            let app = app.ok_or("an app's language needs the app")?;
            vec![cmd(
                Tool::AdbShell,
                &["cmd", "locale", "get-app-locales", app],
            )]
        }
        ("network", false) => vec![
            cmd(Tool::AdbShell, &["cmd", "connectivity", "airplane-mode"]),
            cmd(Tool::AdbEmu, &["network", "status"]),
        ],
        _ => {
            return Err(format!(
                "{setting} is not kept by an iOS simulator; it is set on the app at launch"
            ));
        }
    })
}

/// The value the answers of [`read`]'s commands say, in the device's
/// spelling.
///
/// # Errors
///
/// An answer this does not recognise, or network conditions it would not
/// know how to put back.
pub fn parse(device_id: &str, setting: &str, answers: &[String]) -> Result<String, String> {
    let first = answers.first().map(|a| a.trim()).unwrap_or_default();
    let bad = || format!("{setting}: could not read {first:?}");
    match setting {
        "appearance" if !is_ios(device_id) => first
            .strip_prefix("Night mode: ")
            .map(str::to_string)
            .ok_or_else(bad),
        "locale" => first
            .rsplit_once('[')
            .and_then(|(_, rest)| rest.strip_suffix(']'))
            .map(str::to_string)
            .ok_or_else(bad),
        "network" => {
            let status = answers.get(1).map(String::as_str).unwrap_or_default();
            let numbers: Vec<&str> = status
                .lines()
                .filter(|l| l.contains("speed:") || l.contains("latency:"))
                .filter_map(|l| l.split(':').nth(1)?.split_whitespace().next())
                .collect();
            match (first, numbers.as_slice()) {
                ("enabled", _) => Ok("offline".into()),
                ("disabled", ["0", "0", "0", "0"]) => Ok("online".into()),
                ("disabled", SLOW) => Ok("slow".into()),
                ("disabled", _) => Err("network: the emulator's speed or latency is already \
                     set by something else; smix will not change what it could not put back"
                    .into()),
                _ => Err(bad()),
            }
        }
        _ if first.is_empty() || first == "null" => Err(bad()),
        _ => Ok(first.into()),
    }
}

/// The commands that set `setting` to `value`, in the device's spelling.
///
/// # Errors
///
/// As [`read`].
pub fn set(
    device_id: &str,
    setting: &str,
    value: &str,
    app: Option<&str>,
) -> Result<Vec<Command>, String> {
    let ios = is_ios(device_id);
    Ok(match (setting, ios) {
        ("appearance", true) => vec![cmd(Tool::Simctl, &["ui", device_id, "appearance", value])],
        ("textSize", true) => vec![cmd(Tool::Simctl, &["ui", device_id, "content_size", value])],
        ("appearance", false) => vec![cmd(Tool::AdbShell, &["cmd", "uimode", "night", value])],
        ("textSize", false) => vec![cmd(
            Tool::AdbShell,
            &["settings", "put", "system", "font_scale", value],
        )],
        ("timeZone", false) => vec![cmd(
            Tool::AdbShell,
            &["cmd", "alarm", "set-timezone", value],
        )],
        ("locale", false) => {
            let app = app.ok_or("an app's language needs the app")?;
            // no `--locales` sets the empty list: the app follows the device again
            let mut c = cmd(Tool::AdbShell, &["cmd", "locale", "set-app-locales", app]);
            if !value.is_empty() {
                c.args.extend(["--locales".into(), value.into()]);
            }
            vec![c]
        }
        ("network", false) => match value {
            "offline" => vec![cmd(
                Tool::AdbShell,
                &["cmd", "connectivity", "airplane-mode", "enable"],
            )],
            "slow" => vec![
                cmd(
                    Tool::AdbShell,
                    &["cmd", "connectivity", "airplane-mode", "disable"],
                ),
                cmd(Tool::AdbEmu, &["network", "speed", "edge"]),
                cmd(Tool::AdbEmu, &["network", "delay", "edge"]),
            ],
            _ => vec![
                cmd(
                    Tool::AdbShell,
                    &["cmd", "connectivity", "airplane-mode", "disable"],
                ),
                cmd(Tool::AdbEmu, &["network", "speed", "full"]),
                cmd(Tool::AdbEmu, &["network", "delay", "none"]),
            ],
        },
        _ => {
            return Err(format!(
                "{setting} is not kept by an iOS simulator; it is set on the app at launch"
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIM: &str = "A6E0D8D5-E9AA-43CD-BBFE-0F7CBC7F4150";
    const EMU: &str = "emulator-5554";

    #[test]
    fn a_udid_is_ios_and_a_serial_is_not() {
        assert!(is_ios(SIM));
        assert!(!is_ios(EMU));
        assert!(!is_ios("R5CT00000000"));
    }

    #[test]
    fn a_flows_values_become_the_devices() {
        assert_eq!(device_value(SIM, "appearance", "dark"), Ok("dark".into()));
        assert_eq!(device_value(EMU, "appearance", "dark"), Ok("yes".into()));
        assert_eq!(
            device_value(SIM, "textSize", "extraLarge"),
            Ok("extra-large".into())
        );
        assert_eq!(
            device_value(EMU, "textSize", "extraLarge"),
            Ok("1.15".into())
        );
        assert!(device_value(EMU, "textSize", "huge").is_err());
        assert!(device_value(SIM, "network", "offline").is_err());
        assert!(device_value("R5CT00000000", "network", "offline").is_err());
        assert_eq!(device_value(EMU, "network", "slow"), Ok("slow".into()));
        assert!(device_value(EMU, "network", "lossy").is_err());
    }

    #[test]
    fn answers_read_back_in_the_devices_spelling() {
        let one = |s: &str| vec![s.to_string()];
        assert_eq!(
            parse(EMU, "appearance", &one("Night mode: auto\n")),
            Ok("auto".into())
        );
        assert_eq!(
            parse(SIM, "appearance", &one("light\n")),
            Ok("light".into())
        );
        assert_eq!(parse(EMU, "textSize", &one("1.0\n")), Ok("1.0".into()));
        assert_eq!(
            parse(
                EMU,
                "locale",
                &one("Locales for a.b for user 0 are [ja-JP]")
            ),
            Ok("ja-JP".into())
        );
        assert_eq!(
            parse(EMU, "locale", &one("Locales for a.b for user 0 are []")),
            Ok(String::new())
        );
        assert!(parse(EMU, "textSize", &one("null")).is_err());
    }

    #[test]
    fn network_reads_offline_online_or_refuses_what_it_cannot_put_back() {
        let status = |down: &str, lat: &str| {
            format!(
                "Current network status:\n  download speed:          {down}\n  upload speed:            0 bits/s (0.0 KB/s)\n  minimum latency:  {lat}\n  maximum latency:  0 ms\nOK"
            )
        };
        let read = |air: &str, s: String| parse(EMU, "network", &[air.into(), s]);
        assert_eq!(
            read("disabled", status("0 bits/s (0.0 KB/s)", "0 ms")),
            Ok("online".into())
        );
        assert_eq!(
            read("enabled", status("0 bits/s (0.0 KB/s)", "0 ms")),
            Ok("offline".into())
        );
        assert!(read("disabled", status("236800 bits/s (28.9 KB/s)", "150 ms")).is_err());
        let edge = "Current network status:\n  download speed:     473600 bits/s (57.8 KB/s)\n  upload speed:       473600 bits/s (57.8 KB/s)\n  minimum latency:  80 ms\n  maximum latency:  400 ms\nOK";
        assert_eq!(read("disabled", edge.into()), Ok("slow".into()));
    }

    #[test]
    fn an_empty_app_locale_clears_it() {
        let c = set(EMU, "locale", "", Some("a.b")).expect("cmd");
        assert_eq!(c[0].args, ["cmd", "locale", "set-app-locales", "a.b"]);
        let c = set(EMU, "locale", "ja-JP", Some("a.b")).expect("cmd");
        assert_eq!(c[0].args.last().map(String::as_str), Some("ja-JP"));
    }

    #[test]
    fn language_and_time_zone_are_not_ios_device_settings() {
        assert!(read(SIM, "locale", Some("a.b")).is_err());
        assert!(set(SIM, "timeZone", "Asia/Tokyo", None).is_err());
    }
}
