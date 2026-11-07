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
    match known_device_kind(device) {
        Some(DeviceKind::Emulator | DeviceKind::PhysicalAndroid) => true,
        Some(DeviceKind::Simulator | DeviceKind::PhysicalIos) => false,
        None => device.starts_with("emulator-"),
    }
}

pub(crate) fn runner_dial_port(flag: Option<u16>, device: Option<&str>) -> u16 {
    crate::act::name_the_device(device, device.is_some_and(is_android));
    crate::run_port(flag.or_else(crate::act::runner_port_from_env_opt), || {
        device.and_then(device_port)
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
    let dial = device.map(|d| match known_device_kind(d) {
        Some(kind) => crate::platform::DeviceDial::Known(kind),
        None => crate::platform::DeviceDial::Unknown,
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

/// What kind of device a name on the command line is, when anything
/// says: the registry, else the device itself — an emulator serial adb
/// lists, or a UDID simctl lists. `None` when nothing does, which is the
/// only case `--platform` has to settle.
pub(crate) fn known_device_kind(device: &str) -> Option<smix_simctl::registry::DeviceKind> {
    use smix_simctl::registry::{self, DeviceKind};
    if let Some(sim) = crate::lookup_registered(device) {
        return Some(sim.kind);
    }
    if registry::is_emulator_serial(device) && crate::adb_knows(device) {
        return Some(DeviceKind::Emulator);
    }
    let udid = device.to_ascii_uppercase();
    (registry::is_udid(&udid) && crate::simctl_knows(&udid)).then_some(DeviceKind::Simulator)
}

/// The port the runner smix brought up on `device` answers on, while the
/// process the ledger recorded for it is still the one running.
///
/// It comes ahead of the registry's `runnerPort`: that is where a runner
/// for the device is meant to be bound, this is where one is. Without it
/// a runner brought up on its platform's default was dialled on the
/// registered port, or on the other platform's default.
pub(crate) fn recorded_runner_port(device: &str) -> Option<u16> {
    use smix_lease::Resource;
    let id = crate::lookup_registered(device).map_or_else(|| device.to_string(), |s| s.udid);
    let leases = smix_capsule::runner::machine_leases().ok()?;
    let lease = smix_lease::store::read(&leases, &id).ok()??;
    lease.known_resources().find_map(|r| match r {
        Resource::Runner { port, proc, .. } | Resource::AndroidRunner { port, proc, .. } => {
            smix_lease::store::probe(proc)
                .identity_matches
                .then_some(*port)
        }
        _ => None,
    })
}

/// The port to dial for `device` when no flag or `SMIX_RUNNER_PORT` says:
/// its live runner's, else its registered `runnerPort`, else its
/// platform's default — 28080 for an Android runner, `None` (the iOS
/// default) otherwise.
pub(crate) fn device_port(device: &str) -> Option<u16> {
    recorded_runner_port(device)
        .or_else(|| crate::lookup_registered(device).and_then(|s| s.runner_port))
        .or_else(|| {
            is_android(device).then_some(smix_capsule::runner_android::DEFAULT_ANDROID_PORT)
        })
}
