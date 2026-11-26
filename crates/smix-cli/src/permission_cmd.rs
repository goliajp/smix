//! `smix sim permission` — grant, revoke or reset one runtime permission.

use smix_sdk::PermissionAction;
use smix_sdk::device_control::{DeviceControl, Permission};
use smix_simctl::notifications::Authorization;
use smix_simctl::registry::DeviceKind;
use smix_simctl::{SimctlClient, SimctlPermission};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct PermissionArgs {
    /// Which device, by UDID, serial or registry alias.
    pub device: String,
    /// The app the permission belongs to.
    pub bundle_id: String,
    /// Which permission, e.g. `camera`, `location`, `notifications`.
    pub permission: String,
    /// What to do with it: `grant` (or `allow`), `revoke` (or `deny`), or
    /// `reset`, which is as if it had never been asked.
    #[arg(value_parser = parse_action)]
    pub action: PermissionAction,
}

fn parse_action(s: &str) -> Result<PermissionAction, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "grant" | "allow" => Ok(PermissionAction::Grant),
        "revoke" | "deny" => Ok(PermissionAction::Revoke),
        "reset" => Ok(PermissionAction::Reset),
        other => Err(format!(
            "expected `grant` (`allow`), `revoke` (`deny`) or `reset`, got `{other}`"
        )),
    }
}

fn word(action: PermissionAction) -> &'static str {
    match action {
        PermissionAction::Grant => "grant",
        PermissionAction::Revoke => "revoke",
        PermissionAction::Reset => "reset",
    }
}

pub async fn run(simctl: &SimctlClient, args: PermissionArgs) -> Result<(), crate::CliError> {
    let permission = Permission::from_name(&args.permission).map_err(crate::CliError::Other)?;
    let app = args.bundle_id.as_str();
    let failed = |e: smix_simctl::DeviceControlError| crate::CliError::Other(e.to_string());
    let device = if crate::device_kind_of(&args.device) == DeviceKind::Simulator {
        let udid = crate::resolve_device(&args.device)?;
        // the SDK skips such a permission so a cross-platform flow runs
        // on; a command naming one device and one permission says no
        if permission.to_simctl().is_none() {
            return Err(crate::CliError::Other(format!(
                "{} is an Android permission; a simulator has nothing to set for it",
                permission.name()
            )));
        }
        if permission.to_simctl() == Some(SimctlPermission::Notifications) {
            return notifications(simctl, &udid, app, args.action).await;
        }
        smix_sdk::ios_device::IosDeviceControl::new()
            .set_permission(&udid, app, permission, args.action)
            .await
            .map_err(failed)?;
        udid
    } else {
        let serial = crate::resolve_android_serial(&args.device)?;
        smix_sdk::android_device::AndroidDeviceControl::new()
            .set_permission(&serial, app, permission, args.action)
            .await
            .map_err(failed)?;
        serial
    };
    println!(
        "permission: {device} {app} {} {}",
        permission.name(),
        word(args.action)
    );
    Ok(())
}

/// A simulator's notification authorization is SpringBoard's record, not
/// a permission `simctl privacy` writes; SpringBoard reads it at start.
async fn notifications(
    simctl: &SimctlClient,
    udid: &str,
    app: &str,
    action: PermissionAction,
) -> Result<(), crate::CliError> {
    let to = match action {
        PermissionAction::Grant => Authorization::Allowed,
        PermissionAction::Revoke => Authorization::Denied,
        PermissionAction::Reset => Authorization::NotDetermined,
    };
    if !simctl.app_is_installed(udid, app).await? {
        return Err(crate::CliError::Other(format!(
            "{app} is not installed on {udid}, and its notification permission goes with it"
        )));
    }
    if simctl.notification_authorization(udid, app).await? == to {
        println!(
            "permission: {udid} {app} notifications {} (already so)",
            word(action)
        );
        return Ok(());
    }
    crate::runner_target::refuse_beside_a_runner(
        udid,
        "setting this restarts SpringBoard, which would end it",
    )?;
    let name = simctl.app_name(udid, app).await?;
    simctl
        .set_notification_authorization(udid, app, &name, to)
        .await?;
    println!(
        "permission: {udid} {app} notifications {} (SpringBoard restarted)",
        word(action)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_vocabularies_are_taken() {
        for (s, a) in [
            ("grant", PermissionAction::Grant),
            ("Allow", PermissionAction::Grant),
            ("revoke", PermissionAction::Revoke),
            ("deny", PermissionAction::Revoke),
            ("reset", PermissionAction::Reset),
        ] {
            assert_eq!(parse_action(s), Ok(a), "{s}");
        }
        assert!(parse_action("maybe").is_err());
    }
}
