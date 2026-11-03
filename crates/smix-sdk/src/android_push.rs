//! `sendPush` on Android.
//!
//! There is no local push service to hand a payload to, and a notification
//! posted from the shell belongs to the shell — tapping it does not open
//! the app. So the app posts it: smix sends the explicit broadcast
//! [`ACTION`] to the app's package with the alert's `title`, `body` and the
//! payload's other keys as `data` (a JSON object, as text), and a receiver
//! in the app's debug build posts the notification on its own channel.

use smix_adb::AdbClient;
use smix_simctl::DeviceControlError;

use crate::android_device::adb_to_simctl_err;

/// The broadcast the app's receiver answers.
pub const ACTION: &str = "dev.smix.PUSH";

/// What the broadcast carries, read from an APNs payload.
#[derive(Debug, PartialEq)]
pub(crate) struct Extras {
    pub title: String,
    pub body: String,
    /// The payload without `aps`, as JSON text.
    pub data: String,
}

/// `aps.alert` is a dictionary with `title` and `body`, or a string, which
/// APNs shows as the body.
pub(crate) fn extras(apns: &str) -> Result<Extras, String> {
    let serde_json::Value::Object(mut root) =
        serde_json::from_str::<serde_json::Value>(apns).map_err(|e| format!("not JSON: {e}"))?
    else {
        return Err("not a JSON object".into());
    };
    let aps = root.remove("aps").ok_or("no `aps` object")?;
    let alert = aps.get("alert").cloned().unwrap_or_default();
    let text = |k: &str| {
        alert
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let (title, body) = match &alert {
        serde_json::Value::String(s) => (String::new(), s.clone()),
        _ => (text("title"), text("body")),
    };
    Ok(Extras {
        title,
        body,
        data: serde_json::Value::Object(root).to_string(),
    })
}

/// Whether `cmd package query-receivers` found nothing for the action.
fn no_receiver(said: &str) -> bool {
    !said.lines().any(|l| l.contains('/'))
}

fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub(crate) async fn send(
    client: &AdbClient,
    serial: &str,
    bundle_id: &str,
    apns_json_path: &str,
) -> Result<(), DeviceControlError> {
    let refuse = |sub: &str, why: String| DeviceControlError::non_zero_exit(sub, 1, why);
    let apns = std::fs::read_to_string(apns_json_path)
        .map_err(|e| refuse("sendPush", format!("cannot read {apns_json_path}: {e}")))?;
    let extras = extras(&apns).map_err(|e| refuse("sendPush", format!("{apns_json_path}: {e}")))?;
    let found = client
        .shell(
            serial,
            &[
                "cmd",
                "package",
                "query-receivers",
                "--brief",
                "-a",
                ACTION,
                "-p",
                bundle_id,
            ],
        )
        .await
        .map_err(|e| adb_to_simctl_err(e, "shell cmd package query-receivers"))?;
    if no_receiver(&found) {
        return Err(refuse(
            "sendPush",
            format!(
                "{bundle_id} has no receiver for {ACTION}: on Android the app posts the \
                 notification itself, from a receiver in its debug build — see \"sendPush\" in \
                 the YAML reference for the few lines to add"
            ),
        ));
    }
    let said = client
        .shell(
            serial,
            &[
                "am",
                "broadcast",
                "-a",
                ACTION,
                "-p",
                bundle_id,
                "--es",
                "title",
                &quoted(&extras.title),
                "--es",
                "body",
                &quoted(&extras.body),
                "--es",
                "data",
                &quoted(&extras.data),
            ],
        )
        .await
        .map_err(|e| adb_to_simctl_err(e, "shell am broadcast"))?;
    if !said.contains("Broadcast completed") {
        return Err(refuse(
            "sendPush",
            format!("am broadcast said: {}", said.trim()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alert_dictionary_gives_title_body_and_the_other_keys() {
        let e = extras(
            r#"{"aps":{"alert":{"title":"Order shipped","body":"On its way"}},"route":"detail"}"#,
        )
        .unwrap();
        assert_eq!(e.title, "Order shipped");
        assert_eq!(e.body, "On its way");
        assert_eq!(e.data, r#"{"route":"detail"}"#);
    }

    #[test]
    fn an_alert_string_is_the_body() {
        let e = extras(r#"{"aps":{"alert":"hello"}}"#).unwrap();
        assert_eq!(
            (e.title.as_str(), e.body.as_str(), e.data.as_str()),
            ("", "hello", "{}")
        );
    }

    #[test]
    fn a_payload_without_aps_is_refused() {
        assert!(extras(r#"{"route":"x"}"#).unwrap_err().contains("aps"));
        assert!(extras("[1]").is_err());
    }

    #[test]
    fn a_receiver_is_found_by_its_component_name() {
        assert!(!no_receiver("dev.smix.fixture/.PushReceiver\n"));
        assert!(no_receiver("No receivers found\n"));
        assert!(no_receiver(""));
    }
}
