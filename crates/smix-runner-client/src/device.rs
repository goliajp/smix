//! The device's own controls: its orientation, and the Digital Crown of an
//! Apple Watch or Apple Vision Pro.

use std::time::Duration;

use serde::Serialize;

use crate::{HttpRunnerClient, OkEnvelope, RunnerTransportError};

/// What XCTest takes for a turn of any size before the time the turn
/// itself takes: 5.3 s, measured on watchOS 27.2 for a tenth of a turn and
/// for three turns at half a turn a second (11.2 s), with a margin.
const TURN_OVERHEAD_MS: u64 = 6_000;
/// A turn at XCTest's own speed, when none is asked for: one full turn
/// added 0.4 s to the overhead above.
const DEFAULT_MS_PER_TURN: f64 = 500.0;

#[derive(Serialize)]
struct TurnReq {
    rotations: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    velocity: Option<f64>,
}

/// How long a turn of `rotations` at `velocity` turns a second takes.
fn turn_takes(rotations: f64, velocity: Option<f64>) -> Duration {
    let turning = match velocity {
        Some(v) => rotations.abs() / v * 1000.0,
        None => rotations.abs() * DEFAULT_MS_PER_TURN,
    };
    Duration::from_millis(TURN_OVERHEAD_MS + turning.ceil() as u64)
}

impl HttpRunnerClient {
    /// `POST /set-orientation` — rotate sim via swift
    /// `XCUIDevice.shared.orientation` setter. `orientation` literal:
    /// "portrait" | "portraitUpsideDown" | "landscapeLeft" | "landscapeRight".
    pub async fn set_orientation(&self, orientation: &str) -> Result<(), RunnerTransportError> {
        #[derive(Serialize)]
        struct Req<'a> {
            orientation: &'a str,
        }
        let body: OkEnvelope = self
            .json_post("/set-orientation", &Req { orientation }, None)
            .await?;
        body.require_ok("/set-orientation")
    }

    /// `POST /rotate-crown` — turn the Digital Crown `rotations` turns, at `velocity` turns a
    /// second when given. One is a full turn; a positive turn scrolls a
    /// list on towards its later rows. A device with no crown refuses by
    /// name.
    pub async fn rotate_crown(
        &self,
        rotations: f64,
        velocity: Option<f64>,
    ) -> Result<(), RunnerTransportError> {
        let body: OkEnvelope = self
            .json_post_within(
                "/rotate-crown",
                &TurnReq {
                    rotations,
                    velocity,
                },
                None,
                crate::route_limits::wait_for_request(
                    "/rotate-crown",
                    turn_takes(rotations, velocity),
                    self.auto_activate,
                )?,
            )
            .await?;
        body.require_ok("/rotate-crown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_takes_the_overhead_and_its_own_time() {
        assert_eq!(turn_takes(3.0, Some(0.5)), Duration::from_millis(12_000));
        assert_eq!(turn_takes(-0.1, None), Duration::from_millis(6_050));
    }

    #[test]
    fn the_body_leaves_out_a_speed_not_asked_for() {
        let v = serde_json::to_value(TurnReq {
            rotations: -1.5,
            velocity: None,
        })
        .unwrap();
        assert_eq!(v, serde_json::json!({ "rotations": -1.5 }));
    }
}
