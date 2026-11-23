//! An FCM message handed to an Android app the way Google Play services
//! hands it one: the broadcast `com.google.android.c2dm.intent.RECEIVE`,
//! with the message's parts as extras. The app's own
//! `FirebaseMessagingService` receives it, in whatever state the app is in.
//!
//! Only Play services may send that broadcast, so it is sent as root: an
//! emulator whose image has `su`. A Play Store image and a phone have none.

use serde_json::{Map, Value};
use smix_adb::AdbClient;

/// The broadcast the Firebase SDK's receiver answers.
pub const ACTION: &str = "com.google.android.c2dm.intent.RECEIVE";
/// What an app must hold to show a notification, from Android 13.
const POST: &str = "android.permission.POST_NOTIFICATIONS";

/// Why a push was not delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushRefused {
    /// The payload is not one the platform takes.
    Payload(String),
    /// The app is not on the device.
    NotInstalled(String),
    /// The app may not show notifications.
    NotAllowed(String),
    /// Anything else the device said.
    Device(String),
}

impl std::fmt::Display for PushRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Payload(s) | Self::NotInstalled(s) | Self::NotAllowed(s) | Self::Device(s) => {
                f.write_str(s)
            }
        }
    }
}

/// `android.notification`'s fields, and the extra each travels as.
const ANDROID_FIELDS: &[(&str, &str)] = &[
    ("channel_id", "android_channel_id"),
    ("icon", "icon"),
    ("color", "color"),
    ("sound", "sound"),
    ("tag", "tag"),
    ("click_action", "click_action"),
    ("image", "image"),
];

fn object<'a>(
    root: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a Map<String, Value>>, String> {
    match root.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => Ok(Some(o)),
        Some(_) => Err(format!("`{key}` is not an object")),
    }
}

/// The extras an FCM message arrives with, in the order they are sent.
///
/// The message as FCM's HTTP v1 API takes it, with or without the outer
/// `message`: `notification` (title, body, image), `data` (strings), and
/// `android` (`notification`, `data`).
pub fn extras(payload: &str) -> Result<Vec<(String, String)>, String> {
    let root: Value = serde_json::from_str(payload).map_err(|e| format!("not JSON: {e}"))?;
    let Value::Object(root) = root else {
        return Err("not a JSON object".into());
    };
    let message = object(&root, "message")?.unwrap_or(&root);
    if message.contains_key("aps") {
        return Err(
            "it has `aps`, which is an APNs payload; Android takes an FCM message: \
             {\"notification\": {…}, \"data\": {…}}"
                .into(),
        );
    }
    let android = object(message, "android")?;
    let mut out = Vec::new();
    for (where_, from) in [("data", Some(message)), ("android.data", android)] {
        let Some(data) = from.map(|m| object(m, "data")).transpose()?.flatten() else {
            continue;
        };
        for (k, v) in data {
            let Value::String(v) = v else {
                return Err(format!(
                    "`{where_}.{k}` is not a string; FCM carries only strings in data, so put \
                     nested JSON there as text"
                ));
            };
            if k == "from"
                || k == "message_type"
                || k.starts_with("google.")
                || k.starts_with("gcm.")
            {
                return Err(format!("`{where_}.{k}`: FCM keeps that key for itself"));
            }
            out.retain(|(have, _)| have != k);
            out.push((k.clone(), v.clone()));
        }
    }
    let notification = object(message, "notification")?;
    let on_android = android
        .map(|a| object(a, "notification"))
        .transpose()?
        .flatten();
    if notification.is_some() || on_android.is_some() {
        let mut say = |name: &str, field: &str| -> Result<(), String> {
            let value = on_android
                .and_then(|n| n.get(field))
                .or_else(|| notification.and_then(|n| n.get(field)));
            match value {
                None | Some(Value::Null) => Ok(()),
                Some(Value::String(v)) => {
                    out.push((format!("gcm.notification.{name}"), v.clone()));
                    Ok(())
                }
                Some(_) => Err(format!("`notification.{field}` is not a string")),
            }
        };
        say("title", "title")?;
        say("body", "body")?;
        for (field, name) in ANDROID_FIELDS {
            say(name, field)?;
        }
        // what tells the SDK to show it itself when the app is not in front
        out.push(("gcm.notification.e".into(), "1".into()));
        out.push(("gcm.n.e".into(), "1".into()));
    }
    if out.is_empty() {
        return Err(
            "it has neither `notification` nor `data`, so there is nothing to deliver".into(),
        );
    }
    Ok(out)
}

fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Whether `dumpsys package` says the app was force-stopped for user 0.
fn stopped(dump: &str) -> bool {
    dump.lines()
        .map(str::trim)
        .find(|l| l.starts_with("User 0:"))
        .is_some_and(|l| l.contains("stopped=true") && !l.contains("notLaunched=true"))
}

/// Whether a permission is granted, by `dumpsys package`; `None` when the
/// app does not ask for it.
fn granted(dump: &str, permission: &str) -> Option<bool> {
    dump.lines()
        .map(str::trim)
        .filter(|l| l.starts_with(permission))
        .find_map(|l| {
            if l.contains("granted=true") {
                Some(true)
            } else if l.contains("granted=false") {
                Some(false)
            } else {
                None
            }
        })
}

/// Deliver `payload` to `bundle`. Returns what is worth saying about it.
pub async fn push(
    adb: &AdbClient,
    serial: &str,
    bundle: &str,
    payload: &str,
    message_id: &str,
) -> Result<Vec<String>, PushRefused> {
    let extras = extras(payload).map_err(PushRefused::Payload)?;
    let device = |e: smix_adb::AdbError| PushRefused::Device(e.to_string());
    // `pm path` exits 1 with no output for a package that is not there
    let installed = adb
        .shell(serial, &["pm", "list", "packages", bundle])
        .await
        .map_err(device)?;
    if !installed
        .lines()
        .any(|l| l.trim() == format!("package:{bundle}"))
    {
        return Err(PushRefused::NotInstalled(format!(
            "{bundle} is not installed on {serial}"
        )));
    }
    let receivers = adb
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
                bundle,
            ],
        )
        .await
        .map_err(device)?;
    if !receivers.lines().any(|l| l.contains('/')) {
        return Err(PushRefused::Device(format!(
            "{bundle} has no receiver for {ACTION}, so it takes no FCM messages (the Firebase \
             Messaging SDK adds one)"
        )));
    }
    let dump = adb
        .shell(serial, &["dumpsys", "package", bundle])
        .await
        .map_err(device)?;
    let shows = extras.iter().any(|(k, _)| k == "gcm.n.e");
    let mut notes = Vec::new();
    if granted(&dump, POST) == Some(false) {
        let how = format!("`smix sim permission {serial} {bundle} notifications allow`");
        if shows {
            return Err(PushRefused::NotAllowed(format!(
                "{bundle} may not show notifications on {serial}; {how} allows it"
            )));
        }
        notes.push(format!(
            "{bundle} may not show notifications, so a data message reaches it but nothing it \
             posts is shown; {how} allows it"
        ));
    }
    if stopped(&dump) {
        return Err(PushRefused::Device(format!(
            "{bundle} was force-stopped, and Android delivers no push to it until it is opened \
             again — a real one is held back the same way. To test a killed app, end it without \
             stopping it: send it to the background, then `smix sim terminate {serial} {bundle} --reclaim`"
        )));
    }
    let id = adb
        .shell(serial, &["su", "0", "id", "-u"])
        .await
        .unwrap_or_default();
    if id.trim() != "0" {
        return Err(PushRefused::Device(format!(
            "{serial} gives no root shell, and only Google Play services or root may send an FCM \
             broadcast. Use an emulator image with Google APIs and without the Play Store, or \
             `sendPush` in a flow with the receiver it describes"
        )));
    }
    let mut argv: Vec<String> = ["su", "0", "am", "broadcast", "-a", ACTION, "-p", bundle]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let sent = [
        ("google.message_id", message_id),
        ("from", "smix"),
        ("google.delivered_priority", "high"),
        ("google.original_priority", "high"),
    ];
    let all = sent
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .chain(extras);
    for (k, v) in all {
        argv.extend(["--es".to_string(), quoted(&k), quoted(&v)]);
    }
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let said = adb.shell(serial, &argv).await.map_err(device)?;
    if !said.contains("Broadcast completed") {
        return Err(PushRefused::Device(format!(
            "am broadcast said: {}",
            said.trim()
        )));
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(payload: &str) -> Vec<(String, String)> {
        extras(payload).unwrap_or_else(|e| panic!("{payload}: {e}"))
    }

    fn has(v: &[(String, String)], k: &str, val: &str) -> bool {
        v.iter().any(|(a, b)| a == k && b == val)
    }

    #[test]
    fn data_travels_as_it_is_and_a_notification_as_the_sdk_reads_it() {
        let v = pairs(
            r#"{"message":{"notification":{"title":"T","body":"B"},
                "data":{"body":"{\"type\":\"invoice\"}"},
                "android":{"notification":{"channel_id":"c","title":"AT"}}}}"#,
        );
        assert!(has(&v, "body", r#"{"type":"invoice"}"#));
        assert!(
            has(&v, "gcm.notification.title", "AT"),
            "android's wins: {v:?}"
        );
        assert!(has(&v, "gcm.notification.body", "B"));
        assert!(has(&v, "gcm.notification.android_channel_id", "c"));
        assert!(has(&v, "gcm.n.e", "1") && has(&v, "gcm.notification.e", "1"));
    }

    #[test]
    fn a_data_message_asks_for_nothing_to_be_shown() {
        let v = pairs(r#"{"data":{"a":"1"},"android":{"data":{"a":"2"}}}"#);
        assert_eq!(v, vec![("a".to_string(), "2".to_string())]);
    }

    #[test]
    fn what_fcm_would_not_take_is_refused_by_name() {
        let why = |p: &str| extras(p).unwrap_err();
        assert!(why("{").contains("not JSON"));
        assert!(why("[]").contains("not a JSON object"));
        assert!(why(r#"{"aps":{}}"#).contains("APNs"));
        assert!(why(r#"{"data":{"body":{"a":1}}}"#).contains("data.body"));
        assert!(why(r#"{"data":{"google.x":"1"}}"#).contains("keeps that key"));
        assert!(why(r#"{"notification":{"title":3}}"#).contains("notification.title"));
        assert!(why(r#"{"data":[]}"#).contains("not an object"));
        assert!(why("{}").contains("nothing to deliver"));
    }

    // `dumpsys package`, Android 16
    const USER: &str = "    User 0: ceDataInode=1 deDataInode=2 installed=true hidden=false suspended=false distractionFlags=0 stopped=true notLaunched=false enabled=0";

    #[test]
    fn a_force_stopped_app_is_told_from_one_never_opened() {
        assert!(stopped(USER));
        assert!(!stopped(&USER.replace("stopped=true", "stopped=false")));
        assert!(!stopped(
            &USER.replace("notLaunched=false", "notLaunched=true")
        ));
    }

    #[test]
    fn a_permission_reads_from_its_own_line() {
        let dump = "    requested permissions:\n      android.permission.POST_NOTIFICATIONS\n    runtime permissions:\n      android.permission.POST_NOTIFICATIONS: granted=false, flags=[ USER_SET ]\n";
        assert_eq!(granted(dump, POST), Some(false));
        assert_eq!(
            granted(&dump.replace("=false,", "=true,"), POST),
            Some(true)
        );
        assert_eq!(granted("nothing", POST), None);
    }
}
