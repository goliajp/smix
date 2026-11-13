//! What each kind of ledger row is: a service, a process, a claim on the
//! device, or only a fact about it. Every match here is exhaustive, so a
//! new kind of row has to be classified before it compiles.

use crate::{Lease, Resource};

/// The `kind` of every row this binary could not name, in ledger order.
///
/// A refusal that cannot say WHAT it failed to read is a refusal nobody
/// can act on, so this is what the messages quote.
#[must_use]
pub fn unnamed_kinds(lease: &Lease) -> Vec<String> {
    lease
        .resources
        .iter()
        .filter_map(|r| r.unnamed_kind().map(str::to_string))
        .collect()
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
        // Entitlement is necessary and no longer sufficient. A row this
        // binary cannot read is something open on the device that
        // nothing here can close, and switching the device off would
        // strand it — so having booted it does not settle the question
        // on its own.
        unnamed_kinds(l).is_empty()
            && l.known_resources()
                .any(|r| matches!(r, Resource::Booted { by_us: true }))
    })
}

/// May smix drive this device at all?
///
/// Only a device whose life smix answers for: one it booted, or one a
/// person claimed with a reason. A device that came up any other way —
/// a script spawning the emulator binary, a bare `simctl boot` — was
/// started outside the lifecycle smix keeps, and nothing would shut it
/// down, unlock it or know it is there.
#[must_use]
pub fn may_drive(lease: Option<&Lease>) -> bool {
    lease.is_some_and(|l| l.known_resources().any(answers_for_the_device))
}

/// Whether a row says somebody answers for the device's life. Exhaustive,
/// so a new kind of row is put on one side of this line by whoever adds it.
fn answers_for_the_device(r: &Resource) -> bool {
    match r {
        Resource::Booted { by_us } => *by_us,
        Resource::Claimed { .. } => true,
        Resource::Runner { .. }
        | Resource::AndroidRunner { .. }
        | Resource::PortForward { .. }
        | Resource::Supervisor { .. }
        | Resource::ReversePort { .. }
        | Resource::Emulator { .. }
        | Resource::Environment { .. }
        | Resource::Recording { .. } => false,
    }
}

/// Is this resource a service — a thing that exists to be used by the
/// next command, rather than an activity that excludes it?
///
/// The runner and its plumbing are services: a client driving through
/// them is their purpose. A recording is an activity: two sessions
/// writing over one capture is a collision, not a hand-off. `Booted` is
/// neither a process nor an activity and never blocks adoption.
///
/// Exhaustive on purpose. This was a `matches!` with a default of "not a
/// service", and the `Emulator` row, added later, fell to it without
/// a word: every emulator smix booted then refused the `runner up` → `run`
/// pairing on it. A new kind of row has to be classified here to compile.
pub fn is_service(r: &Resource) -> bool {
    match r {
        Resource::Runner { .. }
        | Resource::AndroidRunner { .. }
        | Resource::PortForward { .. }
        | Resource::Supervisor { .. }
        | Resource::Booted { .. }
        | Resource::Claimed { .. }
        | Resource::ReversePort { .. }
        // Names which AVD this is and where its console went. Like
        // `Booted`, a fact about the device, not something a second
        // session could collide with.
        | Resource::Emulator { .. }
        // Settings to put back, not something a session works through.
        | Resource::Environment { .. } => true,
        Resource::Recording { .. } => false,
    }
}

/// Does this resource stand for a process that can die?
///
/// `Booted` does not: it is a device state, and nothing about it can be
/// probed for liveness. That difference decides whether a ledger without
/// a live holder is an abandoned session or merely a device left on.
pub fn is_process_backed(r: &Resource) -> bool {
    // Exhaustive for the same reason as `is_service`.
    match r {
        Resource::Runner { .. }
        | Resource::Recording { .. }
        | Resource::Supervisor { .. }
        | Resource::AndroidRunner { .. }
        | Resource::PortForward { .. } => true,
        Resource::ReversePort { .. }
        | Resource::Booted { .. }
        | Resource::Claimed { .. }
        | Resource::Emulator { .. }
        | Resource::Environment { .. } => false,
    }
}

/// Whether `lease` holds anything on its holder's behalf: a process it
/// started, a claim, or a row this binary cannot read. A boot row alone
/// does not; it says who may shut the device down.
#[must_use]
pub fn holds_something(lease: &Lease) -> bool {
    lease.resources.iter().any(|row| {
        row.known()
            .is_none_or(|r| is_process_backed(r) || matches!(r, Resource::Claimed { .. }))
    })
}
