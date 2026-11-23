//! What `smix lease` says about a device, as pure functions so the
//! wording is testable — the phrasing is the product here.

use smix_lease::store::LedgerDivergence;
use smix_lease::{Admission, StaleReason};
use std::path::Path;

/// Render the verdict for one device.
///
/// Pure so the wording is testable — the phrasing is the product here,
/// not a detail of it.
pub fn describe(device_id: &str, admission: &Admission) -> String {
    match admission {
        Admission::Granted => format!("{device_id}: free"),
        Admission::Denied(c) if c.holder_alive => format!(
            "{device_id}: held by pid {} ({}) since {}",
            c.holder.pid, c.holder.cmd, c.acquired_at
        ),
        Admission::Denied(c) => format!(
            "{device_id}: in use — the command that took it (pid {}) has exited, \
             but what it started is still running",
            c.holder.pid
        ),
        Admission::Adoptable => format!(
            "{device_id}: a finished session left its runner serving — the next \
             command takes the lease over as-is"
        ),
        Admission::Reclaimable { cleanup, reason } => {
            let why = match reason {
                StaleReason::HolderExited => "holder exited",
                StaleReason::PidRecycled => "holder gone, its pid was reused",
                StaleReason::HeartbeatExpired => "holder stopped responding",
            };
            format!(
                "{device_id}: abandoned ({why}) — {} close(s) owed",
                cleanup.len()
            )
        }
    }
}

/// Who holds a device, for a script: `null` when a new claim would be
/// granted, otherwise the pid and command that hold it and whether that
/// process is still alive.
///
/// The human line says the same thing in words. A script that parsed
/// those words, or read the stored lease and judged the holder's
/// liveness itself, would keep a second copy of `assess` (2026-09-26:
/// e2e scripts deciding whether to yield a device).
pub fn held_by_json(admission: &Admission) -> serde_json::Value {
    match admission {
        Admission::Denied(c) => serde_json::json!({
            "pid": c.holder.pid,
            "cmd": c.holder.cmd,
            "alive": c.holder_alive,
        }),
        Admission::Granted | Admission::Adoptable | Admission::Reclaimable { .. } => {
            serde_json::Value::Null
        }
    }
}

/// The note for one device the tree's old book records differently.
///
/// Pure so the wording is testable. It says when the tree's copy was
/// last written, and never that something is writing it: a book frozen
/// since the ledgers moved disagrees with the machine's on every device
/// it names, forever, and "Something still writes there" turned that
/// into a claim about a live writer with nothing behind it.
pub fn divergence_note(
    d: &LedgerDivergence,
    checkout: &str,
    tree: &Path,
    leases: &str,
    last_written: Option<&str>,
) -> String {
    match d {
        LedgerDivergence::OnlyInCheckout { device_id } => format!(
            "note: {device_id} has a ledger in {checkout} and none here — no other \
             checkout can see it. `smix lease migrate --from {}` brings it over.",
            tree.display()
        ),
        LedgerDivergence::Disagrees { device_id, detail } => {
            let when = match last_written {
                Some(t) => format!("That copy was last written {t}"),
                None => "When it was last written could not be read".to_string(),
            };
            format!(
                "note: {device_id} is recorded differently in {checkout} — {detail}. \
                 {when}; this smix only reads it. This command answers from \
                 {leases} alone."
            )
        }
    }
}
