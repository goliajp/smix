//! `smix describe`: the screen as the elements a reader could name.

use smix_driver::SimctlDriver;

use crate::act::{ActError, runner_client};

/// What `smix describe` was given.
#[derive(clap::Args, Debug)]
pub struct DescribeArgs {
    /// Emit machine-readable JSON instead of the human outline.
    #[arg(long)]
    pub json: bool,
    /// One line per element — role, name, id — without bounds.
    #[arg(long)]
    pub compact: bool,
    /// Runner port; unset: SMIX_RUNNER_PORT, the runner smix brought up on --device, its registered runnerPort, 22087 (iOS) / 28080 (Android).
    /// `--runner-port` is accepted too — `smix run` spells it that way.
    #[arg(long, alias = "runner-port")]
    pub port: Option<u16>,
    /// Device UDID, or an alias / deviceName in the workspace's
    /// `.smix` registry; it finds the runner port that device is on.
    #[arg(long)]
    pub device: Option<String>,
}

pub async fn cmd_describe(json: bool, compact: bool, port: u16) -> Result<(), ActError> {
    let d = SimctlDriver::new(runner_client(port));
    let desc = d
        .describe()
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    if compact {
        print!("{}", desc.compact());
    } else if json {
        let s = serde_json::to_string_pretty(&desc)
            .map_err(|e| ActError::Transport(format!("serde: {e}")))?;
        println!("{s}");
    } else {
        println!("{desc:#?}");
    }
    Ok(())
}
