//! `smix sim repair`: rebuild a simulator's PosterBoard store.
//!
//! Booting through smix already does this for a simulator that is off. This
//! is for one already running with a store PosterBoard cannot load, which
//! has to be shut down first; that is refused unless smix booted it and
//! nobody holds it, the same rule `smix sim shutdown` follows.

use smix_simctl::poster_store;

use crate::CliError;
use crate::device_ownership::{ShutdownVerdict, ownership_of, shutdown_verdict};

pub(crate) async fn repair(
    simctl: &smix_simctl::SimctlClient,
    udid: &str,
) -> Result<std::process::ExitCode, CliError> {
    let data = poster_store::data_dir(udid).ok_or_else(|| {
        CliError::Other("HOME is not set, so the simulator's data cannot be found".into())
    })?;
    let broken = poster_store::broken_posters(&data);
    if broken.is_empty() {
        println!("repair: PosterBoard on {udid} can load its whole store; nothing to do");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    println!(
        "repair: PosterBoard on {udid} cannot load {} poster configuration(s):",
        broken.len()
    );
    for b in &broken {
        println!("  {b}");
    }
    let state = simctl
        .list_devices()
        .await?
        .into_iter()
        .find(|d| d.udid.eq_ignore_ascii_case(udid))
        .map(|d| d.state)
        .ok_or_else(|| CliError::Other(format!("{udid} is not a simulator on this Mac")))?;
    if state == "Shutdown" {
        poster_store::rebuild(&data).map_err(|e| {
            CliError::Other(format!(
                "removing the store from {} failed: {e}",
                data.display()
            ))
        })?;
        println!("repaired: {udid} — PosterBoard writes a new store at the next boot");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    if let ShutdownVerdict::Refuse { message } = shutdown_verdict(ownership_of(udid), udid) {
        return Err(CliError::Other(format!(
            "the store can only be rebuilt with the simulator off, and {message}\n\
             \x20 once it is off, `smix sim repair {udid}` or `smix sim boot {udid}` rebuilds it"
        )));
    }
    simctl.shutdown(udid).await?;
    // the boot is what rebuilds: it finds the store broken on a simulator
    // that is off, removes it, and PosterBoard writes a new one
    simctl
        .boot_and_wait(udid, std::time::Duration::from_secs(120))
        .await?;
    let left = poster_store::broken_posters(&data);
    if let Some(first) = left.first() {
        return Err(CliError::Other(format!(
            "{udid} was rebooted and PosterBoard still cannot load its store: {first}"
        )));
    }
    println!("repaired: {udid} — shut down, store rebuilt, booted again");
    Ok(std::process::ExitCode::SUCCESS)
}
