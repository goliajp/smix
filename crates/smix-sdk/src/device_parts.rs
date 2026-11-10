//! The parts of the device itself a flow works: a foldable's hinge, and
//! the Digital Crown of an Apple Watch or Apple Vision Pro.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};

use crate::{App, Posture, simctl_to_failure};

impl App {
    /// Fold or unfold a foldable iOS simulator (iPhone Duo), returning
    /// once the panel the posture lights is the lit one. Flow step
    /// `setPosture: closed | halfOpen | open`.
    ///
    /// iOS simulators only: an Android emulator or a phone is refused by
    /// name rather than left unfolded.
    ///
    /// Since smix 20.1.0.
    pub async fn set_posture(&self, posture: Posture) -> Result<(), ExpectationFailure> {
        let udid = self.require_udid()?;
        let Some(simctl) = self.device.as_ios_simctl() else {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!(
                    "setPosture: folding is wired for iOS simulators, and this device is {:?}",
                    self.device.platform()
                ),
                ..Default::default()
            }));
        };
        simctl
            .set_posture(udid, posture)
            .await
            .map_err(simctl_to_failure)
    }

    /// Turn the Digital Crown `rotations` turns, at `velocity` turns a
    /// second when given: one is a full turn, and a positive turn scrolls a
    /// list on towards its later rows. Flow step `rotateCrown`.
    ///
    /// An Apple Watch or Apple Vision Pro only; any other device refuses
    /// by name. XCTest takes about 5 s for a turn of any size.
    ///
    /// # Errors
    ///
    /// A turn of zero, or a speed that is not a positive number, before
    /// anything is sent; the device's refusal when it has no crown.
    pub async fn rotate_crown(
        &self,
        rotations: f64,
        velocity: Option<f64>,
    ) -> Result<(), ExpectationFailure> {
        smix_driver::crown_turn_check(rotations, velocity)
            .map_err(|why| refused(format!("rotateCrown: {why}")))?;
        self.driving()?.rotate_crown(rotations, velocity).await
    }
}

fn refused(message: String) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message,
        ..Default::default()
    })
}
