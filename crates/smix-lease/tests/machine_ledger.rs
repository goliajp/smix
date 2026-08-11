//! What the ledger says about a process, and when a ledger may be torn up.
//!
//! Both questions used to be answered by doing I/O and judging in the
//! same breath, which made them untestable except against whatever this
//! machine happened to be running. They are pure functions here, fed
//! what `ps` said rather than asking it.
//!
//! The case that made this necessary is in `zombies`: on 2026-08-11 a
//! ledger in `stables/mailrs` recorded a runner at pid 24428. That pid
//! answered `ps -o lstart=` with a time matching the ledger to the
//! character — so the identity check passed and the ledger reported a
//! live runner on the device. `ps -o state=` said `Z`. It had been a
//! `<defunct>` entry in the process table since 8 August.

use smix_lease::store::{PsFacts, is_running};

fn facts(state: &str) -> PsFacts {
    PsFacts {
        started_at: "Sat Aug  8 20:02:29 2026".into(),
        cmd: "xcodebuild test-without-building".into(),
        state: state.into(),
    }
}

/// A zombie is not a process, however well it answers questions.
#[test]
fn a_defunct_entry_is_not_running() {
    assert!(
        !is_running(&facts("Z")),
        "a zombie passed the liveness check — which is how a ledger came \
         to report a live runner on a device that had none since 8 August"
    );
    // `Z+` and `Z<` are the same state with flags after it.
    assert!(!is_running(&facts("Z+")));
}

/// The ordinary states are running.
#[test]
fn a_sleeping_or_running_process_is_running() {
    for state in ["S", "S+", "R", "R+", "I", "U", "T"] {
        assert!(
            is_running(&facts(state)),
            "state {state:?} read as not running — a holder that is merely \
             stopped or waiting on I/O is still holding the device"
        );
    }
}

/// An answer smix cannot read is not evidence the process is gone.
///
/// Reading an unfamiliar state as dead would let the next command settle
/// a live session. Unknown means alive, which is the direction that
/// costs a wait rather than somebody's work.
#[test]
fn an_unrecognised_state_reads_as_running() {
    assert!(is_running(&facts("")));
    assert!(is_running(&facts("?")));
}

/// A live holder is not reclaimable because it went quiet.
///
/// The heartbeat is written when the ledger is touched, and a holder
/// that takes a device and then serves for hours never touches it
/// again. `smix-mcp` is exactly that: on 2026-08-11 one had held a
/// simulator since 9 August with a heartbeat an hour and twenty minutes
/// old, and its process was alive and serving.
///
/// Ninety seconds of silence used to make it `Reclaimable`, on the
/// reading that a live holder which stopped beating is wedged rather
/// than working. Nothing here can tell those apart. While each tree
/// kept its own ledgers only that tree could act on the mistake; the
/// ledgers are the machine's now, so any `smix lease reconcile` on any
/// checkout would have torn down a live agent's session. Refusing costs
/// a wait; reclaiming costs somebody their work.
#[test]
fn a_live_holder_that_stopped_beating_is_not_reclaimable() {
    use smix_lease::{Admission, Facts, Held, HolderProbe, Lease, ProcIdentity, Resource, assess};
    let holder = ProcIdentity {
        pid: 50057,
        started_at: "Sun Aug  9 07:18:59 2026".into(),
        cmd: "smix-mcp".into(),
    };
    let facts = Facts {
        existing: Some(Held {
            lease: Lease {
                device_id: "FFC57DAE".into(),
                holder: holder.clone(),
                acquired_at: "2026-08-09T07:18:59Z".into(),
                heartbeat_at: "2026-08-11T08:00:55Z".into(),
                resources: vec![Resource::Booted { by_us: true }],
            },
            holder: HolderProbe {
                pid_exists: true,
                identity_matches: true,
            },
            any_resource_alive: false,
        }),
        now: "2026-08-11T09:23:55Z".into(),
        self_pid: 999_999,
    };
    match assess(&facts) {
        Admission::Denied(c) => assert!(
            c.holder_alive,
            "denied, but reported as a dead holder — the message would send \
             somebody looking for a process that is right there"
        ),
        other => panic!(
            "a holder that is alive and serving was judged {other:?} — \
             reconcile would have settled a live session"
        ),
    }
}
