//! `smix sim appearance <device> light|dark` — a simulator through
//! `simctl ui`, an emulator through the system night mode.

use smix_simctl::registry::DeviceKind;
use smix_simctl::{Appearance, SimctlClient};
use std::time::{Duration, Instant};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct AppearanceArgs {
    /// Which device, by UDID, emulator serial or registry alias.
    pub device: String,
    /// `light` or `dark`.
    #[arg(value_parser = parse_appearance)]
    pub mode: Appearance,
}

pub fn parse_appearance(s: &str) -> Result<Appearance, String> {
    match s.to_ascii_lowercase().as_str() {
        "light" => Ok(Appearance::Light),
        "dark" => Ok(Appearance::Dark),
        other => Err(format!("expected 'light' or 'dark', got {:?}", other)),
    }
}

pub async fn run(simctl: &SimctlClient, args: AppearanceArgs) -> Result<(), crate::CliError> {
    let (device, mode) = (args.device.as_str(), args.mode);
    if crate::device_kind_of(device) != DeviceKind::Emulator {
        let udid = crate::resolve_device(device)?;
        simctl.set_appearance(&udid, mode).await?;
        println!("appearance: {udid} → {}", mode.as_str());
        return Ok(());
    }
    let serial = crate::resolve_android_serial(device)?;
    let control = smix_sdk::android_device::AndroidDeviceControl::new();
    let s = serial.clone();
    crate::with_device_lease(&control, &serial, |leased| async move {
        set_night_mode(&s, mode == Appearance::Dark)
            .await
            .map_err(crate::CliError::Other)?;
        Ok(((), leased))
    })
    .await?;
    println!("appearance: {serial} → {}", mode.as_str());
    Ok(())
}

/// Sets the night mode and returns once the configuration apps are handed
/// says so: `cmd uimode night` answers with the setting it stored, which is
/// not yet the configuration a resumed activity redraws from.
async fn set_night_mode(serial: &str, night: bool) -> Result<(), String> {
    let adb = smix_adb::AdbClient::new();
    let word = if night { "yes" } else { "no" };
    adb.shell(serial, &["cmd", "uimode", "night", word])
        .await
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let seen = loop {
        // a read that began before the deadline still counts
        let looked = Instant::now();
        let config = adb
            .shell(serial, &["am", "get-config"])
            .await
            .map_err(|e| e.to_string())?;
        let seen = night_of_config(&config);
        if seen == Some(night) {
            return Ok(());
        }
        if looked >= deadline {
            break seen;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let seen = match seen {
        Some(true) => "night",
        Some(false) => "notnight",
        None => "no night qualifier",
    };
    Err(format!(
        "{serial}: night mode set to {word}, but after 10 s the device configuration still reads {seen} \
         — `adb -s {serial} shell am get-config` shows what it has"
    ))
}

/// The night qualifier of `am get-config`'s `config:` line, e.g.
/// `config: mcc310-…-port-night-xhdpi-…-v36`.
fn night_of_config(out: &str) -> Option<bool> {
    let line = out.lines().find_map(|l| l.trim().strip_prefix("config:"))?;
    line.trim().split('-').find_map(|q| match q {
        "night" => Some(true),
        "notnight" => Some(false),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // read off emulator-5584, API 36, before and after `cmd uimode night yes`
    const DAY: &str = "config: mcc310-mnc260-en-rUS-ldltr-sw800dp-w800dp-h1280dp-xlarge-notlong-notround-nowidecg-lowdr-port-notnight-xhdpi-finger-keysexposed-qwerty-navexposed-dpad-2560x1600-v36\nabi: arm64-v8a\n";
    const NIGHT: &str = "config: mcc310-mnc260-en-rUS-ldltr-sw800dp-w800dp-h1280dp-xlarge-notlong-notround-nowidecg-lowdr-port-night-xhdpi-finger-keysexposed-qwerty-navexposed-dpad-2560x1600-v36\nabi: arm64-v8a\n";

    #[test]
    fn the_night_qualifier_is_read_from_the_config_line() {
        assert_eq!(night_of_config(DAY), Some(false));
        assert_eq!(night_of_config(NIGHT), Some(true));
        assert_eq!(night_of_config("abi: arm64-v8a\n"), None);
        // a word elsewhere on the line is not the qualifier
        assert_eq!(night_of_config("config: en-rUS-nightly-v36\n"), None);
    }

    #[test]
    fn both_modes_parse_in_any_case() {
        assert_eq!(parse_appearance("Dark"), Ok(Appearance::Dark));
        assert_eq!(parse_appearance("light"), Ok(Appearance::Light));
        assert!(parse_appearance("dim").is_err());
    }
}
