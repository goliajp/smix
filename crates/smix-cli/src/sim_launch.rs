//! `smix sim launch`: start an app on any kind of device.

use smix_simctl::registry::DeviceKind;
use smix_simctl::{LaunchResult, SimctlClient};

use crate::{CliError, device_control_for, device_kind_of, resolve_device};

pub(crate) async fn run(
    simctl: &SimctlClient,
    device: &str,
    bundle_id: &str,
    child_env: &[(String, String)],
    launch_args: &[String],
) -> Result<(), CliError> {
    let udid = resolve_device(device)?;
    let kind = device_kind_of(device);
    let pairs: Vec<(&str, &str)> = child_env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    // `SIMCTL_CHILD_*` is a CoreSimulator facility: the
    // simulator's launchd hands the variables to the
    // child. `am start` has no equivalent -- an activity
    // inherits the system's environment and nothing
    // else -- so passing them would be dropping them
    // silently, which §9 #1 ③ is about.
    if !pairs.is_empty() && kind != DeviceKind::Simulator {
        return Err(CliError::Other(format!(
            "--child-env sets the launched process's environment, and only \
             a simulator's launchd can do that; {udid} is driven by \
             {}.\nInstead: pass the values as launch arguments, which \
             every backend carries.",
            if kind == DeviceKind::PhysicalIos {
                "devicectl"
            } else {
                "adb"
            }
        )));
    }
    let pid = if kind == DeviceKind::Simulator {
        let LaunchResult { pid } = simctl
            .launch_with_args_and_env(&udid, bundle_id, launch_args, &pairs)
            .await?;
        pid
    } else {
        device_control_for(kind, &udid)
            .launch_with_args(&udid, bundle_id, launch_args, None)
            .await
            .map_err(|e| CliError::Other(e.to_string()))?
    };
    // `am start` reports no pid and the Android backend
    // returns 0 for it. Printing "(pid 0)" reads as a
    // process number rather than as its absence.
    if pid == 0 {
        println!("launched: {bundle_id} on {udid}");
    } else {
        println!("launched: {bundle_id} on {udid} (pid {pid})");
    }
    Ok(())
}
