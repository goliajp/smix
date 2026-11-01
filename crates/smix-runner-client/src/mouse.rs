//! `POST /mouse-at-norm-coord` — a mouse pointer over a point, or off the app.

use serde::Serialize;
use smix_input::Mouse;

use crate::{HttpRunnerClient, OkEnvelope, RunnerTransportError};

#[derive(Serialize)]
struct Req {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    nx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ny: Option<f64>,
}

impl From<&Mouse> for Req {
    fn from(m: &Mouse) -> Self {
        let at = m.at();
        Req {
            kind: m.kind(),
            nx: at.map(|a| a.0),
            ny: at.map(|a| a.1),
        }
    }
}

impl HttpRunnerClient {
    /// `POST /mouse-at-norm-coord` — mouse events, not touches. Only the
    /// Android runner serves it; an event the system refuses comes back
    /// as `ok: false`.
    pub async fn mouse_at_norm_coord(&self, mouse: &Mouse) -> Result<(), RunnerTransportError> {
        let body: OkEnvelope = self
            .json_post_within(
                "/mouse-at-norm-coord",
                &Req::from(mouse),
                None,
                crate::route_limits::wait_for_request(
                    "/mouse-at-norm-coord",
                    std::time::Duration::ZERO,
                    self.auto_activate,
                )?,
            )
            .await?;
        body.require_ok("/mouse-at-norm-coord")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_the_one_the_android_runner_reads() {
        let v = serde_json::to_value(Req::from(&Mouse::Hover { at: (0.25, 0.75) })).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "hover", "nx": 0.25, "ny": 0.75})
        );
        let v = serde_json::to_value(Req::from(&Mouse::Away)).unwrap();
        assert_eq!(v, serde_json::json!({"kind": "away"}));
    }
}
