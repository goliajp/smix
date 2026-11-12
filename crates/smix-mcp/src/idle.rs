//! Giving a device back when the conversation driving it has gone quiet.
//!
//! An MCP server lives as long as the client that launched it, and a
//! client stays open long after anyone last asked it to touch a device.
//! Its lease stayed with it, so a `smix run` against that device hours
//! later was refused flow after flow by a session nobody was using.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Environment variable naming how long a bound device may go without a
/// command before the server gives it back.
pub const IDLE_ENV: &str = "SMIX_MCP_IDLE_RELEASE";

/// Used when [`IDLE_ENV`] is unset.
pub const DEFAULT_IDLE: Duration = Duration::from_secs(30 * 60);

/// How often the ledger's heartbeat is rewritten while commands keep
/// coming; a refusal reports idle time, and a minute is finer than that.
pub const BEAT_EVERY: Duration = Duration::from_secs(60);

/// The idle limit [`IDLE_ENV`] asks for: `None` keeps the device for as
/// long as the server runs (`off` or `0`), otherwise a number with `s`,
/// `m` or `h`.
///
/// # Errors
///
/// Anything else, worded for the server's stderr.
pub fn limit_from(raw: Option<&str>) -> Result<Option<Duration>, String> {
    let Some(raw) = raw.map(str::trim) else {
        return Ok(Some(DEFAULT_IDLE));
    };
    if raw == "off" || raw == "0" {
        return Ok(None);
    }
    let bad = || format!("{IDLE_ENV}={raw:?}: expected `off`, `0`, or a number with s, m or h");
    let unit = match raw.chars().last() {
        Some('s') => 1,
        Some('m') => 60,
        Some('h') => 3600,
        _ => return Err(bad()),
    };
    let n: u64 = raw[..raw.len() - 1].parse().map_err(|_| bad())?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(Duration::from_secs(n * unit)))
}

/// When the session last served a command, and last wrote the ledger.
#[derive(Debug)]
pub struct Activity {
    last_command: Mutex<Instant>,
    last_beat: Mutex<Option<Instant>>,
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            last_command: Mutex::new(Instant::now()),
            last_beat: Mutex::new(None),
        }
    }
}

impl Activity {
    /// A command arrived now.
    pub fn touch(&self) {
        *self.last_command.lock().expect("activity lock") = Instant::now();
    }

    /// How long since the last command.
    #[must_use]
    pub fn idle(&self) -> Duration {
        self.last_command.lock().expect("activity lock").elapsed()
    }

    /// Whether the ledger's heartbeat is due, marking it written when it
    /// is. The first call after [`Self::reset_beat`] is always due.
    pub fn beat_due(&self) -> bool {
        let mut last = self.last_beat.lock().expect("activity lock");
        if last.is_some_and(|t| t.elapsed() < BEAT_EVERY) {
            return false;
        }
        *last = Some(Instant::now());
        true
    }

    /// Forget the last beat, for a newly bound device.
    pub fn reset_beat(&self) {
        *self.last_beat.lock().expect("activity lock") = None;
    }
}

/// What a tool says after the device was given back for being idle.
#[must_use]
pub fn released_note(udid: &str, idle: Duration) -> String {
    format!(
        "{udid} was given back after {} with no command ({IDLE_ENV}, 30m unless set), so \
         other sessions could use it — call smix_use again to drive it",
        smix_lease::beat::spoken(idle.as_secs())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_means_the_default() {
        assert_eq!(limit_from(None), Ok(Some(DEFAULT_IDLE)));
    }

    #[test]
    fn off_and_zero_keep_the_device() {
        for raw in ["off", "0", "0m", " off "] {
            assert_eq!(limit_from(Some(raw)), Ok(None), "{raw}");
        }
    }

    #[test]
    fn units_are_seconds_minutes_hours() {
        assert_eq!(limit_from(Some("90s")), Ok(Some(Duration::from_secs(90))));
        assert_eq!(limit_from(Some("45m")), Ok(Some(Duration::from_secs(2700))));
        assert_eq!(limit_from(Some("2h")), Ok(Some(Duration::from_secs(7200))));
    }

    #[test]
    fn a_bare_number_or_a_word_is_refused() {
        for raw in ["30", "m", "thirty m", "-5m", "1d", ""] {
            let e = limit_from(Some(raw)).expect_err(raw);
            assert!(e.contains(IDLE_ENV), "{e}");
        }
    }

    #[test]
    fn the_beat_is_due_once_per_interval() {
        let a = Activity::default();
        assert!(a.beat_due());
        assert!(!a.beat_due());
        a.reset_beat();
        assert!(a.beat_due());
    }

    #[test]
    fn the_note_names_the_device_the_wait_and_the_way_back() {
        let n = released_note("AAAA", Duration::from_secs(31 * 60));
        assert!(
            n.contains("AAAA") && n.contains("31m") && n.contains("smix_use"),
            "{n}"
        );
    }
}
