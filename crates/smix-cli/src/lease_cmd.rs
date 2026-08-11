//! `smix lease list | status | reconcile` — the ledger, said out loud.
//!
//! The ledger exists so that a session killed without a graceful path
//! gets one at the next startup. That only helps if a person can see what
//! it thinks: a mechanism that silently decides which processes to signal
//! is one nobody can trust or debug. These three verbs are that window.

use crate::LeaseAction;
use smix_lease::store::LeaseDir;
use smix_lease::{Admission, StaleReason, store};
use std::path::{Path, PathBuf};

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

fn device_ids(leases: &LeaseDir) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(leases.path()) else {
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
///
/// Two directories, not one. `leases` is this machine's ledger
/// directory — the ledgers stopped being a property of the tree you are
/// standing in. `root` is still the tree, and is used for exactly one
/// thing: settling a dead holder's build products, which are in a tree.
pub fn run(root: &Path, leases: &LeaseDir, action: LeaseAction) -> Result<u8, crate::CliError> {
    match action {
        LeaseAction::List => {
            let ids = device_ids(leases);
            if ids.is_empty() {
                println!("no device ledgers under {leases}");
                return Ok(0);
            }
            for id in ids {
                let facts = store::collect_facts(leases, &id).map_err(to_cli_error)?;
                println!("{}", describe(&id, &smix_lease::assess(&facts)));
            }
        }
        LeaseAction::Status { device } => {
            let udid = crate::resolve_device(&device)?;
            let facts = store::collect_facts(leases, &udid).map_err(to_cli_error)?;
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
            let facts = store::collect_facts(leases, &udid).map_err(to_cli_error)?;
            match smix_lease::assess(&facts) {
                Admission::Granted => println!("{udid}: nothing to settle"),
                // Nothing to settle: the next command that wants the
                // device will adopt this lease in passing. Reconcile
                // closes abandoned things, and a serving runner is not
                // abandoned — it is waiting.
                a @ Admission::Adoptable => {
                    println!("{}", describe(&udid, &a));
                }
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
                        store::remove(leases, &udid).map_err(to_cli_error)?;
                        println!("{udid}: settled, ledger cleared");
                    } else {
                        // Keeping the ledger is the point: the next
                        // command must still see what did not close.
                        println!("{udid}: some closes failed — ledger kept so they stay visible");
                    }
                }
            }
        }
        LeaseAction::Owner { device } => return owner(leases, &device),
        LeaseAction::Migrate { from } => migrate(leases, &from)?,
        LeaseAction::Prune { dry_run } => prune(leases, dry_run)?,
    }
    Ok(0)
}

/// Who booted this device, in one line and an exit code.
///
/// The device ref is resolved before the ledger is read, so a name this
/// machine does not know is a `1` — "the question could not be asked" —
/// rather than a `3` that would read as "nobody booted it".
fn owner(leases: &LeaseDir, device: &str) -> Result<u8, crate::CliError> {
    let udid = crate::resolve_device(device)?;
    let facts = store::collect_facts(leases, &udid).map_err(to_cli_error)?;
    let Some(held) = &facts.existing else {
        println!("{udid}: no ledger — nothing here booted it");
        return Ok(3);
    };
    let booted_by_us = held
        .lease
        .resources
        .iter()
        .any(|r| matches!(r, smix_lease::Resource::Booted { by_us: true }));
    if !booted_by_us {
        println!(
            "{udid}: a ledger exists (holder pid {}) but no row says smix booted it",
            held.lease.holder.pid
        );
        return Ok(3);
    }
    let alive = held.holder.pid_exists && held.holder.identity_matches;
    println!(
        "{udid}: booted by smix — holder pid {} ({}), {}",
        held.lease.holder.pid,
        held.lease.holder.cmd,
        if alive { "still running" } else { "exited" }
    );
    Ok(0)
}

/// Fold per-checkout ledgers into this machine's.
fn migrate(leases: &LeaseDir, from: &[PathBuf]) -> Result<(), crate::CliError> {
    let sources: Vec<PathBuf> = if from.is_empty() {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| checkout_lease_dir(&cwd))
            .into_iter()
            .collect()
    } else {
        from.iter()
            .map(|p| {
                if p.ends_with("leases") {
                    p.clone()
                } else {
                    p.join(".smix").join("leases")
                }
            })
            .collect()
    };
    if sources.is_empty() {
        println!("nothing to migrate — no checkout ledgers found. Name them with --from <dir>.");
        return Ok(());
    }
    let mut moved = 0usize;
    let mut refused = 0usize;
    for src in &sources {
        let src_dir = LeaseDir::at(src.clone());
        let ids = device_ids(&src_dir);
        if ids.is_empty() {
            println!("  {} held no ledgers", src.display());
            continue;
        }
        for id in ids {
            let Ok(Some(incoming)) = store::read(&src_dir, &id) else {
                eprintln!("  {}/{id}: unreadable, left where it is", src.display());
                continue;
            };
            match store::read(leases, &id) {
                Ok(None) => {
                    store::write(leases, &incoming).map_err(to_cli_error)?;
                    println!("  + {id} from {}", src.display());
                    moved += 1;
                }
                Ok(Some(existing)) if existing.holder.pid == incoming.holder.pid => {
                    println!("  = {id} already here");
                }
                // Two books, two holders, one device. Nothing here can
                // tell which session is the real one, and picking would
                // hand somebody's device to somebody else. Named, and
                // left for a person.
                Ok(Some(existing)) => {
                    eprintln!(
                        "  ! {id}: this machine already has a ledger held by pid {}, \
                         and {} has one held by pid {}. Settle one of them \
                         (`smix lease status {id}`) and run this again.",
                        existing.holder.pid,
                        src.display(),
                        incoming.holder.pid
                    );
                    refused += 1;
                }
                Err(e) => return Err(to_cli_error(e)),
            }
        }
    }
    println!("{moved} ledger(s) now in {leases}");
    if refused > 0 {
        return Err(crate::CliError::Other(format!(
            "{refused} device(s) had a ledger in two places and were left alone"
        )));
    }
    // The sources stay. Somebody unsure whether this worked has to be
    // able to run it again, and a migration that deletes what it just
    // copied cannot be run twice.
    println!("the source ledgers are untouched");
    Ok(())
}

/// The checkout's own ledger directory, if it has one.
fn checkout_lease_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(".smix").join("leases");
        if candidate.is_dir() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Delete ledgers that no longer describe anything.
fn prune(leases: &LeaseDir, dry_run: bool) -> Result<(), crate::CliError> {
    let ids = device_ids(leases);
    if ids.is_empty() {
        println!("no device ledgers under {leases}");
        return Ok(());
    }
    let mut gone = 0usize;
    for id in ids {
        let facts = store::collect_facts(leases, &id).map_err(to_cli_error)?;
        let Some(held) = &facts.existing else {
            println!("  {id}: empty ledger — removing");
            if !dry_run {
                store::remove(leases, &id).map_err(to_cli_error)?;
            }
            gone += 1;
            continue;
        };
        let holder_alive = held.holder.pid_exists && held.holder.identity_matches;
        if holder_alive {
            println!("  {id}: held by pid {} — kept", held.lease.holder.pid);
            continue;
        }
        if held.any_resource_alive {
            println!("  {id}: holder gone but something it started is still running — kept");
            continue;
        }
        println!(
            "  {id}: holder pid {} is gone and nothing it opened is still running — removing",
            held.lease.holder.pid
        );
        if !dry_run {
            store::remove(leases, &id).map_err(to_cli_error)?;
        }
        gone += 1;
    }
    if dry_run {
        println!("{gone} ledger(s) would be removed");
    } else {
        println!("{gone} ledger(s) removed");
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
