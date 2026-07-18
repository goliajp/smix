// Screenshot pacer.
//
// `xcrun simctl io <udid> screenshot` under high-frequency load
// triggers `brk 1` inside `SimRenderServer`'s
// `com.apple.display.captureservice` dispatch queue on iOS 26.5.2
// (25F84) with SimRenderServer 1051.55, crashing the simulator.
//
// This pacer keeps a rolling window of recent screenshot wall times
// and enforces a minimum interval between calls:
//
// - Fast path (last wall < `slow_threshold`, no recent failures):
//   `min_interval` floor (default 100 ms). Imperceptible for gates
//   that already screenshot at ≥ 200 ms cadence, but kills the
//   worst-case tight loops that were putting SimRenderServer under
//   sustained pressure.
// - Slow path (recent wall > `slow_threshold`): floor lifted to
//   `slow_min_interval` (default 1500 ms). Also transitions the
//   attached `SimHealthMonitor` (if any) toward `Degraded`.
// - Circuit path (recent wall > `circuit_threshold`, or a screenshot
//   failed): circuit opens for `circuit_hold` (default 3 s). During
//   the hold, `compute_wait` returns `Err(CaptureBackpressure)`.
//
// Defaults are conservative — any consumer whose flows already have
// a ≥ 200 ms average cadence is unaffected. Tighter loops slow to
// the pacer floor; that is the intended behavior.
//
// Pixel-preservation invariant is unchanged — the pacer only gates
// invocation, it does not modify bytes.

use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;

/// Knobs for [`ScreenshotPacer`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ScreenshotPacerConfig {
    /// Minimum interval between successive screenshot calls in the
    /// fast path (recent wall < `slow_threshold`).
    pub min_interval: Duration,
    /// Wall-time boundary between fast and slow paths.
    pub slow_threshold: Duration,
    /// Minimum interval enforced in the slow path.
    pub slow_min_interval: Duration,
    /// Wall-time boundary above which the circuit opens.
    pub circuit_threshold: Duration,
    /// How long the circuit stays open after tripping.
    pub circuit_hold: Duration,
    /// Number of recent wall-time samples to retain for the slow-path
    /// classification.
    pub rolling_window: usize,
}

impl Default for ScreenshotPacerConfig {
    fn default() -> Self {
        Self {
            min_interval: Duration::from_millis(100),
            slow_threshold: Duration::from_millis(800),
            slow_min_interval: Duration::from_millis(1500),
            circuit_threshold: Duration::from_millis(1500),
            circuit_hold: Duration::from_secs(3),
            rolling_window: 8,
        }
    }
}

/// Adaptive pacer state. Not `Clone`; wrap in `Arc<Mutex<_>>` for
/// sharing between concurrent screenshot callers.
#[derive(Debug)]
pub struct ScreenshotPacer {
    config: ScreenshotPacerConfig,
    last_call_end: Option<Instant>,
    recent_walls: VecDeque<Duration>,
    circuit_open_until: Option<Instant>,
    monitor: Option<smix_sim_health::SimHealthMonitor>,
}

impl ScreenshotPacer {
    /// New pacer with the given config; no attached monitor.
    pub fn new(config: ScreenshotPacerConfig) -> Self {
        Self {
            config,
            last_call_end: None,
            recent_walls: VecDeque::new(),
            circuit_open_until: None,
            monitor: None,
        }
    }

    /// Attach a sim-health monitor. Every recorded screenshot wall
    /// and failure will be fed to `SimHealthMonitor::record_screenshot`.
    pub fn set_monitor(&mut self, monitor: smix_sim_health::SimHealthMonitor) {
        self.monitor = Some(monitor);
    }

    /// Ask how long the caller must wait before invoking the next
    /// screenshot. `Duration::ZERO` means "fire immediately".
    ///
    /// Returns `Err(retry_after)` when the circuit is open — the
    /// caller is expected to translate this into
    /// `DeviceControlError::CaptureBackpressure { retry_after }`.
    pub fn compute_wait(&mut self) -> Result<Duration, Duration> {
        let now = Instant::now();

        if let Some(until) = self.circuit_open_until {
            if now < until {
                return Err(until - now);
            }
            self.circuit_open_until = None;
        }

        let floor = if self.in_slow_path() {
            self.config.slow_min_interval
        } else {
            self.config.min_interval
        };

        let wait = match self.last_call_end {
            Some(last_end) => {
                let elapsed = now.saturating_duration_since(last_end);
                if elapsed >= floor {
                    Duration::ZERO
                } else {
                    floor - elapsed
                }
            }
            None => Duration::ZERO,
        };

        Ok(wait)
    }

    /// Record a completed screenshot invocation. Feed the sim-health
    /// monitor (if any) and update rolling window + circuit.
    pub fn record(&mut self, wall: Duration, failed: bool) {
        // Trip the circuit BEFORE we advance `last_call_end`, so
        // `compute_wait` will see the open circuit on the next call.
        if failed || wall >= self.config.circuit_threshold {
            self.circuit_open_until = Some(Instant::now() + self.config.circuit_hold);
        }

        while self.recent_walls.len() >= self.config.rolling_window {
            self.recent_walls.pop_front();
        }
        self.recent_walls.push_back(wall);
        self.last_call_end = Some(Instant::now());

        if let Some(m) = &self.monitor {
            m.record_screenshot(wall, failed);
        }
    }

    fn in_slow_path(&self) -> bool {
        // Any recent sample above the slow threshold pushes us into
        // slow-path pacing. Sensitive to a single spike, on purpose —
        // one 800+ ms screenshot is the early warning that
        // SimRenderServer is under pressure.
        self.recent_walls
            .iter()
            .any(|d| *d >= self.config.slow_threshold)
    }

    /// Config accessor (test + diagnostic).
    pub fn config(&self) -> &ScreenshotPacerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_cfg() -> ScreenshotPacerConfig {
        ScreenshotPacerConfig {
            min_interval: Duration::from_millis(10),
            slow_threshold: Duration::from_millis(100),
            slow_min_interval: Duration::from_millis(50),
            circuit_threshold: Duration::from_millis(200),
            circuit_hold: Duration::from_millis(100),
            rolling_window: 4,
        }
    }

    #[test]
    fn first_call_no_wait() {
        let mut p = ScreenshotPacer::new(small_cfg());
        assert_eq!(p.compute_wait().unwrap(), Duration::ZERO);
    }

    #[test]
    fn back_to_back_gets_paced() {
        let mut p = ScreenshotPacer::new(small_cfg());
        p.record(Duration::from_millis(20), false);
        // Immediately after a call, wait should be roughly the min_interval.
        let w = p.compute_wait().unwrap();
        assert!(
            w > Duration::from_millis(1) && w <= Duration::from_millis(10),
            "wait was {w:?}, expected ~10ms floor"
        );
    }

    #[test]
    fn slow_recent_lifts_floor() {
        let mut p = ScreenshotPacer::new(small_cfg());
        // Slow sample enters window → next compute_wait should use
        // slow_min_interval (50 ms), not min_interval (10 ms).
        p.record(Duration::from_millis(150), false);
        let w = p.compute_wait().unwrap();
        assert!(
            w > Duration::from_millis(30) && w <= Duration::from_millis(50),
            "wait was {w:?}, expected ~50ms slow floor"
        );
    }

    #[test]
    fn circuit_trips_on_very_slow() {
        let mut p = ScreenshotPacer::new(small_cfg());
        p.record(Duration::from_millis(300), false); // >= circuit_threshold=200
        let err = p.compute_wait().unwrap_err();
        assert!(err > Duration::from_millis(0) && err <= Duration::from_millis(100));
    }

    #[test]
    fn circuit_trips_on_failure() {
        let mut p = ScreenshotPacer::new(small_cfg());
        p.record(Duration::from_millis(50), true);
        assert!(p.compute_wait().is_err());
    }

    #[test]
    fn circuit_expires() {
        let mut p = ScreenshotPacer::new(small_cfg());
        p.record(Duration::from_millis(50), true);
        assert!(p.compute_wait().is_err());
        std::thread::sleep(Duration::from_millis(110));
        // After hold, should return a normal wait (Ok), not an error.
        assert!(p.compute_wait().is_ok());
    }

    #[test]
    fn rolling_window_evicts_old_slow_samples() {
        let mut p = ScreenshotPacer::new(small_cfg());
        p.record(Duration::from_millis(150), false); // slow
        assert!(p.in_slow_path());
        for _ in 0..4 {
            p.record(Duration::from_millis(20), false);
        }
        // Window size = 4; after 4 fast records, slow sample evicted.
        assert!(!p.in_slow_path());
    }

    #[test]
    fn monitor_receives_observations() {
        let mut p = ScreenshotPacer::new(small_cfg());
        let m = smix_sim_health::SimHealthMonitor::new(smix_sim_health::SimHealthConfig::default());
        p.set_monitor(m.clone());
        // Baseline health so the monitor is not pushed off by
        // HealthFailedNoBaseline before we start.
        m.record_health_ok();
        for _ in 0..10 {
            p.record(Duration::from_millis(2000), false);
        }
        assert_eq!(m.state(), smix_sim_health::SimHealthState::Degraded);
    }
}
