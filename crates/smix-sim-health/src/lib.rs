// smix-sim-health — sim-side liveness sense layer.
//
// The runner client, the simctl client, and the maestro adapter each
// observe one aspect of sim health (respectively: /health response
// age, screenshot wall time, per-flow XCTest state). They feed the
// observations into a `SimHealthMonitor`; the monitor collapses them
// into a `SimHealthState` and broadcasts transitions. Callers that
// want to react (throttle, cycle, bail out) subscribe.
//
// The sense layer does not act. Actions live in the crates that own
// the affected surface — this crate is deliberately business-unaware
// (it does not know what "iOS", "simulator", or "insight" mean).
//
// State machine
// -------------
// - Healthy   — every observation is inside its normal envelope.
// - Degraded  — at least one signal is bad but not fatal (screenshot
//               p95 above the slow threshold, or /health age above
//               the stale threshold but below the dead threshold).
// - Dead      — the runner is unreachable, or a watched process
//               (SimRenderServer / xcodebuild test-host) is gone.
//
// Transitions publish `SimHealthEvent { previous, current, reason }`
// on a broadcast channel. Only real transitions are published;
// repeated identical observations do not spam subscribers.

#![doc = include_str!("../README.md")]

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use tokio::sync::broadcast;

/// Coarse sim health classification.
///
/// The three states form a strictly-ordered severity ladder —
/// `Healthy < Degraded < Dead`. Callers routinely branch on the
/// severity rather than the specific reason (throttle on `Degraded`,
/// bail on `Dead`), so `Ord` is derived to make that idiomatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SimHealthState {
    Healthy,
    Degraded,
    Dead,
}

/// Reason a transition happened. `Reason::None` is used for the
/// initial state before any observation arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HealthReason {
    None,
    ScreenshotSlow { p95_ms: u64 },
    ScreenshotFailed,
    HealthStale { age_ms: u64 },
    HealthFailedNoBaseline,
    ProcessGone { name: String },
    ProcessRecovered { name: String },
    Recovered,
}

/// Snapshot published on each real state transition.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SimHealthEvent {
    pub previous: SimHealthState,
    pub current: SimHealthState,
    pub reason: HealthReason,
}

/// Config knobs. Defaults match the values in
/// `.claude/rfcs/1.0.4-sim-health-and-backpressure.md` §D1.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SimHealthConfig {
    /// Screenshot p95 above this = `Degraded`.
    pub screenshot_slow: Duration,
    /// `/health` last-response age above this = `Degraded`.
    pub health_stale: Duration,
    /// `/health` last-response age above this = `Dead`.
    pub health_dead: Duration,
    /// Rolling window size for screenshot observations. Also the
    /// window over which "recent failures" are counted.
    pub rolling_window: usize,
    /// Broadcast channel capacity. If a subscriber lags past this,
    /// they get a `RecvError::Lagged`; the monitor keeps running.
    pub channel_capacity: usize,
}

impl Default for SimHealthConfig {
    fn default() -> Self {
        Self {
            screenshot_slow: Duration::from_millis(800),
            health_stale: Duration::from_secs(5),
            health_dead: Duration::from_secs(15),
            rolling_window: 32,
            channel_capacity: 64,
        }
    }
}

/// The monitor. Cheap to clone (internally `Arc`).
#[derive(Clone)]
pub struct SimHealthMonitor {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for SimHealthMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimHealthMonitor")
            .field("state", &self.state())
            .field("subscribers", &self.inner.tx.receiver_count())
            .finish()
    }
}

struct Inner {
    cfg: SimHealthConfig,
    state: Mutex<MonitorState>,
    tx: broadcast::Sender<SimHealthEvent>,
}

struct MonitorState {
    current: SimHealthState,
    /// Ring of recent screenshot wall times. Newest at back.
    screenshot_samples: Vec<Duration>,
    /// Ring of recent screenshot failure flags aligned with samples.
    screenshot_failed: Vec<bool>,
    /// Last time `/health` was observed to be OK. `None` until the
    /// first successful health call.
    last_health_ok: Option<Instant>,
    /// Set to `true` on any observed `/health` failure; cleared on
    /// any observed `/health` success. Used to distinguish "never
    /// observed" (neutral) from "observed and failed" (Degraded).
    saw_health_fail: bool,
    /// Which watched processes are currently believed alive.
    process_alive: std::collections::BTreeMap<String, bool>,
}

impl SimHealthMonitor {
    /// Build a monitor with the given config, starting in `Healthy`.
    pub fn new(cfg: SimHealthConfig) -> Self {
        let (tx, _rx) = broadcast::channel(cfg.channel_capacity);
        Self {
            inner: Arc::new(Inner {
                cfg,
                state: Mutex::new(MonitorState {
                    current: SimHealthState::Healthy,
                    screenshot_samples: Vec::new(),
                    screenshot_failed: Vec::new(),
                    last_health_ok: None,
                    saw_health_fail: false,
                    process_alive: std::collections::BTreeMap::new(),
                }),
                tx,
            }),
        }
    }

    /// Current classification. O(1).
    pub fn state(&self) -> SimHealthState {
        self.lock().current
    }

    /// Subscribe to state transitions. See `SimHealthEvent`.
    pub fn subscribe(&self) -> broadcast::Receiver<SimHealthEvent> {
        self.inner.tx.subscribe()
    }

    /// Record a screenshot wall time. Feed on every call, success or
    /// failure — set `failed = true` for a failed call.
    pub fn record_screenshot(&self, wall: Duration, failed: bool) {
        let reason;
        {
            let mut st = self.lock();
            let cap = self.inner.cfg.rolling_window;
            if st.screenshot_samples.len() >= cap {
                st.screenshot_samples.remove(0);
                st.screenshot_failed.remove(0);
            }
            st.screenshot_samples.push(wall);
            st.screenshot_failed.push(failed);
            reason = Self::classify_screenshot(&self.inner.cfg, &st);
        }
        self.recompute(reason);
    }

    /// Record a successful `/health` observation.
    pub fn record_health_ok(&self) {
        {
            let mut st = self.lock();
            st.last_health_ok = Some(Instant::now());
            st.saw_health_fail = false;
        }
        self.recompute(HealthReason::Recovered);
    }

    /// Record a failed `/health` observation. The monitor considers
    /// the runner dead once `health_dead` has passed since the last
    /// successful `/health`; before then it goes `Degraded`.
    pub fn record_health_fail(&self) {
        {
            let mut st = self.lock();
            st.saw_health_fail = true;
        }
        self.recompute(HealthReason::HealthFailedNoBaseline);
    }

    /// Report on a watched process. `alive = false` transitions the
    /// state to `Dead` on any process gone; `alive = true` allows
    /// recovery when all watched processes are alive again.
    pub fn record_process(&self, name: impl Into<String>, alive: bool) {
        let name = name.into();
        let reason;
        {
            let mut st = self.lock();
            st.process_alive.insert(name.clone(), alive);
            reason = if alive {
                HealthReason::ProcessRecovered { name }
            } else {
                HealthReason::ProcessGone { name }
            };
        }
        self.recompute(reason);
    }

    // ---- internals ------------------------------------------------

    fn lock(&self) -> std::sync::MutexGuard<'_, MonitorState> {
        // Poisoned mutexes only happen if a caller panicked while
        // holding the guard; the monitor's business is aggregating
        // observations, so a panic upstream should not silently drop
        // the whole health signal. We surface the last-known state.
        self.inner
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn classify_screenshot(cfg: &SimHealthConfig, st: &MonitorState) -> HealthReason {
        if st.screenshot_failed.iter().rev().take(3).all(|f| *f)
            && st.screenshot_failed.len() >= 3
        {
            return HealthReason::ScreenshotFailed;
        }
        let p95 = p95_ms(&st.screenshot_samples);
        if p95 > cfg.screenshot_slow.as_millis() as u64 {
            return HealthReason::ScreenshotSlow { p95_ms: p95 };
        }
        HealthReason::None
    }

    fn recompute(&self, incoming: HealthReason) {
        let (previous, current, chosen_reason) = {
            let st = self.lock();
            let (target, reason) = compute_target(&self.inner.cfg, &st, incoming);
            (st.current, target, reason)
        };
        if previous == current {
            // No transition, don't broadcast. But if the reason is a
            // fresh problem worth logging even without a transition,
            // future work may surface it via a separate probe channel.
            return;
        }
        {
            let mut st = self.lock();
            st.current = current;
        }
        // A send to an empty broadcast (no subscribers) returns
        // `Err`; that's fine, we do not require subscribers.
        let _ = self.inner.tx.send(SimHealthEvent {
            previous,
            current,
            reason: chosen_reason,
        });
    }
}

/// Compute the target state given the current observation. Pure
/// function on `(cfg, state, incoming)` — kept out of the impl for
/// direct unit-testing.
fn compute_target(
    cfg: &SimHealthConfig,
    st: &MonitorState,
    incoming: HealthReason,
) -> (SimHealthState, HealthReason) {
    // A watched process being dead is the highest severity.
    for (name, alive) in st.process_alive.iter() {
        if !alive {
            return (
                SimHealthState::Dead,
                HealthReason::ProcessGone { name: name.clone() },
            );
        }
    }

    // Health-age based severity. Startup is optimistic: no observation
    // is neutral (does not push state up). Only a real failure or a
    // real timeout past the stale/dead threshold moves us off Healthy.
    let health_age_reason = match st.last_health_ok {
        None => {
            if st.saw_health_fail {
                Some((
                    SimHealthState::Degraded,
                    HealthReason::HealthFailedNoBaseline,
                ))
            } else {
                None
            }
        }
        Some(last) => {
            let age = last.elapsed();
            if age >= cfg.health_dead {
                Some((
                    SimHealthState::Dead,
                    HealthReason::HealthStale {
                        age_ms: age.as_millis() as u64,
                    },
                ))
            } else if age >= cfg.health_stale {
                Some((
                    SimHealthState::Degraded,
                    HealthReason::HealthStale {
                        age_ms: age.as_millis() as u64,
                    },
                ))
            } else if st.saw_health_fail {
                // We had a baseline OK but a subsequent /health failed.
                // Consider Degraded until either another OK or the age
                // threshold escalates us.
                Some((
                    SimHealthState::Degraded,
                    HealthReason::HealthFailedNoBaseline,
                ))
            } else {
                None
            }
        }
    };

    // Screenshot-based severity is at most Degraded (a slow SimRenderServer
    // does not by itself prove the runner is dead).
    let screenshot_reason = match &incoming {
        HealthReason::ScreenshotSlow { p95_ms } => Some((
            SimHealthState::Degraded,
            HealthReason::ScreenshotSlow { p95_ms: *p95_ms },
        )),
        HealthReason::ScreenshotFailed => Some((
            SimHealthState::Degraded,
            HealthReason::ScreenshotFailed,
        )),
        _ => None,
    };

    // Merge: worst of the two candidates wins.
    let candidate = match (health_age_reason, screenshot_reason) {
        (Some(a), Some(b)) => Some(if a.0 >= b.0 { a } else { b }),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };

    match candidate {
        Some((state, reason)) => (state, reason),
        None => (SimHealthState::Healthy, HealthReason::Recovered),
    }
}

fn p95_ms(samples: &[Duration]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut ms: Vec<u64> = samples.iter().map(|d| d.as_millis() as u64).collect();
    ms.sort_unstable();
    let idx = ((samples.len() as f64) * 0.95).ceil() as usize - 1;
    let idx = idx.min(ms.len() - 1);
    ms[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SimHealthConfig {
        SimHealthConfig::default()
    }

    #[test]
    fn starts_healthy() {
        let m = SimHealthMonitor::new(cfg());
        assert_eq!(m.state(), SimHealthState::Healthy);
    }

    #[test]
    fn fast_screenshot_stays_healthy() {
        let m = SimHealthMonitor::new(cfg());
        m.record_health_ok();
        for _ in 0..10 {
            m.record_screenshot(Duration::from_millis(100), false);
        }
        assert_eq!(m.state(), SimHealthState::Healthy);
    }

    #[test]
    fn slow_screenshot_degrades() {
        let m = SimHealthMonitor::new(cfg());
        m.record_health_ok();
        for _ in 0..10 {
            m.record_screenshot(Duration::from_millis(2000), false);
        }
        assert_eq!(m.state(), SimHealthState::Degraded);
    }

    #[test]
    fn triple_screenshot_failure_degrades() {
        let m = SimHealthMonitor::new(cfg());
        m.record_health_ok();
        m.record_screenshot(Duration::from_millis(2000), true);
        m.record_screenshot(Duration::from_millis(2000), true);
        m.record_screenshot(Duration::from_millis(2000), true);
        assert_eq!(m.state(), SimHealthState::Degraded);
    }

    #[test]
    fn watched_process_gone_kills() {
        let m = SimHealthMonitor::new(cfg());
        m.record_health_ok();
        m.record_process("SimRenderServer", true);
        assert_eq!(m.state(), SimHealthState::Healthy);
        m.record_process("SimRenderServer", false);
        assert_eq!(m.state(), SimHealthState::Dead);
        m.record_process("SimRenderServer", true);
        assert_eq!(m.state(), SimHealthState::Healthy);
    }

    #[test]
    fn transitions_are_broadcast() {
        let m = SimHealthMonitor::new(cfg());
        let mut rx = m.subscribe();
        m.record_health_ok();
        m.record_process("xcodebuild", false);
        let evt = rx.try_recv().expect("should receive transition");
        assert_eq!(evt.previous, SimHealthState::Healthy);
        assert_eq!(evt.current, SimHealthState::Dead);
    }

    #[test]
    fn no_transition_no_event() {
        let m = SimHealthMonitor::new(cfg());
        let mut rx = m.subscribe();
        m.record_health_ok();
        for _ in 0..5 {
            m.record_screenshot(Duration::from_millis(50), false);
        }
        assert!(rx.try_recv().is_err(), "no transition should be broadcast");
    }

    #[test]
    fn rolling_window_evicts_old_samples() {
        let mut c = cfg();
        c.rolling_window = 4;
        let m = SimHealthMonitor::new(c);
        m.record_health_ok();
        // Fill with slow samples, then flush with fast ones.
        for _ in 0..4 {
            m.record_screenshot(Duration::from_millis(2000), false);
        }
        assert_eq!(m.state(), SimHealthState::Degraded);
        for _ in 0..4 {
            m.record_screenshot(Duration::from_millis(50), false);
        }
        assert_eq!(m.state(), SimHealthState::Healthy);
    }

    #[test]
    fn p95_ms_basic() {
        // 20 samples: 19 fast, 1 slow at index 19 → p95 = ceil(20*0.95) - 1 = 18 → still the slow one for small sets? Verify.
        let samples: Vec<Duration> = (0..19)
            .map(|_| Duration::from_millis(10))
            .chain(std::iter::once(Duration::from_millis(1000)))
            .collect();
        // sorted: 19 tens + 1 thousand. p95 index = ceil(20*.95)-1 = 18 (0-based). index 18 = 10 (still fast side).
        assert_eq!(p95_ms(&samples), 10);
        // 40 samples: 38 fast, 2 slow → p95 index = ceil(40*.95)-1 = 37. sorted first 38 are fast, index 37 is fast, so 10.
        // Confirms p95 is robust to a single outlier at n=20 but flags at higher slow fractions.
        let samples2: Vec<Duration> = (0..18)
            .map(|_| Duration::from_millis(10))
            .chain(std::iter::repeat_n(Duration::from_millis(1000), 2))
            .collect();
        // 20 samples, 2 slow at end. sorted index 18 is slow (1000).
        assert_eq!(p95_ms(&samples2), 1000);
    }
}
