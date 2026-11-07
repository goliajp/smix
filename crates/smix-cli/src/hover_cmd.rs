//! `smix hover <selector>` / `smix hover --away` — a mouse pointer on an
//! element, or off the app.

use smix_driver::Platform;
use smix_input::Mouse;

use crate::act::{ActError, driver_for, parse_selector};
use crate::platform::RunPlatform;

/// What the command line said, before anything is resolved.
#[derive(clap::Args, Debug)]
pub struct HoverArgs {
    /// The element to rest the pointer on, in `<kind>:<value>` shorthand.
    #[arg(required_unless_present = "away", conflicts_with = "away")]
    pub selector: Option<String>,
    /// Take the pointer off the app instead, so nothing in it stays hovered.
    #[arg(long)]
    pub away: bool,
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

/// How long the element may take to appear, as a flow's `hoverOn` waits.
const APPEAR_WITHIN: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn cmd_hover(args: HoverArgs, port: u16, platform: Platform) -> Result<(), ActError> {
    let d = driver_for(platform, port);
    let Some(raw) = args.selector else {
        d.mouse(&Mouse::Away)
            .await
            .map_err(|e| ActError::Transport(e.to_prompt()))?;
        println!("pointer taken off the app");
        return Ok(());
    };
    let selector = parse_selector(&raw).map_err(|why| ActError::BadSelector(raw.clone(), why))?;
    // a miss reads like any other: what was on screen and what came close
    d.wait_for(&selector, APPEAR_WITHIN, None)
        .await
        .map_err(|e| ActError::Transport(e.to_prompt()))?;
    let tree = d
        .tree(None)
        .await
        .map_err(|e| ActError::Transport(e.to_prompt()))?;
    let at = smix_host_coord_resolver::resolve_to_norm_point_in_box(&tree, &selector, (0.5, 0.5))
        .map_err(|e| {
        ActError::Transport(format!(
            "{raw} is on screen but has no centre to hover: {e:?}"
        ))
    })?;
    let mouse = Mouse::Hover { at };
    mouse
        .check()
        .map_err(|why| ActError::BadSelector(raw.clone(), why))?;
    d.mouse(&mouse)
        .await
        .map_err(|e| ActError::Transport(e.to_prompt()))?;
    println!(
        "pointer resting on {raw} at ({:.3}, {:.3}) — not verified: check the hover style \
         with the next step",
        at.0, at.1
    );
    Ok(())
}
