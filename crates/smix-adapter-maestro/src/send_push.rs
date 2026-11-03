//! `sendPush`: a notification delivered to the app under test.
//!
//! ```yaml
//! - sendPush:
//!     title: Order shipped
//!     body: Your parcel is on its way
//!     data: { route: detail }      # the payload's other keys, for the app
//! - sendPush: { payload: push/order.apns }   # an APNs payload, relative to the flow
//! ```
//!
//! iOS hands the payload to `simctl push`. Android has no local push
//! service to hand it to, so smix sends the app an explicit broadcast and
//! the app's debug build posts the notification itself (see the guide).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_norway::{Mapping, Value};

use crate::parser::reject_unknown_keys;
use crate::{ParseError, Step};

/// What a `sendPush` step delivers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PushPayload {
    /// Spelled in the flow: the alert's title and body, and keys of the
    /// app's own that go beside `aps`.
    Alert {
        /// The alert's title.
        title: String,
        /// The alert's body; empty when the flow gives none.
        #[serde(default)]
        body: String,
        /// The app's own keys, beside `aps`.
        #[serde(default)]
        data: serde_json::Map<String, serde_json::Value>,
    },
    /// An APNs payload file, as `simctl push` takes one.
    File(String),
}

pub(crate) fn parse(v: &Value) -> Result<Step, ParseError> {
    let invalid = |reason: String| ParseError::InvalidValue {
        field: "sendPush".into(),
        reason,
    };
    let map: &Mapping = v.as_mapping().ok_or_else(|| {
        invalid(format!(
            "expected `{{ title: …, body: …, data: … }}` or `{{ payload: <file> }}`, got {v:?}"
        ))
    })?;
    reject_unknown_keys(
        map,
        "sendPush",
        "sendPush",
        &["title", "body", "data", "payload"],
        &[],
    )?;
    let text = |k: &str| {
        map.get(Value::String(k.into()))
            .map(|v| v.as_str().map(str::to_string))
    };
    if let Some(file) = text("payload") {
        if ["title", "body", "data"]
            .iter()
            .any(|k| map.contains_key(Value::String((*k).into())))
        {
            return Err(invalid(
                "`payload:` is the whole notification; it does not take `title`, `body` or `data` \
                 beside it"
                    .into(),
            ));
        }
        let file = file
            .filter(|f| !f.is_empty())
            .ok_or_else(|| invalid("`payload:` is a file path".into()))?;
        return Ok(Step::SendPush(PushPayload::File(file)));
    }
    let title = text("title")
        .flatten()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| invalid("a notification needs a `title:` (or a `payload:` file)".into()))?;
    let body = match text("body") {
        None => String::new(),
        Some(b) => b.ok_or_else(|| invalid("`body:` is text".into()))?,
    };
    let data = match map.get(Value::String("data".into())) {
        None => serde_json::Map::new(),
        Some(d @ Value::Mapping(_)) => match serde_json::to_value(d) {
            Ok(serde_json::Value::Object(o)) => o,
            _ => return Err(invalid("`data:` must be a mapping with string keys".into())),
        },
        Some(other) => return Err(invalid(format!("`data:` is a mapping, got {other:?}"))),
    };
    if data.contains_key("aps") {
        return Err(invalid(
            "`data:` holds the app's own keys; `aps` is built from `title` and `body`".into(),
        ));
    }
    Ok(Step::SendPush(PushPayload::Alert { title, body, data }))
}

impl PushPayload {
    /// The payload as `simctl push` takes it, read from `base` for a file.
    pub(crate) fn apns_json(&self, base: &Path) -> Result<String, String> {
        match self {
            PushPayload::Alert { title, body, data } => {
                let mut root = data.clone();
                root.insert(
                    "aps".into(),
                    serde_json::json!({ "alert": { "title": title, "body": body } }),
                );
                Ok(serde_json::Value::Object(root).to_string())
            }
            PushPayload::File(f) => {
                let path = base.join(f);
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("sendPush: cannot read {}: {e}", path.display()))?;
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(serde_json::Value::Object(o)) if o.contains_key("aps") => Ok(text),
                    Ok(_) => Err(format!(
                        "sendPush: {} is not an APNs payload: it needs an `aps` object",
                        path.display()
                    )),
                    Err(e) => Err(format!("sendPush: {} is not JSON: {e}", path.display())),
                }
            }
        }
    }

    /// How the run record names it.
    pub(crate) fn summary(&self) -> String {
        match self {
            PushPayload::Alert { title, .. } => format!("sendPush \"{title}\""),
            PushPayload::File(f) => format!("sendPush {f}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(s: &str) -> Result<Step, ParseError> {
        parse(&serde_norway::from_str::<Value>(s).unwrap())
    }

    #[test]
    fn an_alert_puts_its_data_beside_aps() {
        let Step::SendPush(p) =
            step("{ title: Order shipped, body: On its way, data: { route: detail, n: 2 } }")
                .unwrap()
        else {
            panic!()
        };
        let json: serde_json::Value =
            serde_json::from_str(&p.apns_json(Path::new(".")).unwrap()).unwrap();
        assert_eq!(json["aps"]["alert"]["title"], "Order shipped");
        assert_eq!(json["aps"]["alert"]["body"], "On its way");
        assert_eq!(json["route"], "detail");
        assert_eq!(json["n"], 2);
    }

    #[test]
    fn a_notification_needs_a_title_or_a_file_and_not_both() {
        assert!(
            step("{ body: no title }")
                .unwrap_err()
                .to_string()
                .contains("title")
        );
        assert!(
            step("{ payload: a.apns, title: x }")
                .unwrap_err()
                .to_string()
                .contains("whole notification")
        );
        assert!(
            step("{ title: x, data: [1] }")
                .unwrap_err()
                .to_string()
                .contains("mapping")
        );
        assert!(
            step("{ title: x, data: { aps: 1 } }")
                .unwrap_err()
                .to_string()
                .contains("aps")
        );
        assert!(step("{ title: x, sound: y }").is_err());
    }

    #[test]
    fn a_file_must_be_an_apns_payload() {
        let dir = std::env::temp_dir().join(format!("smix-send-push-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ok.apns"), r#"{"aps":{"alert":"hi"},"route":"x"}"#).unwrap();
        std::fs::write(dir.join("bad.apns"), r#"{"route":"x"}"#).unwrap();
        let ok = PushPayload::File("ok.apns".into()).apns_json(&dir).unwrap();
        assert!(ok.contains("\"route\""));
        let bad = PushPayload::File("bad.apns".into())
            .apns_json(&dir)
            .unwrap_err();
        assert!(bad.contains("needs an `aps` object"), "{bad}");
        let missing = PushPayload::File("none.apns".into())
            .apns_json(&dir)
            .unwrap_err();
        assert!(missing.contains("cannot read"), "{missing}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
