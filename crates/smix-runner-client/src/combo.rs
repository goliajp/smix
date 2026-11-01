//! `POST /press-key-combo` — a key pressed with modifiers held.

use serde::Serialize;
use smix_input::KeyCombo;

use crate::{HttpRunnerClient, OkEnvelope, RunnerTransportError};

#[derive(Serialize)]
struct Req {
    key: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ctrl: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    shift: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    alt: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    meta: bool,
}

impl From<&KeyCombo> for Req {
    fn from(c: &KeyCombo) -> Self {
        Req {
            key: c.key.android_name(),
            ctrl: c.ctrl,
            shift: c.shift,
            alt: c.alt,
            meta: c.meta,
        }
    }
}

impl HttpRunnerClient {
    /// `POST /press-key-combo` — the key's down and up with the modifiers'
    /// meta state. Only the Android runner serves it; a key the system
    /// refuses comes back as `ok: false`.
    pub async fn press_key_combo(&self, combo: &KeyCombo) -> Result<(), RunnerTransportError> {
        let body: OkEnvelope = self
            .json_post_within(
                "/press-key-combo",
                &Req::from(combo),
                None,
                crate::route_limits::wait_for_request(
                    "/press-key-combo",
                    std::time::Duration::ZERO,
                    self.auto_activate,
                )?,
            )
            .await?;
        body.require_ok("/press-key-combo")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_names_the_key_as_android_does_and_only_the_modifiers_held() {
        let c: KeyCombo = "ctrl+shift+z".parse().unwrap();
        assert_eq!(
            serde_json::to_value(Req::from(&c)).unwrap(),
            serde_json::json!({"key": "Z", "ctrl": true, "shift": true})
        );
        let c: KeyCombo = "alt+left".parse().unwrap();
        assert_eq!(
            serde_json::to_value(Req::from(&c)).unwrap(),
            serde_json::json!({"key": "DPAD_LEFT", "alt": true})
        );
    }
}
