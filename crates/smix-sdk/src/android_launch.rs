//! Launching on Android, done when the app has drawn.

use smix_simctl::DeviceControlError;

use super::{AndroidDeviceControl, adb_to_simctl_err};

impl AndroidDeviceControl {
    /// Start the activity and wait for it to draw, so the next step reads
    /// the app rather than the screen it is opening over. An app that has
    /// not drawn when the wait ends is left running and noted, not failed:
    /// it may still draw before the next step gives up.
    pub(super) async fn launch_until_drawn(
        &self,
        serial: &str,
        bundle_id: &str,
        activity: &str,
        extras: &[(&str, &str)],
    ) -> Result<u32, DeviceControlError> {
        let launched = self
            .client
            .start_activity_until_drawn(serial, bundle_id, activity, extras)
            .await
            .map_err(|e| adb_to_simctl_err(e, "shell am start -W"))?;
        let note = match launched {
            smix_adb::Launched::Drew { .. } => None,
            smix_adb::Launched::NotDrawn { status } => Some(format!(
                "{bundle_id} was launched and Android stopped waiting for it to draw (Status: {status}); \
                 the next step may read the screen it is opening over"
            )),
            smix_adb::Launched::StillWaiting => Some(format!(
                "{bundle_id} was launched and had not drawn after {} s; \
                 the next step may read the screen it is opening over",
                smix_adb::LAUNCH_DRAWS_WITHIN.as_secs()
            )),
        };
        let mut notes = self.launch_notes.lock().await;
        match note {
            Some(n) => notes.insert(serial.to_string(), n),
            None => notes.remove(serial),
        };
        Ok(0)
    }
}
