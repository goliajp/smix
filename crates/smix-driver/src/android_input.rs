//! Typing on Android: into the field that holds focus, and emptying it.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_runner_client::RunnerTransportError;

use crate::android::AndroidDriver;

/// What the runner calls the refusal when no editable field holds focus.
const NO_FOCUSED_FIELD: &str = "no_focused_field";

impl AndroidDriver {
    /// Type `text` where input focus already is, without touching the
    /// screen.
    ///
    /// There is nothing to aim at: the caller named no field. Resolving
    /// "the focused element" and tapping its centre was the old way, and
    /// the tap is what went wrong — on a consumer's number pad it pressed
    /// the `1` key, so the field held one digit more than was typed. The
    /// runner already types into the focused editable node and reads it
    /// back, so a tap adds nothing but a chance to press something else.
    pub(crate) async fn fill_focused(
        &self,
        text: &str,
        clear_first: bool,
    ) -> Result<(), ExpectationFailure> {
        if clear_first {
            self.clear_focused_field("AndroidDriver::fill", None)
                .await?;
        }
        self.runner()
            .input_text(text)
            .await
            .map_err(|e| focused_input_failure(&e))
    }

    /// Empty the field that already holds focus, in one request.
    ///
    /// This sent fifty `/press-key DELETE` posts — fifty sequential
    /// round trips over the adb forward, and once `fill` began
    /// clearing first, on every fill. It was also wrong: fifty deletes
    /// do not empty a field holding more than fifty characters, so the
    /// new text landed after the remainder while the caller was told
    /// its value had been replaced.
    ///
    /// The runner does it now, exactly, through the focused node's
    /// `ACTION_SET_TEXT`.
    ///
    /// `at` names the field by where it was tapped. Without it the
    /// runner empties whatever holds focus, and focus does not move
    /// synchronously with the tap that moves it — a fill naming one
    /// Compose field was measured emptying another.
    pub(crate) async fn clear_focused_field(
        &self,
        stage: &str,
        at: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), ExpectationFailure> {
        let done = match at {
            Some(rect) => self.runner().clear_text_in(rect).await,
            None => self.runner().clear_text().await,
        };
        done.map(|_| ()).map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("{stage}: clear-first failed: {e}"),
                ..Default::default()
            })
        })
    }
}

/// A focused-field refusal reads as "nothing to type into", which is
/// what the caller's advice is written for; anything else stays a
/// driver error.
fn focused_input_failure(e: &RunnerTransportError) -> ExpectationFailure {
    let nothing_focused = matches!(
        e,
        RunnerTransportError::RefusedNaming { kind, .. } if kind == NO_FOCUSED_FIELD
    );
    ExpectationFailure::new(FailureInit {
        code: Some(if nothing_focused {
            FailureCode::ElementNotFound
        } else {
            FailureCode::DriverError
        }),
        message: format!("AndroidDriver::fill: {e}"),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(kind: &str) -> RunnerTransportError {
        RunnerTransportError::RefusedNaming {
            endpoint: "/input-text".into(),
            kind: kind.into(),
            saw: "the window stack".into(),
        }
    }

    #[test]
    fn nothing_focused_is_nothing_to_type_into() {
        assert_eq!(
            focused_input_failure(&refused(NO_FOCUSED_FIELD)).code,
            FailureCode::ElementNotFound
        );
    }

    #[test]
    fn a_field_that_took_the_wrong_text_stays_a_driver_error() {
        assert_eq!(
            focused_input_failure(&refused("text_mismatch")).code,
            FailureCode::DriverError
        );
    }
}
