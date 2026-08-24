//! `smix lease list | status | reconcile` — the ledger, said out loud.
//!
//! The ledger exists so that a session killed without a graceful path
//! gets one at the next startup. That only helps if a person can see what
//! it thinks: a mechanism that silently decides which processes to signal
//! is one nobody can trust or debug. These three verbs are that window.

use crate::LeaseAction;
use smix_lease::store::{CheckoutLedgers, LeaseDir, LedgerDivergence};
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
pub async fn run(
    root: &Path,
    leases: &LeaseDir,
    action: LeaseAction,
) -> Result<u8, crate::CliError> {
    // What the tree underfoot still holds, if it holds anything.
    //
    // Read for one purpose: to say when it disagrees. Never merged into
    // the machine's answer — a tree's book is written by whatever smix
    // that tree last ran, and on this machine that was still 3.0.0
    // ninety-one minutes after the ledgers moved.
    let checkout = std::env::current_dir()
        .ok()
        .and_then(|cwd| CheckoutLedgers::discover(&cwd));
    let divergences: Vec<LedgerDivergence> = match &checkout {
        Some(c) => store::survey(leases, c),
        None => Vec::new(),
    };
    let say_divergences = |only: Option<&str>| {
        let Some(c) = &checkout else { return };
        let rows: Vec<&LedgerDivergence> = divergences
            .iter()
            .filter(|d| only.is_none_or(|id| d.device_id() == id))
            .collect();
        if rows.is_empty() {
            return;
        }
        let tree = c
            .path()
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap_or(c.path());
        eprintln!();
        for d in rows {
            match d {
                LedgerDivergence::OnlyInCheckout { device_id } => eprintln!(
                    "note: {device_id} has a ledger in {c} and none here — no other \
                     checkout can see it. `smix lease migrate --from {}` brings it over.",
                    tree.display()
                ),
                LedgerDivergence::Disagrees { device_id, detail } => eprintln!(
                    "note: {device_id} is recorded differently in {c} — {detail}. \
                     Something still writes there; until they agree, this command \
                     answers from {leases} alone."
                ),
            }
        }
    };
    match action {
        LeaseAction::List => {
            let ids = device_ids(leases);
            if ids.is_empty() {
                println!("no device ledgers under {leases}");
                // Not a return. An empty machine and a tree that still
                // holds ledgers is the state somebody most needs told
                // about — it is what a fresh checkout of a machine mid-
                // migration looks like, and returning here answered
                // "nothing to see" while a tree held the only record of
                // a device.
                say_divergences(None);
                return Ok(0);
            }
            for id in ids {
                let facts = store::collect_facts(leases, &id).map_err(to_cli_error)?;
                println!("{}", describe(&id, &smix_lease::assess(&facts)));
            }
            say_divergences(None);
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
            say_divergences(Some(&udid));
        }
        LeaseAction::Reconcile { device } => {
            let udid = crate::resolve_device(&device)?;
            // A device the two books disagree about is not settled here.
            //
            // Reconcile acts: it stops runners and shuts simulators
            // down. The machine ledger is the only thing it reads, and
            // while a tree holds a different record for the same device
            // the machine's may be the older of the two. That is not a
            // guess about this machine — for ninety-one minutes on
            // 2026-08-11 the machine ledger read "abandoned, 2 close(s)
            // owed" for a simulator whose runner, recorded only in
            // `qualcomm/insight`, was alive and serving on port 22087.
            // Acting then would have killed it.
            //
            // Refusing costs a wait. `lease migrate` is the way out, and
            // it refuses in its own turn when both books name a live
            // holder rather than choosing between two people's work.
            if let Some(d) = divergences.iter().find(|d| d.device_id() == udid) {
                let c = checkout
                    .as_ref()
                    .expect("a divergence implies a checkout book");
                let tree = c
                    .path()
                    .parent()
                    .and_then(std::path::Path::parent)
                    .unwrap_or(c.path());
                eprintln!("{udid}: not settling — {c} has a different record for it.");
                if let LedgerDivergence::Disagrees { detail, .. } = d {
                    eprintln!("  {detail}");
                }
                eprintln!(
                    "  Settling from one book while the other says otherwise is how a \
                     live session gets torn down. Reconcile them first:"
                );
                eprintln!("    smix lease migrate --from {}", tree.display());
                return Ok(1);
            }
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
        LeaseAction::Claim { device } => return claim(leases, &device),
        LeaseAction::Release { device } => release(leases, &device)?,
        LeaseAction::Owner { device } => return owner(leases, &device),
        LeaseAction::Migrate { from, dry_run } => migrate(leases, &from, dry_run)?,
        LeaseAction::Prune { dry_run } => prune(leases, dry_run).await?,
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
    let claimed_at = held.lease.resources.iter().find_map(|r| match r {
        smix_lease::Resource::Claimed { at } => Some(at.clone()),
        _ => None,
    });
    if !booted_by_us && claimed_at.is_none() {
        println!(
            "{udid}: a ledger exists (holder pid {}) but no row says smix booted \
             or claimed it",
            held.lease.holder.pid
        );
        return Ok(3);
    }
    let alive = held.holder.pid_exists && held.holder.identity_matches;
    // Both answer 0 and the sentence says which. A caller acting on the
    // code gets the entitlement it asked about — may I drive this — and
    // one that needs the stricter question reads the words, because a
    // claim is deliberately not a licence to switch the device off.
    if booted_by_us {
        println!(
            "{udid}: booted by smix — holder pid {} ({}), {}",
            held.lease.holder.pid,
            held.lease.holder.cmd,
            if alive { "still running" } else { "exited" }
        );
    } else {
        println!(
            "{udid}: claimed at {} — holder pid {} ({}), {}. Nothing here booted \
             it, so it is this machine's to drive and not to shut down.",
            claimed_at.unwrap_or_default(),
            held.lease.holder.pid,
            held.lease.holder.cmd,
            if alive { "still running" } else { "exited" }
        );
    }
    Ok(0)
}

/// Answer for a device this machine did not boot.
///
/// Admission first, and not as politeness: a claim is a statement about
/// a device nobody is using, and making one over somebody's live session
/// would be the escape hatch this exists to replace, wearing a better
/// name.
fn claim(leases: &LeaseDir, device: &str) -> Result<u8, crate::CliError> {
    let udid = crate::resolve_device(device)?;
    let facts = store::collect_facts(leases, &udid).map_err(to_cli_error)?;
    if let Admission::Denied(c) = smix_lease::assess(&facts) {
        println!(
            "{udid}: held by pid {} ({}) since {} — not a device to claim",
            c.holder.pid, c.holder.cmd, c.acquired_at
        );
        return Ok(3);
    }
    if facts.existing.as_ref().is_some_and(|h| {
        h.lease
            .resources
            .iter()
            .any(|r| matches!(r, smix_lease::Resource::Booted { by_us: true }))
    }) {
        println!("{udid}: smix booted this one — it is already answered for");
        return Ok(0);
    }
    store::record_claim(leases, &udid).map_err(to_cli_error)?;
    println!(
        "{udid}: claimed. Yours to drive; not yours to shut down, because \
         nothing here booted it. `smix lease release {udid}` ends it, and so \
         does the device going off."
    );
    Ok(0)
}

/// Give up a claim, leaving every other row alone.
fn release(leases: &LeaseDir, device: &str) -> Result<(), crate::CliError> {
    let udid = crate::resolve_device(device)?;
    let had = store::read(leases, &udid)
        .map_err(to_cli_error)?
        .is_some_and(|l| {
            l.resources
                .iter()
                .any(|r| matches!(r, smix_lease::Resource::Claimed { .. }))
        });
    if !had {
        println!("{udid}: no claim here to release");
        return Ok(());
    }
    store::drop_resource_kind(
        leases,
        &udid,
        &smix_lease::Resource::Claimed { at: String::new() },
    )
    .map_err(to_cli_error)?;
    println!("{udid}: claim released");
    Ok(())
}

/// Fold per-checkout ledgers into this machine's.
fn migrate(leases: &LeaseDir, from: &[PathBuf], dry_run: bool) -> Result<(), crate::CliError> {
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
        let src_dir = CheckoutLedgers::at(src.clone());
        let ids = src_dir.device_ids();
        if ids.is_empty() {
            println!("  {} held no ledgers", src.display());
            continue;
        }
        for id in ids {
            let Ok(Some(incoming)) = src_dir.read(&id) else {
                eprintln!("  {}/{id}: unreadable, left where it is", src.display());
                continue;
            };
            match store::read(leases, &id) {
                Ok(None) => {
                    // The only branch that differs between a rehearsal
                    // and the real thing. Everything above — which
                    // sources, which ids, which of the three verdicts —
                    // is one path, so what the rehearsal reports is what
                    // the run would do rather than a second opinion
                    // about it.
                    if !dry_run {
                        store::write(leases, &incoming).map_err(to_cli_error)?;
                    }
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
    if dry_run {
        println!("{moved} ledger(s) would move into {leases}; nothing was written");
    } else {
        println!("{moved} ledger(s) now in {leases}");
    }
    if refused > 0 {
        return Err(crate::CliError::Other(format!(
            "{refused} device(s) had a ledger in two places and were left alone"
        )));
    }
    if !dry_run {
        // The sources stay. Somebody unsure whether this worked has to
        // be able to run it again, and a migration that deletes what it
        // just copied cannot be run twice.
        println!("the source ledgers are untouched");
    }
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

/// What this machine can say about whether a device is switched on.
///
/// Three answers, not two. `Unknown` is the one that matters: an Android
/// serial and a physical iPhone are not in `simctl`'s list at all, and
/// reading "not listed" as "off" would let `prune` delete the record of a
/// device it cannot see the state of.
enum Power {
    On,
    Off,
    Unknown,
}

async fn power_of(device_ids: &[String]) -> std::collections::HashMap<String, Power> {
    let listed = smix_simctl::SimctlClient::new().list_devices().await;
    device_ids
        .iter()
        .map(|id| {
            let state = listed.as_ref().ok().and_then(|ds| {
                ds.iter()
                    .find(|d| d.udid.eq_ignore_ascii_case(id))
                    .map(|d| d.state == "Booted")
            });
            let power = match state {
                Some(true) => Power::On,
                Some(false) => Power::Off,
                None => Power::Unknown,
            };
            (id.clone(), power)
        })
        .collect()
}

/// Delete ledgers that no longer describe anything.
async fn prune(leases: &LeaseDir, dry_run: bool) -> Result<(), crate::CliError> {
    let ids = device_ids(leases);
    if ids.is_empty() {
        println!("no device ledgers under {leases}");
        return Ok(());
    }
    let power = power_of(&ids).await;
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
        // One place decides, and it is pure. Judging and doing I/O in
        // the same breath is what made the old rule untestable and let
        // `--help` describe a check the code never performed.
        let on = match power.get(&id).unwrap_or(&Power::Unknown) {
            Power::On => Some(true),
            Power::Off => Some(false),
            Power::Unknown => None,
        };
        if let smix_lease::PruneVerdict::Keep(why) = smix_lease::prune_verdict(held, on) {
            println!("  {id}: {why} — kept");
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
