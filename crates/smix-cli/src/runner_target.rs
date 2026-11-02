//! Which runner a command drives: the arguments every runner verb shares.

use crate::platform::RunPlatform;

#[derive(clap::Args, Debug)]
pub struct RunnerTarget {
    /// Runner port override (defaults to SMIX_RUNNER_PORT env or 22087).
    /// `--runner-port` is accepted too — `smix run` spells it that way.
    #[arg(long, alias = "runner-port")]
    pub port: Option<u16>,
    /// Device UDID, or an alias / deviceName in the workspace's
    /// `.smix` registry. It finds the runner port that device is
    /// registered on, and — where a verb behaves differently by
    /// platform (fill, find, tap and the rest of the act surface) —
    /// reads the driver from the device's registered kind, so an
    /// Android device is driven through its own runner, not simctl.
    #[arg(long)]
    pub device: Option<String>,
    /// Which platform the device is, for a device this machine has not
    /// registered. A registered device's own kind is read otherwise.
    #[arg(long, value_enum)]
    pub platform: Option<RunPlatform>,
}

/// Whether a named device is an Android one: its registered kind, or an
/// emulator serial when it is not registered. It picks which book says
/// what a port reaches (adb's forwards, or the process on the port).
fn is_android(device: &str) -> bool {
    use smix_simctl::registry::DeviceKind;
    match crate::lookup_registered(device) {
        Some(sim) => match sim.kind {
            DeviceKind::Emulator | DeviceKind::PhysicalAndroid => true,
            DeviceKind::Simulator | DeviceKind::PhysicalIos => false,
        },
        None => device.starts_with("emulator-"),
    }
}

pub(crate) fn runner_dial_port(flag: Option<u16>, device: Option<&str>) -> u16 {
    crate::act::name_the_device(device, device.is_some_and(is_android));
    crate::run_port(flag.or_else(crate::act::runner_port_from_env_opt), || {
        device
            .and_then(crate::lookup_registered)
            .and_then(|sim| sim.runner_port)
    })
}

/// Dial both the port and the platform a CLI verb should drive, so
/// `fill` / `find` reach the driver the flow path already uses. The port
/// keeps `runner_dial_port`'s flag → env → registry → default ladder;
/// the platform is read from the device the same way `smix run` reads it
/// (C1) — one rule at both entrances. No `--device` lets the dialed
/// port's registered holder decide (the consumer's `--port 22088`
/// usage). An unclassifiable target is an error, not a silent iOS drive
/// that would 501 on Android.
pub(crate) fn runner_dial_target(
    flag: Option<u16>,
    device: Option<&str>,
    platform: Option<RunPlatform>,
) -> Result<(u16, smix_driver::Platform), crate::CliError> {
    let port = runner_dial_port(flag, device);
    // Said outright, as `smix run --platform` says it.
    if let Some(p) = platform {
        return Ok((port, p.to_driver()));
    }
    let dial = device.map(|d| match crate::lookup_registered(d) {
        Some(sim) => crate::platform::DeviceDial::Registered(sim.kind),
        None => crate::platform::DeviceDial::Unregistered,
    });
    let holders: Vec<smix_simctl::registry::DeviceKind> = if device.is_none() {
        crate::load_registry()
            .registry
            .all()
            .filter(|(_, sim)| sim.runner_port == Some(port))
            .map(|(_, sim)| sim.kind)
            .collect()
    } else {
        Vec::new()
    };
    let plat = crate::platform::dial_platform(dial, &holders).map_err(crate::CliError::Other)?;
    Ok((port, plat.to_driver()))
}

/// The key a project's device pointer is stored under: the workspace
/// root (the nearest ancestor with a `.smix/`), canonicalized, or the
/// cwd when there is no workspace. Lenient on purpose — a `run` outside
/// a workspace still resolves to a key, just one with no pointer, so it
/// falls through to the old behaviour rather than failing.
pub(crate) fn project_key() -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = smix_capsule::runner::workspace_root(&cwd).unwrap_or_else(|| cwd.clone());
    std::fs::canonicalize(&root)
        .unwrap_or(root)
        .to_string_lossy()
        .into_owned()
}
