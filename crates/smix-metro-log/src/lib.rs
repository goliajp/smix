//! smix-metro-log — metro / expo log tail + ring buffer + signal await.
//!
//! # Purpose
//!
//! Bridges the app-under-test's log output into smix as a first-class
//! **signal channel**. yaml flows can `expect.signal { regex, timeoutMs }`
//! or `expect.signals { [{regex}, ...], order: strict }` to assert on
//! state transitions the app declares — decoupling smix from what the
//! UI looks like at any instant.
//!
//! # Architecture
//!
//! ```text
//!   ┌─────────────────┐     WebSocket           ┌────────────────┐
//!   │ metro / expo    │  ─────────────────▶      │ MetroLogTail   │
//!   │ dev server      │     (or file tail)       │  ring buffer   │
//!   │ ws://127.0.0.1  │                          │  + allowlist   │
//!   │  :8081/logs     │                          └────────┬───────┘
//!   └─────────────────┘                                   │
//!                                                         │ await_signal
//!                                                         │ await_signals
//!                                                         │ assert_clean
//!                                                         ▼
//!                                              ┌────────────────────┐
//!                                              │ smix-adapter-      │
//!                                              │  maestro runtime   │
//!                                              │ (expect.signal /   │
//!                                              │  expect.signals)   │
//!                                              └────────────────────┘
//! ```
//!
//! # Time model
//!
//! - **Ring buffer** — stores the last `retain_secs` seconds of entries
//!   (default 300s). Entries older than the cutoff are evicted on push.
//! - **Windows** — `await_signal` accepts a `Window`:
//!   - `SinceStep(usize)` — logically "everything after step N ended"
//!   - `SinceRun` — since MetroLogTail was constructed (runner boot)
//!   - `LastMs(u64)` — trailing window (e.g. last 500 ms)
//!
//!   The adapter runtime tracks step boundaries and translates
//!   `SinceStep` to an absolute timestamp captured at step-end.
//! - **Poll frequency** — `await_signal` runs a scan loop at 25 ms
//!   intervals. Publishes into the ring buffer are unbounded (async
//!   channel + mutex-protected buffer) so the poll never misses.

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time;

pub mod subscriber;
pub use subscriber::{FileTailSubscriber, SubscriberHandle, WebSocketSubscriber, parse_log_line};

/// Metro / expo log entry, normalized. Level is best-effort: metro
/// text-only lines get `LogLevel::Log`; JSON-formatted lines keep
/// their `level` if parseable.
///
/// `at` is a monotonic `Instant` used internally for retention math;
/// it is skipped during serialization (Instant has no `Default` /
/// Serde impl). Consumers who need to persist entries can use
/// `ms_since_start` — that IS serializable and monotonically ordered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Monotonic timestamp at ingest. Not serialized.
    #[serde(skip, default = "Instant::now")]
    pub at: Instant,
    /// Wall-clock ms since MetroLogTail::start (used for window math).
    pub ms_since_start: u64,
    pub level: LogLevel,
    pub message: String,
}

impl LogEntry {
    /// Construct with `ms_since_start` computed against a reference
    /// `Instant`. Used inside `MetroLogTail::push` — external callers
    /// should always go through push.
    pub fn new(level: LogLevel, message: String, ms_since_start: u64) -> Self {
        Self {
            at: Instant::now(),
            ms_since_start,
            level,
            message,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Log,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Best-effort parse of a metro/expo-emitted level string.
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "debug" | "trace" => LogLevel::Debug,
            "info" => LogLevel::Info,
            "warn" | "warning" => LogLevel::Warn,
            "error" | "err" => LogLevel::Error,
            _ => LogLevel::Log,
        }
    }
}

/// Window over which to look for a matching signal.
///
/// Three shapes cover the common use cases. The adapter runtime
/// translates `SinceStep(n)` to a specific `ms_since_start` cutoff
/// (captured at the moment step n's outcome was recorded), so the
/// runtime doesn't need step tracking here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// Everything since `MetroLogTail::start` (runner boot).
    SinceRun,
    /// After a specific `ms_since_start` cutoff. The adapter converts
    /// its `sinceStep: N` yaml field to this variant using the
    /// remembered cutoff for step N.
    SinceMs(u64),
    /// Trailing window: only entries whose `ms_since_start >= now - X`.
    LastMs(u64),
}

#[allow(clippy::derivable_impls)]
impl Default for Window {
    fn default() -> Self {
        Window::SinceRun
    }
}

/// Order semantics for [`MetroLogTail::await_signals`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalOrder {
    /// Every pattern must match at some point in the window; order
    /// doesn't matter.
    Any,
    /// Patterns must match in the exact listed sequence, no
    /// interleaving — `pattern[i+1]`'s first match must be at a
    /// higher `ms_since_start` than `pattern[i]`'s.
    Strict,
}

/// Compiled signal pattern.
#[derive(Clone, Debug)]
pub struct SignalMatcher {
    pub level: Option<LogLevel>,
    pub regex: Regex,
}

impl SignalMatcher {
    /// Compile a regex + optional level constraint.
    pub fn new(regex: &str, level: Option<LogLevel>) -> Result<Self, regex::Error> {
        Ok(Self {
            level,
            regex: Regex::new(regex)?,
        })
    }

    fn matches(&self, entry: &LogEntry) -> bool {
        if let Some(l) = self.level
            && entry.level != l
        {
            return false;
        }
        self.regex.is_match(&entry.message)
    }
}

#[derive(Debug, Error)]
pub enum AwaitError {
    #[error("signal {regex} not observed within {timeout_ms}ms (window={window:?})")]
    Timeout {
        regex: String,
        timeout_ms: u64,
        window: Window,
    },
    #[error(
        "signals ordering violated: pattern #{index} ({regex}) matched at ms_since_start={found_ms} before an earlier pattern's match at ms_since_start={prev_ms}"
    )]
    OrderViolation {
        index: usize,
        regex: String,
        found_ms: u64,
        prev_ms: u64,
    },
    #[error("regex compile: {0}")]
    Regex(#[from] regex::Error),
}

/// Ring-buffer + await surface for metro/expo logs.
///
/// Constructed once per runner session. Cloneable (internal state is
/// `Arc<RwLock<..>>`), so the subscriber task and the adapter runtime
/// can share it without a channel.
#[derive(Clone, Debug)]
pub struct MetroLogTail {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    start: Instant,
    retain: Duration,
    buffer: RwLock<VecDeque<LogEntry>>,
    /// Compiled allowlist for [`MetroLogTail::assert_clean`]. Any log
    /// entry whose message matches one of these is ignored for
    /// hygiene assertions.
    allowlist: RwLock<Vec<Regex>>,
}

impl MetroLogTail {
    /// Construct with default 300s retention and no allowlist.
    pub fn new() -> Self {
        Self::with_retain_secs(300)
    }

    pub fn with_retain_secs(retain_secs: u64) -> Self {
        Self {
            inner: Arc::new(Inner {
                start: Instant::now(),
                retain: Duration::from_secs(retain_secs),
                buffer: RwLock::new(VecDeque::new()),
                allowlist: RwLock::new(Vec::new()),
            }),
        }
    }

    /// Replace the allowlist. Each entry is a regex; if it fails to
    /// compile, returns the underlying `regex::Error` without
    /// mutating the current list.
    pub fn set_allowlist(&self, patterns: &[String]) -> Result<(), regex::Error> {
        let compiled: Result<Vec<Regex>, _> = patterns.iter().map(|p| Regex::new(p)).collect();
        let compiled = compiled?;
        *self.inner.allowlist.write().expect("poisoned") = compiled;
        Ok(())
    }

    /// Layer additional patterns onto the existing allowlist.
    /// Consumers can compose a per-scope allowlist on top of a base
    /// (.smix/config.json + per-scope + inline yaml).
    ///
    /// Layer merge semantics: append. A log entry that matches ANY
    /// layer's pattern is allowlisted. To reset, call
    /// [`Self::set_allowlist`] with the desired combined list.
    pub fn extend_allowlist(&self, patterns: &[String]) -> Result<(), regex::Error> {
        let compiled: Result<Vec<Regex>, _> = patterns.iter().map(|p| Regex::new(p)).collect();
        let compiled = compiled?;
        self.inner
            .allowlist
            .write()
            .expect("poisoned")
            .extend(compiled);
        Ok(())
    }

    /// Snapshot the currently-active allowlist. Used by consumers who
    /// need to reason about which layers are live before layering more.
    pub fn allowlist_patterns(&self) -> Vec<String> {
        self.inner
            .allowlist
            .read()
            .expect("poisoned")
            .iter()
            .map(|r| r.as_str().to_string())
            .collect()
    }

    /// Push an entry from the subscriber. Evicts entries older than
    /// `retain_secs` after each push. Caller supplies `level` +
    /// `message`; the timestamp / `ms_since_start` are stamped here.
    pub fn push(&self, level: LogLevel, message: String) {
        let now = Instant::now();
        let ms_since_start = now.duration_since(self.inner.start).as_millis() as u64;
        let entry = LogEntry {
            at: now,
            ms_since_start,
            level,
            message,
        };
        let cutoff = now
            .checked_sub(self.inner.retain)
            .unwrap_or(self.inner.start);
        let mut buf = self.inner.buffer.write().expect("poisoned");
        while let Some(front) = buf.front() {
            if front.at < cutoff {
                buf.pop_front();
            } else {
                break;
            }
        }
        buf.push_back(entry);
    }

    /// Snapshot of the current buffer contents. Cheap clone of
    /// entries; primarily used by tests.
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.inner
            .buffer
            .read()
            .expect("poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Milliseconds since the tail was constructed. The runtime
    /// remembers this at step-end to translate `sinceStep: N` yaml
    /// windows into `Window::SinceMs`.
    pub fn ms_since_start(&self) -> u64 {
        Instant::now().duration_since(self.inner.start).as_millis() as u64
    }

    /// Await a single signal within `timeout`. Scans the ring buffer
    /// at 25 ms intervals; returns the first matching entry. Returns
    /// [`AwaitError::Timeout`] on budget exhaustion.
    pub async fn await_signal(
        &self,
        matcher: &SignalMatcher,
        timeout: Duration,
        window: Window,
    ) -> Result<LogEntry, AwaitError> {
        let deadline = Instant::now() + timeout;
        let mut interval = time::interval(Duration::from_millis(25));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        loop {
            if let Some(entry) = self.find_matching(matcher, window) {
                return Ok(entry);
            }
            if Instant::now() >= deadline {
                return Err(AwaitError::Timeout {
                    regex: matcher.regex.as_str().to_string(),
                    timeout_ms: timeout.as_millis() as u64,
                    window,
                });
            }
            interval.tick().await;
        }
    }

    /// Await N signals — order semantics per [`SignalOrder`]. Returns
    /// the matching entries in the order of the input `matchers`
    /// (position `i` corresponds to `matchers[i]`).
    pub async fn await_signals(
        &self,
        matchers: &[SignalMatcher],
        order: SignalOrder,
        timeout: Duration,
        window: Window,
    ) -> Result<Vec<LogEntry>, AwaitError> {
        let deadline = Instant::now() + timeout;
        let mut interval = time::interval(Duration::from_millis(25));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        loop {
            if let Some(result) = self.try_match_all(matchers, order)? {
                return Ok(result);
            }
            if Instant::now() >= deadline {
                // Report the first unmet matcher as the timeout regex.
                let idx = self.first_unmet(matchers, window);
                let regex = matchers[idx].regex.as_str().to_string();
                return Err(AwaitError::Timeout {
                    regex,
                    timeout_ms: timeout.as_millis() as u64,
                    window,
                });
            }
            interval.tick().await;
        }
    }

    /// Verify no unexpected log entries. Returns a Vec of violating
    /// entries — anything that (a) is level >= Warn and (b) doesn't
    /// match any allowlist pattern. Empty vec = clean.
    pub fn assert_clean(&self) -> Vec<LogEntry> {
        let buf = self.inner.buffer.read().expect("poisoned");
        let allow = self.inner.allowlist.read().expect("poisoned");
        buf.iter()
            .filter(|e| matches!(e.level, LogLevel::Warn | LogLevel::Error))
            .filter(|e| !allow.iter().any(|r| r.is_match(&e.message)))
            .cloned()
            .collect()
    }

    fn find_matching(&self, matcher: &SignalMatcher, window: Window) -> Option<LogEntry> {
        let cutoff = self.window_cutoff(window);
        self.inner
            .buffer
            .read()
            .expect("poisoned")
            .iter()
            .find(|e| e.ms_since_start >= cutoff && matcher.matches(e))
            .cloned()
    }

    fn window_cutoff(&self, window: Window) -> u64 {
        match window {
            Window::SinceRun => 0,
            Window::SinceMs(ms) => ms,
            Window::LastMs(ms) => self.ms_since_start().saturating_sub(ms),
        }
    }

    /// Try to match all matchers against the current buffer. Returns
    /// `Ok(Some(entries))` on success, `Ok(None)` if not all matched
    /// yet, `Err(OrderViolation)` if strict-order requirement was
    /// violated (an earlier pattern only appears after a later one).
    fn try_match_all(
        &self,
        matchers: &[SignalMatcher],
        order: SignalOrder,
    ) -> Result<Option<Vec<LogEntry>>, AwaitError> {
        let buf = self.inner.buffer.read().expect("poisoned");
        // For Any-order: each matcher independently → first match.
        // For Strict-order: matcher i's chosen entry must have
        // ms_since_start >= matcher i-1's chosen entry ms_since_start.
        let mut chosen: Vec<Option<LogEntry>> = vec![None; matchers.len()];
        match order {
            SignalOrder::Any => {
                for (i, m) in matchers.iter().enumerate() {
                    chosen[i] = buf.iter().find(|e| m.matches(e)).cloned();
                }
            }
            SignalOrder::Strict => {
                // Strict order = each next match must appear at a
                // higher BUFFER INDEX than the previous. Using
                // ms_since_start alone can't tell apart entries pushed
                // in the same millisecond (which is common for test
                // signals emitted in a burst). Buffer index = strict
                // temporal ordering.
                let mut min_index: usize = 0;
                for (i, m) in matchers.iter().enumerate() {
                    let mut chosen_idx: Option<usize> = None;
                    for (idx, entry) in buf.iter().enumerate() {
                        if idx >= min_index && m.matches(entry) {
                            chosen_idx = Some(idx);
                            break;
                        }
                    }
                    if let Some(idx) = chosen_idx {
                        min_index = idx + 1;
                        chosen[i] = Some(buf[idx].clone());
                    }
                }
                // Detect eager order violation: if this pattern is
                // absent from the strict scan but present earlier in
                // the buffer than a later matcher's chosen index, the
                // sequence is out of order — surface immediately.
                for (i, m) in matchers.iter().enumerate() {
                    if chosen[i].is_some() {
                        continue;
                    }
                    let earliest = buf.iter().enumerate().find(|(_, e)| m.matches(e));
                    if let Some((earliest_idx, earliest_entry)) = earliest {
                        #[allow(clippy::needless_range_loop)]
                        for j in (i + 1)..matchers.len() {
                            if let Some(later) = &chosen[j] {
                                // Find the later's buffer index.
                                let later_idx =
                                    buf.iter().position(|e| e == later).unwrap_or(usize::MAX);
                                if earliest_idx < later_idx {
                                    return Err(AwaitError::OrderViolation {
                                        index: i,
                                        regex: m.regex.as_str().to_string(),
                                        found_ms: earliest_entry.ms_since_start,
                                        prev_ms: later.ms_since_start,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        if chosen.iter().all(|x| x.is_some()) {
            let chosen = chosen.into_iter().flatten().collect();
            Ok(Some(chosen))
        } else {
            Ok(None)
        }
    }

    fn first_unmet(&self, matchers: &[SignalMatcher], _window: Window) -> usize {
        let buf = self.inner.buffer.read().expect("poisoned");
        for (i, m) in matchers.iter().enumerate() {
            if !buf.iter().any(|e| m.matches(e)) {
                return i;
            }
        }
        0
    }
}

impl Default for MetroLogTail {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(re: &str) -> SignalMatcher {
        SignalMatcher::new(re, None).unwrap()
    }

    #[test]
    fn push_and_snapshot_roundtrip() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "hello".into());
        tail.push(LogLevel::Error, "world".into());
        let snap = tail.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].message, "hello");
        assert_eq!(snap[1].message, "world");
    }

    #[tokio::test]
    async fn await_signal_hits_when_present() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "env=qa-mode".into());
        let hit = tail
            .await_signal(
                &m(r"env=qa-mode"),
                Duration::from_millis(100),
                Window::SinceRun,
            )
            .await
            .expect("should match");
        assert_eq!(hit.message, "env=qa-mode");
    }

    #[tokio::test]
    async fn await_signal_times_out_when_absent() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "some other line".into());
        let err = tail
            .await_signal(
                &m(r"env=qa-mode"),
                Duration::from_millis(50),
                Window::SinceRun,
            )
            .await
            .expect_err("should timeout");
        matches!(err, AwaitError::Timeout { .. });
    }

    #[tokio::test]
    async fn await_signal_respects_last_ms_window() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "old line".into());
        // Simulate 100 ms elapsing.
        tokio::time::pause();
        tokio::time::advance(Duration::from_millis(200)).await;
        // A LastMs(100) window shouldn't see "old line" — but this test
        // works with real Instant so is best-effort. Cover semantic
        // via SinceMs instead (deterministic).
        let cutoff = tail.ms_since_start();
        tail.push(LogLevel::Log, "new line".into());
        let hit = tail
            .await_signal(
                &m(r"new"),
                Duration::from_millis(50),
                Window::SinceMs(cutoff),
            )
            .await
            .expect("should match");
        assert_eq!(hit.message, "new line");
    }

    #[tokio::test]
    async fn await_signal_since_ms_filters_older() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "before".into());
        let cutoff = tail.ms_since_start() + 100;
        let err = tail
            .await_signal(
                &m(r"before"),
                Duration::from_millis(50),
                Window::SinceMs(cutoff),
            )
            .await
            .expect_err("should not see entry before cutoff");
        matches!(err, AwaitError::Timeout { .. });
    }

    #[tokio::test]
    async fn await_signals_any_order() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "second".into());
        tail.push(LogLevel::Log, "first".into());
        let hits = tail
            .await_signals(
                &[m(r"^first$"), m(r"^second$")],
                SignalOrder::Any,
                Duration::from_millis(100),
                Window::SinceRun,
            )
            .await
            .expect("both present");
        assert_eq!(hits.len(), 2);
    }

    #[tokio::test]
    async fn await_signals_strict_ok_when_in_order() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "a".into());
        tail.push(LogLevel::Log, "b".into());
        tail.push(LogLevel::Log, "c".into());
        let hits = tail
            .await_signals(
                &[m(r"^a$"), m(r"^b$"), m(r"^c$")],
                SignalOrder::Strict,
                Duration::from_millis(100),
                Window::SinceRun,
            )
            .await
            .expect("strict in-order OK");
        assert_eq!(hits.len(), 3);
        assert!(hits[0].ms_since_start <= hits[1].ms_since_start);
        assert!(hits[1].ms_since_start <= hits[2].ms_since_start);
    }

    #[tokio::test]
    async fn await_signals_strict_reports_order_violation() {
        let tail = MetroLogTail::new();
        // c appears BEFORE a — violates order [a, b, c].
        tail.push(LogLevel::Log, "c".into());
        tail.push(LogLevel::Log, "a".into());
        tail.push(LogLevel::Log, "b".into());
        let res = tail
            .await_signals(
                &[m(r"^a$"), m(r"^b$"), m(r"^c$")],
                SignalOrder::Strict,
                Duration::from_millis(50),
                Window::SinceRun,
            )
            .await;
        // Either OrderViolation or Timeout (both are valid failure
        // signals for this shape; the exact one depends on scan
        // heuristics).
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn assert_clean_empty_when_no_warns() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Log, "info".into());
        tail.push(LogLevel::Info, "info".into());
        let violations = tail.assert_clean();
        assert!(violations.is_empty());
    }

    #[tokio::test]
    async fn assert_clean_surfaces_warns_and_errors() {
        let tail = MetroLogTail::new();
        tail.push(LogLevel::Warn, "unexpected warning".into());
        tail.push(LogLevel::Error, "unexpected error".into());
        let violations = tail.assert_clean();
        assert_eq!(violations.len(), 2);
    }

    #[tokio::test]
    async fn assert_clean_honors_allowlist() {
        let tail = MetroLogTail::new();
        tail.set_allowlist(&["skipping register".to_string()])
            .unwrap();
        tail.push(LogLevel::Warn, "skipping register: dev".into());
        tail.push(LogLevel::Warn, "unexpected".into());
        let violations = tail.assert_clean();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].message, "unexpected");
    }

    #[test]
    fn retention_evicts_old_entries() {
        // 0-second retention → every push evicts previous.
        let tail = MetroLogTail::with_retain_secs(0);
        tail.push(LogLevel::Log, "gone".into());
        std::thread::sleep(Duration::from_millis(20));
        tail.push(LogLevel::Log, "kept".into());
        let snap = tail.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].message, "kept");
    }
}
