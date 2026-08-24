//! `smix record start | stop | status` — screen recording as something the
//! ledger knows about.
//!
//! Recording used to live entirely in one process's memory: a
//! `tokio::process::Child` inside whoever called `start_recording`. That
//! made two questions unanswerable. "Is this device recording right now?"
//! had no one to ask. And when the process holding the handle was killed,
//! the `simctl io recordVideo` child kept writing into an mp4 that would
//! never get its trailer — because the trailer is written on SIGINT, and
//! nothing left alive knew there was a process to send one to.
//!
//! A ledger row answers both.

use crate::RecordAction;
use smix_lease::{Resource, store};
use smix_sdk::device_control::DeviceControl;
use std::path::Path;

/// What `status` found, said in a sentence.
///
/// Pure so the wording can be tested. "Is it recording" is not the
/// question people actually have — they want to know where the file is
/// going, and a yes/no answer sends them to `ls` to guess.
pub fn describe_status(device_id: &str, lease: Option<&smix_lease::Lease>) -> String {
    let recording = lease.and_then(|l| {
        l.known_resources().find_map(|r| match r {
            Resource::Recording { path, proc } => Some((path, proc)),
            _ => None,
        })
    });
    match recording {
        None => format!("{device_id}: not recording"),
        Some((path, proc)) => {
            let live = store::probe(proc);
            if live.pid_exists && live.identity_matches {
                format!("{device_id}: recording to {path} (pid {})", proc.pid)
            } else {
                // The row outliving its process is exactly the state
                // reconcile exists for, and saying "recording" here would
                // send someone looking for a file that stopped growing.
                format!(
                    "{device_id}: a recording to {path} was left behind by pid {} — \
                     run `smix lease reconcile {device_id}` to close it",
                    proc.pid
                )
            }
        }
    }
}

/// Run the subcommand.
pub async fn run(root: &Path, action: RecordAction) -> Result<(), crate::CliError> {
    // The ledgers are the machine's; `root` is still the tree, and is
    // used only for settling a dead holder's build products.
    let leases = smix_capsule::runner::machine_leases().map_err(crate::CliError::Other)?;
    match action {
        RecordAction::Start { device, output } => {
            let udid = crate::resolve_device(&device)?;
            let control = smix_sdk::ios_device::IosDeviceControl::new();
            let leased = smix_sdk::leased::Leased::acquire(
                &control,
                root,
                &leases,
                &udid,
                &smix_capsule::reconcile::Reconciler,
            )
            .map_err(|e| crate::CliError::Other(e.to_string()))?;
            for report in leased.settled() {
                println!("settled first: {}", report.line);
            }
            leased.start_recording(&output).await?;
            // Hand the holder role to the recording itself.
            //
            // This command is about to exit, and a ledger whose holder is
            // a dead process with a live recording reads as an orphan —
            // so the next smix command would stop a recording somebody
            // deliberately started. Naming the recording process as the
            // holder makes the ledger say the true thing: this device is
            // busy for exactly as long as that process runs.
            if let Some(pid) = control.recording_pid().await
                && let Some(proc) = store::identify(pid)
                && let Err(e) = store::set_holder(&leases, &udid, proc)
            {
                eprintln!(
                    "warning: recording started but the ledger still names this command as holder: {e}"
                );
            }
            // `release` is deliberately not called. The recording outlives
            // this command, and releasing drops the process-backed rows —
            // which would delete the row that is the only cross-process
            // handle on the recording just started. Letting the guard fall
            // out of scope is the whole of "keep it": there is no `Drop`
            // on it, by design, precisely so that this reads as a choice
            // rather than as something forgotten.
            println!("recording {} to {}", udid, output.display());
        }
        RecordAction::Stop { device } => {
            let udid = crate::resolve_device(&device)?;
            // `stop` cannot go through `IosDeviceControl::stop_recording`:
            // that reads the in-memory handle, and this is a different
            // process from the one that started it. The ledger row is the
            // only handle across a process boundary, so this closes it the
            // same way an abandoned one is closed.
            let Some(lease) = store::read(&leases, &udid).map_err(to_cli_error)? else {
                println!("{udid}: not recording");
                return Ok(());
            };
            let row = lease.known_resources().find_map(|r| match r {
                Resource::Recording { path, proc } => Some((path.clone(), proc.clone())),
                _ => None,
            });
            let Some((path, proc)) = row else {
                println!("{udid}: not recording");
                return Ok(());
            };
            let outcomes = smix_capsule::reconcile::execute(
                root,
                &[smix_lease::CleanupAction::StopRecording {
                    path: path.clone(),
                    proc,
                }],
            );
            for o in &outcomes {
                println!("  {}", o.line());
            }
            if outcomes
                .iter()
                .all(smix_capsule::reconcile::Outcome::is_clean)
            {
                store::drop_resource_kind(
                    &leases,
                    &udid,
                    &Resource::Recording {
                        path: String::new(),
                        proc: store::identify_self(),
                    },
                )
                .map_err(to_cli_error)?;
                println!("stopped: {path}");
            }
        }
        RecordAction::Status { device } => {
            let udid = crate::resolve_device(&device)?;
            let lease = store::read(&leases, &udid).map_err(to_cli_error)?;
            println!("{}", describe_status(&udid, lease.as_ref()));
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
    use smix_lease::{Lease, ProcIdentity};

    fn lease_with(resources: Vec<Resource>) -> Lease {
        Lease {
            device_id: "UDID-1".into(),
            holder: store::identify_self(),
            acquired_at: "2026-08-06T10:00:00Z".into(),
            heartbeat_at: "2026-08-06T10:00:00Z".into(),
            resources: resources.into_iter().map(smix_lease::Row::Known).collect(),
        }
    }

    #[test]
    fn no_ledger_means_not_recording() {
        assert_eq!(describe_status("UDID-1", None), "UDID-1: not recording");
    }

    #[test]
    fn a_ledger_without_a_recording_row_means_not_recording() {
        let lease = lease_with(vec![Resource::Booted { by_us: true }]);
        assert_eq!(
            describe_status("UDID-1", Some(&lease)),
            "UDID-1: not recording"
        );
    }

    #[test]
    fn a_live_recording_says_where_it_is_writing() {
        // "yes" alone sends people to `ls` to guess which file it is.
        let lease = lease_with(vec![Resource::Recording {
            path: "/tmp/run.mov".into(),
            proc: store::identify_self(),
        }]);
        let msg = describe_status("UDID-1", Some(&lease));
        assert!(msg.contains("/tmp/run.mov"), "got: {msg}");
        assert!(msg.contains("recording to"), "got: {msg}");
    }

    #[test]
    fn a_row_whose_process_is_gone_is_not_reported_as_recording() {
        // The file stopped growing when that process died. Calling it
        // "recording" would have someone wait for footage that is not
        // coming.
        let lease = lease_with(vec![Resource::Recording {
            path: "/tmp/run.mov".into(),
            proc: ProcIdentity {
                pid: 0,
                started_at: "Thu Aug  6 10:00:00 2026".into(),
                cmd: "simctl io recordVideo".into(),
            },
        }]);
        let msg = describe_status("UDID-1", Some(&lease));
        assert!(msg.contains("left behind"), "got: {msg}");
        assert!(msg.contains("lease reconcile"), "the way out must be named");
    }
}
