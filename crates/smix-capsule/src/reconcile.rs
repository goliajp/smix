//! Close what a dead holder left open, by the path it never got to take.
//!
//! [`smix_lease`] decides *what* is owed; this decides nothing and does
//! all of it. Every action re-verifies the process it is about to signal
//! before signalling it — the plan was written by a holder that is now
//! gone, and between then and now the kernel may have handed its pids to
//! somebody else. A cleanup that skipped that check would be the one bug
//! this whole mechanism exists to prevent, wearing the mechanism's own
//! uniform.

use smix_lease::{CleanupAction, ProcIdentity};
use std::path::Path;

/// What became of one owed close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Closed gracefully.
    Closed(String),
    /// The process was already gone — nothing to close, and that is a
    /// success: the ledger outliving its resource is the ordinary case
    /// after a hard kill.
    AlreadyGone(String),
    /// The pid is alive but is no longer what the ledger recorded.
    /// Deliberately not signalled.
    Skipped(String),
    /// The close was attempted and failed. Reported, never swallowed:
    /// the next holder needs to know the device is not clean.
    Failed(String),
}

impl Outcome {
    /// One line, for the report a person reads.
    pub fn line(&self) -> &str {
        match self {
            Outcome::Closed(s)
            | Outcome::AlreadyGone(s)
            | Outcome::Skipped(s)
            | Outcome::Failed(s) => s,
        }
    }

    /// Did this leave the device in a state the next holder can trust?
    pub fn is_clean(&self) -> bool {
        !matches!(self, Outcome::Failed(_))
    }
}

/// Is this pid still the process the ledger recorded?
fn still_the_same(recorded: &ProcIdentity) -> Result<bool, Outcome> {
    let probe = smix_lease::store::probe(recorded);
    if !probe.pid_exists {
        return Err(Outcome::AlreadyGone(format!(
            "pid {} already exited",
            recorded.pid
        )));
    }
    if !probe.identity_matches {
        return Err(Outcome::Skipped(format!(
            "pid {} is alive but is no longer the recorded process — not signalled",
            recorded.pid
        )));
    }
    Ok(true)
}

/// SIGINT and wait, so a `simctl` recording flushes its mp4 trailer.
///
/// The wait is the point: SIGINT then walking away leaves the encoder
/// mid-flush, which is the truncated file the hard kill would have
/// produced anyway.
fn stop_recording(path: &str, proc: &ProcIdentity) -> Outcome {
    if let Err(o) = still_the_same(proc) {
        return o;
    }
    // This crate denies unsafe; signalling goes through the same
    // `kill` shell-out `runner.rs` uses, so there is one spelling of
    // "send a signal" in here rather than two.
    crate::runner::signal(proc.pid, "-INT");
    for _ in 0..100 {
        if smix_lease::store::identify(proc.pid).is_none() {
            return Outcome::Closed(format!("recording stopped, {path} kept its trailer"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Outcome::Failed(format!(
        "recording pid {} ignored SIGINT for 10s — {path} may be truncated; \
         left running rather than hard-killed",
        proc.pid
    ))
}

/// Hand the runner to the teardown that already knows how to end an
/// XCUITest session without a crash dialog.
///
/// This deliberately calls the existing path rather than sending signals
/// here. A second implementation of "stop the runner" would be a second
/// place for the SIGINT-first discipline to be forgotten.
///
/// The teardown runs **whatever the probe found**, and that is the point.
/// A hard-killed `xcodebuild` leaves things its own death did not take
/// with it: the XCUITest session on the device, and whoever still answers
/// on the port. Returning early on "the launcher is already gone" would
/// declare the device clean while a session still occupies its automation
/// slot — the failure this whole mechanism exists to prevent.
///
/// Skipping is therefore about signals, not about the teardown: `down`
/// checks the recorded pid is still an `xcodebuild` before signalling it,
/// so a recycled pid is safe to hand it. The probe here only decides how
/// to describe what happened.
fn stop_runner(root: &Path, port: u16, proc: &ProcIdentity) -> Outcome {
    let probe = smix_lease::store::probe(proc);
    let how = if !probe.pid_exists {
        "launcher already exited; cleared the port and the session it left"
    } else if !probe.identity_matches {
        "launcher's pid was reused — not signalled; cleared the port instead"
    } else {
        "stopped SIGINT-first"
    };
    // No: reconcile acts on a ledger row, so the runner it is closing
    // is recorded by definition. Anything else on the port is not the
    // session being settled.
    match crate::runner::down(root, port) {
        Ok(()) => Outcome::Closed(format!("runner on port {port}: {how}")),
        Err(e) => Outcome::Failed(format!("runner on port {port} did not stop: {e}")),
    }
}

/// Stop the supervisor sidecar.
///
/// Emitted before the runner it watches, and the order is the whole
/// point: a supervisor's job is to bring a dead runner back, so stopping
/// the runner while its supervisor is alive is a teardown that undoes
/// itself. `runner.rs` learned this once already and cascades the
/// supervisor first; the ledger encodes the same order so a reconcile
/// running hours later does not have to rediscover it.
///
/// SIGTERM then SIGKILL after 5 s, the same ladder `runner::down` uses.
fn stop_supervisor(proc: &ProcIdentity) -> Outcome {
    if let Err(o) = still_the_same(proc) {
        return o;
    }
    crate::runner::signal(proc.pid, "-TERM");
    for _ in 0..20 {
        if smix_lease::store::identify(proc.pid).is_none() {
            return Outcome::Closed(format!("supervisor pid {} stopped", proc.pid));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    crate::runner::signal(proc.pid, "-9");
    Outcome::Closed(format!(
        "supervisor pid {} ignored SIGTERM for 5s — escalated to SIGKILL",
        proc.pid
    ))
}

/// Stop an Android instrumentation runner.
///
/// Hands off to the same teardown `smix runner down --platform android`
/// performs, which ends the on-device server rather than the host-side
/// client — killing the client would leave the server running and the
/// port still answering. Every call it makes names the serial, which is
/// why the ledger row carries one.
fn stop_android_runner(root: &Path, port: u16, serial: &str, proc: &ProcIdentity) -> Outcome {
    let probe = smix_lease::store::probe(proc);
    let how = if !probe.pid_exists {
        "host-side process already exited; ended the on-device server and freed the port"
    } else if !probe.identity_matches {
        "host-side pid was reused — not signalled; ended the on-device server instead"
    } else {
        "stopped"
    };
    match crate::runner_android::down(root, serial, port) {
        Ok(()) => Outcome::Closed(format!("android runner on {serial}:{port}: {how}")),
        Err(e) => Outcome::Failed(format!(
            "android runner on {serial}:{port} did not stop: {e}"
        )),
    }
}

/// Stop a port forwarder.
///
/// The listener lives inside a process, so ending the process ends it.
/// SIGTERM rather than SIGINT: nothing is buffering output that needs
/// flushing, and a forwarder that has already gone with its process is
/// the ordinary case, not a failure.
pub(crate) fn stop_port_forward(local_port: u16, proc: &ProcIdentity) -> Outcome {
    if let Err(o) = still_the_same(proc) {
        return o;
    }
    crate::runner::signal(proc.pid, "-TERM");
    for _ in 0..20 {
        if smix_lease::store::identify(proc.pid).is_none() {
            return Outcome::Closed(format!("port forward on {local_port} stopped"));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Outcome::Failed(format!(
        "port forward on {local_port} (pid {}) ignored SIGTERM for 5s",
        proc.pid
    ))
}

fn shutdown_sim(udid: &str) -> Outcome {
    let out = std::process::Command::new("xcrun")
        .args(["simctl", "shutdown", udid])
        .output();
    match out {
        Ok(o) if o.status.success() => Outcome::Closed(format!("simulator {udid} shut down")),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // Already off is the state we wanted.
            if stderr.contains("Unable to shutdown device in current state: Shutdown") {
                Outcome::AlreadyGone(format!("simulator {udid} was already shut down"))
            } else {
                Outcome::Failed(format!("simctl shutdown {udid} failed: {}", stderr.trim()))
            }
        }
        Err(e) => Outcome::Failed(format!("could not run simctl shutdown {udid}: {e}")),
    }
}

/// Perform the owed closes, in the order given.
///
/// Order is not an optimisation: the recording is stopped before the
/// runner that was driving what it recorded, and the device is shut down
/// only once nothing is left talking to it.
pub fn execute(root: &Path, actions: &[CleanupAction]) -> Vec<Outcome> {
    actions
        .iter()
        .map(|a| match a {
            CleanupAction::StopRecording { path, proc } => stop_recording(path, proc),
            CleanupAction::StopRunner { port, proc } => stop_runner(root, *port, proc),
            CleanupAction::StopSupervisor { proc } => stop_supervisor(proc),
            CleanupAction::StopPortForward { local_port, proc } => {
                stop_port_forward(*local_port, proc)
            }
            CleanupAction::StopAndroidRunner { port, serial, proc } => {
                stop_android_runner(root, *port, serial, proc)
            }
            CleanupAction::ShutdownSim { udid } => shutdown_sim(udid),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ghost() -> ProcIdentity {
        ProcIdentity {
            pid: 0,
            started_at: "Thu Aug  6 10:00:00 2026".into(),
            cmd: "long gone".into(),
        }
    }

    fn impostor() -> ProcIdentity {
        ProcIdentity {
            pid: std::process::id(),
            started_at: "Thu Jan  1 00:00:00 1970".into(),
            cmd: "not us".into(),
        }
    }

    #[test]
    fn an_exited_process_is_already_gone_not_a_failure() {
        let outcomes = execute(
            Path::new("/nonexistent"),
            &[CleanupAction::StopRecording {
                path: "x.mov".into(),
                proc: ghost(),
            }],
        );
        assert!(matches!(outcomes[0], Outcome::AlreadyGone(_)));
        assert!(outcomes[0].is_clean());
    }

    #[test]
    fn a_recycled_pid_is_never_signalled() {
        // If this ever regresses to sending a signal, it signals the test
        // process itself — which is exactly the blast radius in the field.
        // Port 1 has no runner, so `down` finds nothing to act on.
        let outcomes = execute(
            Path::new("/nonexistent"),
            &[CleanupAction::StopRunner {
                port: 1,
                proc: impostor(),
            }],
        );
        match &outcomes[0] {
            Outcome::Closed(msg) => assert!(msg.contains("pid was reused")),
            other => panic!("expected Closed with a reused-pid note, got {other:?}"),
        }
    }

    #[test]
    fn a_dead_launcher_still_gets_the_port_cleared() {
        // The regression this pins: returning early on "already gone"
        // would report a clean device while the XCUITest session the
        // hard kill left behind still holds its automation slot.
        let outcomes = execute(
            Path::new("/nonexistent"),
            &[CleanupAction::StopRunner {
                port: 1,
                proc: ghost(),
            }],
        );
        match &outcomes[0] {
            Outcome::Closed(msg) => assert!(msg.contains("cleared the port")),
            other => panic!("expected Closed, got {other:?}"),
        }
    }

    #[test]
    fn outcomes_come_back_in_the_order_given() {
        let outcomes = execute(
            Path::new("/nonexistent"),
            &[
                CleanupAction::StopRecording {
                    path: "first.mov".into(),
                    proc: ghost(),
                },
                CleanupAction::StopRunner {
                    port: 1,
                    proc: impostor(),
                },
            ],
        );
        assert!(outcomes[0].line().contains("already exited"));
        assert!(outcomes[1].line().contains("pid was reused"));
    }
}

/// The [`smix_lease::CleanupExecutor`] this crate provides.
///
/// Exists so a caller that must gate device access — the SDK — can be
/// handed the ability to settle an abandoned session without taking on a
/// dependency on runner lifecycles it has no other use for.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reconciler;

impl smix_lease::CleanupExecutor for Reconciler {
    fn execute(&self, root: &Path, actions: &[CleanupAction]) -> Vec<smix_lease::CleanupReport> {
        execute(root, actions)
            .into_iter()
            .map(|o| smix_lease::CleanupReport {
                line: o.line().to_string(),
                clean: o.is_clean(),
            })
            .collect()
    }
}
