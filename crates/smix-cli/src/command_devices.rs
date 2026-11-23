//! Which device a command acts on, for the scope check that runs before
//! any of them.
//!
//! Exhaustive on purpose: a new command has to say here whether it
//! touches a device before this compiles. "Not mentioned" is how a check
//! that sits in front of every command ends up in front of most of them.

use crate::{
    AuthoringAction, CapsuleAction, Cmd, DiagnosticAction, RecordAction, RunnerAction,
    SessionAction,
};

/// What a command acts on.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Touches<'a> {
    /// No device: reading files, the registry, the ledger, or this machine.
    Nothing,
    /// These devices by name.
    Devices(Vec<&'a str>),
    /// A runner, by the device named if any, else by the port it is
    /// dialled on (`None`: the default the command would dial).
    Runner {
        device: Option<&'a str>,
        port: Option<u16>,
    },
}

fn runner<'a>(device: Option<&'a String>, port: Option<u16>) -> Touches<'a> {
    Touches::Runner {
        device: device.map(String::as_str),
        port,
    }
}

pub(crate) fn of(cmd: &Cmd) -> Touches<'_> {
    match cmd {
        Cmd::Status { .. }
        | Cmd::Cleanup { .. }
        | Cmd::Doctor { .. }
        | Cmd::Bench { .. }
        | Cmd::Guard { .. }
        | Cmd::Lease { .. }
        | Cmd::Migrate { .. }
        | Cmd::Annotate { .. } => Touches::Nothing,
        Cmd::Init { device, .. } => match device {
            Some(d) => Touches::Devices(vec![d]),
            None => Touches::Nothing,
        },
        Cmd::Diagnostic { action } => match action {
            DiagnosticAction::Store { .. } => Touches::Nothing,
            DiagnosticAction::Dump { runner_port, .. } => runner(None, *runner_port),
        },
        Cmd::Session { action } => match action {
            SessionAction::State { port, device, .. } => runner(device.as_ref(), *port),
        },
        Cmd::Sim { action } => match crate::sim_action_device(action) {
            Some(d) => Touches::Devices(vec![d]),
            None => Touches::Nothing,
        },
        Cmd::Runner { action } => match action {
            RunnerAction::Up {
                device,
                device_flag,
                runner_port,
                ..
            } => runner(device.as_ref().or(device_flag.as_ref()), *runner_port),
            RunnerAction::Down {
                device_pos,
                device,
                runner_port,
                ..
            } => runner(device_pos.as_ref().or(device.as_ref()), *runner_port),
            RunnerAction::Forward { device, .. } | RunnerAction::Uninstall { device, .. } => {
                Touches::Devices(vec![device])
            }
            RunnerAction::Cycle { runner_port, .. }
            | RunnerAction::Supervise { runner_port, .. }
            | RunnerAction::ListSessions { runner_port } => runner(None, *runner_port),
            RunnerAction::List { .. } | RunnerAction::Install { .. } => Touches::Nothing,
        },
        Cmd::Down { runner_port } => runner(None, *runner_port),
        Cmd::Record { action } => match action {
            RecordAction::Start { device, .. }
            | RecordAction::Stop { device }
            | RecordAction::Status { device } => Touches::Devices(vec![device]),
        },
        Cmd::Capsule { action } => match action {
            CapsuleAction::Up { device, .. } | CapsuleAction::Down { device, .. } => {
                Touches::Devices(vec![device])
            }
        },
        Cmd::Tap { port, device, .. }
        | Cmd::Find { port, device, .. }
        | Cmd::WaitFor { port, device, .. }
        | Cmd::Fill { port, device, .. }
        | Cmd::PressKey { port, device, .. }
        | Cmd::Scroll { port, device, .. }
        | Cmd::Tree { port, device, .. }
        | Cmd::SystemPopups { port, device, .. }
        | Cmd::SystemPopupAction { port, device, .. }
        | Cmd::RunScript { port, device, .. } => runner(device.as_ref(), *port),
        Cmd::Swipe(a) => runner(a.device.as_ref(), a.port),
        Cmd::Pinch(a) => runner(a.device.as_ref(), a.port),
        Cmd::RotateCrown(a) => runner(a.device.as_ref(), a.port),
        Cmd::LongPress(a) => runner(a.device.as_ref(), a.port),
        Cmd::Drag(a) => runner(a.device.as_ref(), a.port),
        Cmd::Hover(a) => runner(a.device.as_ref(), a.port),
        Cmd::WindowSize(a) => runner(a.device.as_ref(), a.port),
        Cmd::Describe(a) => runner(a.device.as_ref(), a.port),
        Cmd::HideKeyboard(t) => runner(t.device.as_ref(), t.port),
        Cmd::Run {
            device,
            also_device,
            runner_port,
            ..
        } => match device {
            Some(d) => Touches::Devices(
                std::iter::once(d.as_str())
                    .chain(also_device.iter().map(String::as_str))
                    .collect(),
            ),
            None => runner(None, *runner_port),
        },
        Cmd::Authoring { action } => match action {
            AuthoringAction::Generate { .. } | AuthoringAction::Propose { .. } => Touches::Nothing,
            AuthoringAction::TapRecord { port, device, .. }
            | AuthoringAction::Suggest { port, device, .. }
            | AuthoringAction::CaptureTree { port, device, .. }
            | AuthoringAction::DiffTree { port, device, .. }
            | AuthoringAction::Record { port, device, .. } => runner(device.as_ref(), *port),
        },
    }
}

/// The devices whose live runner answers on `port`, from the ledger.
fn devices_on_port(leases: &smix_lease::store::LeaseDir, port: u16) -> Vec<String> {
    use smix_lease::Resource;
    leases
        .device_ids()
        .into_iter()
        .filter_map(|id| smix_lease::store::read(leases, &id).ok().flatten())
        .filter(|lease| {
            lease.known_resources().any(|r| match r {
                Resource::Runner { port: p, proc, .. }
                | Resource::AndroidRunner { port: p, proc, .. } => {
                    *p == port && smix_lease::store::probe(proc).identity_matches
                }
                _ => false,
            })
        })
        .map(|lease| lease.device_id)
        .collect()
}

/// The devices `cmd` acts on, as names the scope check can compare.
///
/// A runner dialled by port alone is found through the ledger; a port no
/// runner smix started answers on names nothing, so nothing is checked.
pub(crate) fn devices(cmd: &Cmd) -> Vec<String> {
    match of(cmd) {
        Touches::Nothing => Vec::new(),
        Touches::Devices(d) => d.into_iter().map(str::to_string).collect(),
        Touches::Runner {
            device: Some(d), ..
        } => vec![d.to_string()],
        Touches::Runner { device: None, port } => match smix_capsule::runner::machine_leases() {
            Ok(leases) => {
                devices_on_port(&leases, crate::runner_target::runner_dial_port(port, None))
            }
            Err(_) => Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn touches(args: &[&str], want: &Touches<'_>) {
        let cli = crate::Cli::try_parse_from(std::iter::once("smix").chain(args.iter().copied()))
            .unwrap_or_else(|e| panic!("{args:?}: {e}"));
        assert_eq!(&of(&cli.cmd), want, "{args:?}");
    }

    #[test]
    fn a_command_says_which_device_it_acts_on() {
        touches(
            &["tap", "id:x", "--port", "22100"],
            &Touches::Runner {
                device: None,
                port: Some(22100),
            },
        );
        touches(
            &["tree", "--device", "sim-a"],
            &Touches::Runner {
                device: Some("sim-a"),
                port: None,
            },
        );
        touches(
            &["sim", "screenshot", "emulator-5560", "x.png"],
            &Touches::Devices(vec!["emulator-5560"]),
        );
        touches(
            &["runner", "up", "qip-01", "--platform", "android"],
            &Touches::Runner {
                device: Some("qip-01"),
                port: None,
            },
        );
        touches(
            &["run", "--device", "a", "--also-device", "b", "f.yaml"],
            &Touches::Devices(vec!["a", "b"]),
        );
        touches(
            &["lease", "status", "--scope", "insight"],
            &Touches::Nothing,
        );
        touches(&["sim", "list"], &Touches::Nothing);
    }

    #[test]
    fn a_port_names_the_device_whose_live_runner_answers_on_it() {
        let t = tempfile::tempdir().expect("tempdir");
        let leases = smix_lease::store::LeaseDir::at(t.path().to_path_buf());
        let runner = |port, proc| smix_lease::Resource::AndroidRunner {
            port,
            serial: "emulator-5998".into(),
            proc,
            bundle: None,
        };
        let me = smix_lease::store::identify_self();
        smix_lease::store::add_resource(&leases, "emulator-5998", runner(22998, me.clone()))
            .expect("write");
        let gone = smix_lease::ProcIdentity {
            started_at: "never".into(),
            ..me
        };
        smix_lease::store::add_resource(&leases, "emulator-5996", runner(22996, gone))
            .expect("write");
        assert_eq!(
            devices_on_port(&leases, 22998),
            vec!["emulator-5998".to_string()]
        );
        assert!(
            devices_on_port(&leases, 22996).is_empty(),
            "its runner is not the one recorded"
        );
        assert!(
            devices_on_port(&leases, 22997).is_empty(),
            "nobody on this port"
        );
    }
}
