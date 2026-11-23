//! `smix lease acquire | renew | scope`, `status --scope`, `release --scope`,
//! and the check every device command makes against them.
//!
//! The queue's rules are `smix_lease::scope`'s. This is the command line
//! around them: names to devices, waiting, and the words of a refusal.

use smix_lease::scope::{DEFAULT_IDLE_SECS, Granted, Request, Scope, ScopeDir};
use std::collections::BTreeSet;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use smix_lease::scope::HOLDER_ENV;

/// Held by somebody else, or waited too long: bench-lock's code for the same.
const BUSY: u8 = 75;
/// A refusal, the same code the lifecycle fence uses.
const REFUSED: u8 = 3;

/// `smix lease acquire`.
#[derive(clap::Args, Debug)]
pub struct AcquireArgs {
    /// The scope to take.
    #[arg(long)]
    scope: String,
    /// Who is taking it. Device commands carry the same name in
    /// `SMIX_LEASE_HOLDER`.
    #[arg(long)]
    holder: String,
    /// Queue and return when it is this holder's turn, instead of exiting
    /// 75 at once.
    #[arg(long)]
    wait: bool,
    /// With `--wait`, give up after this many seconds: exit 75 naming who
    /// holds it and who is ahead.
    #[arg(long, value_name = "SECS", requires = "wait")]
    max_wait: Option<u64>,
    /// When the holder expects to be done, and what it is doing — shown to
    /// whoever waits. `+45m build` (also `+SECS`, `+Nh`) is turned into a
    /// clock time when the hold starts; anything else is kept as written.
    #[arg(long, value_name = "WHEN")]
    until: Option<String>,
    /// Minutes without a device command or `lease renew` from the holder
    /// before the hold is released.
    #[arg(long, value_name = "MINUTES", default_value_t = DEFAULT_IDLE_SECS / 60)]
    idle: u64,
    /// A process whose exit also ends the hold, for a caller that has one
    /// of its own.
    #[arg(long)]
    pid: Option<u32>,
}

fn scopes() -> Result<ScopeDir, String> {
    ScopeDir::machine()
        .ok_or_else(|| "no home for this machine's scopes: set XDG_DATA_HOME or HOME".to_string())
}

fn alive(p: &smix_lease::ProcIdentity) -> bool {
    smix_lease::store::probe(p).identity_matches
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

fn fail(e: String) -> crate::CliError {
    crate::CliError::Other(e)
}

/// `+45m build` → `14:20 build`; anything else as written.
pub(crate) fn until_label(raw: &str, now: chrono::DateTime<chrono::Local>) -> String {
    let Some(rest) = raw.strip_prefix('+') else {
        return raw.to_string();
    };
    let (amount, note) = rest.split_once(' ').unwrap_or((rest, ""));
    let secs = if let Some(m) = amount.strip_suffix('m') {
        m.parse::<i64>().ok().map(|m| m * 60)
    } else if let Some(h) = amount.strip_suffix('h') {
        h.parse::<i64>().ok().map(|h| h * 3600)
    } else {
        amount
            .strip_suffix('s')
            .unwrap_or(amount)
            .parse::<i64>()
            .ok()
    };
    match secs {
        Some(s) => {
            let at = (now + chrono::Duration::seconds(s)).format("%H:%M");
            format!("{at} {note}").trim_end().to_string()
        }
        None => raw.to_string(),
    }
}

use smix_lease::scope::shown;

fn describe_holder(scope: &Scope) -> String {
    match &scope.holder {
        None => "nobody".into(),
        Some(h) => {
            let until = h
                .until
                .as_deref()
                .map(|u| format!(", until {u}"))
                .unwrap_or_default();
            format!("{} (since {}{until})", h.holder, shown(&h.since))
        }
    }
}

fn describe_queue(scope: &Scope) -> String {
    if scope.queue.is_empty() {
        return "nobody waiting".into();
    }
    let names: Vec<String> = scope
        .queue
        .iter()
        .enumerate()
        .map(|(i, w)| format!("{}. {} (since {})", i + 1, w.holder, shown(&w.since)))
        .collect();
    format!("waiting: {}", names.join(", "))
}

/// Answer the scope verbs of `smix lease`; anything else comes back for
/// the device ledger's verbs.
pub(crate) async fn run(
    action: crate::lease_cmd::LeaseAction,
) -> Result<Result<u8, crate::lease_cmd::LeaseAction>, crate::CliError> {
    use crate::lease_cmd::LeaseAction as A;
    Ok(Ok(match action {
        A::Status {
            scope: Some(name), ..
        } => status(&name)?,
        A::Release {
            scope: Some(name),
            holder,
            ..
        } => release(&name, holder.as_deref().unwrap_or_default())?,
        A::Acquire(args) => acquire(args).await?,
        A::Renew { scope, holder } => renew(&scope, &holder)?,
        A::Scope { name, devices } => declare(&name, devices)?,
        other => return Ok(Err(other)),
    }))
}

async fn acquire(args: AcquireArgs) -> Result<u8, crate::CliError> {
    let dir = scopes().map_err(fail)?;
    let pid = match args.pid {
        Some(p) => Some(
            smix_lease::store::identify(p)
                .ok_or_else(|| fail(format!("--pid {p}: no such process")))?,
        ),
        None => None,
    };
    let until = args
        .until
        .as_deref()
        .map(|u| until_label(u, chrono::Local::now()));
    let req = Request {
        holder: args.holder.clone(),
        wait: args.wait,
        idle_secs: args.idle * 60,
        until,
        pid,
        asking: smix_lease::store::identify_self(),
    };
    let started = Instant::now();
    let mut said_place = None;
    loop {
        let (granted, scope) = dir
            .update(&args.scope, |s| {
                s.settle(now(), &alive);
                (s.acquire(&req, now()), s.clone())
            })
            .map_err(fail)?;
        match granted {
            Granted::Held => {
                println!("lease: {} held by {}", args.scope, args.holder);
                return Ok(0);
            }
            Granted::Busy => {
                eprintln!(
                    "lease: {} is held by {}; {} — `--wait` queues for it",
                    args.scope,
                    describe_holder(&scope),
                    describe_queue(&scope)
                );
                return Ok(BUSY);
            }
            Granted::Queued(place) if !args.wait => {
                eprintln!(
                    "lease: {} is held by {}; {} is already in its queue, place {place}",
                    args.scope,
                    describe_holder(&scope),
                    args.holder
                );
                return Ok(BUSY);
            }
            Granted::Queued(place) => {
                if said_place != Some(place) {
                    eprintln!(
                        "lease: waiting for {} — held by {}, place {place} in the queue",
                        args.scope,
                        describe_holder(&scope)
                    );
                    said_place = Some(place);
                }
                if let Some(max) = args.max_wait
                    && started.elapsed() >= Duration::from_secs(max)
                {
                    dir.update(&args.scope, |s| s.give_up(&args.holder, &req.asking))
                        .map_err(fail)?;
                    eprintln!(
                        "lease: gave up on {} after {max} s — held by {}; {}",
                        args.scope,
                        describe_holder(&scope),
                        describe_queue(&scope)
                    );
                    return Ok(BUSY);
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn release(name: &str, holder: &str) -> Result<u8, crate::CliError> {
    let dir = scopes().map_err(fail)?;
    let was = dir
        .update(name, |s| {
            let was = s.release(holder);
            s.settle(now(), &alive);
            was
        })
        .map_err(fail)?;
    if was {
        println!("lease: {holder} let go of {name}");
    } else {
        println!("lease: {holder} neither held nor waited for {name}");
    }
    Ok(0)
}

fn renew(name: &str, holder: &str) -> Result<u8, crate::CliError> {
    let dir = scopes().map_err(fail)?;
    let (held, scope) = dir
        .update(name, |s| {
            s.settle(now(), &alive);
            (s.touch(holder, now()), s.clone())
        })
        .map_err(fail)?;
    if held {
        println!("lease: {holder} renewed {name}");
        Ok(0)
    } else {
        eprintln!(
            "lease: {holder} does not hold {name}; it is held by {}",
            describe_holder(&scope)
        );
        Ok(REFUSED)
    }
}

fn declare(name: &str, devices: Vec<String>) -> Result<u8, crate::CliError> {
    let dir = scopes().map_err(fail)?;
    dir.update(name, |s| s.devices.clone_from(&devices))
        .map_err(fail)?;
    println!("lease: {name} covers {}", devices.join(", "));
    Ok(0)
}

fn status(name: &str) -> Result<u8, crate::CliError> {
    let dir = scopes().map_err(fail)?;
    let names = if name.is_empty() {
        dir.names()
    } else {
        vec![name.to_string()]
    };
    if names.is_empty() {
        println!(
            "lease: no scopes on this machine ({})",
            dir.path().display()
        );
    }
    for n in names {
        let scope = dir
            .update(&n, |s| {
                s.settle(now(), &alive);
                s.clone()
            })
            .map_err(fail)?;
        println!(
            "{n}: devices {}",
            if scope.devices.is_empty() {
                "(none declared)".into()
            } else {
                scope.devices.join(", ")
            }
        );
        match &scope.holder {
            Some(h) => {
                let pid = h
                    .pid
                    .as_ref()
                    .map(|p| format!(", ends with pid {}", p.pid))
                    .unwrap_or_default();
                println!(
                    "  held by {} since {}, last command {}, released after {} min idle{pid}{}",
                    h.holder,
                    shown(&h.since),
                    shown(&h.last_seen),
                    h.idle_secs / 60,
                    h.until
                        .as_deref()
                        .map(|u| format!(", until {u}"))
                        .unwrap_or_default()
                );
            }
            None => println!("  free"),
        }
        for (i, w) in scope.queue.iter().enumerate() {
            println!(
                "  {}. {} waiting since {} (pid {}){}",
                i + 1,
                w.holder,
                shown(&w.since),
                w.waiting.pid,
                w.until
                    .as_deref()
                    .map(|u| format!(", until {u}"))
                    .unwrap_or_default()
            );
        }
        for r in scope.released.iter().rev().take(3) {
            println!(
                "  released automatically: {} at {} — {}",
                r.holder,
                shown(&r.at),
                r.reason
            );
        }
    }
    Ok(0)
}

/// Every name `device` is known by: as written, upper-cased (a UDID),
/// and through the registry, the UDID, device name and — for an emulator
/// that is running — the serial it runs as.
pub(crate) fn identities(device: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::from([device.to_string(), device.to_ascii_uppercase()]);
    if let Some(sim) = crate::lookup_registered(device) {
        ids.insert(sim.udid.clone());
        ids.insert(sim.udid.to_ascii_uppercase());
        ids.insert(sim.device_name.clone());
        if sim.kind == smix_simctl::registry::DeviceKind::Emulator {
            use smix_simctl::registry::EmulatorAddress;
            if let EmulatorAddress::At { serial } | EmulatorAddress::MovedTo { serial, .. } =
                crate::emulator_alias_address(&sim)
            {
                ids.insert(serial);
            }
        }
    }
    ids
}

/// Refuse `cmd` when it acts on a device in a scope somebody else holds.
/// `Some(code)` is the exit code to stop with; `None` lets it run.
pub(crate) fn gate(cmd: &crate::Cmd) -> Option<ExitCode> {
    // nothing to read on a machine with no scopes, which is most of them
    if ScopeDir::machine()?.names().is_empty() {
        return None;
    }
    let devices = crate::command_devices::devices(cmd);
    let me = std::env::var(HOLDER_ENV).ok().filter(|s| !s.is_empty());
    let targets: BTreeSet<String> = devices.iter().flat_map(|d| identities(d)).collect();
    let why = smix_lease::scope::refusal(
        &ScopeDir::machine()?,
        &targets,
        &identities,
        me.as_deref(),
        &alive,
    )?;
    eprintln!("error: {why}");
    Some(ExitCode::from(REFUSED))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_until_becomes_a_clock_time_and_keeps_its_note() {
        let t = chrono::DateTime::parse_from_rfc3339("2026-11-23T13:35:00+00:00")
            .unwrap()
            .with_timezone(&chrono::Local);
        let base = t.format("%H:%M").to_string();
        assert_ne!(until_label("+45m build", t), "+45m build");
        assert!(until_label("+45m build", t).ends_with(" build"));
        assert!(
            !until_label("+2h", t).contains(&base),
            "two hours on is another time"
        );
        assert_eq!(until_label("+90 x", t), until_label("+90s x", t));
        assert_eq!(until_label("23:30 nightly", t), "23:30 nightly");
        assert_eq!(until_label("+soon", t), "+soon");
    }
}
