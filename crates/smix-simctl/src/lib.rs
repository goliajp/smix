#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-simctl — xcrun simctl child_process wrapper (outer crate).
//!
//! All operations shell out to `xcrun simctl <subcommand>`; JSON-formatted
//! outputs (list runtimes, list devices, screenshot binary) are parsed with
//! serde_json / raw bytes. Tokio's `process::Command` is the async spawn
//! primitive.
//!
//! This is an outer crate — allowed to depend on the wider tokio ecosystem.
//! Use it from cement (smix-cli / smix-mcp) or from a higher-level driver
//! wrapper.

#![doc(html_root_url = "https://docs.smix.dev/smix-simctl")]

pub mod registry;
/// v1.0.4 — adaptive `xcrun simctl io screenshot` pacer. See
/// [`screenshot_pacer::ScreenshotPacer`] and
/// `.claude/rfcs/1.0.4-sim-health-and-backpressure.md` §D3.
pub mod screenshot_pacer;

use screenshot_pacer::{ScreenshotPacer, ScreenshotPacerConfig};
use serde::{Deserialize, Serialize};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::sleep;

/// Failure variants for any `xcrun simctl` invocation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SimctlError {
    /// Failed to spawn the `xcrun` process itself (PATH lookup / fork failure).
    #[error("spawn xcrun simctl failed: {0}")]
    Spawn(#[from] io::Error),
    /// `xcrun simctl <sub>` exited non-zero.
    ///
    /// v1.0.7 §D2 — carries the full `argv` and `wall_ms` so the
    /// `Display` impl surfaces every argument the consumer needs to
    /// reproduce or file a precise upstream bug. Prior versions
    /// reported only the subcommand name (e.g., `"spawn"`) without
    /// telling the caller which binary or paths simctl was asked to
    /// touch. See `.claude/rfcs/1.0.7-observability-layer.md` §D2.
    #[error("xcrun simctl {} exited {code} ({wall_ms}ms): {stderr}", .argv.join(" "))]
    NonZeroExit {
        /// Subcommand name (e.g. `"boot"`, `"launch"`).
        subcommand: String,
        /// Full argv passed to `xcrun simctl` (excludes the `xcrun
        /// simctl` prefix itself; includes subcommand + all args).
        ///
        /// Since smix 1.0.7.
        argv: Vec<String>,
        /// Exit code from `xcrun simctl`.
        code: i32,
        /// Captured stderr (truncated for log-friendliness).
        stderr: String,
        /// Wall-clock milliseconds the invocation ran before failing.
        ///
        /// Since smix 1.0.7.
        #[allow(dead_code)]
        wall_ms: u64,
    },
    /// `xcrun simctl <sub>` exited 0 but stdout didn't match the expected shape.
    #[error("xcrun simctl {subcommand} returned malformed output: {detail}")]
    Malformed {
        /// Subcommand name.
        subcommand: String,
        /// Parser-side detail.
        detail: String,
    },
    /// `xcrun simctl <sub>` did not complete within the deadline.
    #[error("xcrun simctl {subcommand} timed out after {ms}ms")]
    Timeout {
        /// Subcommand name.
        subcommand: String,
        /// Deadline that was exceeded (milliseconds).
        ms: u64,
    },
    /// The screenshot pacer's circuit is open — a recent screenshot
    /// wall time exceeded the circuit threshold, or a screenshot
    /// failed. Callers should back off for `retry_after` and try
    /// again. See [`screenshot_pacer::ScreenshotPacer`] and
    /// `.claude/rfcs/1.0.4-sim-health-and-backpressure.md` §D3.
    ///
    /// Since smix 1.0.4.
    #[error("screenshot pacer circuit open; retry after {retry_after:?}")]
    CaptureBackpressure {
        /// Suggested minimum delay before the next attempt.
        retry_after: Duration,
    },
}

impl SimctlError {
    /// v1.0.7 §D2 — synthetic `NonZeroExit` for callers translating
    /// a foreign subprocess error into `SimctlError` (e.g.
    /// AndroidDeviceControl adapting adb failures). Fills
    /// `argv = [subcommand]` + `wall_ms = 0`; when the caller has a
    /// real argv, prefer the struct literal.
    pub fn non_zero_exit(
        subcommand: impl Into<String>,
        code: i32,
        stderr: impl Into<String>,
    ) -> Self {
        let sub = subcommand.into();
        Self::NonZeroExit {
            argv: vec![sub.clone()],
            subcommand: sub,
            code,
            stderr: stderr.into(),
            wall_ms: 0,
        }
    }
}

/// Handle to an active `xcrun simctl io recordVideo` child process. Pair
/// with [`SimctlClient::record_video_stop`] for SIGINT-and-wait shutdown
/// (so the mp4 trailer is flushed). Dropping the handle without `stop`
/// would tokio-SIGKILL on Drop and truncate the output file.
#[derive(Debug)]
pub struct RecordingHandle {
    pub(crate) child: tokio::process::Child,
    /// Output mp4 path verbatim as passed to `record_video_start`.
    pub path: String,
    /// Wall-clock start time for "recording in progress for Xs" diagnostics.
    pub started_at: std::time::Instant,
}

// -------------------- types ----------------------------------------------

/// One iOS / watchOS / tvOS runtime installed on the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimctlRuntime {
    /// Fully-qualified runtime identifier (e.g. `"com.apple.CoreSimulator.SimRuntime.iOS-17-0"`).
    pub identifier: String,
    /// Human-readable name (e.g. `"iOS 17.0"`).
    pub name: String,
    /// Version string (e.g. `"17.0"`).
    pub version: String,
    /// Whether the runtime is available for booting devices.
    pub is_available: bool,
}

/// One simulator device known to `xcrun simctl`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimctlDevice {
    /// Device UDID (stable identifier).
    pub udid: String,
    /// Human-readable name.
    pub name: String,
    /// Current state (`"Booted"` / `"Shutdown"` / `"Creating"` / etc.).
    pub state: String,
    /// Whether the device is available for booting.
    pub is_available: bool,
    /// Device-type identifier (e.g. `"com.apple.CoreSimulator.SimDeviceType.iPhone-15"`).
    #[serde(rename = "deviceTypeIdentifier", default)]
    pub device_type_identifier: String,
    /// Runtime identifier this device was created against.
    #[serde(rename = "runtimeIdentifier", default)]
    pub runtime_identifier: String,
}

/// Permission names accepted by `xcrun simctl privacy <udid> grant <name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimctlPermission {
    /// Camera access.
    Camera,
    /// Photos library access.
    Photos,
    /// Location access (while-in-use).
    Location,
    /// Background location access (always).
    LocationAlways,
    /// Notification posting permission.
    Notifications,
    /// Microphone access.
    Microphone,
    /// Contacts access.
    Contacts,
    /// Calendar events access.
    Calendar,
    /// Reminders access.
    Reminders,
    /// Media library (music / video) access.
    Media,
    /// Motion / fitness sensor access.
    Motion,
    /// HomeKit accessory access.
    HomeKit,
    /// HealthKit data access.
    Health,
    /// Bluetooth device discovery / connection.
    Bluetooth,
    /// FaceID / TouchID biometric prompt.
    Faceid,
    /// Address-book (deprecated alias for `Contacts`).
    AddressBook,
}

impl SimctlPermission {
    /// Wire string used by `xcrun simctl privacy <udid> grant <name>`.
    pub fn as_str(self) -> &'static str {
        match self {
            SimctlPermission::Camera => "camera",
            SimctlPermission::Photos => "photos",
            SimctlPermission::Location => "location",
            SimctlPermission::LocationAlways => "location-always",
            SimctlPermission::Notifications => "notifications",
            SimctlPermission::Microphone => "microphone",
            SimctlPermission::Contacts => "contacts",
            SimctlPermission::Calendar => "calendar",
            SimctlPermission::Reminders => "reminders",
            SimctlPermission::Media => "media-library",
            SimctlPermission::Motion => "motion",
            SimctlPermission::HomeKit => "homekit",
            SimctlPermission::Health => "health",
            SimctlPermission::Bluetooth => "bluetooth",
            SimctlPermission::Faceid => "faceid",
            SimctlPermission::AddressBook => "addressbook",
        }
    }
}

/// UI appearance mode for `xcrun simctl ui <udid> appearance`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    /// Light mode.
    Light,
    /// Dark mode.
    Dark,
}

impl Appearance {
    /// Wire string used by `xcrun simctl ui <udid> appearance <mode>`.
    pub fn as_str(self) -> &'static str {
        match self {
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
}

/// Launched-app result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchResult {
    /// Process ID of the launched app.
    pub pid: u32,
}

// -------------------- v1.0.7 §D3 subprocess ring buffer -----------------

/// v1.0.7 §D3 — recorded snapshot of one `xcrun simctl` invocation.
/// Exposed so callers can dump the ring buffer for post-mortem when
/// something upstream fails.
#[derive(Clone, Debug)]
pub struct SubprocessRecord {
    /// argv as passed to `xcrun simctl` (excludes the `xcrun simctl`
    /// prefix; first entry is the subcommand).
    pub argv: Vec<String>,
    /// Exit code; `None` when the process failed to spawn or the
    /// output-capture path failed before recording the exit.
    pub exit_code: Option<i32>,
    /// Wall-clock milliseconds.
    pub wall_ms: u64,
    /// First 256 bytes of stderr (truncated).
    pub stderr_head: String,
    /// Wall-clock timestamp the invocation completed.
    pub timestamp: std::time::SystemTime,
}

mod subprocess_ring {
    use super::SubprocessRecord;
    use std::collections::VecDeque;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, UNIX_EPOCH};

    fn cell() -> &'static Mutex<VecDeque<SubprocessRecord>> {
        static INSTANCE: OnceLock<Mutex<VecDeque<SubprocessRecord>>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(VecDeque::with_capacity(128)))
    }

    /// v1.0.10 §D6 — persist path for the ring buffer. Nil = in-memory
    /// only (legacy pre-v1.0.10 behavior). Set once at process startup
    /// via [`set_persist_path`]; unchanged for the lifetime of the
    /// process. Insight's v1.0.7 dogfood reported empty
    /// `/diagnostic/dump` payloads because supervisor cycles killed the
    /// CLI faster than a dump could snapshot the in-memory buffer.
    /// Persisting side-steps that: the file survives cycles, so
    /// post-mortem tools read the file rather than the (now-gone)
    /// in-memory state.
    fn persist_cell() -> &'static Mutex<Option<PathBuf>> {
        static INSTANCE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(None))
    }

    /// v1.0.10 §D6 — install a persist path. Callers should also call
    /// [`load_persisted`] once after this so the in-memory ring starts
    /// pre-populated with the last-known state.
    pub fn set_persist_path(path: PathBuf) {
        let mut g = match persist_cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = Some(path);
    }

    fn persist_path_copy() -> Option<PathBuf> {
        let g = match persist_cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }

    /// v1.0.10 §D6 — record one invocation. Ring buffer capped at 128
    /// entries; oldest evicted on push. When [`set_persist_path`] is
    /// active, atomically writes the buffer to disk after the mutation
    /// so a subsequent supervisor-cycle doesn't lose the observation.
    pub(super) fn record(entry: SubprocessRecord) {
        {
            let mut g = match cell().lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if g.len() >= 128 {
                g.pop_front();
            }
            g.push_back(entry);
        }
        if let Some(path) = persist_path_copy() {
            let snapshot = snapshot();
            // Best-effort. Failure to persist must never affect the
            // caller of `xcrun simctl` — we swallow errors here and
            // let the next `record()` retry.
            let _ = write_json_atomic(&path, &snapshot);
        }
    }

    /// Snapshot the current ring buffer. Ordered oldest → newest.
    pub fn snapshot() -> Vec<SubprocessRecord> {
        let g = match cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.iter().cloned().collect()
    }

    /// v1.0.10 §D6 — load a previously-persisted ring from disk. No-op
    /// when the file does not exist. Called by CLI startup after
    /// [`set_persist_path`] so the in-memory view starts with the
    /// last-known state. Silently drops parse failures — corrupt files
    /// are noise, not fatal.
    pub fn load_persisted() {
        let Some(path) = persist_path_copy() else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Ok(records) = serde_json::from_slice::<Vec<PersistedRecord>>(&bytes) else {
            return;
        };
        let mut g = match cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        for r in records.into_iter().rev().take(128).rev() {
            g.push_back(r.into_record());
        }
    }

    fn write_json_atomic(path: &Path, records: &[SubprocessRecord]) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            let payload: Vec<PersistedRecord> = records.iter().cloned().map(Into::into).collect();
            let serialized = serde_json::to_vec(&payload)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&serialized)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }

    /// On-disk representation. Kept separate from
    /// [`SubprocessRecord`] because the wall-clock timestamp is a
    /// `SystemTime` which does not serde-derive cleanly; we convert to
    /// UNIX millis for a stable JSON shape.
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    struct PersistedRecord {
        argv: Vec<String>,
        exit_code: Option<i32>,
        wall_ms: u64,
        stderr_head: String,
        timestamp_ms: u64,
    }

    impl From<SubprocessRecord> for PersistedRecord {
        fn from(r: SubprocessRecord) -> Self {
            let timestamp_ms = r
                .timestamp
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            Self {
                argv: r.argv,
                exit_code: r.exit_code,
                wall_ms: r.wall_ms,
                stderr_head: r.stderr_head,
                timestamp_ms,
            }
        }
    }

    impl PersistedRecord {
        fn into_record(self) -> SubprocessRecord {
            let timestamp =
                UNIX_EPOCH + Duration::from_millis(self.timestamp_ms);
            SubprocessRecord {
                argv: self.argv,
                exit_code: self.exit_code,
                wall_ms: self.wall_ms,
                stderr_head: self.stderr_head,
                timestamp,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::time::SystemTime;

        #[test]
        fn persist_roundtrip_after_supervisor_cycle_simulation() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("ring.json");
            set_persist_path(path.clone());

            record(SubprocessRecord {
                argv: vec!["shutdown".into(), "UDID-A".into()],
                exit_code: Some(0),
                wall_ms: 42,
                stderr_head: String::new(),
                timestamp: SystemTime::now(),
            });
            assert!(path.exists(), "persist file must exist after record");

            // Simulate supervisor cycle: clear in-memory then load.
            {
                let mut g = cell().lock().unwrap();
                g.clear();
            }
            assert!(snapshot().is_empty());

            load_persisted();
            let after = snapshot();
            assert_eq!(after.len(), 1);
            assert_eq!(after[0].argv, vec!["shutdown".to_string(), "UDID-A".into()]);
            assert_eq!(after[0].exit_code, Some(0));
            assert_eq!(after[0].wall_ms, 42);
        }
    }
}

/// v1.0.10 §D6 — enable subprocess-ring persistence at the given path.
/// CLI startup wires this to `~/.local/share/smix/subprocess-ring.json`
/// so `/diagnostic/dump` payloads survive supervisor cycles. Optional;
/// missing this call keeps pre-v1.0.10 in-memory-only behavior.
pub fn set_subprocess_ring_persist_path(path: std::path::PathBuf) {
    subprocess_ring::set_persist_path(path);
    subprocess_ring::load_persisted();
}

// v1.0.14 Cluster A — CLI-side resetAppData counter tracking.
//
// The `resetAppData` verb dispatches host-side (simctl openurl + metro
// log tail, no runner HTTP endpoint) so counters can't come from the
// runner's `/diagnostic/dump` payload. This module owns them,
// persisting to `~/.local/share/smix/reset-app-data-counters.json`
// so counter deltas across `smix run` invocations + `smix diagnostic
// dump` (later, separate process) all see the same data.
//
// Public API mirrors [`subprocess_ring`] shape for consistency.
mod reset_app_data_counters {
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    fn cell() -> &'static Mutex<Counters> {
        static INSTANCE: OnceLock<Mutex<Counters>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(Counters::default()))
    }
    fn persist_cell() -> &'static Mutex<Option<PathBuf>> {
        static INSTANCE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(None))
    }

    #[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
    pub struct Counters {
        pub reset_app_data_total: u64,
        pub reset_app_data_timed_out: u64,
    }

    pub fn set_persist_path(path: PathBuf) {
        let mut g = match persist_cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = Some(path);
    }

    fn persist_path_copy() -> Option<PathBuf> {
        let g = match persist_cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }

    pub fn load_persisted() {
        let Some(path) = persist_path_copy() else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Ok(loaded) = serde_json::from_slice::<Counters>(&bytes) else {
            return;
        };
        let mut g = match cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = loaded;
    }

    pub fn increment_total() {
        {
            let mut g = match cell().lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            g.reset_app_data_total = g.reset_app_data_total.saturating_add(1);
        }
        persist_best_effort();
    }

    pub fn increment_timed_out() {
        {
            let mut g = match cell().lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            g.reset_app_data_timed_out = g.reset_app_data_timed_out.saturating_add(1);
        }
        persist_best_effort();
    }

    pub fn snapshot() -> Counters {
        let g = match cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g
    }

    fn persist_best_effort() {
        let Some(path) = persist_path_copy() else {
            return;
        };
        let snapshot = snapshot();
        let _ = write_json_atomic(&path, &snapshot);
    }

    fn write_json_atomic(path: &Path, counters: &Counters) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(counters)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn increment_and_persist_roundtrip() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("counters.json");
            set_persist_path(path.clone());
            // Reset in-memory to avoid cross-test pollution.
            {
                let mut g = cell().lock().unwrap();
                *g = Counters::default();
            }
            increment_total();
            increment_total();
            increment_timed_out();
            assert!(path.exists());
            let raw = std::fs::read_to_string(&path).unwrap();
            let loaded: Counters = serde_json::from_str(&raw).unwrap();
            assert_eq!(loaded.reset_app_data_total, 2);
            assert_eq!(loaded.reset_app_data_timed_out, 1);
        }
    }
}

/// v1.0.14 Cluster A — public snapshot of CLI-side resetAppData
/// counter state. Populated by [`increment_reset_app_data_total`] +
/// [`increment_reset_app_data_timed_out`] as the CLI dispatches the
/// verb; loaded from disk on CLI startup if
/// [`set_reset_app_data_counters_persist_path`] was called.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResetAppDataCounters {
    /// Total resetAppData dispatches (any outcome).
    pub reset_app_data_total: u64,
    /// resetAppData dispatches where the completion signal did not
    /// arrive inside the timeout window. `> 0` = the URL was fired
    /// but the app did not emit the expected reset-complete log line.
    pub reset_app_data_timed_out: u64,
}

/// v1.0.14 Cluster A — enable resetAppData counter persistence at
/// the given path. Callers pass
/// `~/.local/share/smix/reset-app-data-counters.json` at CLI startup
/// so counter state survives across `smix run` → `smix diagnostic
/// dump` invocations.
pub fn set_reset_app_data_counters_persist_path(path: std::path::PathBuf) {
    reset_app_data_counters::set_persist_path(path);
    reset_app_data_counters::load_persisted();
}

/// v1.0.14 Cluster A — advance the resetAppData total counter.
/// Called by the CLI runtime after each dispatch (success or timeout).
pub fn increment_reset_app_data_total() {
    reset_app_data_counters::increment_total();
}

/// v1.0.14 Cluster A — advance the resetAppData timed-out counter.
/// Called by the CLI runtime when the completion signal (log-line
/// pattern match) did not arrive inside the timeout window. Always
/// paired with a preceding [`increment_reset_app_data_total`] on the
/// same dispatch.
pub fn increment_reset_app_data_timed_out() {
    reset_app_data_counters::increment_timed_out();
}

/// v1.0.14 Cluster A — snapshot the current counter state for
/// display / wire emission. Returns zero-valued counters when
/// persistence was never wired.
pub fn reset_app_data_counters_snapshot() -> ResetAppDataCounters {
    let s = reset_app_data_counters::snapshot();
    ResetAppDataCounters {
        reset_app_data_total: s.reset_app_data_total,
        reset_app_data_timed_out: s.reset_app_data_timed_out,
    }
}

// v1.0.15 §6 — flow-attempt persistence for retry attribution. Called
// by `smix run` after each flow completes (all its attempts done);
// read by `smix diagnostic dump` to render the attribution table.
// Backed by `~/.local/share/smix/flow-attempts.json`; capped at last
// 32 flows (heuristic — enough for a batch or two of insight bootstrap
// while keeping the dump snapshot cheap to serialize).
mod flow_attempts {
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct PersistedAttempt {
        pub attempt_index: u32,
        pub status: String,
        pub error_class: Option<String>,
        pub ips_generated: Option<String>,
        pub wall_ms: u64,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct PersistedFlow {
        pub flow_name: String,
        pub attempts: Vec<PersistedAttempt>,
    }

    fn cell() -> &'static Mutex<Vec<PersistedFlow>> {
        static INSTANCE: OnceLock<Mutex<Vec<PersistedFlow>>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(Vec::new()))
    }
    fn persist_cell() -> &'static Mutex<Option<PathBuf>> {
        static INSTANCE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
        INSTANCE.get_or_init(|| Mutex::new(None))
    }

    pub fn set_persist_path(path: PathBuf) {
        let mut g = match persist_cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = Some(path);
    }

    fn persist_path_copy() -> Option<PathBuf> {
        let g = match persist_cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }

    pub fn load_persisted() {
        let Some(path) = persist_path_copy() else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Ok(loaded) = serde_json::from_slice::<Vec<PersistedFlow>>(&bytes) else {
            return;
        };
        let mut g = match cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *g = loaded;
    }

    pub fn record(flow_name: &str, attempts: &[PersistedAttempt]) {
        {
            let mut g = match cell().lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            g.push(PersistedFlow {
                flow_name: flow_name.to_string(),
                attempts: attempts.to_vec(),
            });
            if g.len() > 32 {
                let drop = g.len() - 32;
                g.drain(0..drop);
            }
        }
        persist_best_effort();
    }

    pub fn snapshot() -> Vec<PersistedFlow> {
        let g = match cell().lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }

    fn persist_best_effort() {
        let Some(path) = persist_path_copy() else {
            return;
        };
        let flows = snapshot();
        let _ = write_json_atomic(&path, &flows);
    }

    fn write_json_atomic(path: &Path, flows: &[PersistedFlow]) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(flows)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }
}

/// v1.0.15 §6 — enable flow-attempts persistence at the given path.
/// CLI startup wires this to `~/.local/share/smix/flow-attempts.json`
/// so retry attribution survives across `smix run` → `smix diagnostic
/// dump` invocations.
pub fn set_flow_attempts_persist_path(path: std::path::PathBuf) {
    flow_attempts::set_persist_path(path);
    flow_attempts::load_persisted();
}

/// v1.0.15 §6 — public accessor with just the fields needed by callers.
/// Mirrors [`smix_runner_wire::FlowAttempt`] shape.
#[derive(Clone, Debug)]
pub struct FlowAttemptData {
    /// Zero-based retry index.
    pub attempt_index: u32,
    /// Overall outcome ("ok" / "timeout" / "error" / "crashed").
    pub status: String,
    /// Free-form error class code (`Some` on non-ok).
    pub error_class: Option<String>,
    /// `.ips` filename that appeared during this attempt, when detected.
    pub ips_generated: Option<String>,
    /// Wall-clock milliseconds.
    pub wall_ms: u64,
}

/// v1.0.15 §6 — recorded flow with its attempt list.
#[derive(Clone, Debug)]
pub struct FlowAttemptRecordData {
    /// Flow name (yaml basename or explicit id).
    pub flow_name: String,
    /// Ordered attempts, first try first.
    pub attempts: Vec<FlowAttemptData>,
}

/// v1.0.15 §6 — record the outcome of a flow's attempts. Called from
/// `smix run` after all retries for that flow have completed. `smix
/// diagnostic dump` reads via [`recent_flow_attempts`] later.
pub fn record_flow_attempts<A>(flow_name: &str, attempts: &[A])
where
    A: FlowAttemptShape,
{
    let converted: Vec<flow_attempts::PersistedAttempt> = attempts
        .iter()
        .map(|a| flow_attempts::PersistedAttempt {
            attempt_index: a.attempt_index(),
            status: a.status().to_string(),
            error_class: a.error_class().map(str::to_string),
            ips_generated: a.ips_generated().map(str::to_string),
            wall_ms: a.wall_ms(),
        })
        .collect();
    flow_attempts::record(flow_name, &converted);
}

/// v1.0.15 §6 — abstraction so callers pass either
/// [`smix_runner_wire::FlowAttempt`] or a local struct with the same
/// shape without a cross-crate dep on smix-runner-wire from smix-simctl.
pub trait FlowAttemptShape {
    /// Zero-based retry index.
    fn attempt_index(&self) -> u32;
    /// "ok" / "timeout" / "error" / "crashed".
    fn status(&self) -> &str;
    /// Error class code, if any.
    fn error_class(&self) -> Option<&str>;
    /// `.ips` filename attributable to this attempt, if any.
    fn ips_generated(&self) -> Option<&str>;
    /// Wall-clock milliseconds.
    fn wall_ms(&self) -> u64;
}

/// v1.0.15 §6 — snapshot recent flow attempts for display / wire
/// emission. Returns empty when persistence was never wired.
pub fn recent_flow_attempts() -> Vec<FlowAttemptRecordData> {
    flow_attempts::snapshot()
        .into_iter()
        .map(|f| FlowAttemptRecordData {
            flow_name: f.flow_name,
            attempts: f
                .attempts
                .into_iter()
                .map(|a| FlowAttemptData {
                    attempt_index: a.attempt_index,
                    status: a.status,
                    error_class: a.error_class,
                    ips_generated: a.ips_generated,
                    wall_ms: a.wall_ms,
                })
                .collect(),
        })
        .collect()
}

/// v1.0.7 §D3 — snapshot the process-wide ring buffer of recent
/// `xcrun simctl` invocations. Ordered oldest → newest, capped at 128
/// entries. Reset on process restart.
pub fn recent_subprocesses() -> Vec<SubprocessRecord> {
    subprocess_ring::snapshot()
}

// -------------------- raw spawn primitive --------------------------------

/// Execute `xcrun simctl <args>` and capture stdout/stderr.
async fn simctl_capture(args: &[&str]) -> Result<(Vec<u8>, String), SimctlError> {
    simctl_capture_env(args, &[]).await
}

/// `simctl_capture` with extra envp pairs set on the spawned process.
/// The `xcrun simctl launch` subcommand uses this to inject
/// `SIMCTL_CHILD_<KEY>=<VAL>` vars that the launched app sees as
/// `ProcessInfo().environment["KEY"]`. `env` entries here are passed
/// verbatim — caller composes the `SIMCTL_CHILD_` prefix via
/// [`compose_child_env`].
async fn simctl_capture_env(
    args: &[&str],
    env: &[(String, String)],
) -> Result<(Vec<u8>, String), SimctlError> {
    let mut cmd = Command::new("xcrun");
    cmd.arg("simctl");
    for a in args {
        cmd.arg(a);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let started = std::time::Instant::now();
    let output = cmd.output().await?;
    let wall_ms = started.elapsed().as_millis() as u64;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    // v1.0.7 §D3 — every simctl invocation records to the ring buffer
    // regardless of exit status.
    subprocess_ring::record(SubprocessRecord {
        argv: args.iter().map(|s| s.to_string()).collect(),
        exit_code: output.status.code(),
        wall_ms,
        stderr_head: {
            let mut s = stderr.clone();
            if s.len() > 256 {
                s.truncate(256);
            }
            s
        },
        timestamp: std::time::SystemTime::now(),
    });
    if !output.status.success() {
        return Err(SimctlError::NonZeroExit {
            subcommand: args.first().map(|s| s.to_string()).unwrap_or_default(),
            argv: args.iter().map(|s| s.to_string()).collect(),
            code: output.status.code().unwrap_or(-1),
            stderr,
            wall_ms,
        });
    }
    Ok((output.stdout, stderr))
}

async fn simctl_run(args: &[&str]) -> Result<String, SimctlError> {
    let (stdout, _) = simctl_capture(args).await?;
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// Like [`simctl_run`] but injects `child_env` envp on the spawned
/// process. Used by the env-aware launch path so the launched app can
/// read deploy-time secrets / endpoints via `ProcessInfo`.
async fn simctl_run_env(args: &[&str], env: &[(String, String)]) -> Result<String, SimctlError> {
    let (stdout, _) = simctl_capture_env(args, env).await?;
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// Compose user-provided `(key, value)` pairs into the `SIMCTL_CHILD_*`
/// envp that `xcrun simctl launch` strips and delivers to the launched
/// app. Idempotent: a key that already starts with `SIMCTL_CHILD_` is
/// passed through unchanged.
///
/// # Example
///
/// ```
/// use smix_simctl::compose_child_env;
/// let composed = compose_child_env(&[("SMIX_PERF_RECEIVER_URL", "http://h:9999")]);
/// assert_eq!(
///     composed,
///     vec![(
///         "SIMCTL_CHILD_SMIX_PERF_RECEIVER_URL".to_string(),
///         "http://h:9999".to_string(),
///     )]
/// );
/// ```
pub fn compose_child_env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| {
            let key = if k.starts_with("SIMCTL_CHILD_") {
                (*k).to_string()
            } else {
                format!("SIMCTL_CHILD_{k}")
            };
            (key, (*v).to_string())
        })
        .collect()
}

// -------------------- client --------------------------------------------

/// Stateless wrapper around xcrun simctl. Methods are free functions
/// in spirit (no instance state beyond optionally-cached `xcrun` path);
/// kept as a struct for API ergonomics + future caching.
///
/// Since v1.0.4, the client also holds a [`ScreenshotPacer`] that
/// throttles `xcrun simctl io screenshot` under high-frequency
/// load. Defaults are conservative (100 ms interval floor);
/// consumers whose flows are already loose are unaffected.
#[derive(Debug)]
pub struct SimctlClient {
    /// Screenshot pacer — enforces the RFC 1.0.4 §D3 interval floor,
    /// slow-path lift, and circuit breaker. Shared via `Arc<Mutex<_>>`
    /// so a cloned client (which callers occasionally do) still shares
    /// pressure accounting.
    screenshot_pacer: Arc<std::sync::Mutex<ScreenshotPacer>>,
}

impl Default for SimctlClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SimctlClient {
    /// Construct a new client with default screenshot pacing (100 ms
    /// interval floor, adaptive slow-path lift to 1500 ms, circuit
    /// breaker on ≥ 1500 ms walls or failures).
    pub fn new() -> Self {
        SimctlClient {
            screenshot_pacer: Arc::new(std::sync::Mutex::new(ScreenshotPacer::new(
                ScreenshotPacerConfig::default(),
            ))),
        }
    }

    /// Override the screenshot pacer with a custom config.
    ///
    /// Since smix 1.0.4.
    #[must_use]
    pub fn with_screenshot_pacer(self, config: ScreenshotPacerConfig) -> Self {
        {
            let mut guard = self
                .screenshot_pacer
                .lock()
                .expect("screenshot pacer mutex must not be poisoned");
            *guard = ScreenshotPacer::new(config);
        }
        self
    }

    /// Attach a [`smix_sim_health::SimHealthMonitor`] to receive
    /// screenshot wall-time observations from every `screenshot`
    /// call. Composes with the pacer — the pacer still enforces its
    /// interval / circuit locally, and the monitor sees the same
    /// walls for global state classification.
    ///
    /// Since smix 1.0.4.
    #[must_use]
    pub fn with_sim_health(self, monitor: smix_sim_health::SimHealthMonitor) -> Self {
        {
            let mut guard = self
                .screenshot_pacer
                .lock()
                .expect("screenshot pacer mutex must not be poisoned");
            guard.set_monitor(monitor);
        }
        self
    }

    // ---- inventory ------------------------------------------------------

    /// `xcrun simctl list runtimes -j` → `Vec<SimctlRuntime>`.
    pub async fn list_runtimes(&self) -> Result<Vec<SimctlRuntime>, SimctlError> {
        let raw = simctl_run(&["list", "runtimes", "-j"]).await?;
        #[derive(Deserialize)]
        struct Wrap {
            runtimes: Vec<RawRuntime>,
        }
        #[derive(Deserialize)]
        struct RawRuntime {
            identifier: String,
            name: String,
            version: String,
            #[serde(rename = "isAvailable", default)]
            is_available: bool,
        }
        let w: Wrap = serde_json::from_str(&raw).map_err(|e| SimctlError::Malformed {
            subcommand: "list runtimes".into(),
            detail: e.to_string(),
        })?;
        Ok(w.runtimes
            .into_iter()
            .map(|r| SimctlRuntime {
                identifier: r.identifier,
                name: r.name,
                version: r.version,
                is_available: r.is_available,
            })
            .collect())
    }

    /// `xcrun simctl list devices -j` → flattened `Vec<SimctlDevice>`.
    pub async fn list_devices(&self) -> Result<Vec<SimctlDevice>, SimctlError> {
        let raw = simctl_run(&["list", "devices", "-j"]).await?;
        #[derive(Deserialize)]
        struct Wrap {
            devices: std::collections::BTreeMap<String, Vec<RawDevice>>,
        }
        #[derive(Deserialize)]
        struct RawDevice {
            udid: String,
            name: String,
            state: String,
            #[serde(rename = "isAvailable", default)]
            is_available: bool,
            #[serde(rename = "deviceTypeIdentifier", default)]
            device_type_identifier: String,
        }
        let w: Wrap = serde_json::from_str(&raw).map_err(|e| SimctlError::Malformed {
            subcommand: "list devices".into(),
            detail: e.to_string(),
        })?;
        let mut out = Vec::new();
        for (runtime_id, devices) in w.devices {
            for d in devices {
                out.push(SimctlDevice {
                    udid: d.udid,
                    name: d.name,
                    state: d.state,
                    is_available: d.is_available,
                    device_type_identifier: d.device_type_identifier,
                    runtime_identifier: runtime_id.clone(),
                });
            }
        }
        Ok(out)
    }

    // ---- lifecycle ------------------------------------------------------

    /// `xcrun simctl boot <udid>` — fire-and-forget boot request.
    pub async fn boot(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["boot", udid]).await?;
        Ok(())
    }

    /// `xcrun simctl shutdown <udid>`.
    pub async fn shutdown(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["shutdown", udid]).await?;
        Ok(())
    }

    /// Read the sim's current BCP-47 locale (first entry of
    /// `NSGlobalDomain AppleLanguages`). Returns `Ok(None)` when the
    /// preference is unset (defaults read exits non-zero) or unparseable.
    /// Wire format: `simctl spawn <udid> defaults read -g AppleLanguages`
    /// stdout looks like `"(\n    \"en-US\"\n)\n"`; we extract the first
    /// quoted token.
    pub async fn current_locale(&self, udid: &str) -> Result<Option<String>, SimctlError> {
        let out =
            match simctl_run(&["spawn", udid, "/usr/bin/defaults", "read", "-g", "AppleLanguages"]).await {
                Ok(s) => s,
                // `defaults read` returns non-zero when the key is unset; that
                // is a legitimate "no opinion" state, not an error.
                Err(SimctlError::NonZeroExit { .. }) => return Ok(None),
                Err(e) => return Err(e),
            };
        // First quoted substring.
        if let Some(start) = out.find('"') {
            let rest = &out[start + 1..];
            if let Some(end) = rest.find('"') {
                return Ok(Some(rest[..end].to_string()));
            }
        }
        Ok(None)
    }

    /// Write `AppleLanguages` (array) + `AppleLocale` (scalar) to the
    /// sim's NSGlobalDomain so SpringBoard + apps re-localize on next
    /// launch. AppleLocale is BCP-47 with hyphen replaced by underscore
    /// (`en_US`); AppleLanguages is the BCP-47 tag verbatim.
    /// **The caller must shutdown + reboot the sim for the change to
    /// take effect** — running apps cache the locale at process start.
    pub async fn set_locale(&self, udid: &str, locale: &str) -> Result<(), SimctlError> {
        simctl_run(&[
            "spawn",
            udid,
            "defaults",
            "write",
            "-g",
            "AppleLanguages",
            "-array",
            locale,
        ])
        .await?;
        let locale_underscore = locale.replace('-', "_");
        simctl_run(&[
            "spawn",
            udid,
            "defaults",
            "write",
            "-g",
            "AppleLocale",
            &locale_underscore,
        ])
        .await?;
        Ok(())
    }

    /// Boot + poll device state == "Booted" within timeout. Tries every
    /// 500 ms until success or `timeout_ms` elapses. Idempotent on
    /// already-booted devices (`xcrun simctl boot` returns non-zero when
    /// the device is already booted; we swallow that).
    pub async fn boot_and_wait(&self, udid: &str, timeout: Duration) -> Result<(), SimctlError> {
        // Issue boot; ignore already-booted error (the only friendly path).
        let _ = simctl_run(&["boot", udid]).await;
        let start = std::time::Instant::now();
        loop {
            let devices = self.list_devices().await?;
            if devices
                .iter()
                .any(|d| d.udid == udid && d.state == "Booted")
            {
                return Ok(());
            }
            if start.elapsed() > timeout {
                return Err(SimctlError::Timeout {
                    subcommand: format!("boot {}", udid),
                    ms: timeout.as_millis() as u64,
                });
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    /// `xcrun simctl erase <udid>` — wipe device contents.
    pub async fn erase(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["erase", udid]).await?;
        Ok(())
    }

    /// `xcrun simctl install <udid> <app-path>` — install a `.app` bundle.
    pub async fn install(&self, udid: &str, app_path: &str) -> Result<(), SimctlError> {
        simctl_run(&["install", udid, app_path]).await?;
        Ok(())
    }

    /// `xcrun simctl uninstall <udid> <bundle-id>`.
    pub async fn uninstall(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        simctl_run(&["uninstall", udid, bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl terminate <udid> <bundle-id>` — kill a running app.
    pub async fn terminate(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        simctl_run(&["terminate", udid, bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl launch <udid> <bundleId>` → parse `"<bundle>: <pid>"`.
    pub async fn launch(&self, udid: &str, bundle_id: &str) -> Result<LaunchResult, SimctlError> {
        self.launch_with_args(udid, bundle_id, &[]).await
    }

    /// `xcrun simctl launch <udid> <bundleId> -- <arg>...` — launch with a
    /// process-level argument vector. Empty `args` is equivalent to
    /// [`Self::launch`]. Mirrors maestro yaml `launchApp.arguments`.
    pub async fn launch_with_args(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
    ) -> Result<LaunchResult, SimctlError> {
        self.launch_with_args_and_env(udid, bundle_id, args, &[])
            .await
    }

    /// Like [`Self::launch_with_args`] but also sets `SIMCTL_CHILD_*`
    /// envp on the simctl process so the launched app can read
    /// deploy-time vars via `ProcessInfo().environment["KEY"]`.
    /// `child_env` keys without the `SIMCTL_CHILD_` prefix get it added
    /// automatically (per [`compose_child_env`] semantics). Useful for
    /// prelaunching an app before any `openLink` so iOS treats the
    /// subsequent URL handoff as in-app routing instead of cross-app,
    /// side-stepping the SpringBoard "Open in '`<App>`'?" confirmation
    /// dialog.
    pub async fn launch_with_args_and_env(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
        child_env: &[(&str, &str)],
    ) -> Result<LaunchResult, SimctlError> {
        let mut argv: Vec<&str> = vec!["launch", udid, bundle_id];
        if !args.is_empty() {
            argv.push("--");
            for a in args {
                argv.push(a.as_str());
            }
        }
        let composed = compose_child_env(child_env);
        let out = simctl_run_env(&argv, &composed).await?;
        // Output format: `com.example.app: 12345\n`
        let pid_str =
            out.rsplit(':')
                .next()
                .map(str::trim)
                .ok_or_else(|| SimctlError::Malformed {
                    subcommand: "launch".into(),
                    detail: format!("unexpected stdout shape: {}", out.trim()),
                })?;
        let pid: u32 = pid_str.parse().map_err(|_| SimctlError::Malformed {
            subcommand: "launch".into(),
            detail: format!("non-numeric pid in stdout: {}", out.trim()),
        })?;
        Ok(LaunchResult { pid })
    }

    /// v1.0.4 §D12 — reset every privacy permission granted to
    /// `bundle_id` on the sim: `xcrun simctl privacy <udid> reset all
    /// <bundle-id>`. Companion to [`Self::clear_app_sandbox`] on the
    /// in-place `launchApp: clearState: true` path that replaces
    /// `simctl uninstall + install` (which triggers iOS 26.5 XCUITest
    /// binding loss + ReportCrash's "Insight quit unexpectedly" dialog).
    pub async fn privacy_reset_all(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["privacy", udid, "reset", "all", bundle_id]).await?;
        Ok(())
    }

    /// v1.0.4 §D12 — wipe the app's sandbox on the sim: locate the
    /// Data container via `simctl get_app_container <udid> <bundle>
    /// data`, then `simctl spawn <udid> rm -rf <container>/Documents
    /// <container>/Library <container>/tmp`. The app remains installed
    /// (no `simctl uninstall`), so the XCUITest binding is preserved
    /// and macOS `ReportCrash` does not misinterpret a missing
    /// install-receipt as a crash.
    pub async fn clear_app_sandbox(
        &self,
        udid: &str,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        let raw = simctl_run(&["get_app_container", udid, bundle_id, "data"]).await?;
        let container = raw.trim();
        if container.is_empty() {
            return Err(SimctlError::Malformed {
                subcommand: "clear_app_sandbox".into(),
                detail: format!("empty Data container path for bundle {bundle_id}"),
            });
        }
        let documents = format!("{container}/Documents");
        let library = format!("{container}/Library");
        let tmp = format!("{container}/tmp");
        // v1.0.7 §D1 — `xcrun simctl spawn <UDID> <cmd>` uses
        // `posix_spawn` inside the sim OS; `<cmd>` must be an absolute
        // path (no PATH resolution). Prior versions passed `"rm"` and
        // hit `NSPOSIXErrorDomain code 2: No such file or directory`
        // on iOS 17+ sims. `/bin/rm` is present on every stock sim
        // image.
        //
        // Best-effort: any missing subdir is fine (fresh app that never
        // wrote to that path). `rm -rf` treats absent targets as no-ops.
        simctl_run(&[
            "spawn", udid, "/bin/rm", "-rf", &documents, &library, &tmp,
        ])
        .await?;
        Ok(())
    }

    /// `xcrun simctl openurl <udid> <url>` — open a URL on the device.
    ///
    /// **URL bytes are passed to `xcrun simctl` verbatim** — no
    /// parsing, no percent-encoding rewrite, no query-string
    /// stripping. Verified by [`openurl_argv`] (test-visible helper)
    /// and its unit test asserting query-params like
    /// `?url=http%3A%2F%2Flocalhost%3A8081` reach the argv byte-for-byte.
    /// Feedback §G verification — if the target app's URL router
    /// (e.g. expo-dev-client 57.0.5) shows a picker instead of
    /// auto-connecting, the URL made it through smix intact and the
    /// finding lives on the URL-router side.
    pub async fn open_url(&self, udid: &str, url: &str) -> Result<(), SimctlError> {
        let argv = openurl_argv(udid, url);
        let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
        simctl_run(&refs).await?;
        Ok(())
    }
}

/// v1.0.4 §G — argv construction for `xcrun simctl openurl`. Extracted
/// as a test-visible helper so the URL-preservation contract is
/// unit-testable without invoking `xcrun`.
#[doc(hidden)]
pub fn openurl_argv(udid: &str, url: &str) -> [String; 3] {
    ["openurl".to_string(), udid.to_string(), url.to_string()]
}

impl SimctlClient {

    /// `xcrun simctl push <udid> <bundle-id> <apns-json-path>`.
    /// Deliver an APNS payload to a sim-installed app. The payload file is
    /// a JSON document whose top-level dictionary mirrors what an APNS
    /// provider would send; `aps.alert.body` / `aps.alert.title` surface
    /// as banner content and reach the app's
    /// `UNUserNotificationCenterDelegate`.
    pub async fn send_push(
        &self,
        udid: &str,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["push", udid, bundle_id, apns_json_path]).await?;
        Ok(())
    }

    /// `xcrun simctl ui <udid> appearance <light|dark>` — set UI appearance.
    pub async fn set_appearance(&self, udid: &str, mode: Appearance) -> Result<(), SimctlError> {
        simctl_run(&["ui", udid, "appearance", mode.as_str()]).await?;
        Ok(())
    }

    /// `xcrun simctl privacy <udid> grant <perm> <bundle-id>`.
    pub async fn grant_permission(
        &self,
        udid: &str,
        permission: SimctlPermission,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["privacy", udid, "grant", permission.as_str(), bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl privacy <udid> revoke <perm> <bundle-id>` — explicitly
    /// deny the permission. Mirrors maestro yaml `permissions: { x: deny }`
    /// (the reverse of `grant`). Distinct from `reset`, which returns the
    /// permission to "not determined".
    pub async fn revoke_permission(
        &self,
        udid: &str,
        permission: SimctlPermission,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["privacy", udid, "revoke", permission.as_str(), bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl location <udid> set <lat>,<lng>` — set sim location
    /// to a fixed point. Mirrors maestro `setLocation`.
    pub async fn location_set(
        &self,
        udid: &str,
        latitude: f64,
        longitude: f64,
    ) -> Result<(), SimctlError> {
        let coord = format!("{latitude},{longitude}");
        simctl_run(&["location", udid, "set", &coord]).await?;
        Ok(())
    }

    /// `xcrun simctl location <udid> start [--speed=<m/s>] <waypoints>`
    /// — interpolate sim location along waypoints. Fire-and-return: simctl
    /// injects scenario and returns; sim continues interpolation in background.
    /// Mirrors maestro `travel`.
    pub async fn location_start(
        &self,
        udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), SimctlError> {
        if points.len() < 2 {
            return Err(SimctlError::Malformed {
                subcommand: "location-start".into(),
                detail: format!("requires ≥2 waypoints, got {}", points.len()),
            });
        }
        let mut args: Vec<String> = vec!["location".into(), udid.into(), "start".into()];
        if let Some(s) = speed_mps {
            args.push(format!("--speed={s}"));
        }
        for (lat, lng) in points {
            args.push(format!("{lat},{lng}"));
        }
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        simctl_run(&args_ref).await?;
        Ok(())
    }

    /// `xcrun simctl location <udid> clear` — reset active location
    /// scenario.
    pub async fn location_clear(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["location", udid, "clear"]).await?;
        Ok(())
    }

    /// `xcrun simctl addmedia <udid> <path>...` — add photos / videos /
    /// contacts to sim library. Mirrors maestro `addMedia` (scalar or
    /// array form already flattened on adapter side).
    pub async fn add_media(&self, udid: &str, paths: &[String]) -> Result<(), SimctlError> {
        if paths.is_empty() {
            return Err(SimctlError::Malformed {
                subcommand: "addmedia".into(),
                detail: "no paths supplied".into(),
            });
        }
        let mut args: Vec<&str> = vec!["addmedia", udid];
        for p in paths {
            args.push(p.as_str());
        }
        simctl_run(&args).await?;
        Ok(())
    }

    /// Start recording sim display to `path`. Spawns
    /// `xcrun simctl io <udid> recordVideo <path>` as a long-running child;
    /// returns handle immediately. Caller must pair with
    /// [`Self::record_video_stop`] for clean SIGINT-and-wait shutdown —
    /// dropping the handle would SIGKILL via tokio + lose mp4 trailer.
    pub async fn record_video_start(
        &self,
        udid: &str,
        path: &str,
    ) -> Result<RecordingHandle, SimctlError> {
        let child = tokio::process::Command::new("xcrun")
            .args(["simctl", "io", udid, "recordVideo", path])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        // brief settle for simctl to initialize encoder + open output file.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok(RecordingHandle {
            child,
            path: path.to_string(),
            started_at: std::time::Instant::now(),
        })
    }

    /// Stop a recording via SIGINT + wait (≤10s). SIGINT lets simctl
    /// trap and flush the mp4 trailer; SIGKILL would corrupt output.
    /// Timeout escalates to SIGKILL with explicit error mentioning truncation.
    pub async fn record_video_stop(&self, mut handle: RecordingHandle) -> Result<(), SimctlError> {
        let pid = handle.child.id().ok_or_else(|| SimctlError::Malformed {
            subcommand: "recordVideo-stop".into(),
            detail: "child already reaped".into(),
        })?;
        // SAFETY: libc::kill is a thin POSIX syscall wrapper; pid is owned by
        // this Child instance (no race) and SIGINT is signal-safe.
        let rc = unsafe { libc::kill(pid as i32, libc::SIGINT) };
        if rc != 0 {
            return Err(SimctlError::Malformed {
                subcommand: "recordVideo-stop".into(),
                detail: format!(
                    "kill SIGINT failed: errno={}",
                    std::io::Error::last_os_error()
                ),
            });
        }
        let wait_result =
            tokio::time::timeout(std::time::Duration::from_secs(10), handle.child.wait()).await;
        match wait_result {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(e)) => Err(SimctlError::Malformed {
                subcommand: "recordVideo-stop".into(),
                detail: format!("wait failed: {e}"),
            }),
            Err(_timeout) => {
                let _ = handle.child.kill().await;
                Err(SimctlError::Malformed {
                    subcommand: "recordVideo-stop".into(),
                    detail: "SIGINT timeout (10s) — escalated SIGKILL; output mp4 likely truncated. Inspect simctl recordVideo stderr.".into(),
                })
            }
        }
    }

    /// `xcrun simctl privacy <udid> reset <perm> <bundle-id>` — return the
    /// permission to "not determined" so the next request re-prompts.
    /// May terminate a running instance of the target app (Apple
    /// behavior) — call before launch, not mid-flow.
    pub async fn reset_permission(
        &self,
        udid: &str,
        permission: SimctlPermission,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["privacy", udid, "reset", permission.as_str(), bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl keychain <udid> reset` — clear all keychain entries.
    pub async fn keychain_reset(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["keychain", udid, "reset"]).await?;
        Ok(())
    }

    /// `xcrun simctl pbpaste <udid>` — read clipboard contents.
    pub async fn pasteboard_get(&self, udid: &str) -> Result<String, SimctlError> {
        simctl_run(&["pbpaste", udid]).await
    }

    /// `xcrun simctl pbcopy <udid>` — write clipboard contents (via piped stdin).
    pub async fn pasteboard_set(&self, udid: &str, text: &str) -> Result<(), SimctlError> {
        // pbcopy reads stdin — we pipe via shell echo for simplicity.
        // Long-term: spawn with stdin pipe.
        use tokio::io::AsyncWriteExt;
        let mut cmd = Command::new("xcrun");
        cmd.arg("simctl").arg("pbcopy").arg(udid);
        cmd.stdin(std::process::Stdio::piped());
        let mut child = cmd.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).await?;
            drop(stdin); // close stdin so pbcopy returns
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(SimctlError::NonZeroExit {
                subcommand: "pbcopy".into(),
                argv: vec!["pbcopy".to_string()],
                code: status.code().unwrap_or(-1),
                stderr: String::new(),
                wall_ms: 0,
            });
        }
        Ok(())
    }

    /// Toggle "Reduce Motion" accessibility setting via `defaults write`.
    pub async fn set_reduce_motion(&self, udid: &str, enabled: bool) -> Result<(), SimctlError> {
        let val = if enabled { "1" } else { "0" };
        // `defaults write` lives under spawn; routed via simctl spawn.
        simctl_run(&[
            "spawn",
            udid,
            "defaults",
            "write",
            "com.apple.UIKit",
            "UIAccessibilityReduceMotionEnabled",
            "-bool",
            val,
        ])
        .await?;
        Ok(())
    }

    /// `xcrun simctl io <udid> screenshot <tmpfile>` → raw PNG bytes,
    /// with a byte-level sRGB metadata splice if the produced PNG lacks
    /// an `sRGB` chunk.
    ///
    /// Goes through a temp file: current Xcode's `screenshot -` does not
    /// treat `-` as stdout — it writes a literal file named `-` in cwd
    /// and emits nothing on stdout (observed on Xcode/iOS 26.5).
    ///
    /// **Pixel-preservation invariant**: the returned bytes are
    /// byte-identical to whatever `simctl io screenshot` wrote to disk
    /// EXCEPT for one narrow case — if the PNG does not carry an
    /// `sRGB` ancillary chunk (observed on iOS 26.5 sub-builds
    /// mid-2026), a 13-byte `sRGB` chunk is spliced in immediately
    /// before the first `IDAT`. Pixel data (IDAT bytes) is never
    /// decoded or modified. See [`ensure_srgb_chunk`] for the exact
    /// splice operation.
    pub async fn screenshot(&self, udid: &str) -> Result<Vec<u8>, SimctlError> {
        // v1.0.4 — pace + circuit-check before invoking simctl.
        let wait = {
            let mut pacer = self
                .screenshot_pacer
                .lock()
                .expect("screenshot pacer mutex must not be poisoned");
            pacer
                .compute_wait()
                .map_err(|retry_after| SimctlError::CaptureBackpressure { retry_after })?
        };
        if !wait.is_zero() {
            sleep(wait).await;
        }

        let call_start = std::time::Instant::now();
        let tmp =
            std::env::temp_dir().join(format!("smix-screenshot-{udid}-{}.png", std::process::id()));
        let tmp_str = tmp.display().to_string();
        let result = simctl_capture(&["io", udid, "screenshot", &tmp_str]).await;
        let bytes = result.and_then(|_| {
            std::fs::read(&tmp).map_err(|e| SimctlError::Malformed {
                subcommand: "screenshot".into(),
                detail: format!("read {tmp_str}: {e}"),
            })
        });
        let _ = std::fs::remove_file(&tmp);

        let wall = call_start.elapsed();
        let failed = bytes.is_err();
        {
            let mut pacer = self
                .screenshot_pacer
                .lock()
                .expect("screenshot pacer mutex must not be poisoned");
            pacer.record(wall, failed);
        }

        let bytes = bytes?;
        if bytes.len() < 8 {
            return Err(SimctlError::Malformed {
                subcommand: "screenshot".into(),
                detail: format!("screenshot file too short: {} bytes", bytes.len()),
            });
        }
        Ok(ensure_srgb_chunk(bytes))
    }

    /// `xcrun simctl create <name> <device-type-id> <runtime-id>` → udid.
    pub async fn create_device(
        &self,
        name: &str,
        device_type: &str,
        runtime_id: &str,
    ) -> Result<String, SimctlError> {
        let out = simctl_run(&["create", name, device_type, runtime_id]).await?;
        Ok(out.trim().to_string())
    }

    /// `xcrun simctl delete <udid>` — delete a simulator device.
    pub async fn delete_device(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["delete", udid]).await?;
        Ok(())
    }
}

// -------------------- PNG sRGB chunk normalization (v1.0.2) --------------------
//
// iOS 26.5 sub-builds (mid-2026) started omitting the `sRGB` ancillary
// chunk from `simctl io screenshot` output. macOS Preview.app and other
// viewers that fall back to Display P3 when no ICC profile is embedded
// then over-saturate the image (red gets pushed, text anti-alias picks
// up yellow fringing).
//
// This does NOT affect pixel-comparison (dhash decodes IDAT to RGBA and
// ignores ancillary chunks), but does affect any downstream tool that
// renders the PNG for human review. The normalizer runs on the raw byte
// stream — walks chunks, and if no `sRGB` chunk is seen before the first
// `IDAT`, splices in a synthesized 13-byte `sRGB` chunk (length=1,
// type="sRGB", data=[0 = perceptual intent], CRC over type+data).
//
// Pixel-preservation invariant: IDAT bytes are never decoded. Every
// existing chunk is copied verbatim. Only 13 bytes of new metadata are
// inserted.

const PNG_MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Ensure the PNG carries an `sRGB` ancillary chunk. Called on the raw
/// bytes returned by `xcrun simctl io <udid> screenshot`. If the PNG
/// already has an `sRGB` chunk, returns the input unchanged; otherwise
/// splices in a 13-byte `sRGB` chunk (rendering intent = 0, perceptual)
/// immediately before the first `IDAT`. Returns the input unchanged on
/// any structural anomaly (missing magic, malformed chunk) so a
/// corrupted PNG is passed through untouched for the caller to diagnose.
pub fn ensure_srgb_chunk(bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() < 8 || &bytes[..8] != PNG_MAGIC {
        return bytes;
    }
    let Some((idat_offset, has_srgb)) = scan_png_chunks(&bytes) else {
        return bytes;
    };
    if has_srgb {
        return bytes;
    }
    // Splice the synthesized sRGB chunk right before the first IDAT.
    let mut out = Vec::with_capacity(bytes.len() + 13);
    out.extend_from_slice(&bytes[..idat_offset]);
    out.extend_from_slice(&synthesized_srgb_chunk());
    out.extend_from_slice(&bytes[idat_offset..]);
    out
}

/// Walk PNG chunks starting after the 8-byte magic. Returns
/// `(offset_of_first_IDAT, has_srgb_chunk_before_it)` when the walk
/// reaches an IDAT chunk. Returns `None` if the walk hits EOF or a
/// malformed chunk without seeing an IDAT.
fn scan_png_chunks(bytes: &[u8]) -> Option<(usize, bool)> {
    let mut i: usize = 8;
    let mut has_srgb = false;
    while i + 8 <= bytes.len() {
        let length =
            u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let ctype = &bytes[i + 4..i + 8];
        if ctype == b"IDAT" {
            return Some((i, has_srgb));
        }
        if ctype == b"sRGB" {
            has_srgb = true;
        }
        // 4 (length) + 4 (type) + length (data) + 4 (crc)
        let end = i.checked_add(12)?.checked_add(length)?;
        if end > bytes.len() {
            return None;
        }
        i = end;
    }
    None
}

/// Build the 13-byte `sRGB` chunk with rendering intent = 0 (perceptual).
/// Format: `[len:4][type:4][data:1][crc:4]` = 13 bytes total.
fn synthesized_srgb_chunk() -> [u8; 13] {
    // The CRC is computed over `type || data`.
    let mut crc_input = [0u8; 5];
    crc_input[0..4].copy_from_slice(b"sRGB");
    crc_input[4] = 0; // perceptual
    let crc = crc32_ieee(&crc_input);
    let mut chunk = [0u8; 13];
    chunk[0..4].copy_from_slice(&1u32.to_be_bytes()); // length = 1 (data byte)
    chunk[4..8].copy_from_slice(b"sRGB");
    chunk[8] = 0;
    chunk[9..13].copy_from_slice(&crc.to_be_bytes());
    chunk
}

/// Table-less CRC-32 IEEE 802.3 (polynomial 0xEDB88320) as used by
/// PNG. Small enough for this crate's single call site — avoids
/// pulling in a `crc32fast` dependency.
fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_child_env_adds_prefix() {
        let composed = compose_child_env(&[
            ("SMIX_PERF_RECEIVER_URL", "http://127.0.0.1:9999"),
            ("LAUNCH_FORCE_PUSH", "true"),
        ]);
        assert_eq!(
            composed,
            vec![
                (
                    "SIMCTL_CHILD_SMIX_PERF_RECEIVER_URL".to_string(),
                    "http://127.0.0.1:9999".to_string(),
                ),
                (
                    "SIMCTL_CHILD_LAUNCH_FORCE_PUSH".to_string(),
                    "true".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn compose_child_env_already_prefixed_passes_through() {
        // Defensive: caller may pre-prefix; we must not double-prefix.
        let composed = compose_child_env(&[("SIMCTL_CHILD_FOO", "bar")]);
        assert_eq!(
            composed,
            vec![("SIMCTL_CHILD_FOO".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn compose_child_env_empty_input_is_empty_output() {
        assert!(compose_child_env(&[]).is_empty());
    }

    // -- v1.0.4 §G openurl URL preservation -----------------------------

    #[test]
    fn openurl_argv_preserves_url_verbatim() {
        let udid = "12345678-1234-5678-1234-567812345678";
        let url = "exp+focus-ai-app://expo-development-client/?url=http%3A%2F%2Flocalhost%3A8081";
        let argv = super::openurl_argv(udid, url);
        assert_eq!(argv[0], "openurl");
        assert_eq!(argv[1], udid);
        // Byte-identical URL — no percent-decoding, no query-strip.
        assert_eq!(argv[2], url);
        assert!(argv[2].contains("?url="));
        assert!(argv[2].contains("%3A"));
        assert!(argv[2].contains("%2F"));
    }

    #[test]
    fn openurl_argv_preserves_ampersand_and_hash() {
        let udid = "12345678-1234-5678-1234-567812345678";
        let url = "insight://dev-mutate?action=env&value=staging#anchor";
        let argv = super::openurl_argv(udid, url);
        assert_eq!(argv[2], url);
        assert!(argv[2].contains('&'));
        assert!(argv[2].contains('#'));
    }

    #[test]
    fn openurl_argv_preserves_unicode() {
        let udid = "12345678-1234-5678-1234-567812345678";
        let url = "insight://route?name=%E7%94%B0%E4%B8%AD";
        let argv = super::openurl_argv(udid, url);
        assert_eq!(argv[2], url);
    }

    // -- sRGB chunk normalization ---------------------------------------

    /// Build a minimal PNG: 1×1 8-bit RGBA, one IDAT (zlib-empty-safe),
    /// with or without an sRGB chunk. Returns synthetic bytes suitable
    /// for exercising the chunk-walking logic; no rendering intent.
    fn synth_png(with_srgb: bool) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(super::PNG_MAGIC);
        // IHDR: 1x1, bit_depth=8, color_type=6 (RGBA), rest=0
        let ihdr_data: [u8; 13] = [
            0, 0, 0, 1, // width = 1
            0, 0, 0, 1, // height = 1
            8, // bit depth
            6, // color type = RGBA
            0, 0, 0,
        ];
        emit_chunk(&mut out, b"IHDR", &ihdr_data);
        if with_srgb {
            emit_chunk(&mut out, b"sRGB", &[0]);
        }
        // Placeholder IDAT — content doesn't matter for chunk-walking tests
        emit_chunk(&mut out, b"IDAT", &[0x78, 0x01, 0x00, 0x00]);
        emit_chunk(&mut out, b"IEND", &[]);
        out
    }

    fn emit_chunk(out: &mut Vec<u8>, ctype: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(ctype);
        out.extend_from_slice(data);
        let mut crc_in = Vec::with_capacity(4 + data.len());
        crc_in.extend_from_slice(ctype);
        crc_in.extend_from_slice(data);
        out.extend_from_slice(&super::crc32_ieee(&crc_in).to_be_bytes());
    }

    #[test]
    fn ensure_srgb_passthrough_when_chunk_present() {
        let png = synth_png(true);
        let original_len = png.len();
        let out = super::ensure_srgb_chunk(png.clone());
        assert_eq!(out.len(), original_len);
        assert_eq!(out, png);
    }

    #[test]
    fn ensure_srgb_inserts_chunk_when_absent() {
        let png = synth_png(false);
        let original_len = png.len();
        let out = super::ensure_srgb_chunk(png);
        assert_eq!(out.len(), original_len + 13);
        // First 8 bytes = PNG magic
        assert_eq!(&out[..8], super::PNG_MAGIC);
        // Search for the injected sRGB chunk
        let mut found = false;
        for w in out.windows(4) {
            if w == b"sRGB" {
                found = true;
                break;
            }
        }
        assert!(found, "sRGB chunk should have been spliced in");
    }

    #[test]
    fn ensure_srgb_preserves_idat_bytes_verbatim() {
        // Any pixel corruption at the IDAT level would break the
        // pixel-preservation invariant. Extract IDAT payload from
        // input and output, assert byte-identical.
        let png = synth_png(false);
        let out = super::ensure_srgb_chunk(png.clone());
        assert_eq!(extract_idat_data(&png), extract_idat_data(&out));
    }

    fn extract_idat_data(bytes: &[u8]) -> Vec<u8> {
        let mut i = 8;
        while i + 8 <= bytes.len() {
            let length =
                u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
            let ctype = &bytes[i + 4..i + 8];
            if ctype == b"IDAT" {
                return bytes[i + 8..i + 8 + length].to_vec();
            }
            i += 12 + length;
        }
        vec![]
    }

    #[test]
    fn ensure_srgb_passthrough_on_bad_magic() {
        // Corrupted / non-PNG input must not be modified.
        let bytes = vec![0u8; 32];
        let out = super::ensure_srgb_chunk(bytes.clone());
        assert_eq!(out, bytes);
    }

    #[test]
    fn crc32_matches_known_iend() {
        // The empty-data IEND CRC is a well-known constant.
        // CRC over "IEND" alone: 0xAE_42_60_82.
        assert_eq!(super::crc32_ieee(b"IEND"), 0xAE42_6082);
    }
}
