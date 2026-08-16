#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-error — ExpectationFailure + FailureCode + AI-readable
//! `to_prompt()` + `build_suggestions` (stone, cold path).
//!
//! `build_suggestions` behavior: threshold > 0.5, top 3, sort by score
//! descending → field (name > id > text) → DFS index ascending.
//!
//! # Why a separate stone
//!
//! SDK / driver / runner-client all throw / catch `ExpectationFailure`. As
//! the canonical failure type it must live in a leaf crate that everyone
//! can depend on. Failure messages MUST be AI-readable — `to_prompt()`
//! is the canonical render.

#![doc(html_root_url = "https://docs.smix.dev/smix-error")]

use serde::{Deserialize, Serialize};
use smix_screen::ElementSummary;
use smix_selector::{Selector, describe_selector};
use std::fmt;

/// All failure codes smix surfaces back to the SDK / MCP / CLI
/// (SCREAMING_SNAKE_CASE wire).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
// New codes keep arriving — two in two releases — and on an exhaustive
// enum each one is a major, because a consumer matching every arm stops
// compiling. That is the wrong price for smix naming a failure more
// precisely, and it was about to be paid for the second time.
//
// The cost lands once, here, in a major that was already owed: every
// downstream `match` needs a catch-all arm from now on. What it buys is
// that the next code is additive.
//
// The guard that made a new variant fail to compile lives inside this
// crate now (see `the_vocabulary_is_pinned` below). From out here
// `non_exhaustive` would have made it accept anything, so it moved
// rather than being weakened.
#[non_exhaustive]
pub enum FailureCode {
    /// Selector matched zero elements in the visible tree.
    ElementNotFound,
    /// Element matched but failed the visibility filter.
    NotVisible,
    /// Element matched but `enabled = false`.
    NotEnabled,
    /// Selector matched multiple elements (when uniqueness was required).
    Ambiguous,
    /// Operation exceeded the implicit-wait budget.
    Timeout,
    /// `expect` assertion (e.g. `toHaveText`) did not hold.
    AssertionFailed,
    /// Target app exited or never launched.
    AppNotRunning,
    /// Simulator device is not booted.
    SimulatorNotBooted,
    /// The touch was synthesised, and it did not land inside the
    /// element the selector matched.
    ///
    /// Distinct from `ElementNotFound` because the two send a reader
    /// somewhere different: not-found means fix the selector, missed
    /// means the element was there and the touch went elsewhere — a
    /// stale frame, or something moved between the tree fetch and the
    /// tap.
    TapMissed,
    /// The screen is described in one coordinate space and the touch
    /// would be delivered in another, so no aim can land where the tree
    /// says the element is.
    ///
    /// Distinct from `TapMissed`, and the distinction is the whole
    /// point: a miss says the element was there and the touch went
    /// elsewhere, which invites another attempt with a better point.
    /// There is no better point here — whatever is passed gets
    /// recomputed against the app's frame and then read against the
    /// device's. Retrying is the trap, and the consumer who found this
    /// spent an afternoon in it.
    CoordinateSpaceMismatch,
    /// Catch-all for runner / driver / IO failures.
    DriverError,
}

/// Structured failure thrown by SDK matchers and driver calls. Shape
/// is AI-feed-back-ready.
///
/// `selector` is the typed `Selector` rather than the wire
/// `serde_json::Value` — callers raising failures from the driver / SDK
/// side already have the typed selector, and `to_prompt` invokes
/// `describe_selector` for stable rendering.
///
/// Serializes as `{ ok: false, code, message, selector, suggestions,
/// visibleElements, hint, screenshot, deviceLog }`; the `ok = false`
/// discriminant identifies the failure branch of a `Result` wire.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectationFailure {
    /// Always `false` for failures. Round-trip discriminator for
    /// `Result<T, ExpectationFailure>` wire.
    pub ok: False,
    /// Failure code discriminator.
    pub code: FailureCode,
    /// Human-readable summary (and AI-readable when fed into `to_prompt`).
    pub message: String,
    /// Originating selector, when one is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<Selector>,
    /// Ranked near-miss suggestions (edit-distance to label / id / etc.).
    #[serde(default)]
    pub suggestions: Vec<String>,
    /// Snapshot of visible+enabled elements at failure time.
    #[serde(default)]
    pub visible_elements: Vec<ElementSummary>,
    /// Optional one-line hint (e.g. "try `app.wait_for(...)` first").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The smix that produced this failure.
    ///
    /// Not decoration. A consumer wrote up two defects against smix
    /// behaviour that had been fixed hours earlier, quoting an error
    /// message the current build no longer emits — they had no way to
    /// tell whether the smix in front of them contained the fix for
    /// the thing they had just hit. A failure that names its version
    /// makes that answerable from the failure itself.
    ///
    /// Set by [`ExpectationFailure::new`] from the crate version, so no
    /// call site can forget it.
    #[serde(default)]
    pub smix_version: String,
    /// base64-encoded PNG; omitted from default rendering to keep logs lean.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    /// Last-N-lines of captured device log, folded into AI-readable
    /// output for failure-window system context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_log: Vec<String>,
}

/// Newtype around `false` — used as the [`ExpectationFailure::ok`]
/// discriminator. Serializes/deserializes as the JSON literal `false`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct False(pub bool);

/// Builder/Init form for ergonomic construction.
#[derive(Default)]
pub struct FailureInit {
    /// Optional failure code (defaults to `DriverError` if `None`).
    pub code: Option<FailureCode>,
    /// Human-readable / AI-readable message.
    pub message: String,
    /// Originating selector, when one is available.
    pub selector: Option<Selector>,
    /// Pre-built ranked near-miss suggestions.
    pub suggestions: Vec<String>,
    /// Captured visible+enabled elements at failure time.
    pub visible_elements: Vec<ElementSummary>,
    /// Optional one-line hint string.
    pub hint: Option<String>,
    /// Optional base64 PNG screenshot at failure time.
    pub screenshot: Option<String>,
    /// Optional captured device log tail.
    pub device_log: Vec<String>,
}

impl ExpectationFailure {
    /// Construct from an init struct. `code` defaults to `DriverError`
    /// if `init.code` was None (defensive; SDK callers should always
    /// set it).
    pub fn new(init: FailureInit) -> Self {
        ExpectationFailure {
            ok: False(false),
            code: init.code.unwrap_or(FailureCode::DriverError),
            message: init.message,
            selector: init.selector,
            suggestions: init.suggestions,
            visible_elements: init.visible_elements,
            hint: init.hint,
            smix_version: env!("CARGO_PKG_VERSION").to_string(),
            screenshot: init.screenshot,
            device_log: init.device_log,
        }
    }

    /// AI-facing rendering. Designed so the output can be pasted back
    /// as a user message into a coding agent.
    #[must_use]
    pub fn to_prompt(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "FAIL [{}]: {}",
            format_code(self.code),
            self.message
        ));
        // The version, on every failure, because the reader's next
        // question after "what went wrong" is often "is my smix old".
        // A consumer once wrote up two defects that had been fixed
        // hours earlier, quoting a message this build no longer emits.
        if !self.smix_version.is_empty() {
            lines.push(format!("  smix: {}", self.smix_version));
        }
        if let Some(sel) = &self.selector {
            lines.push(format!("  selector: {}", describe_selector(sel)));
        }
        if !self.suggestions.is_empty() {
            lines.push("  suggestions:".into());
            for s in &self.suggestions {
                lines.push(format!("    - {}", s));
            }
        }
        if !self.visible_elements.is_empty() {
            let n = self.visible_elements.len().min(10);
            lines.push(format!("  visible elements (top {}):", n));
            for el in self.visible_elements.iter().take(10) {
                lines.push(format!("    - {}", render_element(el)));
            }
        }
        if let Some(h) = &self.hint {
            lines.push(format!("  hint: {}", h));
        }
        if !self.device_log.is_empty() {
            // LOG_PROMPT_CAP — defensive ceiling for AI prompt size.
            const LOG_PROMPT_CAP: usize = 200;
            let n = self.device_log.len();
            lines.push(format!("  device log (last {} lines):", n));
            let start = n.saturating_sub(LOG_PROMPT_CAP);
            for dl in &self.device_log[start..] {
                lines.push(format!("    - {}", dl));
            }
        }
        lines.join("\n")
    }
}

impl fmt::Display for ExpectationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_prompt())
    }
}

impl std::error::Error for ExpectationFailure {}

fn format_code(c: FailureCode) -> &'static str {
    match c {
        FailureCode::ElementNotFound => "ELEMENT_NOT_FOUND",
        FailureCode::NotVisible => "NOT_VISIBLE",
        FailureCode::NotEnabled => "NOT_ENABLED",
        FailureCode::Ambiguous => "AMBIGUOUS",
        FailureCode::Timeout => "TIMEOUT",
        FailureCode::AssertionFailed => "ASSERTION_FAILED",
        FailureCode::AppNotRunning => "APP_NOT_RUNNING",
        FailureCode::SimulatorNotBooted => "SIMULATOR_NOT_BOOTED",
        FailureCode::TapMissed => "TAP_MISSED",
        FailureCode::CoordinateSpaceMismatch => "COORDINATE_SPACE_MISMATCH",
        FailureCode::DriverError => "DRIVER_ERROR",
    }
}

fn render_element(el: &ElementSummary) -> String {
    let role_str = el.role.map(|r| r.as_str()).unwrap_or("unknown");
    let mut bits: Vec<String> = vec![role_str.to_string()];
    if let Some(n) = &el.name {
        bits.push(format!("name={:?}", n));
    }
    if let Some(i) = &el.id {
        bits.push(format!("id=\"{}\"", i));
    }
    if let Some(t) = &el.text
        && Some(t) != el.name.as_ref()
    {
        bits.push(format!("text={:?}", t));
    }
    if !el.enabled {
        bits.push("disabled".into());
    }
    bits.join(" ")
}

// -------------------- buildSuggestions ----------------------------------

const SUGGESTION_THRESHOLD: f64 = 0.5;
const SUGGESTION_TOP_N: usize = 3;

/// Generate "Did you mean ...?" suggestions from the current visible
/// element list. Contract: threshold > 0.5, top 3, score desc →
/// field (name > text) → DFS index asc.
///
/// `target = None` → empty vec.
#[must_use]
pub fn build_suggestions(target: Option<&str>, visible: &[ElementSummary]) -> Vec<String> {
    let Some(target) = target else {
        return Vec::new();
    };
    let lower_target = target.to_lowercase();
    let mut candidates: Vec<(f64, &'static str, String, usize)> = Vec::new();
    for (i, el) in visible.iter().enumerate() {
        let mut best: Option<(f64, &'static str, String)> = None;
        if let Some(name) = &el.name
            && !name.is_empty()
        {
            let s = similarity(&name.to_lowercase(), &lower_target);
            if s > SUGGESTION_THRESHOLD {
                best = Some((s, "name", name.clone()));
            }
        }
        if let Some(id) = &el.id
            && !id.is_empty()
        {
            let s = similarity(&id.to_lowercase(), &lower_target);
            if s > SUGGESTION_THRESHOLD && best.as_ref().map(|(bs, _, _)| s > *bs).unwrap_or(true) {
                best = Some((s, "id", id.clone()));
            }
        }
        if let Some(text) = &el.text
            && !text.is_empty()
        {
            let s = similarity(&text.to_lowercase(), &lower_target);
            if s > SUGGESTION_THRESHOLD && best.as_ref().map(|(bs, _, _)| s > *bs).unwrap_or(true) {
                best = Some((s, "text", text.clone()));
            }
        }
        if let Some((score, field, value)) = best {
            candidates.push((score, field, value, i));
        }
    }
    // Sort: score desc → field name>text → index asc.
    let field_rank = |f: &str| match f {
        "name" => 0,
        "id" => 1,
        _ => 2, // text
    };
    candidates.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| field_rank(a.1).cmp(&field_rank(b.1)))
            .then_with(|| a.3.cmp(&b.3))
    });
    candidates
        .into_iter()
        .take(SUGGESTION_TOP_N)
        .map(|(score, field, value, _)| {
            format!(
                "Did you mean {:?}? (similarity {:.2}, field {})",
                value, score, field
            )
        })
        .collect()
}

/// Normalized `[0, 1]` string similarity. 1 = identical,
/// 0 = completely different.
#[must_use]
pub fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let (longer, shorter) = if a.chars().count() >= b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };
    let llen = longer.chars().count();
    if llen == 0 {
        return 1.0;
    }
    let dist = edit_distance(longer, shorter);
    (llen as f64 - dist as f64) / llen as f64
}

/// Levenshtein edit distance (Wagner-Fischer).
#[must_use]
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    let mut dp: Vec<usize> = (0..=b_len).collect();
    for i in 1..=a_len {
        let mut prev = dp[0];
        dp[0] = i;
        for j in 1..=b_len {
            let tmp = dp[j];
            dp[j] = if a_chars[i - 1] == b_chars[j - 1] {
                prev
            } else {
                1 + dp[j].min(dp[j - 1]).min(prev)
            };
            prev = tmp;
        }
    }
    dp[b_len]
}

#[cfg(test)]
mod vocabulary {
    use super::*;

    /// A new `FailureCode` must not compile until it has been thought
    /// about everywhere.
    ///
    /// This match is exhaustive on purpose and lives inside the crate on
    /// purpose: `#[non_exhaustive]` makes an outside `match` accept
    /// anything, so the guard that used to sit in
    /// `tests/sdk_failure_code_parity.rs` would have started passing for
    /// every future variant. Adding a code should break this line, then
    /// the wire-string arm below it, then the three SDK declarations the
    /// parity test reads, then the errors guide.
    #[test]
    fn the_vocabulary_is_pinned() {
        let every = [
            FailureCode::ElementNotFound,
            FailureCode::NotVisible,
            FailureCode::NotEnabled,
            FailureCode::Ambiguous,
            FailureCode::Timeout,
            FailureCode::AssertionFailed,
            FailureCode::AppNotRunning,
            FailureCode::SimulatorNotBooted,
            FailureCode::TapMissed,
            FailureCode::CoordinateSpaceMismatch,
            FailureCode::DriverError,
        ];
        for code in every {
            match code {
                FailureCode::ElementNotFound
                | FailureCode::NotVisible
                | FailureCode::NotEnabled
                | FailureCode::Ambiguous
                | FailureCode::Timeout
                | FailureCode::AssertionFailed
                | FailureCode::AppNotRunning
                | FailureCode::SimulatorNotBooted
                | FailureCode::TapMissed
                | FailureCode::CoordinateSpaceMismatch
                | FailureCode::DriverError => {}
            }
            // Every code renders as a wire string, and a code whose
            // string nobody wrote would reach a consumer as whatever
            // the formatter defaults to.
            assert!(!format_code(code).is_empty(), "{code:?} has no wire string");
        }
        assert_eq!(
            every.len(),
            11,
            "counted off the list above, not remembered"
        );
    }
}
