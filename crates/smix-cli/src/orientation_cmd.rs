//! `smix sim orientation <device> <orientation>` — turn a simulator or
//! emulator the way the flow step `setOrientation` does, through the
//! device's runner.

use smix_driver::{Orientation, Platform};
use smix_simctl::SimctlClient;
use smix_simctl::posture::{Facing, Turned};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct OrientationArgs {
    /// Which device, by UDID, emulator serial or registry alias. Its
    /// runner must be up.
    pub device: String,
    /// `portrait`, `landscape` (= `landscapeLeft`), `landscapeLeft`,
    /// `landscapeRight` or `portraitUpsideDown`. On a tablet whose
    /// screen is landscape at rest, `portrait` still means portrait.
    #[arg(value_parser = parse_orientation)]
    pub orientation: Orientation,
    /// Runner port; unset: SMIX_RUNNER_PORT, the runner smix brought up on DEVICE (the --device of other verbs), its registered runnerPort, 22087 (iOS) / 28080 (Android).
    #[arg(long, alias = "runner-port")]
    pub port: Option<u16>,
}

/// The names `setOrientation` takes in a flow, and its `landscape` alias.
pub fn parse_orientation(s: &str) -> Result<Orientation, String> {
    match s {
        "portrait" => Ok(Orientation::Portrait),
        "portraitUpsideDown" => Ok(Orientation::PortraitUpsideDown),
        "landscape" | "landscapeLeft" => Ok(Orientation::LandscapeLeft),
        "landscapeRight" => Ok(Orientation::LandscapeRight),
        other => Err(format!(
            "unknown orientation '{other}' — expected portrait | landscape | landscapeLeft | landscapeRight | portraitUpsideDown"
        )),
    }
}

pub async fn run(simctl: &SimctlClient, args: OrientationArgs) -> Result<(), crate::CliError> {
    let ios = crate::device_kind_of(&args.device) == smix_simctl::registry::DeviceKind::Simulator;
    let udid = if ios {
        Some(crate::resolve_device(&args.device)?)
    } else {
        None
    };
    let (port, platform) =
        crate::runner_target::runner_dial_target(args.port, Some(&args.device), None)?;
    cmd_orientation(simctl, args, udid, port, platform)
        .await
        .map_err(crate::CliError::Other)
}

async fn cmd_orientation(
    simctl: &SimctlClient,
    args: OrientationArgs,
    udid: Option<String>,
    port: u16,
    platform: Platform,
) -> Result<(), String> {
    // a foldable simulator ignores the runner's turn; the flow step goes
    // through Device Hub for it, and so does this
    if let Some(udid) = udid.as_deref() {
        let facing = match args.orientation {
            Orientation::Portrait => Facing::Portrait,
            Orientation::PortraitUpsideDown => Facing::PortraitUpsideDown,
            Orientation::LandscapeLeft => Facing::LandscapeLeft,
            Orientation::LandscapeRight => Facing::LandscapeRight,
        };
        match simctl.turn_foldable(udid, facing).await {
            Ok(Turned::Done) => return done(&args),
            Ok(Turned::NotFoldable | Turned::CannotTell) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    crate::act::driver_for(platform, port)
        .set_orientation(args.orientation)
        .await
        .map_err(|e| e.to_prompt())?;
    done(&args)
}

fn done(args: &OrientationArgs) -> Result<(), String> {
    println!(
        "orientation: {} → {}",
        args.device,
        args.orientation.as_wire()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flow_name_and_the_landscape_alias_parse() {
        assert_eq!(parse_orientation("portrait"), Ok(Orientation::Portrait));
        assert_eq!(
            parse_orientation("landscape"),
            Ok(Orientation::LandscapeLeft)
        );
        assert_eq!(
            parse_orientation("landscapeRight"),
            Ok(Orientation::LandscapeRight)
        );
        assert_eq!(
            parse_orientation("portraitUpsideDown"),
            Ok(Orientation::PortraitUpsideDown)
        );
        assert!(
            parse_orientation("sideways")
                .unwrap_err()
                .contains("portrait |")
        );
    }
}
