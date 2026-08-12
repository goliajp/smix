//! Who holds a device, what they opened on it, and how to close it after
//! they die.
//!
//! smix could always tear down what it started, as long as it was the one
//! doing the tearing. Everything else — Ctrl-C on `xcodebuild`, closing
//! Simulator.app, an IDE restart, a CI timeout, another agent's `pkill` —
//! killed the process holding a runner, a recording and a booted simulator,
//! and nothing anywhere remembered that those three things existed. The
//! runner died by SIGKILL rather than the SIGINT that lets testmanagerd
//! end the session cleanly, so macOS put up a crash-report dialog; the
//! recording lost its mp4 trailer; the simulator stayed booted. Teardown
//! then went looking for survivors by matching process command lines,
//! which fails in both directions: it kills another project's runner, and
//! it misses the one whose command line reads differently.
//!
//! A ledger fixes the direction of that question. Instead of "what looks
//! like mine", the next smix command asks "what did the last holder write
//! down", and closes exactly those things by the graceful path the holder
//! never got to take. That is the whole idea: **a kill that had no
//! graceful path gets one at the next startup.**
//!
//! Looking is separated from judging, in the shape [`crate::Facts`] shares
//! with `smix-cli`'s readiness module: the caller probes the world, and
//! everything after that is a pure function. It is what lets the ordering
//! of a teardown be tested without a device.

pub mod store;

use serde::{Deserialize, Serialize};

/// How long a holder may go without a heartbeat before its lease is
/// considered abandoned.
///
/// Holders beat every 30 s, so this tolerates two missed beats. The
/// tolerance is not politeness: a machine under load — or one that just
/// came back from sleep — can starve a healthy holder past a single
/// interval, and declaring that holder dead would have a second process
/// tear down a session that is still running.
pub const STALE_AFTER_SECS: i64 = 90;

/// Enough to recognise a process later, or refuse to signal it.
///
/// A pid alone is not an identity: it outlives the process that owned it
/// and the kernel hands the number to somebody else, so a ledger written
/// an hour ago may point at a stranger. The command line is not an
/// identity either — every concurrent `smix run` has the same one. What
/// does pin it is the pair (pid, start time): the kernel will not reissue
/// a pid to a process that started at the same second, and `ps` reports
/// both. The command line rides along because a person reading a refusal
/// needs to know who holds the device, and a pid does not tell them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcIdentity {
    /// Process id.
    pub pid: u32,
    /// Start time as `ps -o lstart=` reports it. Compared verbatim; it is
    /// an opaque token here, not a date to do arithmetic on.
    pub started_at: String,
    /// Command line, for the human reading a refusal.
    pub cmd: String,
}

/// A resource a holder opened on a device, and therefore owes a close.
///
/// Every variant carries what the graceful close needs, and every variant
/// that names a process carries its [`ProcIdentity`] so the executor can
/// confirm the pid is still that process before signalling it.
/// [`CleanupAction`] carries the identity onward — the executor cannot
/// forget what it was never given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Resource {
    /// An XCUITest runner session: the host-side `xcodebuild` process and
    /// the port its HTTP face answers on.
    #[serde(rename_all = "camelCase")]
    Runner {
        /// Port the runner's HTTP face answers on.
        port: u16,
        /// The host-side `xcodebuild` process.
        proc: ProcIdentity,
    },
    /// A `simctl io … recordVideo` child writing to `path`.
    #[serde(rename_all = "camelCase")]
    Recording {
        /// Where the video is being written.
        path: String,
        /// The `simctl io … recordVideo` child.
        proc: ProcIdentity,
    },
    /// The supervisor sidecar watching an iOS runner.
    ///
    /// Ordered ahead of the runner in a cleanup plan, and that is not a
    /// style choice: a live supervisor exists to bring a dead runner
    /// back, so stopping the runner first accomplishes nothing except
    /// giving the supervisor something to do.
    #[serde(rename_all = "camelCase")]
    Supervisor {
        /// The sidecar process.
        proc: ProcIdentity,
    },
    /// An Android instrumentation runner: the host-side process, the
    /// forwarded port, and the device it is bound to.
    #[serde(rename_all = "camelCase")]
    AndroidRunner {
        /// Forwarded port.
        port: u16,
        /// Device serial. Carried because every host-side call in the
        /// teardown must name it — an unpinned one reaches whatever is
        /// attached, which on a developer machine is often a phone.
        serial: String,
        /// Host-side process.
        proc: ProcIdentity,
    },
    /// A local port forwarded to a port on a physical device.
    ///
    /// Closed *after* the runner it serves, which is the opposite of the
    /// general reverse-of-opened rule: the runner's last few requests
    /// still need the pipe, and pulling it first turns an orderly
    /// teardown into a connection error.
    #[serde(rename_all = "camelCase")]
    PortForward {
        /// Port on the host.
        local_port: u16,
        /// Port on the device.
        device_port: u16,
        /// The process holding the listener open. A forwarder lives
        /// inside a process; when that process goes, so does it.
        proc: ProcIdentity,
    },
    /// The device is booted. `by_us` records whether this holder is the
    /// one that booted it.
    #[serde(rename_all = "camelCase")]
    Booted {
        /// True when this holder ran the boot. False when it found the
        /// device already up.
        by_us: bool,
    },
}

/// One device's ledger: a holder, and what it opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    /// UDID (iOS) or serial (Android).
    pub device_id: String,
    /// The process holding the lease, pinned well enough to recognise.
    pub holder: ProcIdentity,
    /// RFC3339. When the lease was taken.
    pub acquired_at: String,
    /// RFC3339. Last heartbeat.
    pub heartbeat_at: String,
    /// What the holder opened on the device, in the order it opened them.
    pub resources: Vec<Resource>,
}

/// What a probe found at the holder's pid.
///
/// Two booleans rather than one `alive`, because the third case is real
/// and dangerous: the pid exists but belongs to something else now. A
/// single flag would collapse "holder is running" and "holder is gone,
/// its pid was reused" into the same answer, and the correct response to
/// them differs — the first means back off, the second means clean up
/// while never signalling that pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HolderProbe {
    /// A process exists at the holder's pid.
    pub pid_exists: bool,
    /// Its start time matches the one recorded in the ledger.
    pub identity_matches: bool,
}

/// A ledger found on disk, together with what its holder's pid looks like
/// right now.
///
/// The two travel as one because a lease without a probe is not a state
/// the caller may be in: it would leave `assess` to decide what an
/// unprobed holder means, and the only answers available there are a
/// guess.
#[derive(Debug, Clone)]
pub struct Held {
    /// The ledger.
    pub lease: Lease,
    /// What the caller found at the holder's pid.
    pub holder: HolderProbe,
    /// Whether any process the ledger records is still running.
    ///
    /// The holder is usually a short-lived command: `smix runner up`
    /// spawns an `xcodebuild` in its own process group and exits, so the
    /// holder is gone within seconds while the runner it started keeps
    /// the device occupied for hours. Judging occupancy by the holder
    /// alone would let the next command treat that live runner as an
    /// orphan and tear it down — the mechanism meant to protect sessions
    /// killing the healthiest one on the machine.
    pub any_resource_alive: bool,
}

/// Whether a ledger still describes anything, and the sentence for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruneVerdict {
    /// Keep it, for the stated reason.
    Keep(&'static str),
    /// Nothing here describes the machine any more.
    Remove,
}

/// Does this ledger still describe something?
///
/// Pure, and fed `device_is_on` rather than asking, for the reason
/// `is_running` is pure: judging and doing I/O in the same breath makes a
/// rule testable only against whatever this machine happens to be doing.
///
/// `device_is_on` is `None` when the machine cannot tell — an Android
/// serial and a physical iPhone are not in `simctl`'s list at all.
/// Not-listed must never collapse into off: that would delete the record
/// of a device whose state nobody here can see.
///
/// The boot row is the reason this exists. `any_resource_alive` reads
/// `Booted` as false by construction, which is correct for what that
/// field means — a boot is not a process and cannot be probed — and
/// wrong as the last word on whether a ledger is empty. A device that is
/// on, with a row saying smix turned it on, is a ledger holding the only
/// answer to a question `lease owner` is asked and `pick-dev-sim` acts
/// on.
pub fn prune_verdict(held: &Held, device_is_on: Option<bool>) -> PruneVerdict {
    if held.holder.pid_exists && held.holder.identity_matches {
        return PruneVerdict::Keep("held by a live holder");
    }
    if held.any_resource_alive {
        return PruneVerdict::Keep("holder gone but something it started is still running");
    }
    let booted_by_us = held
        .lease
        .resources
        .iter()
        .any(|r| matches!(r, Resource::Booted { by_us: true }));
    if booted_by_us {
        return match device_is_on {
            Some(true) => PruneVerdict::Keep(
                "still switched on and this ledger is the only record of who turned it on",
            ),
            None => PruneVerdict::Keep(
                "says smix booted it and this machine cannot tell whether it is still on",
            ),
            Some(false) => PruneVerdict::Remove,
        };
    }
    PruneVerdict::Remove
}

/// What the world looks like. Gathered by the caller; never read from the
/// world in here.
#[derive(Debug, Clone)]
pub struct Facts {
    /// The ledger on disk with its holder probed, if any.
    pub existing: Option<Held>,
    /// Now, RFC3339.
    pub now: String,
    /// This process's pid — a holder re-entering its own lease is not
    /// contention.
    pub self_pid: u32,
}

/// A close this process owes the device, on behalf of a holder that died.
///
/// Ordered by [`plan_cleanup`], carrying the pid *and* the command line
/// that pid must still be running for the signal to be sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum CleanupAction {
    /// SIGINT the `simctl` child and wait, so the mp4 keeps its trailer.
    #[serde(rename_all = "camelCase")]
    StopRecording {
        /// Where the video was being written.
        path: String,
        /// Must be re-verified at the pid before any signal is sent.
        proc: ProcIdentity,
    },
    /// SIGINT-first teardown of the runner, so testmanagerd ends the
    /// XCUITest session rather than the process dying by SIGABRT.
    #[serde(rename_all = "camelCase")]
    StopRunner {
        /// Runner HTTP port.
        port: u16,
        /// Must be re-verified at the pid before any signal is sent.
        proc: ProcIdentity,
    },
    /// Stop the supervisor sidecar. Emitted before the runner it watches.
    #[serde(rename_all = "camelCase")]
    StopSupervisor {
        /// Must be re-verified at the pid before any signal is sent.
        proc: ProcIdentity,
    },
    /// Stop an Android instrumentation runner.
    #[serde(rename_all = "camelCase")]
    StopAndroidRunner {
        /// Forwarded port.
        port: u16,
        /// Device serial — every host-side call in the teardown names it.
        serial: String,
        /// Must be re-verified at the pid before any signal is sent.
        proc: ProcIdentity,
    },
    /// Stop a port forwarder. Emitted after the runner it serves.
    #[serde(rename_all = "camelCase")]
    StopPortForward {
        /// Port on the host, for the report.
        local_port: u16,
        /// Must be re-verified at the pid before any signal is sent.
        proc: ProcIdentity,
    },
    /// `simctl shutdown` — only ever emitted for a device this holder
    /// booted.
    #[serde(rename_all = "camelCase")]
    ShutdownSim {
        /// Device to shut down.
        udid: String,
    },
}

/// Why a lease could not be taken, said with the facts the holder needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contention {
    /// The process that took the lease.
    pub holder: ProcIdentity,
    /// When it took the lease (RFC3339).
    pub acquired_at: String,
    /// Whether that process is still running. False means the holder
    /// command finished but what it started is still on the device —
    /// the ordinary state after `smix runner up` returns.
    pub holder_alive: bool,
}

/// The verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// The holder is gone and everything it left running is a service —
    /// a runner, its forwarder, its supervisor. The caller takes over
    /// the lease as-is: resources kept, holder replaced. Tearing this
    /// down would destroy the thing the caller came to use.
    Adoptable,
    /// Nothing holds this device, or this process already does.
    Granted,
    /// Someone else holds it and is alive. Ambiguity is an error, not a
    /// choice: the caller is told who, not made to wait.
    Denied(Contention),
    /// The holder is gone. The lease can be taken *after* these closes
    /// are performed — never before, or the next holder inherits an
    /// orphaned recording and a runner still occupying the device's
    /// automation slot.
    Reclaimable {
        /// Closes owed, in the order they must happen.
        cleanup: Vec<CleanupAction>,
        /// Why the holder was judged gone — carried so the report can
        /// say it rather than assert a bare "stale".
        reason: StaleReason,
    },
}

/// Why a holder was judged gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleReason {
    /// No process at the recorded pid.
    HolderExited,
    /// A process exists at the pid, but it is not the holder — the number
    /// was recycled. Nothing may be signalled at that pid.
    PidRecycled,
    /// The process is there and is the holder, but it stopped beating.
    ///
    /// No longer produced. `assess` returned this until 2026-08-11,
    /// when the ledgers moved from each checkout to the machine and
    /// every tree could act on it. The heartbeat is written when the
    /// ledger is touched, so a holder that takes a device and then
    /// serves for hours is silent by design — `smix-mcp` had held a
    /// simulator for two days with an eighty-three-minute-old heartbeat
    /// while serving — and treating that silence as wedged would have
    /// any `lease reconcile` tear down a live session. Kept as a name so
    /// a ledger or a caller written against it still resolves.
    HeartbeatExpired,
}

/// Something that can perform the closes a plan names.
///
/// The plan says *what* is owed; performing it needs knowledge this crate
/// does not have and should not acquire — how an XCUITest session ends
/// without a crash dialog, what a `simctl` recording needs in order to
/// keep its trailer. Callers that hold that knowledge implement this;
/// callers that only need to gate access take one as an argument.
///
/// The indirection buys one concrete thing beyond tidiness: admission can
/// be tested against a recording double, so "did it clean up before
/// handing the device over" is a question answerable without a device.
pub trait CleanupExecutor {
    /// Perform the closes, in the order given, and report each.
    fn execute(&self, root: &std::path::Path, actions: &[CleanupAction]) -> Vec<CleanupReport>;
}

/// What became of one owed close, as the gate needs to hear it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    /// One line, for the person reading.
    pub line: String,
    /// Did this leave the device in a state the next holder can trust?
    pub clean: bool,
}

/// Judge the facts.
pub fn assess(facts: &Facts) -> Admission {
    let Some(held) = &facts.existing else {
        return Admission::Granted;
    };
    let lease = &held.lease;

    // Re-entering our own lease is not contention. Without this, a
    // holder's second command against its own device would report itself
    // as the obstacle.
    if lease.holder.pid == facts.self_pid {
        return Admission::Granted;
    }

    let reclaim = |reason| Admission::Reclaimable {
        cleanup: plan_cleanup(lease),
        reason,
    };

    let deny = |holder_alive| {
        Admission::Denied(Contention {
            holder: lease.holder.clone(),
            acquired_at: lease.acquired_at.clone(),
            holder_alive,
        })
    };

    let holder_gone = !held.holder.pid_exists || !held.holder.identity_matches;
    if holder_gone {
        // What it started may well outlive it — and what outlives it is
        // usually the point. `smix runner up` exits by design; the
        // runner it leaves behind is a service the next command drives
        // *through*, not a squatter it must wait out. So a dead holder
        // whose surviving resources are all services hands the lease to
        // whoever asks next, resources intact. This used to deny, which
        // barred the quickstart's own `runner up` → `run` pairing on
        // every device — found the first time a full flow ran against a
        // physical phone.
        //
        // A live recording is different: it is an exclusive activity,
        // not a service, and adopting past it would let a second
        // session run over whatever the first was capturing.
        if held.any_resource_alive {
            if lease.resources.iter().all(is_service) {
                return Admission::Adoptable;
            }
            return deny(false);
        }
        // A ledger holding nothing but a boot is not an abandoned
        // session — it is a device somebody turned on and has not
        // finished with. `smix sim boot` exits the moment the device is
        // up, so treating its ledger as abandoned would have the very
        // next command shut the device down again, which is what the
        // person just asked for the opposite of. The boot row exists to
        // record who may shut it down later, not to claim the device.
        if !lease.resources.iter().any(is_process_backed) {
            return Admission::Granted;
        }
        return reclaim(if held.holder.pid_exists {
            // A process is there, but it is not the holder — the number
            // was recycled. Nothing may be signalled at that pid.
            StaleReason::PidRecycled
        } else {
            StaleReason::HolderExited
        });
    }

    // The holder itself is alive.
    //
    // A stopped heartbeat used to make it reclaimable, on the reading
    // that a live holder which went quiet is wedged rather than
    // working. Nothing here can tell those apart, and the heartbeat is
    // written when the ledger is touched — so a holder that takes a
    // device and then serves for hours is silent by design. `smix-mcp`
    // is exactly that shape: one had held a simulator since 9 August
    // with a heartbeat eighty-three minutes old, and it was serving.
    //
    // While each tree kept its own ledgers, only that tree could act on
    // the mistake. They are the machine's now, so any `lease reconcile`
    // from any checkout would settle it. Refusing costs a wait;
    // reclaiming costs somebody their session — and the way out of a
    // genuinely wedged holder is to end the process, which a person can
    // establish and this cannot.
    if heartbeat_expired(&lease.heartbeat_at, &facts.now) {
        return deny(true);
    }

    deny(true)
}

/// Whether a device is physical, as far as the destructive-action gate
/// is concerned.
///
/// Deliberately not `smix_simctl::registry::DeviceKind` — this crate sits
/// below the registry and must not depend on it. The caller reads the
/// registry and passes the one bit that matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceClass {
    /// True for a phone or tablet somebody might be carrying.
    pub physical: bool,
    /// Whether destructive actions were allowed on this device once,
    /// recorded in the registry.
    pub destructive_opt_in: bool,
}

/// Why a destructive action was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructiveRefused {
    /// The device, as the person addressed it.
    pub device: String,
    /// The command that would allow it.
    pub remedy: String,
}

impl std::fmt::Display for DestructiveRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is a physical device and destructive actions are not allowed on it.\n\
             Erasing an app, resetting a keychain or wiping data on a phone somebody \
             carries is not undoable the way it is on a simulator, so it is off until \
             said otherwise — once, not per command:\n  {}",
            self.device, self.remedy
        )
    }
}

/// May a destructive action run on this device?
///
/// A simulator is never gated: it can be erased and rebuilt in a minute,
/// and gating it would add a step to the common case for no safety.
///
/// A physical device is gated until somebody says otherwise **once**,
/// recorded in the registry. Not a per-command `--yes`: a confirmation
/// that has to be typed every time ends up pasted into a script, which
/// is the same as not having one.
///
/// # Errors
///
/// Returns the refusal with the command that lifts it. A guard that only
/// says no gets worked around rather than obeyed — the same reason
/// `adb-guard`'s refusal names a way to do the thing safely.
pub fn may_destroy(device: &str, class: DeviceClass) -> Result<(), DestructiveRefused> {
    if !class.physical || class.destructive_opt_in {
        return Ok(());
    }
    Err(DestructiveRefused {
        device: device.to_string(),
        remedy: format!("smix sim allow-destructive {device}"),
    })
}

/// What is known about a device reference before anything is done to it.
///
/// The caller does the looking — reads the registry, asks `simctl` or
/// `adb` — and hands over what it found. Same split as [`DeviceClass`]:
/// this crate judges, it does not go and see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    /// In the registry, with these properties.
    Registered(DeviceClass),
    /// Not in the registry, but the platform itself calls it virtual —
    /// a simulator `simctl` lists, or a serial `adb` named `emulator-*`.
    UnregisteredVirtual,
    /// Not in the registry, and not something the platform calls virtual.
    Unknown,
}

/// Why a device reference was refused before anything ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAddressable {
    /// The device, as the person addressed it.
    pub device: String,
}

impl std::fmt::Display for NotAddressable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is not a device smix may address: nothing here has registered it.\n\
             A simulator or emulator needs no registration; anything else does, \
             and being plugged in is not registration — that is the whole point \
             of the rule, because the phone that happens to be attached is \
             somebody's own.\n\
             Say so once and it becomes addressable:\n  \
             smix sim register <name> --udid {} --kind physical-ios|physical-android",
            self.device, self.device
        )
    }
}

/// May this device reference be addressed at all?
///
/// The charter's first constraint on physical devices is that one has to
/// be registered before it can be named — "whichever phone happens to be
/// plugged in" is never a target. This is where that stops being a
/// sentence and starts being a branch.
///
/// It is deliberately upstream of [`may_destroy`]. That gate asks whether
/// a *known* device may be harmed; this one asks whether smix is talking
/// to a device it was invited to talk to. Conflating them is how the two
/// holes of 2026-08-06 opened: an unregistered device reached the
/// executor and was stopped only because `simctl` happened not to
/// recognise it, and the Android path never asked at all.
///
/// # Errors
///
/// Returns the refusal, which names the registration that lifts it. A
/// device nobody registered is not a typo to retry — it is a decision
/// that has not been made yet, so the message asks for the decision.
pub fn may_address(device: &str, known: Known) -> Result<(), NotAddressable> {
    match known {
        // Registration is the invitation. What may then be *done* to it
        // is `may_destroy`'s question, not this one.
        Known::Registered(_) => Ok(()),
        // A simulator nobody registered stays addressable. Erasing one
        // costs a minute of rebuilding, so the charter's protection was
        // never aimed here — and `smix sim boot <fresh-udid>` is an
        // ordinary thing to do that must keep working.
        Known::UnregisteredVirtual => Ok(()),
        Known::Unknown => Err(NotAddressable {
            device: device.to_string(),
        }),
    }
}

/// Is this workspace entitled to shut this device down?
///
/// Being in the registry means "smix knows how to address it", not "smix
/// may turn it off". A teardown that shut down every registered device
/// took away sessions nobody here started — someone running a dev server
/// against a registered simulator lost it to a sweep of somebody else's
/// work. The boot row is the record of entitlement, and `None` (no ledger
/// at all) means no.
#[must_use]
pub fn may_shut_down(lease: Option<&Lease>) -> bool {
    lease.is_some_and(|l| {
        l.resources
            .iter()
            .any(|r| matches!(r, Resource::Booted { by_us: true }))
    })
}

/// Is this resource a service — a thing that exists to be used by the
/// next command, rather than an activity that excludes it?
///
/// The runner and its plumbing are services: a client driving through
/// them is their purpose. A recording is an activity: two sessions
/// writing over one capture is a collision, not a hand-off. `Booted` is
/// neither a process nor an activity and never blocks adoption.
pub fn is_service(r: &Resource) -> bool {
    matches!(
        r,
        Resource::Runner { .. }
            | Resource::AndroidRunner { .. }
            | Resource::PortForward { .. }
            | Resource::Supervisor { .. }
            | Resource::Booted { .. }
    )
}

/// Does this resource stand for a process that can die?
///
/// `Booted` does not: it is a device state, and nothing about it can be
/// probed for liveness. That difference decides whether a ledger without
/// a live holder is an abandoned session or merely a device left on.
pub fn is_process_backed(r: &Resource) -> bool {
    matches!(
        r,
        Resource::Runner { .. }
            | Resource::Recording { .. }
            | Resource::Supervisor { .. }
            | Resource::AndroidRunner { .. }
            | Resource::PortForward { .. }
    )
}

/// Has the holder stopped beating for longer than [`STALE_AFTER_SECS`]?
///
/// An unparseable timestamp answers `false` — refuse to preempt rather
/// than preempt on a reading we could not take. This branch is only ever
/// reached with the holder's process alive and matching, so `false` keeps
/// a running session running and reports a named holder the operator can
/// act on; `true` would tear down a live session on the strength of a
/// corrupt string. Not a fallback for a missing value — a decision about
/// which way to be wrong.
fn heartbeat_expired(heartbeat_at: &str, now: &str) -> bool {
    let parse = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok();
    match (parse(heartbeat_at), parse(now)) {
        (Some(hb), Some(n)) => (n - hb).num_seconds() > STALE_AFTER_SECS,
        _ => false,
    }
}

/// The closes a dead holder's lease owes, in the order they must happen.
///
/// Reverse of the order they were opened: the recording is stopped before
/// the runner that was driving what it recorded, and the device is shut
/// down last, after nothing is left talking to it.
pub fn plan_cleanup(lease: &Lease) -> Vec<CleanupAction> {
    // Reverse-of-opened, with one exception that is not a preference.
    //
    // A supervisor's whole job is to bring a dead runner back. Stop the
    // runner while its supervisor is alive and the teardown undoes
    // itself — so the supervisor goes first regardless of when it was
    // recorded. `runner::down` already cascades in this order; putting
    // it in the plan means a reconcile running hours later, from a
    // different process, does not have to rediscover it.
    let mut plan: Vec<CleanupAction> = lease
        .resources
        .iter()
        .filter_map(|r| match r {
            Resource::Supervisor { proc } => {
                Some(CleanupAction::StopSupervisor { proc: proc.clone() })
            }
            _ => None,
        })
        .collect();
    plan.extend(plan_rest(lease));
    // Forwarders last, for the mirror-image of the supervisor reason:
    // the runner's closing requests still travel through the pipe, and
    // pulling it early turns a clean teardown into a connection error
    // that reads like the runner misbehaved.
    plan.extend(lease.resources.iter().filter_map(|r| match r {
        Resource::PortForward {
            local_port, proc, ..
        } => Some(CleanupAction::StopPortForward {
            local_port: *local_port,
            proc: proc.clone(),
        }),
        _ => None,
    }));
    plan
}

/// Everything except supervisors, reverse-of-opened.
fn plan_rest(lease: &Lease) -> Vec<CleanupAction> {
    lease
        .resources
        .iter()
        .rev()
        .filter_map(|r| match r {
            // Handled ahead of everything else by `plan_cleanup`.
            Resource::Supervisor { .. } => None,
            // Handled after everything else by `plan_cleanup`.
            Resource::PortForward { .. } => None,
            Resource::Recording { path, proc } => Some(CleanupAction::StopRecording {
                path: path.clone(),
                proc: proc.clone(),
            }),
            Resource::Runner { port, proc } => Some(CleanupAction::StopRunner {
                port: *port,
                proc: proc.clone(),
            }),
            Resource::AndroidRunner { port, serial, proc } => {
                Some(CleanupAction::StopAndroidRunner {
                    port: *port,
                    serial: serial.clone(),
                    proc: proc.clone(),
                })
            }
            // Not ours to shut down. Finding the device already up and
            // then turning it off would take away someone else's session
            // as the price of cleaning up our own.
            Resource::Booted { by_us: false } => None,
            Resource::Booted { by_us: true } => Some(CleanupAction::ShutdownSim {
                udid: lease.device_id.clone(),
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder() -> ProcIdentity {
        ProcIdentity {
            pid: 4242,
            started_at: "Thu Aug  6 10:00:00 2026".into(),
            cmd: "smix run examples/hello.yaml".into(),
        }
    }

    fn lease_with(resources: Vec<Resource>) -> Lease {
        Lease {
            device_id: "UDID-1".into(),
            holder: holder(),
            acquired_at: "2026-08-06T10:00:00Z".into(),
            heartbeat_at: "2026-08-06T10:00:00Z".into(),
            resources,
        }
    }

    fn runner_proc() -> ProcIdentity {
        ProcIdentity {
            pid: 5150,
            started_at: "Thu Aug  6 10:00:05 2026".into(),
            cmd: "xcodebuild test -project SmixRunner".into(),
        }
    }

    fn recording_proc() -> ProcIdentity {
        ProcIdentity {
            pid: 5151,
            started_at: "Thu Aug  6 10:00:09 2026".into(),
            cmd: "xcrun simctl io UDID-1 recordVideo".into(),
        }
    }

    fn runner() -> Resource {
        Resource::Runner {
            port: 22087,
            proc: runner_proc(),
        }
    }

    fn recording() -> Resource {
        Resource::Recording {
            path: ".smix/trace/run.mov".into(),
            proc: recording_proc(),
        }
    }

    /// Facts for a probed holder. `now` sits inside the heartbeat window
    /// unless a test is about staleness.
    fn facts(lease: Lease, pid_exists: bool, identity_matches: bool, now: &str) -> Facts {
        facts_with_resources(lease, pid_exists, identity_matches, now, false)
    }

    fn facts_with_resources(
        lease: Lease,
        pid_exists: bool,
        identity_matches: bool,
        now: &str,
        any_resource_alive: bool,
    ) -> Facts {
        Facts {
            existing: Some(Held {
                lease,
                holder: HolderProbe {
                    pid_exists,
                    identity_matches,
                },
                any_resource_alive,
            }),
            now: now.into(),
            self_pid: 9000,
        }
    }

    const FRESH: &str = "2026-08-06T10:00:30Z";

    #[test]
    fn no_lease_is_granted() {
        let f = Facts {
            existing: None,
            now: FRESH.into(),
            self_pid: 9000,
        };
        assert_eq!(assess(&f), Admission::Granted);
    }

    #[test]
    fn own_lease_is_granted() {
        let mut lease = lease_with(vec![runner()]);
        lease.holder.pid = 9000;
        assert_eq!(assess(&facts(lease, true, true, FRESH)), Admission::Granted);
    }

    #[test]
    fn live_holder_denies_with_who_not_just_busy() {
        match assess(&facts(lease_with(vec![runner()]), true, true, FRESH)) {
            Admission::Denied(c) => {
                assert_eq!(c.holder.pid, 4242);
                assert_eq!(c.holder.cmd, "smix run examples/hello.yaml");
                assert_eq!(c.acquired_at, "2026-08-06T10:00:00Z");
            }
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn dead_holder_is_reclaimable_with_cleanup() {
        let lease = lease_with(vec![
            Resource::Booted { by_us: true },
            runner(),
            recording(),
        ]);
        match assess(&facts(lease, false, false, FRESH)) {
            Admission::Reclaimable { cleanup, reason } => {
                assert_eq!(reason, StaleReason::HolderExited);
                assert_eq!(cleanup.len(), 3);
            }
            other => panic!("expected Reclaimable, got {other:?}"),
        }
    }

    #[test]
    fn recycled_pid_is_stale_and_says_so() {
        match assess(&facts(lease_with(vec![runner()]), true, false, FRESH)) {
            Admission::Reclaimable { reason, .. } => assert_eq!(reason, StaleReason::PidRecycled),
            other => panic!("expected Reclaimable, got {other:?}"),
        }
    }

    #[test]
    fn live_holder_past_heartbeat_window_still_denies() {
        // 91 s after the recorded heartbeat — past the window, and the
        // holder is still there.
        //
        // This asserted `Reclaimable` until 2026-08-11. The heartbeat is
        // written when the ledger is touched, so a holder that takes a
        // device and then serves is silent by design, and nothing here
        // can separate silent-because-serving from wedged. Denying costs
        // a wait; reclaiming costs somebody their session.
        let f = facts(
            lease_with(vec![runner()]),
            true,
            true,
            "2026-08-06T10:01:31Z",
        );
        match assess(&f) {
            Admission::Denied(c) => assert!(c.holder_alive),
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn live_holder_within_heartbeat_window_still_denies() {
        let f = facts(
            lease_with(vec![runner()]),
            true,
            true,
            "2026-08-06T10:01:29Z",
        );
        assert!(matches!(assess(&f), Admission::Denied(_)));
    }

    #[test]
    fn unreadable_timestamp_refuses_to_preempt_a_live_holder() {
        let mut lease = lease_with(vec![runner()]);
        lease.heartbeat_at = "not a timestamp".into();
        assert!(matches!(
            assess(&facts(lease, true, true, FRESH)),
            Admission::Denied(_)
        ));
    }

    #[test]
    fn a_live_runner_after_its_launcher_exits_is_adopted_not_denied() {
        // `smix runner up` exits by design; the runner it leaves is a
        // service the next command drives *through*. This used to be
        // pinned as Denied, with a comment about not tearing down a
        // working runner — the right worry, wrong verdict: denial kept
        // the runner alive by making it unusable, which barred the
        // quickstart's own `runner up` → `run` pairing on every device.
        let f = facts_with_resources(lease_with(vec![runner()]), false, false, FRESH, true);
        assert_eq!(assess(&f), Admission::Adoptable);
    }

    #[test]
    fn the_whole_physical_stack_is_adoptable_too() {
        // What `runner up <phone>` actually leaves: boot row, runner,
        // forwarder. All services; all kept by the adopter.
        let lease = lease_with(vec![
            Resource::Booted { by_us: true },
            runner(),
            Resource::PortForward {
                local_port: 22097,
                device_port: 22097,
                proc: runner_proc(),
            },
        ]);
        assert_eq!(
            assess(&facts_with_resources(lease, false, false, FRESH, true)),
            Admission::Adoptable
        );
    }

    #[test]
    fn a_live_recording_is_not_adopted_past() {
        // A recording is an activity, not a service: adopting past it
        // would let a second session run over whatever the first was
        // capturing. Denied — and the denial must say the holder is
        // dead, so the message blames the recording rather than telling
        // someone to wait for a pid that no longer exists.
        let lease = lease_with(vec![runner(), recording()]);
        match assess(&facts_with_resources(lease, false, false, FRESH, true)) {
            Admission::Denied(c) => assert!(!c.holder_alive),
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn a_live_holder_still_denies_no_matter_the_resources() {
        // The real concurrency case, untouched: two commands at once is
        // contention whether or not the resources are services.
        let f = facts_with_resources(lease_with(vec![runner()]), true, true, FRESH, true);
        assert!(matches!(assess(&f), Admission::Denied(_)));
    }

    #[test]
    fn cleanup_is_reverse_of_open_order() {
        let lease = lease_with(vec![
            Resource::Booted { by_us: true },
            runner(),
            recording(),
        ]);
        assert_eq!(
            plan_cleanup(&lease),
            vec![
                CleanupAction::StopRecording {
                    path: ".smix/trace/run.mov".into(),
                    proc: recording_proc(),
                },
                CleanupAction::StopRunner {
                    port: 22087,
                    proc: runner_proc(),
                },
                CleanupAction::ShutdownSim {
                    udid: "UDID-1".into(),
                },
            ]
        );
    }

    #[test]
    fn device_we_did_not_boot_is_not_ours_to_shut_down() {
        let lease = lease_with(vec![Resource::Booted { by_us: false }, runner()]);
        assert_eq!(
            plan_cleanup(&lease),
            vec![CleanupAction::StopRunner {
                port: 22087,
                proc: runner_proc(),
            }]
        );
    }

    #[test]
    fn every_signalling_action_carries_an_identity_to_verify() {
        // The executor must be able to confirm the pid is still that
        // process. An action that cannot be verified is one that signals
        // strangers.
        let lease = lease_with(vec![runner(), recording()]);
        for action in plan_cleanup(&lease) {
            match action {
                CleanupAction::StopRunner { proc, .. }
                | CleanupAction::StopRecording { proc, .. }
                | CleanupAction::StopSupervisor { proc, .. }
                | CleanupAction::StopAndroidRunner { proc, .. }
                | CleanupAction::StopPortForward { proc, .. } => {
                    assert!(
                        !proc.started_at.is_empty(),
                        "no start time to verify against"
                    );
                }
                CleanupAction::ShutdownSim { .. } => {}
            }
        }
    }

    #[test]
    fn lease_json_roundtrips() {
        let lease = lease_with(vec![
            Resource::Booted { by_us: true },
            runner(),
            recording(),
        ]);
        let json = serde_json::to_string_pretty(&lease).expect("serialize");
        let back: Lease = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(lease, back);
        // Another process reads this shape — pin the field spelling.
        assert!(json.contains("\"deviceId\""));
        assert!(json.contains("\"startedAt\""));
        assert!(json.contains("\"byUs\""));
    }
}

#[cfg(test)]
mod boot_only_tests {
    use super::*;

    #[test]
    fn a_ledger_holding_only_a_boot_is_a_free_device_not_an_orphan() {
        // Regression: `smix sim boot` exits as soon as the device is up.
        // Judging its ledger as abandoned made the next command shut the
        // device down — undoing what had just been asked for.
        let lease = Lease {
            device_id: "UDID-1".into(),
            holder: ProcIdentity {
                pid: 0,
                started_at: "Thu Aug  6 10:00:00 2026".into(),
                cmd: "smix sim boot UDID-1".into(),
            },
            acquired_at: "2026-08-06T10:00:00Z".into(),
            heartbeat_at: "2026-08-06T10:00:00Z".into(),
            resources: vec![Resource::Booted { by_us: true }],
        };
        let facts = Facts {
            existing: Some(Held {
                lease,
                holder: HolderProbe {
                    pid_exists: false,
                    identity_matches: false,
                },
                any_resource_alive: false,
            }),
            now: "2026-08-06T10:00:30Z".into(),
            self_pid: 9000,
        };
        assert_eq!(assess(&facts), Admission::Granted);
    }

    #[test]
    fn a_boot_plus_a_dead_runner_is_still_an_orphan() {
        // The distinction is the process-backed row, not the boot.
        let lease = Lease {
            device_id: "UDID-1".into(),
            holder: ProcIdentity {
                pid: 0,
                started_at: "Thu Aug  6 10:00:00 2026".into(),
                cmd: "smix run x.yaml".into(),
            },
            acquired_at: "2026-08-06T10:00:00Z".into(),
            heartbeat_at: "2026-08-06T10:00:00Z".into(),
            resources: vec![
                Resource::Booted { by_us: true },
                Resource::Runner {
                    port: 1,
                    proc: ProcIdentity {
                        pid: 0,
                        started_at: "Thu Aug  6 10:00:05 2026".into(),
                        cmd: "xcodebuild".into(),
                    },
                },
            ],
        };
        let facts = Facts {
            existing: Some(Held {
                lease,
                holder: HolderProbe {
                    pid_exists: false,
                    identity_matches: false,
                },
                any_resource_alive: false,
            }),
            now: "2026-08-06T10:00:30Z".into(),
            self_pid: 9000,
        };
        match assess(&facts) {
            Admission::Reclaimable { cleanup, .. } => {
                assert_eq!(cleanup.len(), 2, "the runner and the boot are both owed");
            }
            other => panic!("expected Reclaimable, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;

    fn proc(pid: u32, cmd: &str) -> ProcIdentity {
        ProcIdentity {
            pid,
            started_at: "Thu Aug  6 10:00:00 2026".into(),
            cmd: cmd.into(),
        }
    }

    fn lease(resources: Vec<Resource>) -> Lease {
        Lease {
            device_id: "UDID-1".into(),
            holder: proc(4242, "smix runner up"),
            acquired_at: "2026-08-06T10:00:00Z".into(),
            heartbeat_at: "2026-08-06T10:00:00Z".into(),
            resources,
        }
    }

    #[test]
    fn the_supervisor_is_stopped_before_the_runner_it_watches() {
        // Not a style preference. A supervisor exists to bring a dead
        // runner back, so a teardown that stops the runner first is one
        // that undoes itself — `runner::down` already cascades in this
        // order, and encoding it in the plan means a reconcile running
        // hours later does not have to rediscover it.
        let l = lease(vec![
            Resource::Supervisor {
                proc: proc(5000, "smix runner supervise"),
            },
            Resource::Runner {
                port: 22087,
                proc: proc(5150, "xcodebuild test"),
            },
        ]);
        let plan = plan_cleanup(&l);
        let sup = plan
            .iter()
            .position(|a| matches!(a, CleanupAction::StopSupervisor { .. }))
            .expect("supervisor in plan");
        let run = plan
            .iter()
            .position(|a| matches!(a, CleanupAction::StopRunner { .. }))
            .expect("runner in plan");
        assert!(
            sup < run,
            "supervisor must be stopped first, got plan: {plan:?}"
        );
    }

    #[test]
    fn an_android_runner_is_process_backed_and_carries_its_serial() {
        let r = Resource::AndroidRunner {
            port: 28080,
            serial: "emulator-5554".into(),
            proc: proc(6000, "am instrument"),
        };
        assert!(is_process_backed(&r));
        let l = lease(vec![r]);
        match plan_cleanup(&l).as_slice() {
            [CleanupAction::StopAndroidRunner { serial, port, .. }] => {
                // Every host-side call in the teardown names the device.
                // Without it the command reaches whatever is attached,
                // which on a developer machine is often a phone.
                assert_eq!(serial, "emulator-5554");
                assert_eq!(*port, 28080);
            }
            other => panic!("expected one android action, got {other:?}"),
        }
    }

    #[test]
    fn a_supervisor_alone_does_not_make_a_device_occupied() {
        // A sidecar watching nothing is not a session. Treating it as
        // occupancy would keep the device claimed after the session it
        // was watching has ended.
        let r = Resource::Supervisor {
            proc: proc(5000, "smix runner supervise"),
        };
        assert!(is_process_backed(&r), "it is still a process to clean up");
    }

    #[test]
    fn the_new_rows_roundtrip_with_their_field_spelling_pinned() {
        let l = lease(vec![
            Resource::Supervisor {
                proc: proc(5000, "smix runner supervise"),
            },
            Resource::AndroidRunner {
                port: 28080,
                serial: "emulator-5554".into(),
                proc: proc(6000, "am instrument"),
            },
        ]);
        let json = serde_json::to_string_pretty(&l).expect("serialize");
        let back: Lease = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(l, back);
        assert!(json.contains("\"supervisor\""));
        assert!(json.contains("\"androidRunner\""));
        assert!(json.contains("\"serial\""));
    }
}

#[cfg(test)]
mod entitlement_tests {
    use super::*;

    fn lease(resources: Vec<Resource>) -> Lease {
        Lease {
            device_id: "UDID-1".into(),
            holder: ProcIdentity {
                pid: 1,
                started_at: "Thu Aug  6 10:00:00 2026".into(),
                cmd: "smix".into(),
            },
            acquired_at: "2026-08-06T10:00:00Z".into(),
            heartbeat_at: "2026-08-06T10:00:00Z".into(),
            resources,
        }
    }

    #[test]
    fn a_device_with_no_ledger_is_not_ours_to_shut_down() {
        // The case that took away somebody's dev server: registered,
        // booted by them, swept by us.
        assert!(!may_shut_down(None));
    }

    #[test]
    fn a_device_we_found_already_up_is_not_ours_either() {
        let l = lease(vec![Resource::Booted { by_us: false }]);
        assert!(!may_shut_down(Some(&l)));
    }

    #[test]
    fn a_device_we_booted_is_ours() {
        let l = lease(vec![Resource::Booted { by_us: true }]);
        assert!(may_shut_down(Some(&l)));
    }

    #[test]
    fn a_ledger_without_a_boot_row_says_nothing_about_entitlement() {
        // We have a runner on it, but somebody else turned it on.
        let l = lease(vec![Resource::Runner {
            port: 22087,
            proc: ProcIdentity {
                pid: 2,
                started_at: "Thu Aug  6 10:00:05 2026".into(),
                cmd: "xcodebuild".into(),
            },
        }]);
        assert!(!may_shut_down(Some(&l)));
    }
}

#[cfg(test)]
mod destructive_gate_tests {
    use super::*;

    fn sim() -> DeviceClass {
        DeviceClass {
            physical: false,
            destructive_opt_in: false,
        }
    }

    fn phone(opted_in: bool) -> DeviceClass {
        DeviceClass {
            physical: true,
            destructive_opt_in: opted_in,
        }
    }

    #[test]
    fn a_simulator_is_never_gated() {
        // Gating it would add a step to the common case and buy nothing:
        // a simulator can be erased and rebuilt in a minute.
        assert!(may_destroy("sim-smix-02", sim()).is_ok());
    }

    #[test]
    fn a_phone_without_opt_in_is_refused() {
        assert!(may_destroy("panda", phone(false)).is_err());
    }

    #[test]
    fn a_phone_with_opt_in_is_allowed() {
        assert!(may_destroy("panda", phone(true)).is_ok());
    }

    #[test]
    fn a_registered_device_is_addressable_whatever_it_is() {
        // Registration is the invitation. Whether the thing may then be
        // *harmed* is a separate question with a separate gate.
        assert!(may_address("panda", Known::Registered(phone(false))).is_ok());
        assert!(may_address("panda", Known::Registered(phone(true))).is_ok());
        assert!(may_address("sim-smix-02", Known::Registered(sim())).is_ok());
    }

    #[test]
    fn an_unregistered_simulator_stays_addressable() {
        // `smix sim boot <a udid nobody registered>` is an ordinary thing
        // to do. The charter's protection is aimed at devices somebody
        // carries, not at ones that rebuild in a minute.
        assert!(
            may_address(
                "C0FFEE00-0000-4000-8000-000000000000",
                Known::UnregisteredVirtual
            )
            .is_ok()
        );
    }

    #[test]
    fn a_device_nobody_registered_is_refused() {
        assert!(may_address("D51116A4-B2AD-5432-8A75-6FBB13F17B58", Known::Unknown).is_err());
    }

    #[test]
    fn the_addressability_refusal_asks_for_a_decision_not_a_retry() {
        // An unregistered device is not a typo. Telling someone to check
        // the spelling would send them looking for a mistake they did not
        // make; what is missing is a decision nobody has taken yet, so
        // the message asks for that decision by name.
        let err =
            may_address("R5CT52DF07D", Known::Unknown).expect_err("an unknown device must refuse");
        let msg = err.to_string();
        assert!(msg.contains("R5CT52DF07D"), "got: {msg}");
        assert!(msg.contains("smix sim register"), "got: {msg}");
        for retry_flavoured in ["check the spelling", "try again", "did you mean"] {
            assert!(
                !msg.to_lowercase().contains(retry_flavoured),
                "the refusal must not read as a typo, got: {msg}"
            );
        }
    }

    #[test]
    fn the_refusal_names_the_device_and_the_way_out() {
        // A guard that only says no gets worked around rather than
        // obeyed. Both halves are asserted because either alone is
        // useless: the device name without the remedy leaves someone
        // guessing, the remedy without the name leaves them unsure it
        // applies to what they just ran.
        let err = may_destroy("panda", phone(false)).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("panda"), "got: {msg}");
        assert!(
            msg.contains("smix sim allow-destructive panda"),
            "got: {msg}"
        );
        assert!(
            msg.contains("not undoable"),
            "the refusal should say why, got: {msg}"
        );
    }
}

#[cfg(test)]
mod forward_ordering_tests {
    use super::*;

    fn proc(pid: u32, cmd: &str) -> ProcIdentity {
        ProcIdentity {
            pid,
            started_at: "Thu Aug  6 10:00:00 2026".into(),
            cmd: cmd.into(),
        }
    }

    fn lease(resources: Vec<Resource>) -> Lease {
        Lease {
            device_id: "00008120-001410C11A42201E".into(),
            holder: proc(4242, "smix runner up"),
            acquired_at: "2026-08-06T10:00:00Z".into(),
            heartbeat_at: "2026-08-06T10:00:00Z".into(),
            resources,
        }
    }

    fn full_session() -> Lease {
        lease(vec![
            Resource::PortForward {
                local_port: 22087,
                device_port: 22087,
                proc: proc(7000, "smix forward"),
            },
            Resource::Supervisor {
                proc: proc(5000, "smix runner supervise"),
            },
            Resource::Runner {
                port: 22087,
                proc: proc(5150, "xcodebuild test"),
            },
        ])
    }

    #[test]
    fn the_pipe_is_pulled_after_the_runner_it_serves() {
        // The mirror image of the supervisor rule. A runner's closing
        // requests still travel through the forwarder; pulling it first
        // turns an orderly teardown into a connection error that reads
        // like the runner misbehaved.
        let plan = plan_cleanup(&full_session());
        let fwd = plan
            .iter()
            .position(|a| matches!(a, CleanupAction::StopPortForward { .. }))
            .expect("forward in plan");
        let run = plan
            .iter()
            .position(|a| matches!(a, CleanupAction::StopRunner { .. }))
            .expect("runner in plan");
        assert!(fwd > run, "forward must go last, got plan: {plan:?}");
    }

    #[test]
    fn a_full_session_tears_down_supervisor_then_runner_then_pipe() {
        // Three rules meeting in one plan, each for its own reason.
        let plan = plan_cleanup(&full_session());
        let kinds: Vec<&str> = plan
            .iter()
            .map(|a| match a {
                CleanupAction::StopSupervisor { .. } => "supervisor",
                CleanupAction::StopRunner { .. } => "runner",
                CleanupAction::StopPortForward { .. } => "forward",
                CleanupAction::StopRecording { .. } => "recording",
                CleanupAction::StopAndroidRunner { .. } => "android",
                CleanupAction::ShutdownSim { .. } => "shutdown",
            })
            .collect();
        assert_eq!(kinds, vec!["supervisor", "runner", "forward"]);
    }

    #[test]
    fn a_forwarder_is_something_to_clean_up_but_not_occupancy() {
        // Same reasoning as a recording: a pipe nobody is talking
        // through does not make a device busy. Counting it would leave
        // an abandoned forwarder running forever, since nothing would
        // ever judge the ledger reclaimable.
        let r = Resource::PortForward {
            local_port: 22087,
            device_port: 22087,
            proc: proc(7000, "smix forward"),
        };
        assert!(is_process_backed(&r), "it is still a process to close");
    }

    #[test]
    fn the_row_roundtrips_with_its_spelling_pinned() {
        let l = lease(vec![Resource::PortForward {
            local_port: 22087,
            device_port: 22087,
            proc: proc(7000, "smix forward"),
        }]);
        let json = serde_json::to_string(&l).expect("serialize");
        assert!(json.contains("\"portForward\""), "got: {json}");
        assert!(json.contains("\"localPort\""), "got: {json}");
        assert!(json.contains("\"devicePort\""), "got: {json}");
        assert_eq!(serde_json::from_str::<Lease>(&json).expect("parse"), l);
    }
}
