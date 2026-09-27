//! `POST /hide-keyboard` and how long it may take.
//!
//! Dismissing runs up to four strategies, each followed by up to a second
//! of looking. On a slow simulator one XCUITest query costs about a second,
//! so the whole chain can pass 15 s — and it did on CI, where the host gave
//! up at 15 s while the runner was still working, and reported a step that
//! "may have acted" instead of the runner's own answer. The runner is now
//! told a budget and stops starting strategies once it is spent; the host
//! waits for that budget and the same margin typing uses.

use std::time::Duration;

use serde::Serialize;

use crate::input_text::ANSWER_MARGIN;
use crate::{HttpRunnerClient, OkEnvelope, RunnerTransportError};

/// The runner's budget for dismissing: the slowest chain above, rounded up.
pub const HIDE_KEYBOARD_BUDGET: Duration = Duration::from_secs(20);

/// How long the host waits for the answer.
pub fn hide_keyboard_wait() -> Duration {
    HIDE_KEYBOARD_BUDGET.saturating_add(ANSWER_MARGIN)
}

#[derive(Serialize)]
struct Req {
    #[serde(rename = "budgetMs")]
    budget_ms: u64,
}

impl HttpRunnerClient {
    /// `POST /hide-keyboard`.
    pub async fn hide_keyboard(&self) -> Result<(), RunnerTransportError> {
        let budget_ms = u64::try_from(HIDE_KEYBOARD_BUDGET.as_millis()).unwrap_or(u64::MAX);
        let body: OkEnvelope = self
            .json_post_within(
                "/hide-keyboard",
                &Req { budget_ms },
                None,
                hide_keyboard_wait(),
            )
            .await?;
        body.require_ok("/hide-keyboard")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::REQUEST_TIMEOUT;

    #[test]
    fn the_host_waits_past_the_budget_it_hands_the_runner() {
        assert!(hide_keyboard_wait() > HIDE_KEYBOARD_BUDGET);
        // the reason this module exists: the old wait was the plain one
        assert!(hide_keyboard_wait() > REQUEST_TIMEOUT);
        assert!(HIDE_KEYBOARD_BUDGET > REQUEST_TIMEOUT);
    }

    #[test]
    fn the_budget_goes_on_the_wire_in_milliseconds() {
        let body = serde_json::to_string(&Req { budget_ms: 20_000 }).expect("json");
        assert_eq!(body, r#"{"budgetMs":20000}"#);
    }
}
