//! Which runner a command drives: the arguments every runner verb shares.

use crate::platform::RunPlatform;

#[derive(clap::Args, Debug)]
pub struct RunnerTarget {
    /// Runner port override (defaults to SMIX_RUNNER_PORT env or 22087).
    /// `--runner-port` is accepted too — `smix run` spells it that way.
    #[arg(long, alias = "runner-port")]
    pub port: Option<u16>,
    /// Device UDID, or an alias / deviceName in the workspace's
    /// `.smix` registry. It finds the runner port that device is
    /// registered on, and — where a verb behaves differently by
    /// platform (fill, find, tap and the rest of the act surface) —
    /// reads the driver from the device's registered kind, so an
    /// Android device is driven through its own runner, not simctl.
    #[arg(long)]
    pub device: Option<String>,
    /// Which platform the device is, for a device this machine has not
    /// registered. A registered device's own kind is read otherwise.
    #[arg(long, value_enum)]
    pub platform: Option<RunPlatform>,
}
