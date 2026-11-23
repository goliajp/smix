//! `smix lease list | status | reconcile` — the ledger, said out loud.
//!
//! The ledger exists so that a session killed without a graceful path
//! gets one at the next startup. That only helps if a person can see what
//! it thinks: a mechanism that silently decides which processes to signal
//! is one nobody can trust or debug. These three verbs are that window.

use smix_lease::store::{CheckoutLedgers, LeaseDir, LedgerDivergence};
use smix_lease::{Admission, StaleReason, store};
use std::path::{Path, PathBuf};

#[derive(clap::Subcommand, Debug)]
pub enum LeaseAction {
    /// Every device with a ledger, and whether its holder is still there.
    List,
    /// One device: the holder, what is open, and the verdict. With
    /// `--scope` instead, a scope's holder and queue (see `acquire`).
    Status {
        /// The device: an alias, a UDID or a serial.
        #[arg(required_unless_present = "scope", conflicts_with = "scope")]
        device: Option<String>,
        /// A scope instead of a device; `--scope` with no name lists every
        /// scope on this machine.
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        scope: Option<String>,
        /// Print the device's ledger as JSON — `{"device", "path", "lease",
        /// "heldBy"}`: the file it lives in, the lease as it is stored (null
        /// when there is none), and who holds the device now — `{"pid",
        /// "cmd", "alive"}`, or null when a new claim would be granted —
        /// for a script to read instead of building the ledger's path or
        /// judging the holder itself.
        #[arg(long)]
        json: bool,
    },
    /// Close what a dead holder left open, by the graceful path it never
    /// got to take. A live holder is reported, never preempted.
    Reconcile { device: String },
    /// Answer for a device smix did not boot, so smix may drive it (not
    /// shut it down). Logged with its reason; refused over a live holder.
    Claim {
        /// The device to take on, by UDID, serial or registry alias.
        device: String,
        /// Why this device is taken on; written to the ledger and the log.
        #[arg(long)]
        reason: String,
    },
    /// Give up a claim made by `lease claim`. With `--scope` and
    /// `--holder` instead, give up a scope or leave its queue.
    ///
    /// Removes only the claim. A device this machine really did boot
    /// keeps saying so, because that row answers a different question.
    Release {
        #[arg(required_unless_present = "scope", conflicts_with = "scope")]
        device: Option<String>,
        /// The scope to give up.
        #[arg(long, requires = "holder")]
        scope: Option<String>,
        /// Who is giving it up.
        #[arg(long)]
        holder: Option<String>,
    },
    /// Take a scope: a group of devices one holder at a time, in the order
    /// they asked.
    ///
    /// Exits 0 holding it. Held by somebody else: exits 75 at once naming
    /// them, or with `--wait` queues and returns when it is this holder's
    /// turn. Asking again as the same holder keeps the place it has.
    ///
    /// The hold lasts while the holder keeps working: every device command
    /// run with `SMIX_LEASE_HOLDER=<holder>` renews it, `lease renew` does
    /// for a step that does not touch a device, and a hold not renewed
    /// within `--idle` minutes is released and the next in line gets it.
    /// While it is held, a device command on one of the scope's devices
    /// run by anyone else is refused.
    Acquire(crate::scope_lease::AcquireArgs),
    /// Renew a scope this holder has, for a long step that runs no device
    /// command (a build). Exits 3 when it does not hold it.
    Renew {
        #[arg(long)]
        scope: String,
        #[arg(long)]
        holder: String,
    },
    /// Say which devices a scope covers. Names are compared as devices:
    /// a registry alias covers the serial or UDID it runs as.
    Scope {
        /// The scope's name.
        name: String,
        /// A device it covers; repeat for each. Replaces the list.
        #[arg(long = "device", required = true)]
        devices: Vec<String>,
    },
    /// Who answers for this device, in one line and an exit code.
    ///
    /// `0` means a ledger says smix booted it, and names the session;
    /// `3` means no ledger says that — somebody else turned it on, or
    /// nobody wrote it down; `1` means the question could not be asked.
    /// Three codes rather than two: a check that answers "safe" when it
    /// means "I do not know" is the shape this cycle keeps finding.
    ///
    /// A claim answers `0` as well, and says so: the same entitlement to
    /// drive, deliberately not to shut down.
    ///
    /// Not a teardown permission: it answers "did smix boot this", not
    /// "did *you*" — which task a device is for is its owner, in
    /// `smix status`.
    Owner { device: String },
    /// Fold per-checkout ledgers into this machine's.
    ///
    /// Ledgers used to live in whichever `.smix/` was above the working
    /// directory. Adds and never removes; running it twice does nothing
    /// the second time.
    Migrate {
        /// A checkout (or its `.smix/leases`) to read. Repeatable.
        /// Defaults to the one above the working directory.
        #[arg(long = "from", value_name = "DIR")]
        from: Vec<PathBuf>,
        /// Say what would move and move nothing.
        ///
        /// The same decision path as the real thing, split only at the
        /// write — a rehearsal that works the answer out for itself is
        /// answering about itself.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete ledgers that no longer describe anything.
    ///
    /// A ledger that can only be added to stops describing the machine:
    /// a holder that died without releasing, or a boot row for a device
    /// that is off, sits there for ever and every later command has to
    /// reason around it. Says what it deleted and why; keeps everything
    /// else and says why too.
    Prune {
        /// Report what would go, and delete nothing.
        #[arg(long)]
        dry_run: bool,
        /// Look at this one device's ledger and no other.
        ///
        /// Prune reads every ledger on the machine, and a machine is
        /// shared: without this, cleaning up after your own device meant
        /// running a judgement over somebody else's rows too.
        #[arg(long, value_name = "DEVICE")]
        device: Option<String>,
    },
    /// Devices that left while a ledger still said they were here.
    ///
    /// Noticed by the next `run`, `runner`, `sim` or `lease` command after
    /// the device went: an emulator that exited, a simulator shut down
    /// from outside, a phone unplugged. Each entry says which device (and
    /// its AVD), when it was noticed, when its ledger last heard from it,
    /// who held it, whether smix booted it, what answers on its port now,
    /// and — for an emulator smix started — the last lines it printed on
    /// its console. An emulator smix did not start has no console here to
    /// read, and its exit status is nobody's to collect.
    History {
        /// Print the history as JSON.
        #[arg(long)]
        json: bool,
    },
}

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

/// Run the subcommand.
///
/// `leases` is this machine's ledger directory — the ledgers stopped
/// being a property of the tree you are standing in.
pub async fn run(leases: &LeaseDir, action: LeaseAction) -> Result<u8, crate::CliError> {
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
            let written = c
                .ledger_path(d.device_id())
                .ok()
                .and_then(|p| std::fs::metadata(p).ok())
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    chrono::DateTime::<chrono::Local>::from(t)
                        .format("%Y-%m-%d %H:%M:%S %:z")
                        .to_string()
                });
            eprintln!(
                "{}",
                divergence_note(
                    d,
                    &c.to_string(),
                    tree,
                    &leases.to_string(),
                    written.as_deref()
                )
            );
        }
    };
    match action {
        LeaseAction::List => {
            let ids = leases.device_ids();
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
        LeaseAction::Status { scope: Some(name), .. } => {
            return crate::scope_lease::status(&name);
        }
        LeaseAction::Release { scope: Some(name), holder, .. } => {
            return crate::scope_lease::release(&name, holder.as_deref().unwrap_or_default());
        }
        LeaseAction::Acquire(args) => return crate::scope_lease::acquire(args).await,
        LeaseAction::Renew { scope, holder } => return crate::scope_lease::renew(&scope, &holder),
        LeaseAction::Scope { name, devices } => return crate::scope_lease::declare(&name, devices),
        LeaseAction::Status { device: Some(device), json: true, .. } => {
            let udid = crate::resolve_device(&device)?;
            let lease = store::read(leases, &udid).map_err(to_cli_error)?;
            let path = store::lease_path(leases, &udid).map_err(to_cli_error)?;
            let facts = store::collect_facts(leases, &udid).map_err(to_cli_error)?;
            let held_by = held_by_json(&smix_lease::assess(&facts));
            println!(
                "{}",
                serde_json::json!({ "device": udid, "path": path, "lease": lease, "heldBy": held_by })
            );
        }
        LeaseAction::Status {
            device: Some(device),
            json: false,
            ..
        } => {
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
                    let outcomes = smix_capsule::reconcile::execute(&cleanup);
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
        LeaseAction::Claim { device, reason } => return claim(leases, &device, &reason),
        LeaseAction::Release {
            device: Some(device),
            ..
        } => release(leases, &device)?,
        // clap requires a device or a scope; both scope arms returned above
        LeaseAction::Status { device: None, .. } | LeaseAction::Release { device: None, .. } => {
            unreachable!("clap requires a device or --scope")
        }
        LeaseAction::Owner { device } => return owner(leases, &device),
        LeaseAction::Migrate { from, dry_run } => migrate(leases, &from, dry_run)?,
        LeaseAction::Prune { dry_run, device } => prune(leases, dry_run, device.as_deref()).await?,
        LeaseAction::History { json } => crate::departures::print_history(leases, json)?,
    }
    Ok(0)
}

/// Who booted this device, in one line and an exit code. An unknown name
/// is a `1` (the question could not be asked), not a `3` (nobody booted it).
fn owner(leases: &LeaseDir, device: &str) -> Result<u8, crate::CliError> {
    let udid = crate::resolve_device(device)?;
    let facts = store::collect_facts(leases, &udid).map_err(to_cli_error)?;
    let Some(held) = &facts.existing else {
        println!("{udid}: no ledger — nothing here booted it");
        return Ok(3);
    };
    let booted_by_us = held
        .lease
        .known_resources()
        .any(|r| matches!(r, smix_lease::Resource::Booted { by_us: true }));
    let claimed_at = held.lease.known_resources().find_map(|r| match r {
        smix_lease::Resource::Claimed { at, .. } => Some(at.clone()),
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

/// Answer for a device this machine did not boot. Admission first: a
/// claim over somebody's live session would be an escape hatch.
fn claim(leases: &LeaseDir, device: &str, reason: &str) -> Result<u8, crate::CliError> {
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
            .known_resources()
            .any(|r| matches!(r, smix_lease::Resource::Booted { by_us: true }))
    }) {
        println!("{udid}: smix booted this one — it is already answered for");
        return Ok(0);
    }
    store::record_claim(leases, &udid, reason).map_err(to_cli_error)?;
    crate::fence::log_exception("lease claim", &udid, reason);
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
            l.known_resources()
                .any(|r| matches!(r, smix_lease::Resource::Claimed { .. }))
        });
    if !had {
        println!("{udid}: no claim here to release");
        return Ok(());
    }
    store::drop_resource_kind(
        leases,
        &udid,
        &smix_lease::Resource::Claimed {
            at: String::new(),
            reason: None,
        },
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

/// Delete ledgers that no longer describe anything.
async fn prune(
    leases: &LeaseDir,
    dry_run: bool,
    only: Option<&str>,
) -> Result<(), crate::CliError> {
    let ids: Vec<String> = match only {
        Some(device) => {
            let id = crate::resolve_device(device)?;
            if !leases.device_ids().contains(&id) {
                println!("{id}: no ledger — nothing to prune");
                return Ok(());
            }
            vec![id]
        }
        None => leases.device_ids(),
    };
    if ids.is_empty() {
        println!("no device ledgers under {leases}");
        return Ok(());
    }
    // The same question `lease history` is kept by, asked the same way:
    // one judgement of whether a device is here. The old one asked
    // simctl and nothing else, so an emulator — which simctl has never
    // heard of — was "cannot tell whether it is still on" for ever,
    // although adb could say.
    let live = crate::departures::ask_what_is_here(&ids).await;
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
        let on = is_on(&smix_lease::vanish::presence(&held.lease, &live));
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

/// What prune needs from a presence: on, off, or unknown.
fn is_on(p: &smix_lease::vanish::Presence) -> Option<bool> {
    use smix_lease::vanish::Presence;
    match p {
        Presence::Present => Some(true),
        Presence::Gone { .. } => Some(false),
        Presence::CannotTell => None,
    }
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

    fn disagreement() -> LedgerDivergence {
        LedgerDivergence::Disagrees {
            device_id: "5D087114".into(),
            detail: "holder: machine says pid 70842, the tree says pid 47495".into(),
        }
    }

    #[test]
    fn a_disagreement_says_when_the_tree_was_written_and_not_that_it_still_is() {
        // A frozen book always disagrees with a live one. The note said
        // "Something still writes there" about a file last touched on
        // 2026-08-11, and a release review took it at its word.
        let msg = divergence_note(
            &disagreement(),
            "/home/u/repo/.smix/leases",
            Path::new("/home/u/repo/.smix/leases")
                .ancestors()
                .nth(2)
                .expect("a book two levels under its tree"),
            "/home/u/.local/share/smix/leases",
            Some("2026-08-11 22:21:54 +09:00"),
        );
        assert!(!msg.contains("still writes"), "{msg}");
        assert!(
            msg.contains("last written 2026-08-11 22:21:54 +09:00"),
            "{msg}"
        );
        assert!(
            msg.contains("answers from /home/u/.local/share/smix/leases alone"),
            "{msg}"
        );
    }

    #[test]
    fn a_disagreement_whose_file_has_no_time_says_so() {
        let msg = divergence_note(
            &disagreement(),
            "/home/u/repo/.smix/leases",
            Path::new("/home/u/repo/.smix/leases")
                .ancestors()
                .nth(2)
                .expect("a book two levels under its tree"),
            "/home/u/.local/share/smix/leases",
            None,
        );
        assert!(
            msg.contains("When it was last written could not be read"),
            "{msg}"
        );
    }

    #[test]
    fn a_live_holder_is_held_by_for_a_script() {
        let v = held_by_json(&Admission::Denied(Contention {
            holder: ident(4242),
            acquired_at: "2026-08-06T10:00:00Z".into(),
            holder_alive: true,
        }));
        assert_eq!(v["pid"], 4242);
        assert_eq!(v["alive"], true);
        assert!(
            v["cmd"]
                .as_str()
                .is_some_and(|c| c.contains("smix run hello.yaml"))
        );
    }

    /// Its launcher exited and what it started still runs: a new claim is
    /// refused, so a script must see the device as held.
    #[test]
    fn a_device_whose_launcher_exited_is_still_held_for_a_script() {
        let v = held_by_json(&Admission::Denied(Contention {
            holder: ident(4242),
            acquired_at: "2026-08-06T10:00:00Z".into(),
            holder_alive: false,
        }));
        assert_eq!(v["pid"], 4242);
        assert_eq!(v["alive"], false);
    }

    #[test]
    fn a_free_device_is_held_by_nobody() {
        assert!(held_by_json(&Admission::Granted).is_null());
        assert!(held_by_json(&Admission::Adoptable).is_null());
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
