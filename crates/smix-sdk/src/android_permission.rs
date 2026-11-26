//! Grant or revoke one Android runtime permission, and check the device
//! did it.
//!
//! `pm grant` / `pm revoke` exit 0 without changing anything in at least
//! one case seen: on an API 37 emulator, revoking ACCESS_LOCAL_NETWORK from
//! an app that holds it with `REVOKE_WHEN_REQUESTED` left it granted. So the
//! app's runtime state is read back after every change.

use smix_adb::AdbClient;
use smix_simctl::DeviceControlError;

use crate::PermissionAction;
use crate::android_device::adb_to_simctl_err;

pub(crate) async fn set(
    client: &AdbClient,
    serial: &str,
    package: &str,
    permission: &str,
    action: PermissionAction,
) -> Result<(), DeviceControlError> {
    // Android has no per-permission reset: `pm reset-permissions` reverts
    // every app on the device. Reset is the nearest thing, a revoke.
    let (verb, done) = match action {
        PermissionAction::Grant => ("grant", client.pm_grant(serial, package, permission).await),
        PermissionAction::Revoke | PermissionAction::Reset => (
            "revoke",
            client.pm_revoke(serial, package, permission).await,
        ),
    };
    let command = format!("adb -s {serial} shell pm {verb} {package} {permission}");
    done.map_err(|e| adb_to_simctl_err(e, &format!("shell pm {verb}")))?;
    let line = client
        .runtime_permission(serial, package, permission)
        .await
        .map_err(|e| adb_to_simctl_err(e, "shell dumpsys package"))?;
    not_applied(line.as_deref(), package, permission, verb == "grant").map_or(Ok(()), |detail| {
        Err(DeviceControlError::NotApplied {
            subcommand: command,
            detail,
        })
    })
}

/// Why the device's line for the permission does not show it `granted`,
/// or `None` when it does.
fn not_applied(
    line: Option<&str>,
    package: &str,
    permission: &str,
    granted: bool,
) -> Option<String> {
    let Some(line) = line else {
        return Some(format!(
            "{package} has no runtime state for {permission} afterwards: the app does not \
             request it, or it is not a runtime permission on this Android release"
        ));
    };
    let want = if granted {
        "granted=true"
    } else {
        "granted=false"
    };
    if line.contains(want) {
        return None;
    }
    let mut why = format!("the device still reads `{line}` for {package}");
    if line.contains("REVOKE_WHEN_REQUESTED") {
        why.push_str(
            ". REVOKE_WHEN_REQUESTED marks a permission Android granted on its own, \
             as it does for an app that targets a release older than the permission; \
             it is taken away when the app asks for it",
        );
    }
    Some(why)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "android.permission.ACCESS_LOCAL_NETWORK";

    #[test]
    fn the_line_read_back_decides_whether_it_was_done() {
        let on = format!("{P}: granted=true, flags=[ USER_SENSITIVE_WHEN_GRANTED]");
        let off = format!("{P}: granted=false, flags=[ USER_SENSITIVE_WHEN_GRANTED]");
        assert_eq!(not_applied(Some(&on), "a", P, true), None);
        assert_eq!(not_applied(Some(&off), "a", P, false), None);
        let still = not_applied(Some(&on), "a", P, false).expect("not revoked");
        assert!(
            still.contains("granted=true") && still.contains(" a"),
            "{still}"
        );
        assert!(!still.contains("REVOKE_WHEN_REQUESTED"), "{still}");
        assert!(
            not_applied(Some(&off), "a", P, true).is_some(),
            "not granted"
        );
        let missing = not_applied(None, "a", P, true).expect("nothing to read");
        assert!(missing.contains("does not request it"), "{missing}");
    }

    #[test]
    fn a_permission_android_granted_by_itself_is_named() {
        let line = format!(
            "{P}: granted=true, flags=[ REVOKE_WHEN_REQUESTED|USER_SENSITIVE_WHEN_GRANTED]"
        );
        let why = not_applied(Some(&line), "com.android.dialer", P, false).expect("not revoked");
        assert!(why.contains("Android granted on its own"), "{why}");
    }
}
