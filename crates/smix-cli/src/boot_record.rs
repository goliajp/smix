//! Who turned a simulator on, read before the boot and written after it.
//!
//! Whether this process brought a device up decides, later, whether smix
//! may shut it down: a device someone else had running is not ours to turn
//! off as the price of cleaning up after ourselves. Every path that boots a
//! simulator asks here, so no path can disagree with another about it —
//! `capsule up` kept its own copy of the rule, read it from how the boot
//! answered (the same either way), and claimed every device it touched.

use crate::{BootClaim, EmulatorState, boot_claim, booted_udids};

/// What a boot about to be issued would make of `udid`. Read before the
/// boot: afterwards the device is running either way.
pub(crate) async fn claim_before_boot(simctl: &smix_simctl::SimctlClient, udid: &str) -> BootClaim {
    let was_up = booted_udids(simctl).await.contains(&udid.to_uppercase());
    boot_claim(if was_up {
        EmulatorState::AlreadyRunning
    } else {
        EmulatorState::WasOff
    })
}

/// Write the claim to the machine's ledger. A write that fails is said,
/// not fatal: the device is up either way.
pub(crate) fn record_simulator_boot(udid: &str, claim: BootClaim) {
    let recorded = smix_capsule::runner::machine_leases().and_then(|leases| {
        smix_lease::store::record_boot(&leases, udid, claim == BootClaim::ClaimAsOurs)
            .map_err(|e| e.to_string())
    });
    if let Err(e) = recorded {
        eprintln!("warning: boot not recorded in the device ledger: {e}");
    }
}
