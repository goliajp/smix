//! `smix sim shutdown`: turn off a device this machine turned on.

use crate::CliError;
use crate::device_ownership::{ShutdownVerdict, ownership_of, shutdown_verdict};

/// Shut `udid` down, if this machine's ledger says smix booted it.
///
/// A device already off is the state that was asked for, and is answered
/// as that before anything else is asked: the ownership refusal below
/// begins "is running", and said it of a simulator that was shut down.
pub(crate) async fn shutdown(
    simctl: &smix_simctl::SimctlClient,
    udid: &str,
    emulator: bool,
) -> Result<(), CliError> {
    let udid = udid.to_string();
    if is_running(simctl, &udid, emulator).await == Some(false) {
        forget_the_boot(&udid);
        println!("shutdown: {udid} was already shut down; nothing to do");
        return Ok(());
    }
    // The boot row records who may shut a device down. It was once never
    // read: the shutdown went ahead and then cleared the row, and a device
    // somebody else booted was stopped by whoever tidied up next.
    match shutdown_verdict(ownership_of(&udid), &udid) {
        ShutdownVerdict::Proceed => {}
        ShutdownVerdict::Refuse { message } => {
            return Err(CliError::Other(message));
        }
    }
    // Gone off since the look above is still the state asked for. The
    // mechanism differs by platform and the rule above does not: `emu kill` and `simctl shutdown` stop a
    // device in different ways, and both are only reached
    // once the ledger has said this process may.
    if emulator {
        let adb = smix_adb::AdbClient::new();
        if let Err(e) = adb.stop_emulator(&udid).await {
            // An emulator that is already gone is the state
            // asked for, same as the simctl arm below.
            if !format!("{e}").contains("device") {
                return Err(CliError::Other(format!("could not stop {udid}: {e}")));
            }
        }
    } else {
        match simctl.shutdown(&udid).await {
            Ok(()) => {}
            Err(smix_simctl::DeviceControlError::NonZeroExit { ref stderr, .. })
                if stderr.contains("current state: Shutdown") => {}
            Err(e) => return Err(e.into()),
        }
    }
    forget_the_boot(&udid);
    println!("shutdown: {udid}");
    Ok(())
}

/// Is the device on? `None` when neither tool would say.
async fn is_running(
    simctl: &smix_simctl::SimctlClient,
    udid: &str,
    emulator: bool,
) -> Option<bool> {
    if emulator {
        let devices = smix_adb::AdbClient::new().devices().await.ok()?;
        Some(devices.iter().any(|d| d.serial == udid))
    } else {
        let devices = simctl.list_devices().await.ok()?;
        let d = devices.iter().find(|d| d.udid.eq_ignore_ascii_case(udid))?;
        Some(d.state == "Booted")
    }
}

/// The boot row records who may shut this device down. Once it is off,
/// the answer is nobody — leaving the row behind would have a later
/// teardown shut down a device this process never turned on.
fn forget_the_boot(udid: &str) {
    if let Ok(leases) = smix_capsule::runner::machine_leases()
        && let Err(e) = smix_lease::store::drop_resource_kind(
            &leases,
            udid,
            &smix_lease::Resource::Booted { by_us: true },
        )
    {
        eprintln!("warning: boot row not cleared from the device ledger: {e}");
    }
}
