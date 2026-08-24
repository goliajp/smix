//! What is running on this machine, next to what the ledgers claim.
//!
//! A lease records what somebody *said* they opened. `lsof` shows what
//! is *actually* listening. The 2026-08-11 incident is those two coming
//! apart: a runner held port 22087 and the ledger the rule told us to
//! consult had no record of it, so it could neither be confirmed an
//! orphan nor killed.
//!
//! Both sides, side by side, and when only one has something the row
//! says which one. The judging is separated from the probing for the
//! reason the rest of this crate separates them: a rule reachable only
//! when some session happens to be alive on some machine is a rule
//! nobody checks.
//!
//! **The pairing key is (device, port), never a pid.** A healthy iOS
//! runner has two of them: `runner up` spawns `xcodebuild` on the host
//! and records *that* pid in the ledger, while the socket is held by the
//! `SmixRunner` app inside the simulator — a different process
//! (measured: ledger 14176, listener 14209). Pairing by pid would split
//! one live runner into a ledger-only row and a process-only row, which
//! for this command is worse than printing nothing.

use smix_lease::{Lease, Resource};

/// A smix runner socket found on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    /// The port it holds.
    pub port: u16,
    /// The simulator it belongs to, read out of the app's command line.
    pub device_id: String,
    /// The process holding the socket — the app, inside the simulator.
    pub app_pid: u32,
    /// The `xcodebuild` session driving it, when one can be found.
    ///
    /// `None` is not "there is none": it is "nothing here could name
    /// it". A runner whose session has exited keeps serving.
    pub session_pid: Option<u32>,
}

/// Which side of the machine a runner turned up on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// The ledger records it and it is listening.
    Both {
        /// The process holding the socket.
        app_pid: u32,
        /// The session the ledger names.
        ledger_session_pid: u32,
        /// The session found alongside the socket, if one was.
        live_session_pid: Option<u32>,
    },
    /// The ledger records it; nothing is listening on that port.
    LedgerOnly {
        /// The session the ledger names.
        ledger_pid: u32,
    },
    /// It is listening and no ledger on this machine mentions it.
    ///
    /// The shape of the incident. `named_by` carries a tree that has a
    /// record of it — evidence about where to look, never authority:
    /// what a checkout holds was written by whatever smix that tree last
    /// ran.
    ProcessOnly {
        /// The process holding the socket.
        app_pid: u32,
        /// The session driving it, if one was found.
        session_pid: Option<u32>,
        /// A checkout that names this device, with its path.
        named_by: Option<String>,
    },
    /// The ledger records it and the process it recorded is gone.
    ///
    /// The Android counterpart of `LedgerOnly`, and a separate verdict
    /// because the evidence is different and the sentence must say
    /// which. An iOS row is judged by whether anything is listening on
    /// the port; an Android one has no listener, so it is judged by
    /// whether the recorded process is still that process — pid AND
    /// start time, so a recycled number reads as gone rather than as
    /// the runner. Rendering "nothing is listening there" over this
    /// would describe a probe nobody ran.
    ProcessGone {
        /// The session the ledger names, now ended.
        ledger_pid: u32,
    },
    /// The ledger records it and this command has no probe for it.
    ///
    /// Said rather than left out. §9 #1 ③: a capability that is not
    /// available must be loud, because "quietly doing nothing" is worse
    /// than "this device cannot do that" — and an Android runner
    /// silently filed as `LedgerOnly` would read as "it has gone away".
    NotProbed {
        /// The session the ledger names.
        ledger_pid: u32,
        /// Why there is no probe.
        why: &'static str,
    },
}

/// One port, one device, and which side knows about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerRow {
    /// The port.
    pub port: u16,
    /// The device it is bound to.
    pub device_id: String,
    /// Where it turned up.
    pub seen: Seen,
}

/// Fold the machine's ledgers and the live listeners into one list.
///
/// All three inputs are required. There is no shape of this call that
/// reads one side only — that is the rule "this command cannot answer
/// from the ledger alone, nor from the process table alone" handed to
/// the compiler rather than to a scan, after a cycle in which changing
/// what an argument meant left twenty-five call sites compiling.
///
/// `live_pids` is the Android half of the same rule. An iOS runner is
/// found by its listener; an Android one has none to find, and the
/// command answered `not-probed` about every Android row — always, by
/// construction. A consumer read four of those and checked each pid by
/// hand; all four were long dead. The ledger held the answer the whole
/// time: `AndroidRunner` records a `ProcIdentity`, which is a pid AND the
/// start time that tells a recycled pid from the real one.
///
/// Membership means "a process at that pid exists AND its start time
/// still matches the ledger" — the caller does that check, because it is
/// I/O and this function reads nothing. A bare pid set would report
/// whoever inherited the number, which is the failure `ProcIdentity`
/// exists to prevent.
///
/// `checkout` never classifies anything. It can appear in `named_by` and
/// nowhere else: a tree's book is evidence about where a runner came
/// from, and the rule since 2026-08-11 is that it may stop a decision,
/// not make one. Here it does not even stop one.
#[must_use]
pub fn attribute(
    machine: &[(String, Lease)],
    listeners: &[Listener],
    checkout: &[(String, Lease)],
    live_pids: &std::collections::HashSet<u32>,
) -> Vec<RunnerRow> {
    let mut rows: Vec<RunnerRow> = Vec::new();

    for (device_id, lease) in machine {
        for resource in lease.known_resources() {
            let (port, ledger_pid, probe) = match resource {
                Resource::Runner { port, proc } => (*port, proc.pid, Probe::Ios),
                Resource::AndroidRunner { port, proc, .. } => (
                    *port,
                    proc.pid,
                    if live_pids.contains(&proc.pid) {
                        Probe::AndroidAlive
                    } else {
                        Probe::AndroidGone
                    },
                ),
                _ => continue,
            };
            let seen = match probe {
                // The process the ledger recorded is still that process.
                // There is no listener to pair it with, so the pid is
                // both halves: it is the session and it is running.
                Probe::AndroidAlive => Seen::Both {
                    app_pid: ledger_pid,
                    ledger_session_pid: ledger_pid,
                    live_session_pid: Some(ledger_pid),
                },
                Probe::AndroidGone => Seen::ProcessGone { ledger_pid },
                Probe::Ios => match listeners
                    .iter()
                    .find(|l| l.port == port && l.device_id.eq_ignore_ascii_case(device_id))
                {
                    Some(l) => Seen::Both {
                        app_pid: l.app_pid,
                        ledger_session_pid: ledger_pid,
                        live_session_pid: l.session_pid,
                    },
                    None => Seen::LedgerOnly { ledger_pid },
                },
            };
            rows.push(RunnerRow {
                port,
                device_id: device_id.clone(),
                seen,
            });
        }
    }

    for l in listeners {
        if rows
            .iter()
            .any(|r| r.port == l.port && r.device_id.eq_ignore_ascii_case(&l.device_id))
        {
            continue;
        }
        let named_by = checkout
            .iter()
            .find(|(device_id, _)| device_id.eq_ignore_ascii_case(&l.device_id))
            .map(|(_, lease)| lease.device_id.clone());
        rows.push(RunnerRow {
            port: l.port,
            device_id: l.device_id.clone(),
            seen: Seen::ProcessOnly {
                app_pid: l.app_pid,
                session_pid: l.session_pid,
                named_by,
            },
        });
    }

    rows.sort_by_key(|r| (r.port, r.device_id.clone()));
    rows
}

/// Every smix runner socket this machine is holding open.
///
/// One `lsof` over all listening TCP sockets, then a filter: a socket
/// counts when the process holding it is running out of a simulator's
/// container path, which is what makes it *this* device's runner rather
/// than any other program on the port. `udid_from_device_path` already
/// carried that judgement for the teardown guard; this is the same
/// judgement asked machine-wide instead of one port at a time.
///
/// The session pid is looked up separately, and its absence means "no
/// `xcodebuild` here answers for that device" rather than "there is
/// none": a session that has exited leaves its runner serving, which is
/// the ordinary state after `runner up` returns.
#[must_use]
pub fn listeners() -> Vec<Listener> {
    let Ok(out) = std::process::Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-FpPn"])
        .output()
    else {
        return Vec::new();
    };
    // `-F` output is one field per line, tagged by its first character:
    // `p<pid>` opens a process, `n<name>` gives an address per socket.
    let text = String::from_utf8_lossy(&out.stdout);
    let mut found: Vec<(u32, u16)> = Vec::new();
    let mut pid: Option<u32> = None;
    for line in text.lines() {
        match line.as_bytes().first() {
            Some(b'p') => pid = line[1..].parse().ok(),
            Some(b'n') => {
                if let Some(p) = pid
                    && let Some(port) = line.rsplit(':').next().and_then(|s| s.parse::<u16>().ok())
                {
                    found.push((p, port));
                }
            }
            _ => {}
        }
    }

    let sessions: Vec<(u32, String)> = pgrep_f("xcodebuild.*SmixRunner")
        .into_iter()
        .filter_map(|p| crate::runner::pid_command(p).map(|c| (p, c)))
        .collect();

    let mut out_rows: Vec<Listener> = Vec::new();
    for (app_pid, port) in found {
        let Some(cmd) = crate::runner::pid_command(app_pid) else {
            continue;
        };
        let Some(device_id) = crate::runner::udid_from_device_path(&cmd) else {
            continue;
        };
        let session_pid = sessions
            .iter()
            .find(|(_, c)| crate::runner::xcodebuild_drives_udid(c, &device_id))
            .map(|(p, _)| *p);
        if out_rows
            .iter()
            .any(|l| l.port == port && l.device_id == device_id)
        {
            continue;
        }
        out_rows.push(Listener {
            port,
            device_id,
            app_pid,
            session_pid,
        });
    }
    out_rows.sort_by_key(|l| (l.port, l.device_id.clone()));
    out_rows
}

fn pgrep_f(pattern: &str) -> Vec<u32> {
    let Ok(out) = std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

enum Probe {
    /// Find the listener on the port and pair it with the ledger.
    Ios,
    /// The recorded process is still that process. An Android runner has
    /// no listener to find, so its own pid is the evidence.
    AndroidAlive,
    /// The recorded process is gone, or its pid now belongs to something
    /// else. Either way this row describes a session that has ended.
    AndroidGone,
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_lease::ProcIdentity;

    fn proc(pid: u32) -> ProcIdentity {
        ProcIdentity {
            pid,
            started_at: "Tue Aug 11 20:32:21 2026".into(),
            cmd: "xcodebuild test".into(),
        }
    }

    fn ledger(device: &str, resource: Resource) -> (String, Lease) {
        (
            device.to_string(),
            Lease {
                device_id: device.to_string(),
                holder: proc(36931),
                acquired_at: "2026-08-11T20:32:00Z".into(),
                heartbeat_at: "2026-08-11T20:32:00Z".into(),
                resources: vec![smix_lease::Row::Known(resource)],
            },
        )
    }

    fn listener(device: &str, port: u16, app: u32, session: Option<u32>) -> Listener {
        Listener {
            port,
            device_id: device.into(),
            app_pid: app,
            session_pid: session,
        }
    }

    /// The ordinary case, and the one pid-pairing would have broken.
    #[test]
    fn a_recorded_runner_that_is_listening_is_one_row() {
        let rows = attribute(
            &[ledger(
                "D",
                Resource::Runner {
                    port: 22087,
                    proc: proc(14176),
                },
            )],
            &[listener("D", 22087, 14209, Some(14176))],
            &[],
            &std::collections::HashSet::new(),
        );
        assert_eq!(rows.len(), 1, "one runner, one row: {rows:?}");
        assert_eq!(
            rows[0].seen,
            Seen::Both {
                app_pid: 14209,
                ledger_session_pid: 14176,
                live_session_pid: Some(14176),
            }
        );
    }

    /// The app pid and the ledger's pid are different by construction.
    ///
    /// `runner up` records the `xcodebuild` it spawned; the socket is
    /// held by the app inside the simulator. Two pids for one runner is
    /// the healthy state, not a disagreement — so this must still be one
    /// row even when the live session cannot be matched to the recorded
    /// one.
    #[test]
    fn a_ledger_naming_another_session_is_still_one_row() {
        let rows = attribute(
            &[ledger(
                "D",
                Resource::Runner {
                    port: 22087,
                    proc: proc(99999),
                },
            )],
            &[listener("D", 22087, 14209, Some(14176))],
            &[],
            &std::collections::HashSet::new(),
        );
        assert_eq!(rows.len(), 1, "still one runner: {rows:?}");
        match &rows[0].seen {
            Seen::Both {
                ledger_session_pid,
                live_session_pid,
                ..
            } => {
                assert_eq!(*ledger_session_pid, 99999);
                assert_eq!(*live_session_pid, Some(14176));
            }
            other => panic!("expected Both, got {other:?}"),
        }
    }

    /// Written down, and nothing is there.
    #[test]
    fn a_recorded_runner_with_no_listener_is_ledger_only() {
        let rows = attribute(
            &[ledger(
                "D",
                Resource::Runner {
                    port: 22087,
                    proc: proc(99120),
                },
            )],
            &[],
            &[],
            &std::collections::HashSet::new(),
        );
        assert_eq!(rows[0].seen, Seen::LedgerOnly { ledger_pid: 99120 });
    }

    /// The incident: listening, and no ledger on this machine has it.
    #[test]
    fn a_listener_no_machine_ledger_knows_is_process_only() {
        let rows = attribute(
            &[],
            &[listener("E", 22300, 14209, Some(14176))],
            &[],
            &std::collections::HashSet::new(),
        );
        assert_eq!(
            rows[0].seen,
            Seen::ProcessOnly {
                app_pid: 14209,
                session_pid: Some(14176),
                named_by: None,
            }
        );
    }

    /// A tree that has a record of it is named — as evidence.
    #[test]
    fn a_checkout_that_knows_the_device_is_named_not_obeyed() {
        let rows = attribute(
            &[],
            &[listener("E", 22300, 14209, None)],
            &[ledger(
                "E",
                Resource::Runner {
                    port: 22300,
                    proc: proc(14176),
                },
            )],
            &std::collections::HashSet::new(),
        );
        match &rows[0].seen {
            Seen::ProcessOnly { named_by, .. } => {
                assert_eq!(named_by.as_deref(), Some("E"));
            }
            other => panic!(
                "a checkout naming the device must show up as evidence on the \
                 process-only row, not change what the row is: {other:?}"
            ),
        }
        assert_eq!(rows.len(), 1, "the checkout must not add a row: {rows:?}");
    }

    /// Android is said, not omitted — and now it is answered.
    ///
    /// This asserted `NotProbed` until 7.1, which is what the command
    /// said about every Android row. Saying "I did not look" is better
    /// than silence and worse than looking, and the ledger held what was
    /// needed all along. The intent survives; the answer changed.
    #[test]
    fn an_android_runner_with_a_dead_process_is_answered_not_deferred() {
        let rows = attribute(
            &[ledger(
                "F",
                Resource::AndroidRunner {
                    port: 7100,
                    serial: "emulator-5554".into(),
                    proc: proc(4242),
                },
            )],
            &[],
            &[],
            &std::collections::HashSet::new(),
        );
        match &rows[0].seen {
            Seen::ProcessGone { ledger_pid } => assert_eq!(*ledger_pid, 4242),
            Seen::NotProbed { .. } => panic!(
                "still deferring. The pid and its start time are in the ledger; \
                 answering `not-probed` sends the reader to `ps`, which is what \
                 a consumer did four times in one session."
            ),
            other => panic!(
                "a dead Android runner filed as {other:?} would read as in use: \
                 {other:?}"
            ),
        }
    }
}

#[cfg(test)]
mod android_liveness {
    use super::*;
    use smix_lease::{Lease, ProcIdentity, Resource};
    use std::collections::HashSet;

    // `runner list` describes itself as the command you run before
    // touching anything, so the question it exists to answer is "is this
    // row still alive". The Android rows answered `not-probed` — always,
    // by construction: `Resource::AndroidRunner` mapped to `Probe::None`
    // with "this command probes iOS listeners only".
    //
    // A consumer read four of those and checked each pid by hand. All
    // four processes were long dead. The ledger had what was needed the
    // whole time: `AndroidRunner` records a ProcIdentity, which is a pid
    // AND the start time that tells a recycled pid from the real one —
    // strictly better evidence than the `kill -0` they suggested.

    fn proc(pid: u32) -> ProcIdentity {
        ProcIdentity {
            pid,
            started_at: "Mon Aug 25 00:00:00 2026".into(),
            cmd: "smix runner up emulator-5554".into(),
        }
    }

    fn android_ledger(device: &str, port: u16, pid: u32) -> (String, Lease) {
        (
            device.into(),
            Lease {
                device_id: device.into(),
                holder: proc(pid),
                acquired_at: "2026-08-25T00:00:00Z".into(),
                heartbeat_at: "2026-08-25T00:00:00Z".into(),
                resources: vec![smix_lease::Row::Known(Resource::AndroidRunner {
                    port,
                    serial: device.into(),
                    proc: proc(pid),
                })],
            },
        )
    }

    #[test]
    fn an_android_runner_whose_process_is_gone_reads_as_gone() {
        let rows = attribute(
            &[android_ledger("emulator-5554", 28080, 63329)],
            &[],
            &[],
            &HashSet::new(),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].seen,
            Seen::ProcessGone { ledger_pid: 63329 },
            "a dead process is a dead row, not an unanswered question: {rows:?}"
        );
    }

    #[test]
    fn an_android_runner_still_running_says_so() {
        let rows = attribute(
            &[android_ledger("emulator-5556", 28080, 4242)],
            &[],
            &[],
            &HashSet::from([4242u32]),
        );
        assert!(
            matches!(rows[0].seen, Seen::Both { .. }),
            "a live process is the row being in use: {rows:?}"
        );
    }

    #[test]
    fn the_pid_alone_is_not_the_evidence() {
        // Membership means "a process at this pid exists AND its start
        // time still matches the ledger". The caller does that check;
        // handing this a bare pid set that included a recycled one would
        // report somebody else's process as the runner, which is the
        // failure `ProcIdentity` exists to prevent.
        let rows = attribute(
            &[android_ledger("emulator-5558", 28080, 999)],
            &[],
            &[],
            &HashSet::from([1000u32]),
        );
        assert_eq!(
            rows[0].seen,
            Seen::ProcessGone { ledger_pid: 999 },
            "a different pid being alive says nothing about this one: {rows:?}"
        );
    }
}
