//! `smix cleanup --owner <task>`: take down what one task brought up, and
//! nothing else.
//!
//! A task that ends is meant to leave nothing behind, and four tasks each
//! keeping their own list and deleting from it is how one deleted a link
//! another was still using. Here the list is the ledger's: the resources
//! recorded under that owner, closed by the graceful path a reconcile uses,
//! in its order. A device somebody is still working on is left whole and
//! named; what belongs to other owners, or to nobody, stays and is listed.
//! Simulator runtimes and images belong to the machine and are never
//! touched.

use std::process::ExitCode;

use smix_lease::{CleanupAction, Lease, Resource, Row, owners};

/// What one device gives up to the cleanup, and what it keeps and why.
#[derive(Debug, PartialEq)]
pub(crate) struct Selection {
    pub take: Vec<Resource>,
    pub keep: Vec<(String, String)>,
}

/// Which of `lease`'s resources are `owner`'s to take now. `holder_alive`:
/// whether the process holding the device is still running — then nothing
/// is taken, because closing a runner or the device under a live command
/// pulls it out from under work in progress.
pub(crate) fn select(
    lease: &Lease,
    rows: &owners::Owners,
    owner: &str,
    holder_alive: bool,
) -> Selection {
    let whose = |kind: &str| {
        rows.rows
            .iter()
            .find(|r| r.kind == kind)
            .map(|r| r.owner.as_str())
    };
    let mut take = Vec::new();
    let mut keep = Vec::new();
    for r in lease.known_resources() {
        let kind = owners::kind_of(r);
        match whose(&kind) {
            Some(o) if o == owner && holder_alive => keep.push((
                kind,
                format!(
                    "held by pid {} ({}), which is still running",
                    lease.holder.pid, lease.holder.cmd
                ),
            )),
            Some(o) if o == owner => take.push(r.clone()),
            Some(o) => keep.push((kind, format!("{o}'s"))),
            None => keep.push((kind, "no owner".into())),
        }
    }
    Selection { take, keep }
}

fn action_kind(a: &CleanupAction) -> &'static str {
    match a {
        CleanupAction::CannotClose { .. } => "unnamed",
        CleanupAction::StopRecording { .. } => "recording",
        CleanupAction::StopRunner { .. } => "runner",
        CleanupAction::StopSupervisor { .. } => "supervisor",
        CleanupAction::StopAndroidRunner { .. } => "androidRunner",
        CleanupAction::StopPortForward { .. } => "portForward",
        CleanupAction::RemoveReversePort { .. } => "reversePort",
        CleanupAction::ShutdownSim { .. } => "booted",
    }
}

fn ok(o: &smix_capsule::reconcile::Outcome) -> Result<String, String> {
    use smix_capsule::reconcile::Outcome;
    match o {
        Outcome::Closed(s) | Outcome::AlreadyGone(s) => Ok(s.clone()),
        Outcome::Skipped(s) | Outcome::Failed(s) => Err(s.clone()),
    }
}

pub(crate) async fn run(owner: Option<&str>, dry_run: bool, orphans: bool) -> ExitCode {
    if orphans {
        return match reclaim_left_behind(dry_run).await {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::from(1),
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        };
    }
    let Some(owner) = owner else {
        eprintln!(
            "error: cleanup takes down one owner's resources (--owner <task> or SMIX_OWNER), \
             or the devices left behind (--orphans)"
        );
        return ExitCode::from(2);
    };
    let leases = match smix_capsule::runner::machine_leases() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    let (mut taken, mut kept, mut failed) = (0, 0, 0);
    for id in leases.device_ids() {
        let (Ok(Some(lease)), Ok(rows)) = (
            smix_lease::store::read(&leases, &id),
            owners::read(&leases, &id),
        ) else {
            continue;
        };
        if !rows.names().contains(&owner) {
            continue;
        }
        let held = smix_lease::store::probe(&lease.holder);
        let sel = select(
            &lease,
            &rows,
            owner,
            held.pid_exists && held.identity_matches,
        );
        let mine = Lease {
            resources: sel.take.iter().cloned().map(Row::Known).collect(),
            ..lease.clone()
        };
        for action in smix_lease::plan_cleanup(&mine) {
            let kind = action_kind(&action);
            if dry_run {
                println!("{id}: would close {kind}");
                taken += 1;
                continue;
            }
            let outcome = smix_capsule::reconcile::execute(std::slice::from_ref(&action));
            match outcome.first().map(ok) {
                Some(Ok(said)) => {
                    taken += 1;
                    println!("{id}: closed {kind} — {said}");
                    if let Some(sample) = sel.take.iter().find(|r| owners::kind_of(r) == kind)
                        && let Err(e) = smix_lease::store::drop_resource_kind(&leases, &id, sample)
                    {
                        eprintln!("warning: {id}: {kind} closed but still in the ledger: {e}");
                    }
                }
                Some(Err(said)) => {
                    failed += 1;
                    println!("{id}: did not close {kind} — {said}");
                }
                None => {}
            }
        }
        for (kind, why) in &sel.keep {
            kept += 1;
            println!("{id}: left {kind} — {why}");
        }
    }
    let verb = if dry_run { "would close" } else { "closed" };
    println!("{owner}: {verb} {taken}, left {kept}, failed {failed}");
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// How long a device smix booted may sit with nobody holding it and no
/// command touching it before it counts as left behind:
/// `SMIX_IDLE_DEVICE_SECS`, 30 minutes when unset, `0` for never. A task
/// between two runs holds nothing either, so the time is what tells a
/// pause from a device its task forgot.
pub(crate) fn idle_limit() -> Result<Option<u64>, String> {
    match std::env::var("SMIX_IDLE_DEVICE_SECS") {
        Err(_) => Ok(Some(30 * 60)),
        Ok(v) => match v.parse::<u64>() {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some(n)),
            Err(_) => Err(format!(
                "SMIX_IDLE_DEVICE_SECS={v:?}: expected a whole number of seconds"
            )),
        },
    }
}

/// Seconds `lease`'s device has sat idle, when it is left behind: running,
/// booted by smix, its holder gone, and untouched for `limit` seconds.
pub(crate) fn left_behind(
    lease: &Lease,
    running: bool,
    holder_alive: bool,
    now: &str,
    limit: u64,
) -> Option<u64> {
    let booted_by_us = lease
        .known_resources()
        .any(|r| matches!(r, Resource::Booted { by_us: true }));
    if !running || !booted_by_us || holder_alive {
        return None;
    }
    let at = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok();
    let idle = (at(now)? - at(&lease.heartbeat_at)?).num_seconds();
    u64::try_from(idle).ok().filter(|s| *s >= limit)
}

/// Close every device left behind, as a cleanup closes an owner's: the
/// lease's own plan, device last. Says each one, returns how many could
/// not be closed.
pub(crate) async fn reclaim_left_behind(dry_run: bool) -> Result<usize, String> {
    let Some(limit) = idle_limit()? else {
        return Ok(0);
    };
    let leases = smix_capsule::runner::machine_leases()?;
    let running = crate::status_cmd::running_ids().await;
    let now = smix_lease::store::now_rfc3339();
    let mut failed = 0;
    for id in leases.device_ids() {
        let Ok(Some(lease)) = smix_lease::store::read(&leases, &id) else {
            continue;
        };
        let held = smix_lease::store::probe(&lease.holder);
        let alive = held.pid_exists && held.identity_matches;
        let Some(idle) = left_behind(&lease, running.contains(&id), alive, &now, limit) else {
            continue;
        };
        let whose = owners::read(&leases, &id)
            .ok()
            .and_then(|o| o.names().first().map(|n| (*n).to_string()))
            .unwrap_or_else(|| "no owner".into());
        let idle = if idle < 120 {
            format!("{idle} s")
        } else {
            format!("{} min", idle / 60)
        };
        let what = format!("{id} ({whose}), idle {idle} with nobody holding it");
        if dry_run {
            println!("would reclaim {what}");
            continue;
        }
        let outcomes = smix_capsule::reconcile::execute(&smix_lease::plan_cleanup(&lease));
        let bad: Vec<String> = outcomes.iter().filter_map(|o| ok(o).err()).collect();
        if bad.is_empty() {
            eprintln!("reclaimed {what}");
            if let Err(e) = smix_lease::store::remove(&leases, &id) {
                eprintln!("warning: {id} was reclaimed and its ledger stays: {e}");
            }
        } else {
            failed += 1;
            eprintln!("could not reclaim {what}: {}", bad.join("; "));
        }
    }
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_lease::ProcIdentity;
    use smix_lease::owners::{OwnerRow, Owners};

    fn proc_() -> ProcIdentity {
        ProcIdentity {
            pid: 7,
            started_at: "Tue Nov  3 16:00:00 2026".into(),
            cmd: "smix run a.yaml".into(),
        }
    }

    fn lease() -> Lease {
        Lease {
            device_id: "SIM-1".into(),
            holder: proc_(),
            acquired_at: "2026-11-03T07:00:00Z".into(),
            heartbeat_at: "2026-11-03T07:00:00Z".into(),
            resources: vec![
                Row::Known(Resource::Booted { by_us: true }),
                Row::Known(Resource::Runner {
                    port: 22191,
                    proc: proc_(),
                    bundle: None,
                    log: None,
                }),
                Row::Known(Resource::Supervisor { proc: proc_() }),
            ],
        }
    }

    fn rows(pairs: &[(&str, &str)]) -> Owners {
        Owners {
            device_id: "SIM-1".into(),
            rows: pairs
                .iter()
                .map(|(k, o)| OwnerRow {
                    kind: (*k).into(),
                    owner: (*o).into(),
                    recorded_at: "t".into(),
                })
                .collect(),
        }
    }

    #[test]
    fn only_the_owners_resources_are_taken_and_the_rest_say_why_not() {
        let sel = select(
            &lease(),
            &rows(&[("booted", "task-a"), ("runner", "task-b")]),
            "task-b",
            false,
        );
        assert_eq!(sel.take.len(), 1);
        assert_eq!(owners::kind_of(&sel.take[0]), "runner");
        assert_eq!(
            sel.keep,
            vec![
                ("booted".into(), "task-a's".into()),
                ("supervisor".into(), "no owner".into())
            ]
        );
    }

    #[test]
    fn a_device_still_in_use_gives_up_nothing() {
        let sel = select(
            &lease(),
            &rows(&[("booted", "task-a"), ("runner", "task-a")]),
            "task-a",
            true,
        );
        assert!(sel.take.is_empty());
        assert!(
            sel.keep
                .iter()
                .any(|(k, why)| k == "runner" && why.contains("still running")),
            "{:?}",
            sel.keep
        );
    }

    #[test]
    fn left_behind_is_running_booted_by_smix_unheld_and_idle_long_enough() {
        let mut l = lease();
        l.heartbeat_at = "2026-11-03T07:00:00+00:00".into();
        let now = "2026-11-03T07:40:00+00:00";
        assert_eq!(left_behind(&l, true, false, now, 1800), Some(2400));
        assert_eq!(
            left_behind(&l, true, true, now, 1800),
            None,
            "somebody holds it"
        );
        assert_eq!(left_behind(&l, false, false, now, 1800), None, "it is off");
        assert_eq!(
            left_behind(&l, true, false, now, 3600),
            None,
            "not idle long enough"
        );
        l.resources = vec![Row::Known(Resource::Booted { by_us: false })];
        assert_eq!(
            left_behind(&l, true, false, now, 1800),
            None,
            "smix did not boot it"
        );
    }

    #[test]
    fn the_device_goes_last() {
        let sel = select(
            &lease(),
            &rows(&[("booted", "task-a"), ("runner", "task-a")]),
            "task-a",
            false,
        );
        let mine = Lease {
            resources: sel.take.into_iter().map(Row::Known).collect(),
            ..lease()
        };
        let kinds: Vec<&str> = smix_lease::plan_cleanup(&mine)
            .iter()
            .map(action_kind)
            .collect();
        assert_eq!(kinds.last(), Some(&"booted"), "{kinds:?}");
        assert!(kinds.contains(&"runner"), "{kinds:?}");
    }
}
