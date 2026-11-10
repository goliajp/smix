//! `smix rotate-crown <turns>` — the Digital Crown of an Apple Watch or
//! Apple Vision Pro.

use smix_driver::Platform;

use crate::act::driver_for;
use crate::platform::RunPlatform;

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct CrownArgs {
    /// Turns of the crown: 1 is a full turn, negative turns back. A
    /// positive turn scrolls a list on towards its later rows.
    #[arg(allow_hyphen_values = true)]
    pub rotations: f64,
    /// Turns a second; XCTest's own speed when unset.
    #[arg(long)]
    pub velocity: Option<f64>,
    /// Runner port; unset: SMIX_RUNNER_PORT, the runner smix brought up on --device, its registered runnerPort, 22087 (iOS) / 28080 (Android).
    #[arg(long, alias = "runner-port")]
    pub port: Option<u16>,
    /// Device UDID, or an alias in this machine's registry.
    #[arg(long)]
    pub device: Option<String>,
    /// Which platform the device is, for a device this machine has not
    /// registered.
    #[arg(long, value_enum)]
    pub platform: Option<RunPlatform>,
}

pub async fn cmd_rotate_crown(
    args: CrownArgs,
    port: u16,
    platform: Platform,
) -> Result<(), String> {
    smix_driver::crown_turn_check(args.rotations, args.velocity)
        .map_err(|why| format!("rotate-crown: {why}"))?;
    driver_for(platform, port)
        .rotate_crown(args.rotations, args.velocity)
        .await
        .map_err(|e| e.to_prompt())?;
    println!("turned the crown {} turns", args.rotations);
    Ok(())
}
