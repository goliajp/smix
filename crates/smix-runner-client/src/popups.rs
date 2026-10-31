//! `GET /system-popups`, kept to the runner's pace.
//!
//! The runner lets one session read popups at most every 500 ms and answers
//! a read that comes sooner with 429 and how long to wait
//! (`{"error":"rate_limited","retryAfterMs":N}`). That answer is the
//! runner saying when, not a failure: a `tapOn` that looked at popups right
//! after another step did would otherwise be told there was nothing to read.

use std::time::Duration;

use serde::Deserialize;

use crate::{HttpRunnerClient, IncludeScope, RunnerTransportError, SystemPopup};

/// How many paced answers in a row are waited out. The runner's floor is
/// 500 ms per session, so a second 429 means another caller shares the
/// session and is reading at the same time; a third is not waited for.
const PACED_ANSWERS_WAITED: usize = 2;

impl HttpRunnerClient {
    /// `GET /system-popups?include=` — list system popups.
    pub async fn system_popups(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<Vec<SystemPopup>, RunnerTransportError> {
        #[derive(Deserialize)]
        struct Envelope {
            popups: Vec<SystemPopup>,
        }
        let mut waited = 0;
        loop {
            match self.json_get::<Envelope>("/system-popups", include).await {
                Err(RunnerTransportError::NonSuccessStatus { status, body, .. })
                    if waited < PACED_ANSWERS_WAITED
                        && let Some(wait) = paced(status, &body) =>
                {
                    waited += 1;
                    tokio::time::sleep(wait).await;
                }
                done => return done.map(|env| env.popups),
            }
        }
    }
}

/// The wait a 429 from the popup pacer asks for; `None` for any other answer.
fn paced(status: u16, body: &str) -> Option<Duration> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RateLimited {
        error: String,
        retry_after_ms: u64,
    }
    if status != 429 {
        return None;
    }
    let said: RateLimited = serde_json::from_str(body).ok()?;
    (said.error == "rate_limited").then(|| Duration::from_millis(said.retry_after_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    // the runner's own 429 body, sim-smix-02, 2026-11-01
    const SAID: &str = r#"{"ok":false,"error":"rate_limited","retryAfterMs":313}"#;

    #[test]
    fn the_pacer_answer_carries_its_wait() {
        assert_eq!(paced(429, SAID), Some(Duration::from_millis(313)));
    }

    #[test]
    fn any_other_answer_is_not_waited_for() {
        assert_eq!(paced(500, SAID), None);
        assert_eq!(paced(429, r#"{"ok":false,"error":"busy"}"#), None);
        assert_eq!(paced(429, "too many"), None);
    }
}
