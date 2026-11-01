//! `smix swipe <direction>` / `smix swipe --from X,Y --to X,Y` — one swipe.

use smix_driver::Platform;

use crate::act;
use crate::platform::RunPlatform;

/// What the command line said, before anything is resolved.
#[derive(clap::Args, Debug)]
pub struct SwipeArgs {
    /// `up` / `down` / `left` / `right`. Leave it out when giving
    /// `--from` and `--to`.
    pub direction: Option<String>,
    /// Where the finger starts, as `X%,Y%` or `X,Y` in 0..1 —
    /// the authorised coordinate escape hatch (§9 #3), for screens
    /// with nothing nameable to swipe between. Requires `--to`.
    #[arg(long = "from")]
    pub from_point: Option<String>,
    /// Where the finger ends, same form as `--from`.
    #[arg(long = "to")]
    pub to_point: Option<String>,
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

pub async fn cmd_swipe(args: SwipeArgs, port: u16, platform: Platform) -> Result<(), String> {
    // Two ways in, and saying which was meant is the caller's job: a swipe
    // given both a direction and a pair of points has two answers, and
    // picking one silently is how a flow ends up doing something nobody
    // asked for.
    match (args.direction, args.from_point, args.to_point) {
        (Some(d), None, None) => act::cmd_swipe(d, port, platform)
            .await
            .map_err(|e| e.to_string()),
        (None, Some(from), Some(to)) => act::cmd_swipe_between(&from, &to, port, platform)
            .await
            .map_err(|e| e.to_string()),
        (None, None, None) => Err("swipe needs either a direction (`up` / `down` / `left` / \
                                   `right`) or both `--from` and `--to`"
            .to_string()),
        (None, Some(_), None) | (None, None, Some(_)) => {
            Err("a coordinate swipe needs both ends: give `--from` and `--to`".to_string())
        }
        (Some(_), _, _) => {
            Err("swipe takes a direction or a `--from`/`--to` pair, not both".to_string())
        }
    }
}
