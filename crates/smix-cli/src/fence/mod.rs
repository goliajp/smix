//! The device lifecycle fence: devices are started, stopped and created
//! through smix, and anything else is refused or written down.
//!
//! Two halves. `smix guard lifecycle` judges a shell command before an
//! agent runs it (the plugin's hook pipes the command in). The runtime
//! half lives in `smix_capsule::device_fence` and refuses to drive a
//! device the ledger has no owner for, which also catches devices that a
//! script started where no hook could see.

pub(crate) mod plugin_freshness;
pub(crate) mod rules;
pub(crate) mod unanswered;

use std::io::{Read, Write};

/// Exit status for a refusal. Not 2: the hook shim maps this to the
/// hook's own "block" status, and anything else (an older smix without
/// this subcommand exits 2 from its argument parser) is told apart.
pub(crate) const REFUSED: u8 = 3;

/// What `smix guard` judges.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum GuardAction {
    /// Read a shell command on stdin; exit 3 when it starts, stops,
    /// erases or creates a device outside smix. The plugin's hook runs it.
    Lifecycle,
}

/// `smix doctor`'s fence checks: devices running outside smix, and a
/// plugin older than this binary. Appended to `checks`; true when any of
/// them should fail the command.
pub(crate) async fn doctor_checks(
    simctl: &smix_simctl::SimctlClient,
    checks: &mut Vec<crate::readiness::Check>,
) -> bool {
    use crate::readiness::{Check, Status};
    let booted = crate::booted_udids(simctl).await;
    let emulators = crate::android_devices()
        .into_iter()
        .filter(|a| unanswered::is_running_emulator(&a.serial, &a.state))
        .map(|a| a.serial)
        .collect::<Vec<_>>();
    let outside = unanswered::unanswered(booted.iter().chain(&emulators).map(String::as_str));
    let mut failed = false;
    checks.push(match unanswered::report(&outside) {
        Some(report) => {
            failed = true;
            Check {
                id: "devices-outside-smix",
                status: Status::Blocked,
                detail: report,
            }
        }
        None => Check {
            id: "devices-outside-smix",
            status: Status::Ok,
            detail: "every running device was started or claimed through smix".into(),
        },
    });
    checks.push(match plugin_freshness::doctor_verdict() {
        Ok(detail) => Check {
            id: "plugin-current",
            status: Status::Ok,
            detail,
        },
        Err(detail) => {
            failed = true;
            Check {
                id: "plugin-current",
                status: Status::Blocked,
                detail,
            }
        }
    });
    failed
}

/// Refuse to run against `device` when it is a running simulator or
/// emulator that smix neither booted nor was told to take on. Devices
/// that are off or physical are left to the paths that handle them.
pub(crate) async fn refuse_if_outside(
    simctl: &smix_simctl::SimctlClient,
    device: Option<&str>,
) -> Result<(), crate::CliError> {
    let Some(id) = device else { return Ok(()) };
    let running = id.starts_with("emulator-")
        || crate::booted_udids(simctl)
            .await
            .contains(&id.to_uppercase());
    if running {
        smix_capsule::device_fence::check(id).map_err(crate::CliError::Other)?;
    }
    Ok(())
}

/// `smix guard lifecycle`: read the command text on stdin, refuse a
/// lifecycle step taken outside smix, log the excepted ones.
pub(crate) fn guard_lifecycle() -> Result<u8, crate::CliError> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| crate::CliError::Other(format!("read the command on stdin: {e}")))?;
    match rules::judge(&text) {
        rules::Verdict::Allow => Ok(0),
        rules::Verdict::Excepted(rows) => {
            for (reason, command) in rows {
                log_exception("command", &command, &reason);
            }
            Ok(0)
        }
        rules::Verdict::Deny(why) => {
            eprintln!("smix fence: {why}");
            if let Some(stale) = plugin_freshness::hook_plugin_staleness() {
                eprintln!("smix fence: {stale}");
            }
            Ok(REFUSED)
        }
    }
}

/// Append one exception to the machine's log, so that going around the
/// fence always leaves a line somebody can read. A log that cannot be
/// written is said, and does not undo the exception.
pub(crate) fn log_exception(kind: &str, subject: &str, reason: &str) {
    let Some(root) = smix_lease::store::machine_root() else {
        eprintln!("warning: no machine directory; fence exception not logged");
        return;
    };
    let line = serde_json::json!({
        "at": smix_lease::store::now_rfc3339(),
        "kind": kind,
        "subject": subject,
        "reason": reason,
        "cwd": std::env::current_dir().ok(),
        "pid": std::process::id(),
    });
    let written = std::fs::create_dir_all(&root).and_then(|()| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("fence-exceptions.jsonl"))
            .and_then(|mut f| writeln!(f, "{line}"))
    });
    if let Err(e) = written {
        eprintln!("warning: fence exception not logged: {e}");
    }
}
