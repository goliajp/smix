//! `smix runner down`: which runner a teardown stops.
//!
//! A consumer with an iOS runner on 22087 and an Android one on 22089 ran
//! `runner down --device emulator-5560` and lost the iOS one: without
//! `--platform android` the device was not read at all, and the default
//! port was closed. `--device` now names the runner — its platform from
//! the device, and on iOS the port its record gives.

use crate::platform::resolve_run_platform;
use crate::{CliError, RunPlatform};
use smix_simctl::registry::DeviceKind;

/// Which platform's runner to stop: the one said, else the device's,
/// else iOS's, which is what `down` with neither has always stopped.
pub(crate) fn down_platform(
    explicit: Option<RunPlatform>,
    device_kind: Option<DeviceKind>,
) -> Result<RunPlatform, String> {
    match (explicit, device_kind) {
        (Some(p), _) => Ok(p),
        (None, Some(k)) => resolve_run_platform(None, Some(k)),
        (None, None) => Ok(RunPlatform::Ios),
    }
}

/// The port of the iOS runner driving `device`, from its record.
///
/// A port given beside the device has to be that runner's: two that
/// disagree name two runners, and stopping either one is a guess. With
/// no record, only `--include-unrecorded` with a port says which runner.
pub(crate) fn ios_down_port(
    device: &str,
    recorded: Option<u16>,
    flag: Option<u16>,
    include_unrecorded: bool,
) -> Result<u16, String> {
    match (recorded, flag) {
        (Some(r), Some(f)) if r != f => Err(format!(
            "the runner recorded for {device} answers on port {r}, not {f} — \
             nothing was stopped. Name one runner: drop --runner-port to stop \
             {device}'s, or drop --device to stop the one on {f}."
        )),
        (Some(r), _) => Ok(r),
        (None, Some(f)) if include_unrecorded => Ok(f),
        (None, _) => Err(format!(
            "no runner is recorded for {device}, so nothing was stopped. \
             `smix runner list` shows which runners are up and where; a runner \
             this machine has no record of is stopped with --runner-port <port> \
             --include-unrecorded."
        )),
    }
}

pub(crate) fn run(
    platform: Option<RunPlatform>,
    device: Option<String>,
    include_unrecorded: bool,
    take_over: bool,
    port_flag: Option<u16>,
) -> Result<(), CliError> {
    let kind = device.as_deref().map(crate::device_kind_of);
    let platform = down_platform(platform, kind).map_err(CliError::Other)?;
    if platform == RunPlatform::Android {
        let serial = device.ok_or_else(|| {
            CliError::Other(
                "runner down --platform android needs --device \
                 <adb-serial>: an adb command without one acts on \
                 whichever device is attached"
                    .to_string(),
            )
        })?;
        let port = port_flag.unwrap_or_else(|| {
            std::env::var("SMIX_RUNNER_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(smix_capsule::runner_android::DEFAULT_ANDROID_PORT)
        });
        let serial = crate::resolve_android_serial(&serial)?;
        return smix_capsule::runner_android::down_with(&serial, port, take_over)
            .map_err(CliError::Other);
    }
    let port = match device {
        Some(d) => {
            let udid = crate::resolve_device(&d)?;
            let leases = smix_capsule::runner::machine_leases().map_err(CliError::Other)?;
            let recorded = smix_capsule::runner_state::find(
                &leases,
                smix_capsule::runner_state::Lookup::Device(&udid),
            )
            .map_err(CliError::Other)?
            .map(|st| st.port);
            ios_down_port(&d, recorded, port_flag, include_unrecorded).map_err(CliError::Other)?
        }
        None => port_flag.unwrap_or_else(crate::runner_port),
    };
    if take_over {
        smix_capsule::runner::down_taking_over(port, include_unrecorded)
    } else if include_unrecorded {
        smix_capsule::runner::down_including_unrecorded(port)
    } else {
        smix_capsule::runner::down(port)
    }
    .map_err(CliError::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_says_which_platform_s_runner_goes() {
        assert_eq!(down_platform(None, Some(DeviceKind::Emulator)), Ok(RunPlatform::Android));
        assert_eq!(
            down_platform(None, Some(DeviceKind::PhysicalAndroid)),
            Ok(RunPlatform::Android)
        );
        assert_eq!(down_platform(None, Some(DeviceKind::Simulator)), Ok(RunPlatform::Ios));
    }

    #[test]
    fn a_platform_said_outright_wins_and_neither_is_ios() {
        assert_eq!(
            down_platform(Some(RunPlatform::Ios), Some(DeviceKind::Emulator)),
            Ok(RunPlatform::Ios)
        );
        assert_eq!(down_platform(None, None), Ok(RunPlatform::Ios));
    }

    #[test]
    fn an_ios_device_s_runner_is_the_one_its_record_names() {
        assert_eq!(ios_down_port("sim-a", Some(22091), None, false), Ok(22091));
        assert_eq!(ios_down_port("sim-a", Some(22091), Some(22091), false), Ok(22091));
    }

    #[test]
    fn a_port_that_is_not_the_device_s_runner_stops_nothing() {
        let e = ios_down_port("sim-a", Some(22091), Some(22087), true).unwrap_err();
        assert!(e.contains("22091") && e.contains("22087"), "{e}");
    }

    #[test]
    fn a_device_with_no_runner_falls_to_no_default_port() {
        let e = ios_down_port("sim-a", None, None, false).unwrap_err();
        assert!(e.contains("no runner is recorded for sim-a"), "{e}");
        assert!(ios_down_port("sim-a", None, Some(22087), false).is_err());
        assert_eq!(ios_down_port("sim-a", None, Some(22087), true), Ok(22087));
    }
}
