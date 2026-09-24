//! Noticing a device that left, and reading back the ones that did.
//!
//! The judgement is in `smix_lease::vanish`, pure. This is the I/O around
//! it: ask adb and simctl what is here, compare every ledger, keep what
//! left, and say so once.

use smix_lease::store::{self, LeaseDir};
use smix_lease::vanish::{self, AdbDevice, LiveDevices, Presence, Vanished};

/// How much of a dead emulator's console to keep with its departure.
///
/// The last screenful: enough to hold a qemu fatal line and the few that
/// led to it, not so much that one departure drowns the history.
const CONSOLE_TAIL_LINES: usize = 20;

/// Compare every ledger with what the machine says is here, and keep a
/// record of each device that is gone.
///
/// Called before the `run`, `runner`, `sim` and `lease` verbs — the
/// commands that touch devices — so a departure is noticed by the next
/// one of them, whoever runs it. A failure here does not stop the verb
/// the user asked for, and it is not swallowed either: it is said.
pub async fn notice(noticed_by: &str) {
    let Ok(leases) = smix_capsule::runner::machine_leases() else {
        // No machine directory means no ledgers to compare; the verb
        // itself says so if it needs one.
        return;
    };
    let ids = leases.device_ids();
    if ids.is_empty() {
        return;
    }
    let live = ask_what_is_here(&ids).await;
    for id in ids {
        if let Err(e) = notice_one(&leases, &id, &live, noticed_by) {
            eprintln!("warning: could not check whether {id} is still here: {e}");
        }
    }
}

fn notice_one(
    leases: &LeaseDir,
    id: &str,
    live: &LiveDevices,
    noticed_by: &str,
) -> Result<(), store::LeaseError> {
    let facts = store::collect_facts(leases, id)?;
    let Some(held) = &facts.existing else {
        return Ok(());
    };
    let Presence::Gone { slot_now } = vanish::presence(&held.lease, live) else {
        return Ok(());
    };
    let console_log = held.lease.known_resources().find_map(|r| match r {
        smix_lease::Resource::Emulator { console_log, .. } => console_log.clone(),
        _ => None,
    });
    let tail = console_log.as_deref().map(console_tail).unwrap_or_default();
    let v = vanish::vanished_from(held, slot_now, &store::now_rfc3339(), noticed_by, tail);
    if vanish::record(leases, &v)? {
        eprintln!(
            "note: {} — recorded; `smix lease history` has it",
            one_line(&v)
        );
    }
    Ok(())
}

/// What adb and simctl say is here. Each is asked only when a ledger
/// names a device of its kind; each answers `None` when it could not be
/// asked, which `presence` reads as "cannot tell", never as "gone".
pub(crate) async fn ask_what_is_here(ids: &[String]) -> LiveDevices {
    let wants_apple = ids.iter().any(|id| smix_simctl::registry::is_udid(id));
    let wants_adb = ids.iter().any(|id| !smix_simctl::registry::is_udid(id));
    let adb = wants_adb
        .then(|| smix_adb::AdbClient::new().live_devices())
        .flatten()
        .map(|ds| {
            ds.into_iter()
                .map(|(serial, avd)| AdbDevice { serial, avd })
                .collect()
        });
    let simulators = if wants_apple {
        smix_simctl::SimctlClient::new()
            .list_devices()
            .await
            .ok()
            .map(|ds| {
                ds.into_iter()
                    .map(|d| (d.udid, d.state == "Booted"))
                    .collect()
            })
    } else {
        None
    };
    LiveDevices { adb, simulators }
}

/// The last lines of a console log, or a line saying why there are none.
fn console_tail(path: &str) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let from = lines.len().saturating_sub(CONSOLE_TAIL_LINES);
            lines[from..].iter().map(|l| (*l).to_string()).collect()
        }
        Err(e) => vec![format!("(the console log {path} could not be read: {e})")],
    }
}

/// One departure in one line: what, when, whose.
fn one_line(v: &Vanished) -> String {
    let what = match &v.avd {
        Some(avd) => format!("{} (AVD {avd})", v.device_id),
        None => v.device_id.clone(),
    };
    let slot = match &v.slot_now {
        Some(other) => format!("; its port now answers for `{other}`"),
        None => String::new(),
    };
    let holder = if v.holder_alive {
        "still running"
    } else {
        "no longer running"
    };
    format!(
        "{what} left without smix hearing about it{slot}. Last heard from at {}, \
         held by pid {} (`{}`, {holder}){}",
        v.last_heartbeat,
        v.holder.pid,
        v.holder.cmd,
        if v.booted_by_smix {
            "; smix booted it"
        } else {
            ""
        }
    )
}

/// `smix lease history`.
pub fn print_history(leases: &LeaseDir, json: bool) -> Result<(), crate::CliError> {
    let kept = vanish::history(leases).map_err(|e| crate::CliError::Other(e.to_string()))?;
    if json {
        let text = serde_json::to_string_pretty(&kept)
            .map_err(|e| crate::CliError::Other(e.to_string()))?;
        println!("{text}");
        return Ok(());
    }
    if kept.is_empty() {
        println!(
            "no device has left without smix hearing about it (history: {})",
            leases.path().join(vanish::HISTORY_FILE).display()
        );
        return Ok(());
    }
    for v in &kept {
        println!("{}  noticed by `{}`", v.noticed_at, v.noticed_by);
        println!("  {}", one_line(v));
        if let Some(log) = &v.console_log {
            println!("  console: {log}");
        }
        for line in &v.console_tail {
            println!("    | {line}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_console_log_says_so_instead_of_leaving_nothing() {
        let tail = console_tail("/nonexistent/smix-console-for-a-test.log");
        assert_eq!(tail.len(), 1);
        assert!(tail[0].contains("could not be read"), "{tail:?}");
    }

    #[test]
    fn a_long_console_keeps_only_its_last_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("c.log");
        let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, text).expect("write");
        let tail = console_tail(&path.to_string_lossy());
        assert_eq!(tail.len(), CONSOLE_TAIL_LINES);
        assert_eq!(tail.last().map(String::as_str), Some("line 49"));
    }
}
