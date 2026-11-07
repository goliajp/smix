//! `smix window-size <W>x<H>` — an app's desktop window resized, then read
//! back.

use smix_driver::Platform;
use smix_input::WindowSize;

use crate::act::driver_for;
use crate::platform::RunPlatform;

/// What the command line said, before anything is resolved.
#[derive(clap::Args, Debug)]
pub struct WindowSizeArgs {
    /// Width by height in dp, caption bar included: `1024x768`; or a named
    /// size, `compact` (412x800), `medium` (700x800) or `expanded` (1024x768).
    pub size: WindowSize,
    /// The app whose window to resize; the app in front when left out.
    #[arg(long)]
    pub app: Option<String>,
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

pub async fn cmd_window_size(
    args: WindowSizeArgs,
    port: u16,
    platform: Platform,
) -> Result<(), String> {
    let got = driver_for(platform, port)
        .set_window_size(args.app.as_deref(), args.size)
        .await
        .map_err(|e| e.to_prompt())?;
    args.size.read_back(got)?;
    println!("window reads back at {got}");
    Ok(())
}
