//! Typing on Android: into the field that holds focus, and emptying it.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_runner_client::{IncludeScope, RunnerTransportError};
use smix_selector::Selector;

use crate::android::AndroidDriver;
use crate::android_aim::resolve_rect_with_implicit_wait;

/// How the text goes into the field: typed through the keyboard, or set
/// on the field with `ACTION_SET_TEXT`, which no keyboard sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    Type,
    Set,
}

impl Entry {
    fn verb(self) -> &'static str {
        match self {
            Entry::Type => "fill",
            Entry::Set => "set_text",
        }
    }
}

/// What the runner calls the refusal when no editable field holds focus.
const NO_FOCUSED_FIELD: &str = "no_focused_field";

impl AndroidDriver {
    /// Put `text` in the field `selector` names, the one way `entry` says.
    ///
    /// Host-resolve → tap to focus → `/input-text`, naming the field by
    /// its box. Under `force_key_events` nothing is resolved or tapped:
    /// the mode exists for fields the tree cannot address, so resolving
    /// first would fail for exactly the callers who asked for it.
    pub(crate) async fn enter_text(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
        clear_first: bool,
        entry: Entry,
    ) -> Result<(), ExpectationFailure> {
        let verb = entry.verb();
        if self.forces_key_events() {
            if clear_first {
                self.clear_focused_field(&format!("AndroidDriver::{verb} (key-events)"), None)
                    .await?;
            }
            return self
                .post_text(text, None, entry)
                .await
                .map_err(|e| driver_error(format!("AndroidDriver::{verb} (key-events): {e}")));
        }
        if matches!(selector, Selector::Focused { .. }) {
            return self.fill_focused(text, clear_first, entry).await;
        }
        let ((nx, ny), rect) = resolve_rect_with_implicit_wait(self, selector, include).await?;
        self.runner()
            .tap_at_norm_coord(nx, ny)
            .await
            .map_err(|e| driver_error(format!("AndroidDriver::{verb}: focus tap failed: {e}")))?;
        if clear_first {
            self.clear_focused_field(&format!("AndroidDriver::{verb}"), Some(rect))
                .await?;
        }
        self.post_text(text, Some(rect), entry)
            .await
            .map_err(|e| driver_error(format!("AndroidDriver::{verb}: input_text failed: {e}")))
    }

    async fn post_text(
        &self,
        text: &str,
        rect: Option<(f64, f64, f64, f64)>,
        entry: Entry,
    ) -> Result<(), RunnerTransportError> {
        let runner = self.runner();
        match (entry, rect) {
            (Entry::Type, None) => runner.input_text(text).await,
            (Entry::Type, Some(r)) => runner.input_text_in(text, r).await,
            (Entry::Set, None) => runner.set_text(text).await,
            (Entry::Set, Some(r)) => runner.set_text_in(text, r).await,
        }
    }

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
        entry: Entry,
    ) -> Result<(), ExpectationFailure> {
        if clear_first {
            self.clear_focused_field(&format!("AndroidDriver::{}", entry.verb()), None)
                .await?;
        }
        self.post_text(text, None, entry)
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

fn driver_error(message: String) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message,
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
