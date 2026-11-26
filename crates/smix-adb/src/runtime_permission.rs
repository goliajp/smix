//! An app's runtime permissions: `pm grant` / `pm revoke`, and what
//! `dumpsys package` says the app holds afterwards.

use crate::{AdbClient, AdbError};

/// Pull the granted names out of `dumpsys package`'s `runtime permissions:`
/// section.
///
/// Each entry reads `<name>: granted=<bool>, flags=[...]`, indented under
/// the heading. The section ends at the first line that does not parse as
/// one, since dumpsys simply moves on to its next topic.
pub(crate) fn parse_granted_runtime_permissions(dumpsys_stdout: &str) -> Vec<String> {
    let mut granted = Vec::new();
    let mut in_section = false;
    for line in dumpsys_stdout.lines() {
        let trimmed = line.trim();
        if trimmed == "runtime permissions:" {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((name, rest)) = trimmed.split_once(": ") else {
            in_section = false;
            continue;
        };
        if !name.contains('.') {
            in_section = false;
            continue;
        }
        if rest.starts_with("granted=true") {
            granted.push(name.to_string());
        }
    }
    granted
}

/// The `runtime permissions:` line for `permission`, as dumpsys printed it
/// (`<name>: granted=<bool>, flags=[...]`), or `None` when the app has no
/// runtime state for it: it never asked for it, or it is not a runtime
/// permission on this release.
pub(crate) fn runtime_permission_line<'a>(
    dumpsys_stdout: &'a str,
    permission: &str,
) -> Option<&'a str> {
    let wanted = format!("{permission}: ");
    let mut in_section = false;
    for line in dumpsys_stdout.lines() {
        let trimmed = line.trim();
        if trimmed == "runtime permissions:" {
            in_section = true;
            continue;
        }
        if in_section {
            if trimmed.starts_with(&wanted) {
                return Some(trimmed);
            }
            if !trimmed
                .split_once(": ")
                .is_some_and(|(n, _)| n.contains('.'))
            {
                in_section = false;
            }
        }
    }
    None
}

impl AdbClient {
    /// `adb -s <serial> shell pm grant <pkg> <android.permission.X>`.
    pub async fn pm_grant(
        &self,
        serial: &str,
        package: &str,
        permission: &str,
    ) -> Result<(), AdbError> {
        self.run_capture(Some(serial), "shell", &["pm", "grant", package, permission])
            .await?;
        Ok(())
    }

    /// The runtime permissions `<pkg>` currently holds.
    ///
    /// Read from `dumpsys package`, which is the only host-side view of
    /// them: `pm reset-permissions` exists but reverts every app on the
    /// device, including whatever the runner was granted to do its job.
    pub async fn runtime_permissions_granted(
        &self,
        serial: &str,
        package: &str,
    ) -> Result<Vec<String>, AdbError> {
        let (stdout, _) = self
            .run_capture(Some(serial), "shell", &["dumpsys", "package", package])
            .await?;
        Ok(parse_granted_runtime_permissions(&stdout))
    }

    /// `adb -s <serial> shell pm revoke <pkg> <android.permission.X>`.
    pub async fn pm_revoke(
        &self,
        serial: &str,
        package: &str,
        permission: &str,
    ) -> Result<(), AdbError> {
        self.run_capture(
            Some(serial),
            "shell",
            &["pm", "revoke", package, permission],
        )
        .await?;
        Ok(())
    }

    /// The device's own line for one runtime permission of `<pkg>`; see
    /// [`runtime_permission_line`].
    pub async fn runtime_permission(
        &self,
        serial: &str,
        package: &str,
        permission: &str,
    ) -> Result<Option<String>, AdbError> {
        let (stdout, _) = self
            .run_capture(Some(serial), "shell", &["dumpsys", "package", package])
            .await?;
        Ok(runtime_permission_line(&stdout, permission).map(str::to_string))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// From `dumpsys package com.android.dialer` on an API 37 emulator,
    /// cut to the two sections that name the permission.
    const API37: &str = "\
    requested permissions:
      android.permission.ACCESS_LOCAL_NETWORK
      android.permission.CAMERA
    install permissions:
      android.permission.INTERNET: granted=true
    User 0: ceDataInode=549184 installed=true
      runtime permissions:
        android.permission.ACCESS_LOCAL_NETWORK: granted=true, flags=[ REVOKE_WHEN_REQUESTED|USER_SENSITIVE_WHEN_GRANTED|USER_SENSITIVE_WHEN_DENIED]
        android.permission.CAMERA: granted=false, flags=[ GRANTED_BY_DEFAULT|USER_SENSITIVE_WHEN_GRANTED|USER_SENSITIVE_WHEN_DENIED]
      enabledComponents:
        com.android.dialer.Main
";

    #[test]
    fn one_permission_is_read_from_the_runtime_section_only() {
        assert_eq!(
            runtime_permission_line(API37, "android.permission.CAMERA"),
            Some(
                "android.permission.CAMERA: granted=false, flags=[ GRANTED_BY_DEFAULT|USER_SENSITIVE_WHEN_GRANTED|USER_SENSITIVE_WHEN_DENIED]"
            )
        );
        assert!(
            runtime_permission_line(API37, "android.permission.ACCESS_LOCAL_NETWORK")
                .is_some_and(|l| l.contains("granted=true"))
        );
        // requested and install sections name it too; neither is runtime state
        assert_eq!(
            runtime_permission_line(API37, "android.permission.INTERNET"),
            None
        );
        assert_eq!(
            runtime_permission_line(API37, "android.permission.RECORD_AUDIO"),
            None
        );
        assert_eq!(
            runtime_permission_line("", "android.permission.CAMERA"),
            None
        );
    }
}
