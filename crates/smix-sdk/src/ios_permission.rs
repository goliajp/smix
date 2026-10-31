//! What a refused `simctl privacy` call means, said so it can be acted on.

use smix_simctl::{DeviceControlError, SimctlPermission};

/// Notifications are not a TCC service: iOS keeps their authorization in
/// SpringBoard's bulletin board, which `simctl privacy` cannot write on
/// iOS 27 simulators ("Failed to create TCC authorization record /
/// Operation not permitted") whether granting, revoking or resetting.
/// Writing that store directly takes a SpringBoard restart, which ends the
/// app and runner under test, so the prompt is answered instead.
pub(crate) fn refusal_in_plain_words(
    permission: SimctlPermission,
    e: DeviceControlError,
) -> DeviceControlError {
    let said = e.to_string();
    if !(permission == SimctlPermission::Notifications && said.contains("Operation not permitted"))
    {
        return e;
    }
    DeviceControlError::non_zero_exit(
        "privacy notifications",
        1,
        format!(
            "setPermissions cannot set notifications on this simulator: `simctl privacy` \
             refuses it (Operation not permitted), because notification authorization is \
             not kept where simctl writes. Let the app ask, then answer the prompt with \
             `- tapOn: \"Allow\"` (or \"Don't Allow\"); tapOn presses a button on a system \
             popup when the app has none by that name.\nsimctl said: {said}"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // simctl's words on sim-smix-02, iOS 27, 2026-11-01
    const REFUSED: &str = "An error was encountered processing the command \
        (domain=NSPOSIXErrorDomain, code=1):\nSimulator device failed to complete the \
        requested operation.\nOperation not permitted\nUnderlying error \
        (domain=NSPOSIXErrorDomain, code=1):\n\tFailed to create TCC authorization record\n\t\
        Operation not permitted";

    #[test]
    fn a_refused_notification_grant_says_what_to_do_instead() {
        let e = DeviceControlError::non_zero_exit("privacy", 1, REFUSED);
        let said = refusal_in_plain_words(SimctlPermission::Notifications, e).to_string();
        assert!(said.contains("tapOn: \"Allow\""), "{said}");
        assert!(
            said.contains("Failed to create TCC authorization record"),
            "{said}"
        );
    }

    #[test]
    fn other_refusals_are_passed_through_untouched() {
        let e = DeviceControlError::non_zero_exit("privacy", 1, REFUSED);
        let before = e.to_string();
        assert_eq!(
            refusal_in_plain_words(SimctlPermission::Photos, e).to_string(),
            before
        );
        let other = DeviceControlError::non_zero_exit("privacy", 1, "Invalid device");
        let before = other.to_string();
        assert_eq!(
            refusal_in_plain_words(SimctlPermission::Notifications, other).to_string(),
            before
        );
    }
}
