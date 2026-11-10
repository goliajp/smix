//! Which runner a command drives: the arguments every runner verb shares.

use crate::platform::RunPlatform;

#[derive(clap::Args, Debug)]
pub struct RunnerTarget {
    /// Runner port; unset: SMIX_RUNNER_PORT, the runner smix brought up on --device, its registered runnerPort, 22087 (iOS) / 28080 (Android).
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
    name_the_device(device, device.is_some_and(is_android));
    crate::run_port(flag.or_else(crate::act::runner_port_from_env_opt), || {
        device.and_then(device_port)
    })
}

/// Dial both the port and the platform a CLI verb should drive, so
/// `fill` / `find` reach the driver the flow path already uses. The port
/// keeps `runner_dial_port`'s flag → env → recorded runner → registry →
/// platform default ladder;
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

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    fn port_helps(cmd: &clap::Command, path: &str, out: &mut Vec<(String, String)>) {
        for arg in cmd.get_arguments() {
            if arg.get_long() == Some("port") {
                let help = arg
                    .get_long_help()
                    .or(arg.get_help())
                    .map(|h| h.to_string());
                out.push((path.to_string(), help.unwrap_or_default()));
            }
        }
        for sub in cmd.get_subcommands() {
            port_helps(sub, &format!("{path} {}", sub.get_name()), out);
        }
    }

    // every verb that dials a runner by device says where an unset port
    // comes from; 24.0.0 changed the order and left the old sentence on
    // every `--port` that still read "SMIX_RUNNER_PORT env or 22087"
    #[test]
    fn a_port_left_unset_is_described_by_the_dial_order() {
        let mut helps = Vec::new();
        port_helps(&crate::Cli::command(), "smix", &mut helps);
        let dialled: Vec<_> = helps
            .iter()
            .filter(|(_, h)| h.contains("SMIX_RUNNER_PORT"))
            .collect();
        assert!(
            dialled.len() >= 19,
            "only {} `--port` helps name SMIX_RUNNER_PORT — the walk lost the verbs",
            dialled.len()
        );
        let stale: Vec<_> = dialled
            .iter()
            .filter(|(_, h)| !(h.contains("--device") && h.contains("28080")))
            .map(|(p, _)| p.as_str())
            .collect();
        assert!(
            stale.is_empty(),
            "`--port` help still says the old order: {stale:?}"
        );
    }
}

/// The `smix runner up` that would bring back a runner for `device`, with
/// the app it was last brought up for when its ledger still says, and a
/// placeholder to fill in when it does not.
pub(crate) fn runner_up_command(device: &str, android: bool) -> String {
    use smix_lease::Resource;
    let id = crate::lookup_registered(device).map_or_else(|| device.to_string(), |s| s.udid);
    let bundle = smix_capsule::runner::machine_leases()
        .ok()
        .and_then(|leases| smix_lease::store::read(&leases, &id).ok().flatten())
        .and_then(|lease| {
            lease.known_resources().find_map(|r| match r {
                Resource::Runner { bundle, .. } | Resource::AndroidRunner { bundle, .. } => {
                    bundle.clone()
                }
                _ => None,
            })
        });
    command_to_bring_up(device, android, bundle.as_deref())
}

fn command_to_bring_up(device: &str, android: bool, bundle: Option<&str>) -> String {
    let platform = if android { " --platform android" } else { "" };
    let bundle = bundle.unwrap_or("<your-app>");
    format!("smix runner up {device}{platform} --bundle {bundle}")
}

#[cfg(test)]
mod bring_up_tests {
    use super::command_to_bring_up;

    #[test]
    fn the_command_names_the_device_the_platform_and_the_app() {
        assert_eq!(
            command_to_bring_up("emulator-5560", true, Some("com.example.app")),
            "smix runner up emulator-5560 --platform android --bundle com.example.app"
        );
        assert_eq!(
            command_to_bring_up("sim-a", false, None),
            "smix runner up sim-a --bundle <your-app>"
        );
    }
}

/// The device the command line named, with the book that says which
/// device a port reaches. Set once, when the verb dials its port.
static NAMED_DEVICE: std::sync::OnceLock<(String, smix_runner_client::OwnerProbe)> =
    std::sync::OnceLock::new();

/// Record the device the command named, so every runner client this
/// process builds refuses a port that reaches another one. `smix run`
/// refused that before its first request; the one-shot verbs built
/// their clients bare and read whichever device held the port.
pub(crate) fn name_the_device(device: Option<&str>, android: bool) {
    if let Some(d) = device {
        let probe = if android {
            smix_runner_client::OwnerProbe::Android
        } else {
            smix_runner_client::OwnerProbe::Ios
        };
        let _ = NAMED_DEVICE.set((d.to_string(), probe));
    }
}

/// Print a command's failure, with how to start its runner when no runner
/// ever answered, and exit 1.
pub(crate) fn fail(error: &str) -> std::process::ExitCode {
    eprintln!("error: {error}{}", how_to_start_the_runner(error));
    std::process::ExitCode::from(1)
}

/// The full `smix runner up` for the device this command named, when what
/// failed was that no runner ever answered; nothing otherwise.
fn how_to_start_the_runner(error: &str) -> String {
    match NAMED_DEVICE.get() {
        Some((device, probe)) if error.contains(smix_runner_client::NEVER_ANSWERED) => format!(
            "\n  start it with: {}",
            runner_up_command(
                device,
                matches!(probe, smix_runner_client::OwnerProbe::Android)
            )
        ),
        _ => String::new(),
    }
}

/// A runner client on `port` that checks, before its first request, that
/// the port reaches the device the command named.
pub(crate) fn runner_client(port: u16) -> smix_runner_client::HttpRunnerClient {
    let client = smix_runner_client::HttpRunnerClient::new(port);
    match NAMED_DEVICE.get() {
        Some((device, probe)) => client.with_port_owner_check(Some(device.clone()), *probe),
        None => client,
    }
}
