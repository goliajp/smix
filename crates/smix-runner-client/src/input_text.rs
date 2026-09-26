//! `POST /input-text` and how long it may take.
//!
//! Typing is paid for per character: an Android `input text` measured
//! 40–90 ms a character on an emulator at load 9, and a 120-character
//! fill took 11–15 s against a 15 s request timeout. So the wait is
//! derived from the text, and the runner is told the same budget, so
//! that it stops typing before the host stops listening rather than
//! finishing for nobody.

use std::time::Duration;

use serde::Serialize;

use crate::{HttpRunnerClient, OkEnvelope, REQUEST_TIMEOUT, RunnerTransportError};

/// What each character adds to the budget: about twice the slowest rate
/// measured on an idle-enough emulator, for a machine under load.
const PER_CHARACTER: Duration = Duration::from_millis(250);

/// Kept past the budget for the answer: the runner checks the budget
/// before each `input text` it sends, so one already under way can
/// still be finishing when the budget runs out.
const ANSWER_MARGIN: Duration = Duration::from_secs(15);

/// The budget sent to the runner as `budgetMs` for typing `text`.
///
/// Counted in characters (code points), which is what the runner cuts
/// its chunks by.
pub fn input_text_budget(text: &str) -> Duration {
    let characters = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    REQUEST_TIMEOUT.saturating_add(PER_CHARACTER.saturating_mul(characters))
}

/// How long the host waits for the answer to typing `text`.
pub fn input_text_wait(text: &str) -> Duration {
    input_text_budget(text).saturating_add(ANSWER_MARGIN)
}

/// Every `/input-text` body: the text, the budget, and the optional
/// field target flattened in beside them.
#[derive(Serialize)]
struct Req<'a, F: Serialize> {
    text: &'a str,
    #[serde(rename = "budgetMs")]
    budget_ms: u64,
    #[serde(flatten)]
    field: F,
}

#[derive(Serialize)]
struct Focused {}

#[derive(Serialize)]
struct InRect {
    #[serde(rename = "focusRect")]
    focus_rect: [f64; 4],
}

#[derive(Serialize)]
struct AtPoint {
    #[serde(rename = "focusNx")]
    focus_nx: f64,
    #[serde(rename = "focusNy")]
    focus_ny: f64,
}

impl HttpRunnerClient {
    async fn post_input_text<F: Serialize>(
        &self,
        text: &str,
        field: F,
    ) -> Result<(), RunnerTransportError> {
        let budget_ms = u64::try_from(input_text_budget(text).as_millis()).unwrap_or(u64::MAX);
        let body: OkEnvelope = self
            .json_post_within(
                "/input-text",
                &Req {
                    text,
                    budget_ms,
                    field,
                },
                None,
                input_text_wait(text),
            )
            .await?;
        body.require_ok("/input-text")?;
        Ok(())
    }

    /// `POST /input-text` — type text into currently-focused
    /// input. Caller must tap to focus the field first (AndroidDriver
    /// orchestrates). Android-specific.
    pub async fn input_text(&self, text: &str) -> Result<(), RunnerTransportError> {
        self.post_input_text(text, Focused {}).await
    }

    /// `POST /input-text`, naming the field by the box it lies in.
    ///
    /// The point form ([`Self::input_text_at`]) asks the runner for a
    /// focused field containing that point, which is wrong whenever the
    /// caller named the layout around a field rather than the field:
    /// the wrapper's centre can sit on a label, and requiring the field
    /// to contain it refused every fill in an app built that way. What
    /// identifies the field is lying inside what was named.
    pub async fn input_text_in(
        &self,
        text: &str,
        rect: (f64, f64, f64, f64),
    ) -> Result<(), RunnerTransportError> {
        let field = InRect {
            focus_rect: [rect.0, rect.1, rect.2, rect.3],
        };
        self.post_input_text(text, field).await
    }

    /// `POST /input-text`, naming the field by where it was tapped.
    ///
    /// [`Self::input_text`] types into whatever holds focus, which is
    /// ambiguous straight after a focus tap: focus does not move
    /// synchronously with the tap that moves it, so the runner could
    /// still find the previously focused field and type there. Measured
    /// on emulator-5554 — a fill naming one Compose field cleared and
    /// filled another.
    ///
    /// Passing the tap point makes the request say which field it
    /// means, and the runner waits for focus to reach the field
    /// containing that point rather than for any field to have it.
    pub async fn input_text_at(
        &self,
        text: &str,
        nx: f64,
        ny: f64,
    ) -> Result<(), RunnerTransportError> {
        let field = AtPoint {
            focus_nx: nx,
            focus_ny: ny,
        };
        self.post_input_text(text, field).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_text_gets_the_plain_request_timeout() {
        assert_eq!(input_text_budget(""), REQUEST_TIMEOUT);
    }

    #[test]
    fn the_budget_grows_with_every_character() {
        // 120 characters took 11–15 s on an emulator at load 9; the old
        // flat 15 s cut them off.
        let budget = input_text_budget(&"x".repeat(120));
        assert_eq!(budget, REQUEST_TIMEOUT + PER_CHARACTER * 120);
        assert!(budget >= Duration::from_secs(40), "{budget:?}");
    }

    #[test]
    fn characters_are_counted_not_bytes() {
        // four two-byte characters and one four-byte one
        assert_eq!(input_text_budget("éééé😀"), input_text_budget("abcde"));
    }

    #[test]
    fn the_host_waits_past_the_budget_it_sends() {
        let text = "x".repeat(120);
        assert!(input_text_wait(&text) > input_text_budget(&text));
    }

    #[test]
    fn a_body_carries_the_budget_beside_the_field() {
        let body = serde_json::to_value(Req {
            text: "ab",
            budget_ms: 15_500,
            field: AtPoint {
                focus_nx: 0.5,
                focus_ny: 0.25,
            },
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"text": "ab", "budgetMs": 15_500, "focusNx": 0.5, "focusNy": 0.25})
        );
    }
}
