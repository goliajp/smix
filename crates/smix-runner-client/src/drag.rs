//! `POST /drag-at-norm-coord` — press, hold, move, rest, lift.

use serde::Serialize;
use smix_input::Drag;

use crate::{HttpRunnerClient, OkEnvelope, RunnerTransportError};

/// What the runners add to the gesture the request describes: Android's
/// 500 ms idle wait afterwards.
const AFTER_MS: u64 = 500;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Req {
    from_nx: f64,
    from_ny: f64,
    to_nx: f64,
    to_ny: f64,
    hold_ms: u64,
    duration_ms: u64,
    release_ms: u64,
}

impl From<&Drag> for Req {
    fn from(d: &Drag) -> Self {
        Req {
            from_nx: d.from.0,
            from_ny: d.from.1,
            to_nx: d.to.0,
            to_ny: d.to.1,
            hold_ms: d.hold_ms,
            duration_ms: d.duration_ms,
            release_ms: d.release_ms,
        }
    }
}

impl HttpRunnerClient {
    /// `POST /drag-at-norm-coord` — the whole touch in one request, since
    /// a finger cannot be held down across two.
    pub async fn drag_at_norm_coord(&self, drag: &Drag) -> Result<(), RunnerTransportError> {
        let longest = std::time::Duration::from_millis(drag.total_ms() + AFTER_MS);
        let body: OkEnvelope = self
            .json_post_within(
                "/drag-at-norm-coord",
                &Req::from(drag),
                None,
                crate::route_limits::wait_for_request(
                    "/drag-at-norm-coord",
                    longest,
                    self.auto_activate,
                )?,
            )
            .await?;
        body.require_ok("/drag-at-norm-coord")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_the_one_both_runners_read() {
        let d = Drag::new((0.2, 0.3), (0.8, 0.9))
            .with_hold_ms(600)
            .with_duration_ms(400)
            .with_release_ms(0);
        let v = serde_json::to_value(Req::from(&d)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "fromNx": 0.2, "fromNy": 0.3, "toNx": 0.8, "toNy": 0.9,
                "holdMs": 600, "durationMs": 400, "releaseMs": 0
            })
        );
    }
}
