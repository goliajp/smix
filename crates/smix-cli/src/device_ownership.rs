//! Who turned a device on, and whether this process may turn it off.

/// Whether this process is the one that turned an emulator on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EmulatorState {
    WasOff,
    AlreadyRunning,
}

/// What to write to the ledger after a boot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BootClaim {
    ClaimAsOurs,
    LeaveUnclaimed,
}

/// What the ledger says about who holds a device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Ownership {
    BootedByUs,
    HeldByOther { pid: u32 },
    NoLedgerRow,
}

/// Whether this process may stop a device.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShutdownVerdict {
    Proceed,
    Refuse { message: String },
}

/// A device that was already up was turned on by somebody else, and
/// claiming it is how a later teardown comes to stop their work.
pub(crate) fn boot_claim(state: EmulatorState) -> BootClaim {
    match state {
        EmulatorState::WasOff => BootClaim::ClaimAsOurs,
        EmulatorState::AlreadyRunning => BootClaim::LeaveUnclaimed,
    }
}

/// What this machine's ledger says about a device, in the shape the
/// verdict below takes.
///
/// A translation of `smix_lease::assess`, not a second opinion. The
/// first draft asked only "is there a row", which cannot tell a device
/// somebody is using from one whose holder died hours ago — and a
/// stale row would then have locked the device out of smix's hands
/// permanently. The ledger already draws that line (`Admission`), and
/// drawing it twice is how the two answers start disagreeing.
pub(crate) fn ownership_of(serial: &str) -> Ownership {
    let Ok(leases) = smix_capsule::runner::machine_leases() else {
        return Ownership::NoLedgerRow;
    };
    let Ok(facts) = smix_lease::store::collect_facts(&leases, serial) else {
        return Ownership::NoLedgerRow;
    };
    // Booted-by-us is read off the resources rather than the admission,
    // because `Booted` is a device state and not a process: `assess`
    // deliberately does not treat it as something to probe for
    // liveness. Whether this device is ours and whether somebody's
    // session is alive on it are two questions.
    let booted_by_us = facts.existing.as_ref().is_some_and(|h| {
        h.lease
            .known_resources()
            .any(|r| matches!(r, smix_lease::Resource::Booted { by_us: true }))
    });
    match smix_lease::assess(&facts) {
        smix_lease::Admission::Denied(c) if c.holder_alive => {
            Ownership::HeldByOther { pid: c.holder.pid }
        }
        _ if booted_by_us => Ownership::BootedByUs,
        _ => Ownership::NoLedgerRow,
    }
}

/// Whoever booted it stops it.
///
/// The two refusals are kept apart because they send a reader somewhere
/// different: no ledger row means nothing here started the device and
/// there is nobody to ask; a row held by another process means
/// something did start it and may still be using it, and names who.
pub(crate) fn shutdown_verdict(owner: Ownership, serial: &str) -> ShutdownVerdict {
    match owner {
        Ownership::BootedByUs => ShutdownVerdict::Proceed,
        Ownership::NoLedgerRow => ShutdownVerdict::Refuse {
            message: format!(
                "{serial} is running and this machine's ledger has no record of smix \
                 booting it — whoever booted it stops it.\n\
                 \x20 if it is yours and you started it by hand, stop it the same way\n\
                 \x20 if it should be smix's, start it with `smix sim boot {serial}` \
                 and the ledger will say so"
            ),
        },
        Ownership::HeldByOther { pid } => ShutdownVerdict::Refuse {
            message: format!(
                "{serial} is held by pid {pid}, which is still alive — stopping it now \
                 takes a device out from under work that is running.\n\
                 \x20 `smix lease status {serial}` says what it has open\n\
                 \x20 `smix lease reconcile {serial}` closes it gracefully once that \
                 process is gone"
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Who turned this device on, and may this process turn it off.
    //
    // The iOS path has answered both since the ledger moved to the
    // machine: `record_boot(.., !was_up)` declines to claim a device
    // that was already running, and teardown reads that claim back.
    // Android had neither, and the day this was written another
    // person's smix stopped this machine's emulator a dozen times
    // while a release was using it — nobody was doing anything wrong,
    // because nothing could be asked.

    #[test]
    fn a_device_already_running_is_not_claimed() {
        assert_eq!(
            boot_claim(EmulatorState::AlreadyRunning),
            BootClaim::LeaveUnclaimed,
            "smix did not turn this one on, and saying otherwise is how a \
             teardown later stops somebody else's work"
        );
    }

    #[test]
    fn a_device_this_process_started_is_claimed() {
        assert_eq!(boot_claim(EmulatorState::WasOff), BootClaim::ClaimAsOurs);
    }

    /// The seed of the whole version. Everything else in v6.1 is this
    /// sentence applied in another place.
    #[test]
    fn an_emulator_we_did_not_boot_is_not_ours_to_stop() {
        match shutdown_verdict(Ownership::NoLedgerRow, "emulator-5560") {
            ShutdownVerdict::Refuse { message } => {
                assert!(
                    message.contains("emulator-5560"),
                    "a refusal that does not name the device cannot be acted on: {message}"
                );
                assert!(
                    message.to_lowercase().contains("boot"),
                    "the refusal must say the rule it is applying — whoever booted it \
                     stops it: {message}"
                );
            }
            other => panic!("stopping a device smix never started was allowed: {other:?}"),
        }
    }

    #[test]
    fn an_emulator_we_booted_is_ours_to_stop() {
        assert!(matches!(
            shutdown_verdict(Ownership::BootedByUs, "emulator-5560"),
            ShutdownVerdict::Proceed
        ));
    }

    /// Held by another live process, with a ledger row that is not ours.
    /// Distinct from "no row" because the two send a reader somewhere
    /// different: no row means nothing here started it, and a row means
    /// something did and may still be using it.
    #[test]
    fn an_emulator_held_by_someone_else_names_the_holder() {
        match shutdown_verdict(Ownership::HeldByOther { pid: 4242 }, "emulator-5560") {
            ShutdownVerdict::Refuse { message } => assert!(
                message.contains("4242"),
                "the refusal must name who is holding it, or the reader has nothing \
                 to go and ask: {message}"
            ),
            other => panic!("stopping a device another process holds was allowed: {other:?}"),
        }
    }
}
