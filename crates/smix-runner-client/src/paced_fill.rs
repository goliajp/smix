//! `POST /fill` with the whole text at a pace, and how long that takes.
//!
//! The host used to post one character per request, 50 ms apart, so that
//! a React Native field's JS thread heard each key before the next: the
//! daemon types at up to 200 a second, and a field typed that fast kept
//! only its first two or three. A request costs a round trip and an
//! existence check, about 0.33 s on a consumer's simulator, so a
//! 112-character sign-in sent 112 requests. The runner now takes the
//! pace and types the whole text in one request.

use std::time::Duration;

use serde::Serialize;

use crate::{HttpRunnerClient, IncludeScope, RunnerKeyboardResult, RunnerTransportError, Selector};

/// Keystrokes a second for a paced fill — maestro types at this rate on
/// iOS, React Native fields among the apps it drives.
pub const FILL_PACE: u32 = 10;

/// One automation stall inside `/fill` before any key goes in: an
/// existence check measured at 17.9 s on CI, rounded up.
const FILL_STALL: Duration = Duration::from_secs(20);

/// Deletes a clear sends at most before the text: the field's own length
/// and four more, which this side cannot know; a long field is assumed.
const CLEAR_KEYS: u64 = 68;

/// How long `/fill` may take to type `text` at `pace` keystrokes a
/// second, clearing first when `clear_first`: the stall, then every key.
pub fn paced_fill_budget(text: &str, pace: u32, clear_first: bool) -> Duration {
    let keys = u64::try_from(text.chars().count()).unwrap_or(u64::MAX)
        + if clear_first { CLEAR_KEYS } else { 0 };
    let per_key_ms = 1000 / u64::from(pace.max(1));
    FILL_STALL.saturating_add(Duration::from_millis(keys.saturating_mul(per_key_ms)))
}

#[derive(Serialize)]
struct Req<'a> {
    selector: &'a Selector,
    text: &'a str,
    #[serde(rename = "clearFirst")]
    clear_first: bool,
    #[serde(rename = "typingFrequency")]
    typing_frequency: u32,
}

impl HttpRunnerClient {
    /// `POST /fill` of the whole `text` at `pace` keystrokes a second.
    /// A runner that predates `typingFrequency` ignores it and types at
    /// full speed.
    pub async fn fill_paced(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
        clear_first: bool,
        pace: u32,
    ) -> Result<RunnerKeyboardResult, RunnerTransportError> {
        let wait = crate::route_limits::wait_for_request(
            "/fill",
            paced_fill_budget(text, pace, clear_first),
            self.auto_activate,
        )?;
        self.json_post_within(
            "/fill",
            &Req {
                selector,
                text,
                clear_first,
                typing_frequency: pace,
            },
            include,
            wait,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_is_the_stall_and_every_key_at_the_pace() {
        assert_eq!(paced_fill_budget("", FILL_PACE, false), FILL_STALL);
        assert_eq!(
            paced_fill_budget(&"x".repeat(14), FILL_PACE, false),
            FILL_STALL + Duration::from_millis(1400)
        );
        // a clear's deletes go in at the same pace
        assert_eq!(
            paced_fill_budget("ab", FILL_PACE, true),
            FILL_STALL + Duration::from_millis((2 + CLEAR_KEYS) * 100)
        );
    }

    #[test]
    fn a_long_text_still_fits_the_runners_handler_limit() {
        let text = "x".repeat(2000);
        crate::route_limits::wait_for_request(
            "/fill",
            paced_fill_budget(&text, FILL_PACE, true),
            false,
        )
        .expect("2000 characters at 10 a second fit");
    }

    #[test]
    fn the_runner_reads_the_pace_by_this_name() {
        let body = serde_json::to_value(Req {
            selector: &Selector::Focused {
                focused: smix_selector::True(true),
            },
            text: "a",
            clear_first: false,
            typing_frequency: FILL_PACE,
        })
        .unwrap();
        // KeyboardRoute.decodeFill reads `typingFrequency`, 1...200
        assert_eq!(body["typingFrequency"], 10);
    }
}
