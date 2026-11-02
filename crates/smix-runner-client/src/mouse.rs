//! `POST /mouse-at-norm-coord` — a mouse pointer over a point, or off the app.

use serde::Serialize;
use smix_input::Mouse;

use crate::{HttpRunnerClient, OkEnvelope, RunnerTransportError};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Req {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    nx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ny: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to_nx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to_ny: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dx_dp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dy_dp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wheel: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ctrl: bool,
}

impl From<&Mouse> for Req {
    fn from(m: &Mouse) -> Self {
        let at = m.at();
        let mut r = Req {
            kind: m.kind(),
            nx: at.map(|a| a.0),
            ny: at.map(|a| a.1),
            ..Req::default()
        };
        match *m {
            Mouse::Wheel { notches, ctrl, .. } => {
                r.wheel = Some(notches);
                r.ctrl = ctrl;
            }
            Mouse::Drag { to, .. } => {
                r.to_nx = Some(to.0);
                r.to_ny = Some(to.1);
            }
            Mouse::DragBy { by_dp, .. } => {
                r.dx_dp = Some(by_dp.0);
                r.dy_dp = Some(by_dp.1);
            }
            _ => {}
        }
        r
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
        let v = serde_json::to_value(Req::from(&Mouse::SecondaryClick { at: (0.5, 0.5) })).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "secondary", "nx": 0.5, "ny": 0.5})
        );
        let v = serde_json::to_value(Req::from(&Mouse::Wheel {
            at: (0.5, 0.5),
            notches: -2.0,
            ctrl: true,
        }))
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "wheel", "nx": 0.5, "ny": 0.5, "wheel": -2.0, "ctrl": true})
        );
        let v = serde_json::to_value(Req::from(&Mouse::Drag {
            from: (0.25, 0.5),
            to: (0.75, 0.5),
        }))
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "drag", "nx": 0.25, "ny": 0.5, "toNx": 0.75, "toNy": 0.5})
        );
        let v = serde_json::to_value(Req::from(&Mouse::DragBy {
            from: (0.25, 0.5),
            by_dp: (96.0, 0.0),
        }))
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "drag", "nx": 0.25, "ny": 0.5, "dxDp": 96.0, "dyDp": 0.0})
        );
    }
}
