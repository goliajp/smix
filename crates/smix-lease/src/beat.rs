//! A holder saying it is still using the device, and how long ago it last
//! did.
//!
//! The heartbeat is not a liveness test — a live holder is never
//! preempted on its account, see `assess`. It is what a refusal can tell
//! the person who was refused: a holder that took a device a day ago and
//! has not touched it since is a different conversation from one that is
//! mid-run.

use crate::store::{self, LeaseDir, LeaseError};

/// Mark the lease on `device_id` as used just now, when this process
/// holds it.
///
/// Returns whether anything was written. A lease held by somebody else is
/// left alone: their heartbeat is theirs to write, and a beat from here
/// would tell the next refused caller that the device was busy when this
/// process was the one being busy.
///
/// # Errors
///
/// The ledger could not be read or written.
pub fn beat(dir: &LeaseDir, device_id: &str) -> Result<bool, LeaseError> {
    let Some(mut lease) = store::read(dir, device_id)? else {
        return Ok(false);
    };
    if lease.holder.pid != std::process::id() {
        return Ok(false);
    }
    lease.heartbeat_at = store::now_rfc3339();
    store::write(dir, &lease)?;
    Ok(true)
}

/// Whole seconds from `heartbeat_at` to `now`, both RFC3339. `None` when
/// either does not parse or the heartbeat is in the future.
#[must_use]
pub fn idle_secs(heartbeat_at: &str, now: &str) -> Option<u64> {
    let at = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok();
    u64::try_from((at(now)? - at(heartbeat_at)?).num_seconds()).ok()
}

/// `secs` the way a person reads an idle time: `14h 3m`, `25m`, `40s`.
#[must_use]
pub fn spoken(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lease, ProcIdentity};

    fn lease(pid: u32) -> Lease {
        Lease {
            device_id: "AAAAAAAA-0000-4000-8000-000000000001".into(),
            holder: ProcIdentity {
                pid,
                started_at: "Mon Nov  9 00:00:00 2026".into(),
                cmd: "smix-mcp".into(),
            },
            acquired_at: "2026-11-09T00:00:00Z".into(),
            heartbeat_at: "2026-11-09T00:00:00Z".into(),
            resources: Vec::new(),
        }
    }

    fn dir() -> (tempfile::TempDir, LeaseDir) {
        let t = tempfile::tempdir().expect("tempdir");
        let d = LeaseDir::at(t.path());
        (t, d)
    }

    #[test]
    fn a_beat_moves_the_heartbeat_of_a_lease_this_process_holds() {
        let (_t, d) = dir();
        let l = lease(std::process::id());
        store::write(&d, &l).expect("write");
        assert!(beat(&d, &l.device_id).expect("beat"));
        let after = store::read(&d, &l.device_id).expect("read").expect("there");
        assert_ne!(after.heartbeat_at, l.heartbeat_at);
        assert_eq!(after.acquired_at, l.acquired_at);
    }

    #[test]
    fn a_beat_leaves_somebody_elses_lease_alone() {
        let (_t, d) = dir();
        let l = lease(std::process::id() + 1);
        store::write(&d, &l).expect("write");
        assert!(!beat(&d, &l.device_id).expect("beat"));
        let after = store::read(&d, &l.device_id).expect("read").expect("there");
        assert_eq!(after.heartbeat_at, l.heartbeat_at);
    }

    #[test]
    fn a_beat_with_no_lease_writes_nothing() {
        let (_t, d) = dir();
        assert!(!beat(&d, "AAAAAAAA-0000-4000-8000-000000000002").expect("beat"));
        assert!(d.device_ids().is_empty());
    }

    #[test]
    fn idle_is_counted_from_the_heartbeat() {
        assert_eq!(
            idle_secs("2026-11-11T11:26:59Z", "2026-11-12T01:30:00Z"),
            Some(14 * 3600 + 3 * 60 + 1)
        );
        assert_eq!(idle_secs("not a time", "2026-11-12T01:30:00Z"), None);
        assert_eq!(
            idle_secs("2026-11-12T01:30:01Z", "2026-11-12T01:30:00Z"),
            None
        );
    }

    #[test]
    fn idle_reads_as_hours_and_minutes() {
        assert_eq!(spoken(40), "40s");
        assert_eq!(spoken(25 * 60 + 9), "25m");
        assert_eq!(spoken(2 * 3600), "2h");
        assert_eq!(spoken(14 * 3600 + 3 * 60 + 1), "14h 3m");
    }
}
