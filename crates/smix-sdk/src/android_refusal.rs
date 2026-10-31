//! Refusals for what the Android backend cannot do.

use smix_simctl::DeviceControlError;

/// The refusal for something this backend cannot do, read from the one
/// table that says so.
///
/// These sentences used to be written here, and `DevicectlClient` had
/// seventeen more of its own. Two sets of prose about the same question,
/// with nothing able to say whether they agreed — and they did not: the
/// platform table's first draft had both clipboard cells as `Works` on
/// Android, taken from a research note rather than from this file, which
/// has refused them since Android 10 sealed the clipboard.
pub(crate) fn refused_here(
    action: &str,
    kind: smix_simctl::registry::DeviceKind,
) -> DeviceControlError {
    use crate::device_control::{Availability, availability};

    match availability(action, kind) {
        Some(Availability::RefusedByName { why, instead }) => DeviceControlError::non_zero_exit(
            action,
            -1,
            format!("{action} is not available on Android: {why}\nInstead: {instead}").as_str(),
        ),
        other => DeviceControlError::non_zero_exit(
            action,
            -1,
            format!(
                "{action} was refused on Android, but the platform table says {other:?}. \
                 The table and this code disagree; fix one."
            )
            .as_str(),
        ),
    }
}
