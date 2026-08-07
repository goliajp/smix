//! `smix lease list | status | reconcile` — the ledger, said out loud.
//!
//! The ledger exists so that a session killed without a graceful path
//! gets one at the next startup. That only helps if a person can see what
//! it thinks: a mechanism that silently decides which processes to signal
//! is one nobody can trust or debug. These three verbs are that window.

use crate::LeaseAction;
use smix_lease::{Admission, StaleReason, store};
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

fn device_ids(root: &Path) -> Vec<String> {
    let dir = store::lease_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    ids.sort();
    ids
}

/// Run the subcommand.
pub fn run(root: &Path, action: LeaseAction) -> Result<(), crate::CliError> {
    match action {
        LeaseAction::List => {
            let ids = device_ids(root);
            if ids.is_empty() {
                println!(
                    "no device ledgers under {}",
                    store::lease_dir(root).display()
                );
                return Ok(());
            }
            for id in ids {
                let facts = store::collect_facts(root, &id).map_err(to_cli_error)?;
                println!("{}", describe(&id, &smix_lease::assess(&facts)));
            }
        }
        LeaseAction::Status { device } => {
            let udid = crate::resolve_device(&device)?;
            let facts = store::collect_facts(root, &udid).map_err(to_cli_error)?;
            let admission = smix_lease::assess(&facts);
            println!("{}", describe(&udid, &admission));
            if let Some(held) = &facts.existing {
                for r in &held.lease.resources {
                    println!("  open: {r:?}");
                }
            }
            if let Admission::Reclaimable { cleanup, .. } = &admission {
                for a in cleanup {
                    println!("  owed: {a:?}");
                }
                println!("run `smix lease reconcile {device}` to close them");
            }
        }
        LeaseAction::Reconcile { device } => {
            let udid = crate::resolve_device(&device)?;
            let facts = store::collect_facts(root, &udid).map_err(to_cli_error)?;
            match smix_lease::assess(&facts) {
                Admission::Granted => println!("{udid}: nothing to settle"),
                // A live session is not ours to end, and a command that
                // quietly ended one would make every other command in
                // this tool unsafe to run next to somebody's work.
                a @ Admission::Denied(_) => {
                    println!("{}", describe(&udid, &a));
                    println!("not touching it — a live session is not this command's to end");
                }
                Admission::Reclaimable { cleanup, reason } => {
                    println!(
                        "{}",
                        describe(
                            &udid,
                            &Admission::Reclaimable {
                                cleanup: cleanup.clone(),
                                reason,
                            }
                        )
                    );
                    let outcomes = smix_capsule::reconcile::execute(root, &cleanup);
                    let mut all_clean = true;
                    for o in &outcomes {
                        println!("  {}", o.line());
                        all_clean &= o.is_clean();
                    }
                    if all_clean {
                        store::remove(root, &udid).map_err(to_cli_error)?;
                        println!("{udid}: settled, ledger cleared");
                    } else {
                        // Keeping the ledger is the point: the next
                        // command must still see what did not close.
                        println!("{udid}: some closes failed — ledger kept so they stay visible");
                    }
                }
            }
        }
    }
    Ok(())
}

fn to_cli_error(e: store::LeaseError) -> crate::CliError {
    crate::CliError::Other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_lease::{CleanupAction, Contention, ProcIdentity};

    fn ident(pid: u32) -> ProcIdentity {
        ProcIdentity {
            pid,
            started_at: "Thu Aug  6 10:00:00 2026".into(),
            cmd: "smix run hello.yaml".into(),
        }
    }

    #[test]
    fn a_live_holder_is_named_not_just_reported_busy() {
        let msg = describe(
            "UDID-1",
            &Admission::Denied(Contention {
                holder: ident(4242),
                acquired_at: "2026-08-06T10:00:00Z".into(),
                holder_alive: true,
            }),
        );
        assert!(msg.contains("4242"), "the pid is what makes it actionable");
        assert!(msg.contains("smix run hello.yaml"));
    }

    #[test]
    fn an_exited_launcher_says_what_is_still_running() {
        // Reporting this as "held by pid N" would send someone looking
        // for a process that is not there.
        let msg = describe(
            "UDID-1",
            &Admission::Denied(Contention {
                holder: ident(4242),
                acquired_at: "2026-08-06T10:00:00Z".into(),
                holder_alive: false,
            }),
        );
        assert!(msg.contains("has exited"));
        assert!(msg.contains("still running"));
    }

    #[test]
    fn an_abandoned_device_says_why_and_how_much_is_owed() {
        let msg = describe(
            "UDID-1",
            &Admission::Reclaimable {
                cleanup: vec![CleanupAction::ShutdownSim {
                    udid: "UDID-1".into(),
                }],
                reason: StaleReason::PidRecycled,
            },
        );
        assert!(msg.contains("pid was reused"));
        assert!(msg.contains("1 close"));
    }

    #[test]
    fn a_free_device_says_so_plainly() {
        assert_eq!(describe("UDID-1", &Admission::Granted), "UDID-1: free");
    }
}
