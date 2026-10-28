//! `POST /pinch-at-norm-coord` — two fingers in one gesture.

use serde::Serialize;
use smix_input::Pinch;

use crate::{HttpRunnerClient, OkEnvelope, RunnerTransportError};

/// What both runners add to the gesture the request describes: 200 ms
/// moving to the start and 200 ms still before lifting, and on Android a
/// 500 ms idle wait afterwards.
const AFTER_MS: u64 = 200 + 200 + 500;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Req {
    cx: f64,
    cy: f64,
    from_span: f64,
    to_span: f64,
    duration_ms: u64,
    pan_dx: f64,
    pan_dy: f64,
    pan_ms: u64,
    run_up: &'static str,
}

impl From<&Pinch> for Req {
    fn from(p: &Pinch) -> Self {
        let (pan_dx, pan_dy, pan_ms) = p.pan.map_or((0.0, 0.0, 0), |pan| (pan.dx, pan.dy, pan.ms));
        Req {
            cx: p.centre.0,
            cy: p.centre.1,
            from_span: p.from_span,
            to_span: p.to_span,
            duration_ms: p.duration_ms,
            pan_dx,
            pan_dy,
            pan_ms,
            run_up: p.run_up.as_wire(),
        }
    }
}

impl HttpRunnerClient {
    /// `POST /pinch-at-norm-coord` — both fingers' touches dispatched in
    /// one gesture. A finger that would leave the screen comes back as the
    /// runner's 400, naming the finger and the point.
    pub async fn pinch_at_norm_coord(&self, pinch: &Pinch) -> Result<(), RunnerTransportError> {
        let longest = std::time::Duration::from_millis(pinch.total_ms() + AFTER_MS);
        let body: OkEnvelope = self
            .json_post_within(
                "/pinch-at-norm-coord",
                &Req::from(pinch),
                None,
                crate::route_limits::wait_for_request(
                    "/pinch-at-norm-coord",
                    longest,
                    self.auto_activate,
                )?,
            )
            .await?;
        body.require_ok("/pinch-at-norm-coord")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_input::Pan;

    #[test]
    fn the_body_is_the_one_both_runners_read() {
        let p = Pinch::new((0.5, 0.4), 0.2, 0.6)
            .with_pan(Some(Pan::new(0.1, -0.2, 300)))
            .with_run_up(smix_input::RunUp::Slide);
        let v = serde_json::to_value(Req::from(&p)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "cx": 0.5, "cy": 0.4, "fromSpan": 0.2, "toSpan": 0.6, "durationMs": 500,
                "panDx": 0.1, "panDy": -0.2, "panMs": 300, "runUp": "slide"
            })
        );
        let v = serde_json::to_value(Req::from(
            &p.with_pan(None).with_run_up(smix_input::RunUp::default()),
        ))
        .unwrap();
        assert_eq!(v["panMs"], 0);
        assert_eq!(v["runUp"], "spread");
    }
}
